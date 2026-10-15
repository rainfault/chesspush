//! Native commands and application-lifetime workers for personal Statistics.
use super::{engine, metrics, projection, query, store, sync, types::*};
use crate::{domain::AppError, lichess::LichessService, storage};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{Emitter, Manager};

pub struct StatisticsRuntime {
    pub database: PathBuf,
    pub shutdown: AtomicBool,
    pub workers: AtomicUsize,
    pub sync_requested: AtomicBool,
}
// Count owned work before dispatch, so shutdown cannot race a not-yet-started task.
// Capturing this guard also releases activity on panic or a dropped task closure.
struct RuntimeActivity(Arc<StatisticsRuntime>);
impl RuntimeActivity {
    fn acquire(runtime: Arc<StatisticsRuntime>) -> Result<Self, AppError> {
        if runtime.shutdown.load(Ordering::SeqCst) {
            return Err(AppError::new("cancelled", "ChessPush закрывается"));
        }
        runtime.workers.fetch_add(1, Ordering::SeqCst);
        let activity = Self(runtime);
        if activity.0.shutdown.load(Ordering::SeqCst) {
            return Err(AppError::new("cancelled", "ChessPush закрывается"));
        }
        Ok(activity)
    }
}
impl Drop for RuntimeActivity {
    fn drop(&mut self) {
        self.0.workers.fetch_sub(1, Ordering::SeqCst);
    }
}
fn command_activity(app: &tauri::AppHandle) -> Result<RuntimeActivity, AppError> {
    RuntimeActivity::acquire(app.state::<Arc<StatisticsRuntime>>().inner().clone())
}
#[derive(Serialize)]
pub struct StatisticsState {
    pub accounts: Vec<store::AccountObservation>,
    pub active_account_id: Option<String>,
    pub sync: store::SyncStatus,
    pub engine: engine::EngineQueueStatus,
    pub quarantine: Vec<store::QuarantineRecord>,
}
#[derive(Serialize)]
pub struct GameDetail {
    pub game: StoredGame,
    pub runs: Vec<EngineRun>,
    pub events: Vec<DecisionEvent>,
    pub analyses: std::collections::BTreeMap<String, super::engine_metrics::EngineDetails>,
}
fn database(app: &tauri::AppHandle) -> PathBuf {
    app.state::<Arc<StatisticsRuntime>>().database.clone()
}
fn notify(app: &tauri::AppHandle) {
    let _ = app.emit("statistics-updated", ());
}
fn queue_available(path: &std::path::Path) -> Result<(), AppError> {
    let conn = storage::open(path)?;
    if engine::queue_status(&conn)?.configured {
        let ids = store::games(&conn, None)?
            .into_iter()
            .map(|g| g.id)
            .collect::<Vec<_>>();
        engine::enqueue(&conn, &ids)?;
    }
    Ok(())
}
pub fn install(app: &mut tauri::App, database: PathBuf) -> Result<(), AppError> {
    let conn = storage::open(&database)?;
    engine::recover_jobs(&conn)?;
    drop(conn);
    let runtime = Arc::new(StatisticsRuntime {
        database,
        shutdown: AtomicBool::new(false),
        workers: AtomicUsize::new(0),
        sync_requested: AtomicBool::new(true),
    });
    app.manage(runtime.clone());
    let sync_app = app.handle().clone();
    let sync_runtime = runtime.clone();
    let sync_activity = RuntimeActivity::acquire(runtime.clone())?;
    std::thread::Builder::new()
        .name("statistics-sync".into())
        .spawn(move || {
            let _activity = sync_activity;
            let mut last = Instant::now() - Duration::from_secs(60);
            let mut identity = None;
            while !sync_runtime.shutdown.load(Ordering::Relaxed) {
                let service = sync_app.state::<LichessService>();
                let current = service
                    .verified_identity()
                    .ok()
                    .flatten()
                    .map(|i| (i.id, i.generation));
                if current != identity {
                    identity = current;
                    sync_runtime.sync_requested.store(true, Ordering::Relaxed);
                }
                if sync_runtime.sync_requested.swap(false, Ordering::Relaxed)
                    || last.elapsed() >= Duration::from_secs(60)
                {
                    let _ = sync::run(&sync_runtime.database, &service, &sync_runtime.shutdown);
                    let _ = queue_available(&sync_runtime.database);
                    notify(&sync_app);
                    last = Instant::now();
                }
                std::thread::sleep(Duration::from_millis(250));
            }
        })
        .map_err(|e| AppError::new("statistics", e))?;
    let engine_app = app.handle().clone();
    let engine_runtime = runtime.clone();
    let engine_activity = RuntimeActivity::acquire(runtime.clone())?;
    std::thread::Builder::new()
        .name("statistics-engine".into())
        .spawn(move || {
            let _activity = engine_activity;
            let mut queued = Instant::now() - Duration::from_secs(20);
            while !engine_runtime.shutdown.load(Ordering::Relaxed) {
                if queued.elapsed() >= Duration::from_secs(20) {
                    let _ = queue_available(&engine_runtime.database);
                    queued = Instant::now();
                }
                match engine::worker_tick(&engine_runtime.database, &engine_runtime.shutdown) {
                    Ok(true) => notify(&engine_app),
                    Err(_) => {
                        notify(&engine_app);
                        std::thread::sleep(Duration::from_millis(500));
                    }
                    Ok(false) => std::thread::sleep(Duration::from_millis(250)),
                }
            }
        })
        .map_err(|e| AppError::new("statistics", e))?;
    Ok(())
}
/// The first caller owns the close sequence; subsequent close events only stay prevented.
pub fn stop(app: &tauri::AppHandle) -> bool {
    !app.state::<Arc<StatisticsRuntime>>()
        .shutdown
        .swap(true, Ordering::SeqCst)
}
pub fn idle(app: &tauri::AppHandle) -> bool {
    app.state::<Arc<StatisticsRuntime>>()
        .workers
        .load(Ordering::SeqCst)
        == 0
}
#[tauri::command]
pub async fn statistics_state(
    app: tauri::AppHandle,
    account_id: Option<String>,
) -> Result<StatisticsState, AppError> {
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let conn = storage::open(&path)?;
        let accounts = store::accounts(&conn)?;
        let selected = account_id
            .or(store::active_account(&conn)?.map(|a| a.id))
            .or_else(|| accounts.first().map(|a| a.id.clone()));
        let mut engine = engine::queue_status(&conn)?;
        if let Some(id) = selected.as_deref() {
            let ids = store::games(&conn, Some(id))?
                .into_iter()
                .map(|g| g.id)
                .collect::<std::collections::HashSet<_>>();
            engine.jobs.retain(|j| ids.contains(&j.game_id));
        }
        Ok(StatisticsState {
            active_account_id: store::active_account(&conn)?.map(|a| a.id),
            sync: store::sync_status(&conn, selected.as_deref())?,
            quarantine: store::quarantined(&conn, selected.as_deref())?,
            accounts,
            engine,
        })
    })
    .await
    .map_err(|_| AppError::new("statistics", "Состояние статистики недоступно"))?
}
#[tauri::command]
pub async fn statistics_report(
    app: tauri::AppHandle,
    filter: StatisticsFilter,
) -> Result<StatisticsReport, AppError> {
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let mut report = calculate_report(&path, filter)?;
        projection::compact(&mut report);
        Ok(report)
    })
    .await
    .map_err(|_| AppError::new("statistics", "Не удалось рассчитать статистику"))?
}
fn calculate_report(
    path: &std::path::Path,
    mut filter: StatisticsFilter,
) -> Result<StatisticsReport, AppError> {
    let conn = storage::open(path)?;
    if filter.account_id.is_none() {
        filter.account_id = store::active_account(&conn)?
            .or(store::accounts(&conn)?.into_iter().next())
            .map(|a| a.id);
    }
    let games = store::games(&conn, filter.account_id.as_deref())?;
    let runs = engine::runs(&conn)?;
    use sha2::{Digest, Sha256};
    let mut fingerprint = Sha256::new();
    fingerprint.update(
        serde_json::to_vec(&(&games, &runs, &filter))
            .map_err(|e| AppError::new("statistics", e))?,
    );
    let revision = format!("{:x}", fingerprint.finalize());
    let mut report = query::report(&games, &runs, &filter);
    report.evidence_revision = revision;
    let baselines = match filter.account_id.as_deref() {
        Some(id) => store::rating_snapshots(&conn, id)?
            .into_iter()
            .map(|s| (s.pool, s.rating, s.timestamp_ms))
            .collect::<Vec<_>>(),
        None => Vec::new(),
    };
    query::append_rating_baselines(&mut report, &baselines);
    if let Some(account) = filter
        .account_id
        .as_deref()
        .map(|id| store::account(&conn, id))
        .transpose()?
        .flatten()
    {
        query::append_observation(&mut report, account.t0_ms, store::now_ms());
    }
    Ok(report)
}
#[tauri::command]
pub async fn statistics_evidence(
    app: tauri::AppHandle,
    filter: StatisticsFilter,
    kind: String,
    key: String,
    revision: String,
    offset: usize,
    limit: usize,
) -> Result<serde_json::Value, AppError> {
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let report = calculate_report(&path, filter)?;
        if report.evidence_revision != revision {
            return Err(AppError::new(
                "statistics_changed",
                "Статистика обновилась; откройте список заново",
            ));
        }
        projection::page(&report, &kind, &key, offset, limit)
            .map_err(|e| AppError::new("statistics", e))
    })
    .await
    .map_err(|_| AppError::new("statistics", "Не удалось загрузить случаи"))?
}
#[tauri::command]
pub async fn statistics_game(
    app: tauri::AppHandle,
    id: String,
    account_id: Option<String>,
) -> Result<GameDetail, AppError> {
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let conn = storage::open(&path)?;
        let game = store::games(&conn, account_id.as_deref())?
            .into_iter()
            .find(|g| g.id == id)
            .ok_or_else(|| AppError::new("statistics", "Партия не найдена"))?;
        let runs = engine::runs(&conn)?
            .into_iter()
            .filter(|r| r.game_id == id)
            .collect::<Vec<_>>();
        let events = runs
            .iter()
            .flat_map(|r| metrics::game_metrics(&game, Some(r)).events)
            .collect();
        let analyses = runs
            .iter()
            .map(|run| {
                let filter = StatisticsFilter {
                    account_id: Some(game.account_id.clone()),
                    profile_id: Some(run.profile_id.clone()),
                    variant: Some(game.variant.clone()),
                    include_uncertain: Some(true),
                    ..Default::default()
                };
                (
                    run.id.clone(),
                    query::report(
                        std::slice::from_ref(&game),
                        std::slice::from_ref(run),
                        &filter,
                    )
                    .analysis,
                )
            })
            .collect();
        Ok(GameDetail {
            game,
            runs,
            events,
            analyses,
        })
    })
    .await
    .map_err(|_| AppError::new("statistics", "Не удалось открыть партию"))?
}
#[tauri::command]
pub fn statistics_sync(app: tauri::AppHandle) -> Result<(), AppError> {
    app.state::<Arc<StatisticsRuntime>>()
        .sync_requested
        .store(true, Ordering::Relaxed);
    Ok(())
}
#[tauri::command]
pub async fn statistics_engine_config(
    app: tauri::AppHandle,
) -> Result<engine::EngineConfig, AppError> {
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || engine::configuration(&storage::open(&path)?))
        .await
        .map_err(|_| AppError::new("engine", "Настройки недоступны"))?
}
#[tauri::command]
pub async fn statistics_engine_validate(
    app: tauri::AppHandle,
    config: engine::EngineConfig,
) -> Result<String, AppError> {
    let activity = command_activity(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        if activity.0.shutdown.load(Ordering::SeqCst) {
            return Err(AppError::new("cancelled", "ChessPush закрывается"));
        }
        let _activity = activity;
        engine::validate_config(&config).map(|i| i.name)
    })
    .await
    .map_err(|_| AppError::new("engine", "Движок недоступен"))?
}
#[tauri::command]
pub async fn statistics_engine_save(
    app: tauri::AppHandle,
    config: engine::EngineConfig,
) -> Result<String, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if activity.0.shutdown.load(Ordering::SeqCst) {
            return Err(AppError::new("cancelled", "ChessPush закрывается"));
        }
        let _activity = activity;
        let conn = storage::open(&path)?;
        if config.path.trim().is_empty() {
            return Err(AppError::new(
                "engine",
                "Укажите исполняемый файл Stockfish",
            ));
        }
        let identity = engine::save_configuration(&conn, &config)?;
        queue_available(&path)?;
        notify(&handle);
        Ok(identity.name)
    })
    .await
    .map_err(|_| AppError::new("engine", "Не удалось сохранить движок"))?
}
#[tauri::command]
pub async fn statistics_engine_enqueue(
    app: tauri::AppHandle,
    game_ids: Vec<String>,
) -> Result<usize, AppError> {
    let path = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let n = engine::enqueue_force(&storage::open(&path)?, &game_ids)?;
        notify(&handle);
        Ok(n)
    })
    .await
    .map_err(|_| AppError::new("engine", "Очередь недоступна"))?
}
#[tauri::command]
pub async fn statistics_engine_pause(app: tauri::AppHandle, paused: bool) -> Result<(), AppError> {
    let path = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        engine::set_paused(&storage::open(&path)?, paused)?;
        notify(&handle);
        Ok(())
    })
    .await
    .map_err(|_| AppError::new("engine", "Очередь недоступна"))?
}
#[tauri::command]
pub async fn statistics_engine_cancel(app: tauri::AppHandle, job_id: i64) -> Result<(), AppError> {
    let path = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        engine::cancel_job(&storage::open(&path)?, job_id)?;
        notify(&handle);
        Ok(())
    })
    .await
    .map_err(|_| AppError::new("engine", "Очередь недоступна"))?
}

#[tauri::command]
pub async fn statistics_quarantine_retry(
    app: tauri::AppHandle,
    account_id: String,
    id: String,
) -> Result<(), AppError> {
    let path = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        store::retry_quarantined(&storage::open(&path)?, &account_id, &id, store::now_ms())?;
        handle
            .state::<Arc<StatisticsRuntime>>()
            .sync_requested
            .store(true, Ordering::Relaxed);
        notify(&handle);
        Ok(())
    })
    .await
    .map_err(|_| AppError::new("statistics", "Повторная загрузка недоступна"))?
}

#[cfg(test)]
mod shutdown_tests {
    use super::*;
    fn runtime() -> Arc<StatisticsRuntime> {
        Arc::new(StatisticsRuntime {
            database: PathBuf::new(),
            shutdown: AtomicBool::new(false),
            workers: AtomicUsize::new(0),
            sync_requested: AtomicBool::new(false),
        })
    }
    #[test]
    fn activity_is_counted_before_dispatch_and_shutdown_rejects_new_tasks() {
        let runtime = runtime();
        let activity = RuntimeActivity::acquire(runtime.clone()).unwrap();
        assert_eq!(runtime.workers.load(Ordering::SeqCst), 1);
        runtime.shutdown.store(true, Ordering::SeqCst);
        assert!(RuntimeActivity::acquire(runtime.clone()).is_err());
        assert_eq!(runtime.workers.load(Ordering::SeqCst), 1);
        drop(activity);
        assert_eq!(runtime.workers.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn activity_is_released_for_a_dropped_dispatch_closure() {
        let runtime = runtime();
        let activity = RuntimeActivity::acquire(runtime.clone()).unwrap();
        let task = move || {
            let _activity = activity;
        };
        assert_eq!(runtime.workers.load(Ordering::SeqCst), 1);
        drop(task);
        assert_eq!(runtime.workers.load(Ordering::SeqCst), 0);
    }
    #[test]
    fn activity_is_released_when_the_worker_panics() {
        let runtime = runtime();
        let activity = RuntimeActivity::acquire(runtime.clone()).unwrap();
        let result = std::panic::catch_unwind(move || {
            let _activity = activity;
            panic!("simulated owned worker failure");
        });
        assert!(result.is_err());
        assert_eq!(runtime.workers.load(Ordering::SeqCst), 0);
    }
}

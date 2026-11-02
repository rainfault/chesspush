//! Native boundary for the personal game-review use cases.
use crate::{
    domain::AppError,
    game_review,
    knowledge::KnowledgeDocument,
    statistics::desktop::{StatisticsRuntime, command_activity},
    storage,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use tauri::{Emitter, Manager};

struct LiveJob {
    id: String,
    cancel: Arc<AtomicBool>,
}
static LIVE_JOB: Mutex<Option<LiveJob>> = Mutex::new(None);
static LIVE_GATE: Mutex<()> = Mutex::new(());
fn notify(app: &tauri::AppHandle) {
    for name in [
        "game-review-updated",
        "knowledge-updated",
        "statistics-updated",
    ] {
        let _ = app.emit(name, ());
    }
}
#[tauri::command]
pub async fn game_review_list(
    app: tauri::AppHandle,
    request: game_review::ListRequest,
) -> Result<game_review::GamePage, AppError> {
    let activity = command_activity(&app)?;
    let path = app.state::<Arc<StatisticsRuntime>>().database.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        game_review::list(&storage::open(&path)?, request)
    })
    .await
    .map_err(|_| AppError::new("game_review", "Список партий недоступен"))?
}
#[tauri::command]
pub async fn game_review_open(
    app: tauri::AppHandle,
    game_id: String,
    account_id: Option<String>,
    source_revision: Option<i64>,
) -> Result<game_review::Review, AppError> {
    let activity = command_activity(&app)?;
    let path = app.state::<Arc<StatisticsRuntime>>().database.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let review = game_review::open(
            &storage::open(&path)?,
            &game_id,
            account_id.as_deref(),
            source_revision,
        )?;
        if review.created {
            notify(&handle);
        }
        Ok(review)
    })
    .await
    .map_err(|_| AppError::new("game_review", "Разбор партии недоступен"))?
}
#[tauri::command]
pub async fn game_review_mark(
    app: tauri::AppHandle,
    game_id: String,
    account_id: Option<String>,
    source_revision: i64,
    reviewed: bool,
) -> Result<(), AppError> {
    let activity = command_activity(&app)?;
    let path = app.state::<Arc<StatisticsRuntime>>().database.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        game_review::mark(
            &storage::open(&path)?,
            &game_id,
            account_id.as_deref(),
            source_revision,
            reviewed,
        )?;
        notify(&handle);
        Ok(())
    })
    .await
    .map_err(|_| AppError::new("game_review", "Не удалось сохранить отметку партии"))?
}
#[tauri::command]
pub async fn game_review_save_knowledge(
    app: tauri::AppHandle,
    request: game_review::SaveKnowledgeRequest,
) -> Result<KnowledgeDocument, AppError> {
    let activity = command_activity(&app)?;
    let path = app.state::<Arc<StatisticsRuntime>>().database.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let document = game_review::save_knowledge(&storage::open(&path)?, request)?;
        notify(&handle);
        Ok(document)
    })
    .await
    .map_err(|_| AppError::new("game_review", "Не удалось сохранить знание"))?
}
#[tauri::command]
pub async fn knowledge_use_repertoire(
    app: tauri::AppHandle,
    document_id: String,
    expected_revision: i64,
    side: String,
) -> Result<KnowledgeDocument, AppError> {
    let activity = command_activity(&app)?;
    let path = app.state::<Arc<StatisticsRuntime>>().database.clone();
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let document = game_review::use_repertoire(
            &storage::open(&path)?,
            &document_id,
            expected_revision,
            &side,
        )?;
        notify(&handle);
        Ok(document)
    })
    .await
    .map_err(|_| AppError::new("game_review", "Не удалось подключить знание к репертуару"))?
}
#[tauri::command]
pub async fn game_review_evaluate(
    app: tauri::AppHandle,
    document_id: String,
    revision: i64,
    chapter_id: String,
    node_id: String,
    request_id: String,
) -> Result<game_review::LiveEvaluation, AppError> {
    if request_id.is_empty() || request_id.len() > 100 {
        return Err(AppError::new("validation", "Некорректный запрос анализа"));
    }
    let activity = command_activity(&app)?;
    let runtime = app.state::<Arc<StatisticsRuntime>>().inner().clone();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut job = LIVE_JOB
            .lock()
            .map_err(|_| AppError::new("game_review", "Анализ недоступен"))?;
        if let Some(previous) = job.as_ref() {
            previous.cancel.store(true, Ordering::Relaxed);
        }
        *job = Some(LiveJob {
            id: request_id.clone(),
            cancel: cancel.clone(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let result = (|| {
            let _gate = LIVE_GATE
                .lock()
                .map_err(|_| AppError::new("game_review", "Анализ недоступен"))?;
            if cancel.load(Ordering::Relaxed) || runtime.shutdown.load(Ordering::Relaxed) {
                return Err(AppError::new("cancelled", "Анализ отменён"));
            }
            game_review::evaluate(
                &storage::open(&runtime.database)?,
                &document_id,
                revision,
                &chapter_id,
                &node_id,
                || cancel.load(Ordering::Relaxed) || runtime.shutdown.load(Ordering::Relaxed),
            )
        })();
        if let Ok(mut job) = LIVE_JOB.lock()
            && job.as_ref().is_some_and(|job| job.id == request_id)
        {
            *job = None;
        }
        result
    })
    .await
    .map_err(|_| AppError::new("game_review", "Не удалось выполнить анализ позиции"))?
}
#[tauri::command]
pub fn game_review_cancel_evaluation(request_id: String) -> Result<(), AppError> {
    let job = LIVE_JOB
        .lock()
        .map_err(|_| AppError::new("game_review", "Анализ недоступен"))?;
    if let Some(job) = job.as_ref()
        && job.id == request_id
    {
        job.cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}

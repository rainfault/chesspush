use crate::{
    application::{self, ParseRequest, ParseResult},
    domain::AppError,
    lichess::{self, AccountStatus, LichessService},
    parser::Progress,
    storage,
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{Emitter, Manager, State};

struct ActiveJob {
    id: String,
    cancel: Arc<AtomicBool>,
}
// Shared only because the worker and cancellation command run on different threads.
#[derive(Default)]
struct Jobs(Mutex<Option<ActiveJob>>);
struct AppDirectory(PathBuf);
#[derive(Clone, Serialize)]
struct JobProgress {
    id: String,
    progress: Progress,
}
#[derive(Clone, Serialize)]
struct JobFinished {
    id: String,
    result: Result<ParseResult, AppError>,
}

#[tauri::command]
fn start_parse(
    app: tauri::AppHandle,
    request: ParseRequest,
    id: String,
    jobs: State<Jobs>,
    directory: State<AppDirectory>,
) -> Result<(), AppError> {
    if id.is_empty() || id.len() > 100 {
        return Err(AppError::new("validation", "Некорректный id операции"));
    }
    let mut active = jobs
        .0
        .lock()
        .map_err(|_| AppError::new("state", "Состояние операции недоступно"))?;
    if active.is_some() {
        return Err(AppError::new("busy", "Операция уже выполняется"));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    *active = Some(ActiveJob {
        id: id.clone(),
        cancel: cancel.clone(),
    });
    let directory = directory.0.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = application::run(request, &directory, &cancel, |p| {
            let _ = app.emit(
                "parser-progress",
                JobProgress {
                    id: id.clone(),
                    progress: p.clone(),
                },
            );
        });
        // Release before finished: a new run started in response to this event is valid.
        if let Ok(mut jobs) = app.state::<Jobs>().0.lock() {
            *jobs = None;
        }
        let _ = app.emit("parser-finished", JobFinished { id, result });
    });
    Ok(())
}
#[tauri::command]
fn cancel_parse(id: String, jobs: State<Jobs>) -> Result<(), AppError> {
    let active = jobs
        .0
        .lock()
        .map_err(|_| AppError::new("state", "Состояние операции недоступно"))?;
    if let Some(job) = active.as_ref()
        && job.id == id
    {
        job.cancel.store(true, Ordering::Relaxed);
    }
    Ok(())
}
#[tauri::command]
fn library_files(directory: State<AppDirectory>) -> Result<Vec<storage::LibraryFile>, AppError> {
    storage::list(&storage::open(&directory.0.join("chesspush.sqlite3"))?)
}
#[tauri::command]
fn remember_file(path: PathBuf, directory: State<AppDirectory>) -> Result<(), AppError> {
    if !path.is_file() {
        return Err(AppError::new("io", "Файл не найден"));
    }
    storage::remember(
        &storage::open(&directory.0.join("chesspush.sqlite3"))?,
        &path,
        "source",
    )?;
    Ok(())
}
#[tauri::command]
fn reveal_file(id: i64, directory: State<AppDirectory>) -> Result<(), AppError> {
    let conn = storage::open(&directory.0.join("chesspush.sqlite3"))?;
    let path: String = conn.query_row("SELECT path FROM files WHERE id=?1", [id], |r| r.get(0))?;
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(AppError::new("io", "Файл перемещён или удалён"));
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path.display()))
            .spawn()?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()?;
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(&path))
            .spawn()?;
    }
    Ok(())
}
#[tauri::command]
async fn library_games(
    directory: State<'_, AppDirectory>,
    file_id: i64,
    offset: usize,
) -> Result<crate::library::LibraryPage, AppError> {
    let path = directory.0.join("chesspush.sqlite3");
    tauri::async_runtime::spawn_blocking(move || {
        crate::library::page(&storage::open(&path)?, file_id, offset)
    })
    .await
    .map_err(|_| AppError::new("library", "Не удалось открыть список партий"))?
}
#[tauri::command]
async fn library_game(
    directory: State<'_, AppDirectory>,
    id: i64,
) -> Result<crate::library::LibraryDetail, AppError> {
    let path = directory.0.join("chesspush.sqlite3");
    tauri::async_runtime::spawn_blocking(move || crate::library::detail(&storage::open(&path)?, id))
        .await
        .map_err(|_| AppError::new("library", "Не удалось открыть партию"))?
}
#[tauri::command]
async fn statistics_controls(
    directory: State<'_, AppDirectory>,
    account_id: Option<String>,
) -> Result<Vec<String>, AppError> {
    let path = directory.0.join("chesspush.sqlite3");
    tauri::async_runtime::spawn_blocking(move || {
        crate::recent::controls(&storage::open(&path)?, account_id.as_deref())
    })
    .await
    .map_err(|_| AppError::new("controls", "Не удалось загрузить контроли времени"))?
}
#[tauri::command]
async fn statistics_recent(
    directory: State<'_, AppDirectory>,
) -> Result<Vec<crate::recent::RecentGame>, AppError> {
    let path = directory.0.join("chesspush.sqlite3");
    tauri::async_runtime::spawn_blocking(move || crate::recent::list(&storage::open(&path)?))
        .await
        .map_err(|_| AppError::new("recent", "Не удалось загрузить последние партии"))?
}

#[tauri::command]
fn get_theme(directory: State<AppDirectory>) -> Result<String, AppError> {
    Ok(storage::setting(
        &storage::open(&directory.0.join("chesspush.sqlite3"))?,
        "theme",
    )?
    .unwrap_or("light".into()))
}
#[tauri::command]
fn set_theme(theme: String, directory: State<AppDirectory>) -> Result<(), AppError> {
    if !["light", "dark"].contains(&theme.as_str()) {
        return Err(AppError::new("validation", "Неизвестная тема"));
    }
    storage::set_setting(
        &storage::open(&directory.0.join("chesspush.sqlite3"))?,
        "theme",
        &theme,
    )
}

// Blocking network/credential operations run outside the webview/main event loop.
#[tauri::command]
async fn account_status(app: tauri::AppHandle) -> Result<AccountStatus, AppError> {
    tauri::async_runtime::spawn_blocking(move || app.state::<LichessService>().refresh())
        .await
        .map_err(|_| AppError::new("state", "Подключение Lichess недоступно"))?
}
#[tauri::command]
async fn connect_lichess_token(
    app: tauri::AppHandle,
    token: String,
) -> Result<AccountStatus, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let status = app.state::<LichessService>().connect_token(token)?;
        let _ = app.emit("lichess-account-updated", &status);
        Ok(status)
    })
    .await
    .map_err(|_| AppError::new("state", "Подключение Lichess недоступно"))?
}
#[tauri::command]
fn start_lichess_auth(
    app: tauri::AppHandle,
    service: State<LichessService>,
) -> Result<AccountStatus, AppError> {
    let flow = service.begin_auth()?;
    if lichess::open_browser(&flow.url).is_err() {
        let status = service.finish_browser_failure(&flow)?;
        let _ = app.emit("lichess-account-updated", &status);
        return Ok(status);
    }
    let status = service.current()?;
    let _ = app.emit("lichess-account-updated", &status);
    tauri::async_runtime::spawn_blocking(move || {
        let result = app.state::<LichessService>().complete_auth(flow);
        // Errors are deliberately sanitized by the service and contain no credentials.
        if let Ok(status) = result {
            let _ = app.emit("lichess-account-updated", &status);
        }
    });
    Ok(status)
}
#[tauri::command]
fn cancel_lichess_auth(
    app: tauri::AppHandle,
    service: State<LichessService>,
) -> Result<AccountStatus, AppError> {
    let status = service.cancel_auth()?;
    let _ = app.emit("lichess-account-updated", &status);
    Ok(status)
}
#[tauri::command]
async fn disconnect_lichess(app: tauri::AppHandle) -> Result<AccountStatus, AppError> {
    tauri::async_runtime::spawn_blocking(move || {
        let status = app.state::<LichessService>().disconnect()?;
        let _ = app.emit("lichess-account-updated", &status);
        Ok(status)
    })
    .await
    .map_err(|_| AppError::new("state", "Подключение Lichess недоступно"))?
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Jobs::default())
        .setup(|app| {
            // Tests may isolate application data; release always uses the standard app directory.
            let directory = app.path().app_data_dir()?;
            #[cfg(debug_assertions)]
            let directory = std::env::var_os("CHESSPUSH_TEST_DATA")
                .map(PathBuf::from)
                .unwrap_or(directory);
            storage::open(&directory.join("chesspush.sqlite3"))?;
            app.manage(LichessService::new(&directory)?);
            app.manage(AppDirectory(directory.clone()));
            crate::statistics::desktop::install(app, directory.join("chesspush.sqlite3"))?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            crate::knowledge::desktop::knowledge_list,
            crate::knowledge::desktop::knowledge_load,
            crate::knowledge::desktop::knowledge_create,
            crate::knowledge::desktop::knowledge_import,
            crate::knowledge::desktop::knowledge_apply,
            crate::knowledge::desktop::knowledge_export,
            crate::knowledge::desktop::knowledge_navigate,
            crate::knowledge::desktop::knowledge_attach_file,
            crate::knowledge::desktop::knowledge_set_attachment,
            crate::knowledge::desktop::knowledge_delete_repertoire,
            crate::knowledge::desktop::knowledge_repertoire,
            crate::knowledge::desktop::knowledge_repertoire_export,
            crate::knowledge::desktop::training_overview,
            crate::knowledge::desktop::training_start,
            crate::knowledge::desktop::training_answer,
            crate::knowledge::desktop::training_hint,
            crate::knowledge::desktop::training_next,
            crate::game_graph_desktop::game_graph_query,
            crate::game_graph_desktop::game_graph_evidence,
            crate::game_graph_desktop::game_graph_resolve_occurrence,
            crate::game_graph_desktop::game_graph_load_state,
            crate::game_graph_desktop::game_graph_save_state,
            start_parse,
            cancel_parse,
            library_files,
            library_games,
            library_game,
            remember_file,
            reveal_file,
            get_theme,
            set_theme,
            account_status,
            connect_lichess_token,
            start_lichess_auth,
            cancel_lichess_auth,
            disconnect_lichess,
            crate::statistics::desktop::statistics_state,
            crate::statistics::desktop::statistics_report,
            crate::statistics::desktop::statistics_openings,
            crate::statistics::desktop::statistics_preferences,
            crate::statistics::desktop::statistics_preferences_save,
            crate::statistics::desktop::statistics_reset,
            statistics_recent,
            statistics_controls,
            crate::statistics::desktop::statistics_evidence,
            crate::statistics::desktop::statistics_game,
            crate::statistics::desktop::statistics_sync,
            crate::statistics::desktop::statistics_engine_config,
            crate::statistics::desktop::statistics_engine_validate,
            crate::statistics::desktop::statistics_engine_save,
            crate::statistics::desktop::statistics_engine_enqueue,
            crate::statistics::desktop::statistics_engine_pause,
            crate::statistics::desktop::statistics_engine_cancel,
            crate::statistics::desktop::statistics_quarantine_retry
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Prevent immediately. Credential/session and parser locks may wait for a
                // committed SQLite batch, and must never be acquired on the window thread.
                api.prevent_close();
                if !crate::statistics::desktop::stop(window.app_handle()) {
                    return;
                }
                let handle = window.app_handle().clone();
                std::thread::spawn(move || {
                    let _ = handle.state::<LichessService>().cancel_auth();
                    if let Ok(active) = handle.state::<Jobs>().0.lock()
                        && let Some(job) = active.as_ref()
                    {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                    loop {
                        let parser_idle = handle
                            .state::<Jobs>()
                            .0
                            .lock()
                            .map(|j| j.is_none())
                            .unwrap_or(true);
                        // Never bypass owned UCI task cleanup with process::exit. Each
                        // external wait has its own deadline; its guard includes child Drop.
                        if parser_idle && crate::statistics::desktop::idle(&handle) {
                            handle.exit(0);
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                });
            }
        })
        .run(tauri::generate_context!())
        .expect("ChessPush could not start");
}

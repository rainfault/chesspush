use crate::{
    application::{self, ParseRequest, ParseResult},
    domain::AppError,
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
    time::Instant,
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
            app.manage(AppDirectory(directory));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_parse,
            cancel_parse,
            library_files,
            remember_file,
            reveal_file,
            get_theme,
            set_theme
        ])
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let jobs = window.state::<Jobs>();
                if let Ok(active) = jobs.0.lock()
                    && let Some(job) = active.as_ref()
                {
                    job.cancel.store(true, Ordering::Relaxed);
                    api.prevent_close();
                    let handle = window.app_handle().clone();
                    // Let the worker flush its partial PGN and SQLite batch before exiting.
                    std::thread::spawn(move || {
                        let started = Instant::now();
                        loop {
                            let idle = handle
                                .state::<Jobs>()
                                .0
                                .lock()
                                .map(|j| j.is_none())
                                .unwrap_or(true);
                            if idle {
                                handle.exit(0);
                                break;
                            }
                            if started.elapsed().as_secs() > 30 {
                                break;
                            }
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                    });
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("ChessPush could not start");
}

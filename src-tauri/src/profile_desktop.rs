//! Native boundary for profile snapshots; restore application occurs on restart.
use crate::{
    domain::AppError,
    profile::{self, BackupSummary, PreparedRestore, ProfileStatus, UiState},
    statistics::desktop::{StatisticsRuntime, command_activity},
};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::Manager;

static PROFILE_OPERATION: AtomicBool = AtomicBool::new(false);
pub(crate) struct ProfileOperation;
impl Drop for ProfileOperation {
    fn drop(&mut self) {
        PROFILE_OPERATION.store(false, Ordering::Release);
    }
}
/// The parser holds this same owned lease until its managed PGN is finished.
pub(crate) fn acquire_operation() -> Result<ProfileOperation, AppError> {
    PROFILE_OPERATION
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map(|_| ProfileOperation)
        .map_err(|_| AppError::new("busy", "Другая операция с профилем ещё выполняется"))
}
fn directory(app: &tauri::AppHandle) -> Result<PathBuf, AppError> {
    app.state::<Arc<StatisticsRuntime>>()
        .database
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| AppError::new("profile", "Каталог профиля недоступен"))
}
fn parser_idle(app: &tauri::AppHandle) -> Result<(), AppError> {
    if crate::desktop::profile_parser_busy(app) {
        return Err(AppError::new(
            "busy",
            "Дождитесь завершения отбора или импорта PGN",
        ));
    }
    Ok(())
}
#[tauri::command]
pub async fn profile_backup(
    app: tauri::AppHandle,
    path: String,
    ui_state: UiState,
) -> Result<BackupSummary, AppError> {
    let activity = command_activity(&app)?;
    parser_idle(&app)?;
    let directory = directory(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let _operation = acquire_operation()?;
        profile::backup(&directory, std::path::Path::new(&path), ui_state)
    })
    .await
    .map_err(|_| AppError::new("profile", "Не удалось создать резервную копию"))?
}
#[tauri::command]
pub async fn profile_restore_preview(
    app: tauri::AppHandle,
    path: String,
) -> Result<BackupSummary, AppError> {
    let activity = command_activity(&app)?;
    let directory = directory(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let _operation = acquire_operation()?;
        profile::preview(&directory, std::path::Path::new(&path))
    })
    .await
    .map_err(|_| AppError::new("profile", "Не удалось проверить резервную копию"))?
}
#[tauri::command]
pub async fn profile_restore_prepare(
    app: tauri::AppHandle,
    path: String,
    expected_sha256: String,
    current_ui_state: UiState,
) -> Result<PreparedRestore, AppError> {
    let activity = command_activity(&app)?;
    parser_idle(&app)?;
    let directory = directory(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let _operation = acquire_operation()?;
        profile::prepare_restore(
            &directory,
            std::path::Path::new(&path),
            &expected_sha256,
            current_ui_state,
        )
    })
    .await
    .map_err(|_| AppError::new("profile", "Не удалось подготовить восстановление"))?
}
#[tauri::command]
pub async fn profile_restore_cancel(app: tauri::AppHandle) -> Result<(), AppError> {
    let activity = command_activity(&app)?;
    let directory = directory(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let _operation = acquire_operation()?;
        profile::cancel_restore(&directory)
    })
    .await
    .map_err(|_| AppError::new("profile", "Не удалось отменить восстановление"))?
}
#[tauri::command]
pub async fn profile_restored_ui(app: tauri::AppHandle) -> Result<Option<UiState>, AppError> {
    let activity = command_activity(&app)?;
    let directory = directory(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let _operation = acquire_operation()?;
        profile::restored_ui(&directory)
    })
    .await
    .map_err(|_| AppError::new("profile", "Настройки восстановленного профиля недоступны"))?
}
#[tauri::command]
pub async fn profile_restored_ui_ack(app: tauri::AppHandle) -> Result<(), AppError> {
    let activity = command_activity(&app)?;
    let directory = directory(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let _operation = acquire_operation()?;
        profile::acknowledge_restored_ui(&directory)
    })
    .await
    .map_err(|_| {
        AppError::new(
            "profile",
            "Не удалось подтвердить восстановление интерфейса",
        )
    })?
}
#[tauri::command]
pub async fn profile_status(app: tauri::AppHandle) -> Result<ProfileStatus, AppError> {
    let activity = command_activity(&app)?;
    let directory = directory(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        profile::status(&directory)
    })
    .await
    .map_err(|_| AppError::new("profile", "Состояние профиля недоступно"))?
}

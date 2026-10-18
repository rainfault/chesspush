//! Thin asynchronous native boundary over the shared headless document API.
use super::*;
use crate::{
    statistics::desktop::{StatisticsRuntime, command_activity},
    storage,
};
use std::sync::Arc;
use tauri::{Emitter, Manager};
fn database(app: &tauri::AppHandle) -> std::path::PathBuf {
    app.state::<Arc<StatisticsRuntime>>().database.clone()
}
#[tauri::command]
pub async fn knowledge_list(app: tauri::AppHandle) -> Result<Vec<DocumentSummary>, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        list(&storage::open(&path)?)
    })
    .await
    .map_err(|_| error("Список документов недоступен"))?
}
#[tauri::command]
pub async fn knowledge_load(
    app: tauri::AppHandle,
    document_id: String,
    revision: Option<i64>,
) -> Result<KnowledgeDocument, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        load(&storage::open(&path)?, &document_id, revision)
    })
    .await
    .map_err(|_| error("Документ недоступен"))?
}
#[tauri::command]
pub async fn knowledge_create(
    app: tauri::AppHandle,
    request: CreateRequest,
) -> Result<KnowledgeDocument, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let result = create(&storage::open(&path)?, request)?;
        let _ = handle.emit("knowledge-updated", ());
        Ok(result)
    })
    .await
    .map_err(|_| error("Не удалось создать документ"))?
}
#[tauri::command]
pub async fn knowledge_import(
    app: tauri::AppHandle,
    request: ImportRequest,
) -> Result<KnowledgeDocument, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let result = import(&storage::open(&path)?, request)?;
        let _ = handle.emit("knowledge-updated", ());
        Ok(result)
    })
    .await
    .map_err(|_| error("Не удалось импортировать PGN"))?
}
#[tauri::command]
pub async fn knowledge_apply(
    app: tauri::AppHandle,
    document_id: String,
    expected_revision: i64,
    operation: EditOperation,
) -> Result<KnowledgeDocument, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let result = apply(
            &storage::open(&path)?,
            &document_id,
            expected_revision,
            operation,
        )?;
        let _ = handle.emit("knowledge-updated", ());
        Ok(result)
    })
    .await
    .map_err(|_| error("Не удалось сохранить документ"))?
}
#[tauri::command]
pub async fn knowledge_export(
    app: tauri::AppHandle,
    document_id: String,
    revision: Option<i64>,
) -> Result<String, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        export_document(&load(&storage::open(&path)?, &document_id, revision)?)
    })
    .await
    .map_err(|_| error("Не удалось экспортировать PGN"))?
}
#[tauri::command]
pub async fn knowledge_navigate(
    app: tauri::AppHandle,
    document_id: String,
    chapter_id: String,
    node_id: String,
    revision: Option<i64>,
) -> Result<Navigation, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        navigate(
            &storage::open(&path)?,
            &document_id,
            &chapter_id,
            &node_id,
            revision,
        )
    })
    .await
    .map_err(|_| error("Позиция документа недоступна"))?
}
#[tauri::command]
pub async fn knowledge_attach_file(
    app: tauri::AppHandle,
    path: String,
    side: String,
) -> Result<KnowledgeDocument, AppError> {
    let activity = command_activity(&app)?;
    let database = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let result = fragments::attach_file(
            &storage::open(&database)?,
            std::path::Path::new(&path),
            &side,
        )?;
        let _ = handle.emit("knowledge-updated", ());
        Ok(result)
    })
    .await
    .map_err(|_| error("Не удалось прикрепить PGN"))?
}
#[tauri::command]
pub async fn knowledge_set_attachment(
    app: tauri::AppHandle,
    document_id: String,
    expected_revision: i64,
    active: bool,
) -> Result<KnowledgeDocument, AppError> {
    let activity = command_activity(&app)?;
    let database = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let result = fragments::set_attachment(
            &storage::open(&database)?,
            &document_id,
            expected_revision,
            active,
        )?;
        let _ = handle.emit("knowledge-updated", ());
        Ok(result)
    })
    .await
    .map_err(|_| error("Не удалось обновить репертуар"))?
}
#[tauri::command]
pub async fn knowledge_repertoire(
    app: tauri::AppHandle,
    side: String,
) -> Result<KnowledgeDocument, AppError> {
    let activity = command_activity(&app)?;
    let database = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        fragments::combined(&storage::open(&database)?, &side)
    })
    .await
    .map_err(|_| error("Не удалось открыть общий репертуар"))?
}
#[tauri::command]
pub async fn knowledge_repertoire_export(
    app: tauri::AppHandle,
    side: String,
    path: String,
) -> Result<(), AppError> {
    let activity = command_activity(&app)?;
    let database = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        fragments::export_combined(
            &storage::open(&database)?,
            &side,
            std::path::Path::new(&path),
        )
    })
    .await
    .map_err(|_| error("Не удалось сохранить PGN"))?
}

#[tauri::command]
pub async fn knowledge_delete_repertoire(
    app: tauri::AppHandle,
    document_id: String,
    expected_revision: i64,
) -> Result<(), AppError> {
    let activity = command_activity(&app)?;
    let database = database(&app);
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        fragments::delete_repertoire(&storage::open(&database)?, &document_id, expected_revision)?;
        let _ = handle.emit("knowledge-updated", ());
        Ok(())
    })
    .await
    .map_err(|_| error("Не удалось удалить репертуар"))?
}

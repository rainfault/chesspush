//! Async IPC for bounded graph projections. Navigation never mutates primary games.
use crate::{
    domain::AppError,
    game_graph::{self, *},
    statistics::{
        desktop::{StatisticsRuntime, command_activity},
        store,
    },
    storage,
};
use std::{path::PathBuf, sync::Arc};
use tauri::Manager;
fn database(app: &tauri::AppHandle) -> PathBuf {
    app.state::<Arc<StatisticsRuntime>>().database.clone()
}
fn select_account(
    conn: &rusqlite::Connection,
    filter: &mut crate::statistics::types::StatisticsFilter,
) -> Result<(), AppError> {
    if filter.account_id.is_none() {
        filter.account_id = store::active_account(conn)?
            .or(store::accounts(conn)?.into_iter().next())
            .map(|a| a.id);
    }
    Ok(())
}
#[tauri::command]
pub async fn game_graph_query(
    app: tauri::AppHandle,
    mut request: GraphRequest,
) -> Result<GraphProjection, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let conn = storage::open(&path)?;
        select_account(&conn, &mut request.filter)?;
        game_graph::query(&conn, &request)
    })
    .await
    .map_err(|_| AppError::new("graph", "Не удалось построить граф партий"))?
}
#[tauri::command]
pub async fn game_graph_evidence(
    app: tauri::AppHandle,
    mut request: GraphEvidenceRequest,
) -> Result<GraphEvidencePage, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        let conn = storage::open(&path)?;
        select_account(&conn, &mut request.filter)?;
        game_graph::evidence(&conn, &request)
    })
    .await
    .map_err(|_| AppError::new("graph", "Не удалось загрузить посещения"))?
}
#[tauri::command]
pub async fn game_graph_resolve_occurrence(
    app: tauri::AppHandle,
    reference: OccurrenceRef,
) -> Result<OccurrenceContext, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        game_graph::resolve_occurrence(&storage::open(&path)?, &reference)
    })
    .await
    .map_err(|_| AppError::new("graph", "Не удалось открыть ревизию партии"))?
}
#[tauri::command]
pub async fn game_graph_load_state(
    app: tauri::AppHandle,
    account_id: Option<String>,
) -> Result<Option<serde_json::Value>, AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        game_graph::load_state(&storage::open(&path)?, account_id.as_deref())
    })
    .await
    .map_err(|_| AppError::new("graph", "Не удалось загрузить состояние графа"))?
}
#[tauri::command]
pub async fn game_graph_save_state(
    app: tauri::AppHandle,
    account_id: Option<String>,
    state: serde_json::Value,
) -> Result<(), AppError> {
    let activity = command_activity(&app)?;
    let path = database(&app);
    tauri::async_runtime::spawn_blocking(move || {
        let _activity = activity;
        game_graph::save_state(&storage::open(&path)?, account_id.as_deref(), &state)
    })
    .await
    .map_err(|_| AppError::new("graph", "Не удалось сохранить состояние графа"))?
}

//! Read-only views of existing library records; never creates observed-game membership.
use crate::{
    domain::{AppError, Game},
    statistics::types::Replay,
};
use rusqlite::Connection;
use serde::Serialize;
#[derive(Serialize)]
pub struct LibraryEntry {
    pub id: i64,
    pub metadata: Game,
}
#[derive(Serialize)]
pub struct LibraryPage {
    pub total: usize,
    pub entries: Vec<LibraryEntry>,
}
#[derive(Serialize)]
pub struct LibraryDetail {
    pub pgn: String,
    pub replay: Replay,
}
pub fn page(conn: &Connection, file_id: i64, offset: usize) -> Result<LibraryPage, AppError> {
    let total = conn.query_row(
        "SELECT count(*) FROM file_games WHERE file_id=?1",
        [file_id],
        |r| r.get(0),
    )?;
    let mut statement=conn.prepare("SELECT g.id,g.metadata FROM games g JOIN file_games fg ON fg.game_id=g.id WHERE fg.file_id=?1 ORDER BY g.id LIMIT 20 OFFSET ?2")?;
    let rows = statement
        .query_map(
            rusqlite::params![file_id, offset.min(i64::MAX as usize) as i64],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    let entries = rows
        .into_iter()
        .map(|(id, raw)| {
            Ok(LibraryEntry {
                id,
                metadata: serde_json::from_str(&raw)?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    Ok(LibraryPage { total, entries })
}
pub fn detail(conn: &Connection, id: i64) -> Result<LibraryDetail, AppError> {
    let pgn: String = conn.query_row("SELECT pgn FROM games WHERE id=?1", [id], |r| r.get(0))?;
    let replay = crate::statistics::chess::replay_pgn(&pgn).map_err(|e| AppError::new("pgn", e))?;
    Ok(LibraryDetail { pgn, replay })
}

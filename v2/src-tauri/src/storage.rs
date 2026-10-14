use crate::domain::{AppError, Record, SourceKind};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub fn open(path: &Path) -> Result<Connection, AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > 1 {
        return Err(AppError::new(
            "schema",
            "База создана более новой версией ChessPush",
        ));
    }
    if version == 0 {
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))?;
    }
    Ok(conn)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LibraryFile {
    pub id: i64,
    pub path: String,
    pub kind: String,
    pub games: u64,
}
pub fn remember(conn: &Connection, path: &Path, kind: &str) -> Result<i64, AppError> {
    let path = path.to_string_lossy();
    conn.execute("INSERT INTO files(path,kind) VALUES(?1,?2) ON CONFLICT(path) DO UPDATE SET updated_at=unixepoch()", params![path,kind])?;
    Ok(
        conn.query_row("SELECT id FROM files WHERE path=?1", [path.as_ref()], |r| {
            r.get(0)
        })?,
    )
}
pub fn list(conn: &Connection) -> Result<Vec<LibraryFile>, AppError> {
    Ok(conn.prepare("SELECT f.id,f.path,f.kind,count(fg.game_id) FROM files f LEFT JOIN file_games fg ON fg.file_id=f.id GROUP BY f.id ORDER BY f.updated_at DESC,f.id DESC")?
        .query_map([],|r| Ok(LibraryFile {id:r.get(0)?,path:r.get(1)?,kind:r.get(2)?,games:r.get(3)?}))?.collect::<Result<Vec<_>,_>>()?)
}
pub fn save_batch(
    conn: &mut Connection,
    records: &[Record],
    file_ids: &[i64],
    source: &Path,
    kind: SourceKind,
) -> Result<(), AppError> {
    let tx = conn.transaction()?;
    for record in records {
        let g = &record.metadata;
        // Lichess ids are case-sensitive. Non-Lichess Site strings are not ids.
        let identity = if !g.game_id.is_empty() {
            format!("lichess:{}", g.game_id)
        } else {
            format!("pgn:{:x}", Sha256::digest(record.raw_pgn.as_bytes()))
        };
        tx.execute("INSERT INTO games(identity,metadata,tags,pgn,source,source_kind) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(identity) DO NOTHING",
            params![identity,serde_json::to_string(g)?,serde_json::to_string(&record.tags)?,record.raw_pgn,source.to_string_lossy(),serde_json::to_string(&kind)?])?;
        let id: i64 = tx.query_row("SELECT id FROM games WHERE identity=?1", [identity], |r| {
            r.get(0)
        })?;
        for file in file_ids {
            tx.execute(
                "INSERT OR IGNORE INTO file_games(file_id,game_id) VALUES(?1,?2)",
                params![file, id],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}
pub fn setting(conn: &Connection, key: &str) -> Result<Option<String>, AppError> {
    use rusqlite::OptionalExtension;
    Ok(conn
        .query_row("SELECT value FROM settings WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), AppError> {
    conn.execute("INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![key,value])?;
    Ok(())
}

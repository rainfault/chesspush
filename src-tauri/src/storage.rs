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
    if version > 5 {
        return Err(AppError::new(
            "schema",
            "База создана более новой версией ChessPush",
        ));
    }
    // SQLite backup includes committed WAL pages. Never copy the live main file alone.
    if version > 0 && version < 5 {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AppError::new("backup", e))?
            .as_nanos();
        let destination =
            path.with_extension(format!("before-statistics-v{version}-{stamp}.sqlite3"));
        conn.backup("main", &destination, None)?;
    }
    if version == 0 {
        conn.execute_batch(include_str!("../migrations/001_initial.sql"))?;
    }
    if version < 2 {
        conn.execute_batch(include_str!("../migrations/002_legacy_import.sql"))?;
    }
    if version < 3 {
        conn.execute_batch(include_str!("../migrations/003_statistics.sql"))?;
    }
    if version < 4 {
        conn.execute_batch(include_str!("../migrations/004_analysis.sql"))?;
    }
    if version < 5 {
        conn.execute_batch(include_str!(
            "../migrations/005_statistics_quarantine_history.sql"
        ))?;
    }
    if version < 5 {
        let violations: i64 =
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })?;
        if violations != 0 {
            return Err(AppError::new(
                "schema",
                "Нарушена целостность связей базы данных",
            ));
        }
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

#[cfg(test)]
mod statistics_migration_tests {
    use super::*;
    #[test]
    fn v2_upgrade_preserves_primary_legacy_and_wal_with_consistent_backup() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("old.sqlite3");
        let old = Connection::open(&path).unwrap();
        old.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        old.execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        old.execute_batch(include_str!("../migrations/002_legacy_import.sql"))
            .unwrap();
        old.execute("INSERT INTO games(identity,metadata,tags,pgn,source,source_kind) VALUES('pgn:fixture','{}','{}','1. e4 *','fixture','\"my_games\"')",[]).unwrap();
        old.execute("INSERT INTO legacy_rows(source_path,table_name,row_id,payload) VALUES('old','notes','1','{\"note\":\"retained\"}')",[]).unwrap();
        set_setting(&old, "theme", "dark").unwrap();
        // Keep WAL writer connection open: backup must capture its committed rows.
        let upgraded = open(&path).unwrap();
        assert_eq!(
            upgraded
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            5
        );
        assert_eq!(
            upgraded
                .query_row("SELECT count(*) FROM games", [], |r| r.get::<_, usize>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            setting(&upgraded, "theme").unwrap().as_deref(),
            Some("dark")
        );
        assert_eq!(
            upgraded
                .query_row("SELECT count(*) FROM legacy_rows", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            upgraded
                .query_row("SELECT count(*) FROM statistics_account_games", [], |r| {
                    r.get::<_, usize>(0)
                })
                .unwrap(),
            0
        );
        let copies: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("before-statistics")
                    && e.path().extension().is_some_and(|v| v == "sqlite3")
            })
            .collect();
        assert_eq!(copies.len(), 1);
        let backup = Connection::open(copies[0].path()).unwrap();
        assert_eq!(
            backup
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            2
        );
        assert_eq!(
            backup
                .query_row("SELECT count(*) FROM games", [], |r| r.get::<_, usize>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            backup
                .query_row("SELECT payload FROM legacy_rows", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "{\"note\":\"retained\"}"
        );
        drop(open(&path).unwrap());
        assert_eq!(
            std::fs::read_dir(temp.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e
                    .file_name()
                    .to_string_lossy()
                    .contains("before-statistics")
                    && e.path().extension().is_some_and(|v| v == "sqlite3"))
                .count(),
            1
        );
    }
    #[test]
    fn newer_schema_is_rejected_without_changes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("new.sqlite3");
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        drop(conn);
        assert_eq!(open(&path).unwrap_err().code, "schema");
        let conn = Connection::open(&path).unwrap();
        assert_eq!(
            conn.pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            99
        );
    }
}

#[cfg(test)]
mod quarantine_history_migration_tests {
    use super::*;
    #[test]
    fn v4_quarantine_backfill_preserves_raw_and_observation() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("v4.sqlite3");
        let old = Connection::open(&path).unwrap();
        old.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        old.execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        old.execute_batch(include_str!("../migrations/002_legacy_import.sql"))
            .unwrap();
        old.execute_batch(include_str!("../migrations/003_statistics.sql"))
            .unwrap();
        old.execute_batch(include_str!("../migrations/004_analysis.sql"))
            .unwrap();
        old.execute("INSERT INTO statistics_accounts(id,platform,external_id,username,t0_ms) VALUES('lichess:fixture','lichess','fixture','Fixture',1000)",[]).unwrap();
        old.execute("INSERT INTO statistics_quarantine(account_id,external_id,received_at_ms,raw_json,reason) VALUES('lichess:fixture','badpgn01',2000,'{\"id\":\"badpgn01\",\"pgn\":\"legacy invalid source\"}','invalid_pgn')",[]).unwrap();
        let migrated = open(&path).unwrap();
        assert_eq!(
            migrated
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            5
        );
        let old_raw: String = migrated
            .query_row("SELECT raw_json FROM statistics_quarantine", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            migrated
                .query_row(
                    "SELECT raw_json FROM statistics_quarantine_history",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            old_raw
        );
        assert_eq!(
            migrated
                .query_row("SELECT t0_ms FROM statistics_accounts", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            1000
        );
        assert!(
            migrated
                .execute("DELETE FROM statistics_quarantine_history", [])
                .is_err()
        );
        let backups: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("before-statistics-v4")
                    && e.path().extension().is_some_and(|ext| ext == "sqlite3")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        let backup = Connection::open(backups[0].path()).unwrap();
        assert_eq!(
            backup
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            4
        );
        assert_eq!(
            backup
                .query_row("SELECT raw_json FROM statistics_quarantine", [], |r| r
                    .get::<_, String>(
                    0
                ))
                .unwrap(),
            old_raw
        );
    }
}

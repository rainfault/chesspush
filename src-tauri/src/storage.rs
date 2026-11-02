use crate::domain::{AppError, Record, SourceKind};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

pub const SCHEMA_VERSION: i64 = 11;

pub fn open(path: &Path) -> Result<Connection, AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(AppError::new(
            "schema",
            "База создана более новой версией ChessPush",
        ));
    }
    // SQLite backup includes committed WAL pages. Never copy the live main file alone.
    if version > 0 && version < SCHEMA_VERSION {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| AppError::new("backup", e))?
            .as_nanos();
        let prefix = if version < 7 {
            "before-statistics"
        } else if version < 8 {
            "before-repertoire-training"
        } else if version < 9 {
            "before-adaptive-training"
        } else if version < 10 {
            "before-knowledge-library"
        } else {
            "before-game-review"
        };
        let destination = path.with_extension(format!("{prefix}-v{version}-{stamp}.sqlite3"));
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
    if version < 6 {
        conn.execute_batch(include_str!("../migrations/006_knowledge.sql"))?;
    }
    if version < 7 {
        conn.execute_batch(include_str!("../migrations/007_game_graph.sql"))?;
    }
    if version < 8 {
        conn.execute_batch(include_str!("../migrations/008_repertoire_training.sql"))?;
    }
    if version < 9 {
        conn.execute_batch(include_str!(
            "../migrations/009_adaptive_repertoire_training.sql"
        ))?;
    }
    if version < 10 {
        conn.execute_batch(include_str!("../migrations/010_knowledge_library.sql"))?;
        crate::knowledge::seed_tag_history(&conn)?;
    }
    if version < 11 {
        conn.execute_batch(include_str!("../migrations/011_game_review.sql"))?;
    }
    if version < SCHEMA_VERSION {
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
            SCHEMA_VERSION
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
            SCHEMA_VERSION
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

#[cfg(test)]
mod repertoire_training_migration_tests {
    use super::*;
    use crate::knowledge::{ImportRequest, import, load};

    #[test]
    fn v7_upgrade_preserves_repertoire_and_wal_backup() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("repertoire.sqlite3");
        let old = Connection::open(&path).unwrap();
        old.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
            .unwrap();
        for sql in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_legacy_import.sql"),
            include_str!("../migrations/003_statistics.sql"),
            include_str!("../migrations/004_analysis.sql"),
            include_str!("../migrations/005_statistics_quarantine_history.sql"),
            include_str!("../migrations/006_knowledge.sql"),
            include_str!("../migrations/007_game_graph.sql"),
        ] {
            old.execute_batch(sql).unwrap();
        }
        let document = import(
            &old,
            ImportRequest {
                kind: "repertoire".into(),
                title: "Repertoire".into(),
                side: "white".into(),
                pgn: "1. e4 {Centre} e5 (1... c5 2. Nf3) 2. Nf3 *".into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();

        // Keep the old WAL connection open while the backup and migration run.
        let upgraded = open(&path).unwrap();
        assert_eq!(load(&upgraded, &document.id, None).unwrap(), document);
        assert_eq!(
            upgraded
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        let backups: Vec<_> = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("repertoire.before-repertoire-training-v7-")
                    && entry.path().extension().is_some_and(|ext| ext == "sqlite3")
            })
            .collect();
        assert_eq!(backups.len(), 1);
        let backup = Connection::open(backups[0].path()).unwrap();
        assert_eq!(load(&backup, &document.id, None).unwrap(), document);
        assert_eq!(
            backup
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            7
        );
        drop(open(&path).unwrap());
        assert_eq!(
            std::fs::read_dir(temp.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .contains("before-repertoire-training")
                        && entry.path().extension().is_some_and(|ext| ext == "sqlite3")
                })
                .count(),
            1
        );
    }

    #[test]
    fn v8_upgrade_preserves_schedule_reviews_sessions_and_consistent_backup() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("v8.sqlite3");
        let old = Connection::open(&path).unwrap();
        old.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
            .unwrap();
        for sql in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_legacy_import.sql"),
            include_str!("../migrations/003_statistics.sql"),
            include_str!("../migrations/004_analysis.sql"),
            include_str!("../migrations/005_statistics_quarantine_history.sql"),
            include_str!("../migrations/006_knowledge.sql"),
            include_str!("../migrations/007_game_graph.sql"),
            include_str!("../migrations/008_repertoire_training.sql"),
        ] {
            old.execute_batch(sql).unwrap();
        }
        let document = import(
            &old,
            ImportRequest {
                kind: "repertoire".into(),
                title: "Retained".into(),
                side: "white".into(),
                pgn: "1.e4 {Plan} e5 (1...c5 2.Nf3) *".into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        set_setting(&old, "theme", "dark").unwrap();
        old.execute("INSERT INTO repertoire_training_srs VALUES('decision','white','position','e2e4',3,15,2.3,555000,123000)", []).unwrap();
        let legacy_json = r#"{"id":"legacy","scope":{"side":"white","chapter_id":null,"node_id":null},"mode":"srs","fingerprint":"old","items":["decision"],"completed":[],"failed":["decision"],"current":"decision","initial_fen":"start","path":[],"feedback":"retry","attempted_uci":null,"created_at_ms":123}"#;
        old.execute(
            "INSERT INTO repertoire_training_sessions VALUES('legacy','scope','srs',?1,123,124)",
            [legacy_json],
        )
        .unwrap();
        old.execute("INSERT INTO repertoire_training_reviews(session_id,decision_id,answered_at_ms,kind,grade) VALUES('legacy','decision',123,'hint',1)", []).unwrap();
        let upgraded = open(&path).unwrap();
        assert_eq!(load(&upgraded, &document.id, None).unwrap(), document);
        assert_eq!(
            setting(&upgraded, "theme").unwrap().as_deref(),
            Some("dark")
        );
        let schedule: (i64,i64,f64,i64,i64,Option<f64>,Option<f64>,i64) = upgraded.query_row("SELECT repetitions,interval_days,ease,due_at_ms,last_review_at_ms,stability,difficulty,scheduler_version FROM repertoire_training_srs", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?))).unwrap();
        assert_eq!(schedule, (3, 15, 2.3, 555000, 123000, None, None, 0));
        assert_eq!(
            upgraded
                .query_row(
                    "SELECT state_json FROM repertoire_training_sessions",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            legacy_json
        );
        assert_eq!(
            upgraded
                .query_row("SELECT grade FROM repertoire_training_reviews", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );
        assert_eq!(
            upgraded
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert_eq!(
            upgraded
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        let backups = std::fs::read_dir(temp.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .contains("before-adaptive-training-v8")
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "sqlite3")
            })
            .collect::<Vec<_>>();
        assert_eq!(backups.len(), 1);
        let backup = Connection::open(backups[0].path()).unwrap();
        assert_eq!(
            backup
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            8
        );
        assert_eq!(
            backup
                .query_row(
                    "SELECT state_json FROM repertoire_training_sessions",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            legacy_json
        );
        assert_eq!(load(&backup, &document.id, None).unwrap(), document);
        drop(open(&path).unwrap());
        assert_eq!(
            std::fs::read_dir(temp.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| entry
                    .file_name()
                    .to_string_lossy()
                    .contains("before-adaptive-training-v8")
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|extension| extension == "sqlite3"))
                .count(),
            1
        );
    }
}

#[cfg(test)]
mod knowledge_library_migration_tests {
    use super::*;
    use crate::knowledge::{self, CreateRequest, ImportRequest, KnowledgeLink, LinkTarget};

    #[test]
    fn v9_upgrade_backfills_knowledge_without_changing_documents_games_or_training() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("v9.sqlite3");
        let old = Connection::open(&path).unwrap();
        old.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL;")
            .unwrap();
        for sql in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_legacy_import.sql"),
            include_str!("../migrations/003_statistics.sql"),
            include_str!("../migrations/004_analysis.sql"),
            include_str!("../migrations/005_statistics_quarantine_history.sql"),
            include_str!("../migrations/006_knowledge.sql"),
            include_str!("../migrations/007_game_graph.sql"),
            include_str!("../migrations/008_repertoire_training.sql"),
            include_str!("../migrations/009_adaptive_repertoire_training.sql"),
        ] {
            old.execute_batch(sql).unwrap();
        }
        let repertoire = knowledge::import(
            &old,
            ImportRequest {
                title: "Preserved".into(),
                kind: "repertoire".into(),
                side: "white".into(),
                pgn: "1.e4 {Plan} e5 (1...c5) *".into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        let mut study = knowledge::create(
            &old,
            CreateRequest {
                title: "Ending".into(),
                kind: "study".into(),
                side: "both".into(),
                initial_fen: Some("4k3/8/8/8/8/8/8/4K2R b K - 98 71".into()),
                links: vec![],
            },
        )
        .unwrap();
        let key = study.chapters[0].root.position_key.clone();
        study.chapters[0].links.push(KnowledgeLink {
            target: LinkTarget::Position {
                position_key: key.clone(),
            },
            provenance: "legacy_manual_link".into(),
            path_uci: vec![],
            initial_fen: None,
        });
        // Simulate valid metadata/links written before the catalog existed.
        let tag_json = r#"["План в центре"]"#;
        let links_json = serde_json::to_string(&study.chapters[0].links).unwrap();
        for table in ["knowledge_documents", "knowledge_document_versions"] {
            let id_column = if table == "knowledge_documents" {
                "id"
            } else {
                "document_id"
            };
            old.execute(&format!("UPDATE {table} SET data_json=json_set(data_json,'$.header.metadata.tags',json(?1),'$.chapters[0].header.links',json(?2)) WHERE {id_column}=?3"), params![tag_json,links_json,study.id]).unwrap();
        }
        let expected_study = knowledge::load(&old, &study.id, None).unwrap();
        let expected_repertoire = knowledge::load(&old, &repertoire.id, None).unwrap();
        let raw_documents: String = old.query_row("SELECT json_group_array(json_array(id,revision,data_json)) FROM (SELECT * FROM knowledge_documents ORDER BY id)", [], |row|row.get(0)).unwrap();
        let raw_versions: String = old.query_row("SELECT json_group_array(json_array(document_id,revision,data_json)) FROM (SELECT * FROM knowledge_document_versions ORDER BY document_id,revision)", [], |row|row.get(0)).unwrap();
        old.execute("INSERT INTO games(identity,metadata,tags,pgn,source,source_kind) VALUES('fixture','{}','{}','1.d4 d5 *','fixture','\"my_games\"')",[]).unwrap();
        old.execute("INSERT INTO repertoire_training_srs VALUES('decision','white','position','e2e4',7,40,2.4,555000,123000,12.5,4.2,1)",[]).unwrap();
        let state = r#"{"id":"retained","mode":"srs","completed":["decision"]}"#;
        old.execute(
            "INSERT INTO repertoire_training_sessions VALUES('retained','scope','srs',?1,123,124)",
            [state],
        )
        .unwrap();
        old.execute("INSERT INTO repertoire_training_reviews(session_id,decision_id,answered_at_ms,kind,uci,grade) VALUES('retained','decision',123,'answer','e2e4',3)",[]).unwrap();
        set_setting(&old, "theme", "dark").unwrap();
        let snapshot = |connection: &Connection| -> String {
            connection.query_row("SELECT json_array((SELECT json_group_array(json_array(repetitions,interval_days,ease,due_at_ms,last_review_at_ms,stability,difficulty,scheduler_version)) FROM repertoire_training_srs),(SELECT json_group_array(json_array(id,scope_key,mode,state_json,created_at_ms,updated_at_ms)) FROM repertoire_training_sessions),(SELECT json_group_array(json_array(id,session_id,decision_id,answered_at_ms,kind,uci,grade)) FROM repertoire_training_reviews),(SELECT json_group_array(json_array(identity,metadata,tags,pgn,source,source_kind)) FROM games),(SELECT value FROM settings WHERE key='theme'))", [], |row|row.get(0)).unwrap()
        };
        let before = snapshot(&old);
        // Keep the v9 connection and committed WAL open during backup/migration.
        let upgraded = open(&path).unwrap();
        assert_eq!(
            upgraded
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert_eq!(snapshot(&upgraded), before);
        assert_eq!(
            knowledge::load(&upgraded, &study.id, None).unwrap(),
            expected_study
        );
        assert_eq!(
            knowledge::load(&upgraded, &repertoire.id, None).unwrap(),
            expected_repertoire
        );
        assert_eq!(upgraded.query_row("SELECT json_group_array(json_array(id,revision,data_json)) FROM (SELECT * FROM knowledge_documents ORDER BY id)",[],|row|row.get::<_,String>(0)).unwrap(),raw_documents);
        assert_eq!(upgraded.query_row("SELECT json_group_array(json_array(document_id,revision,data_json)) FROM (SELECT * FROM knowledge_document_versions ORDER BY document_id,revision)",[],|row|row.get::<_,String>(0)).unwrap(),raw_versions);
        assert_eq!(
            knowledge::tag_suggestions(&upgraded, "ПЛАН").unwrap(),
            vec!["План в центре"]
        );
        let projected = knowledge::graph_overlay::position(&upgraded, &key)
            .unwrap()
            .unwrap();
        assert_eq!(projected.fen, study.chapters[0].root.fen);
        assert_eq!(projected.references.len(), 1);
        assert_eq!(
            projected.references[0].chapter_title,
            study.chapters[0].title
        );
        assert_eq!(
            upgraded
                .query_row(
                    "SELECT provenance FROM knowledge_position_links WHERE document_id=?1",
                    [&study.id],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            "legacy_manual_link"
        );
        assert_eq!(
            upgraded
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
        let backups = || {
            std::fs::read_dir(directory.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .contains("before-knowledge-library-v9")
                        && entry
                            .path()
                            .extension()
                            .is_some_and(|extension| extension == "sqlite3")
                })
                .map(|entry| entry.path())
                .collect::<Vec<_>>()
        };
        let copies = backups();
        assert_eq!(copies.len(), 1);
        let backup = Connection::open(&copies[0]).unwrap();
        assert_eq!(
            backup
                .pragma_query_value::<i64, _>(None, "user_version", |row| row.get(0))
                .unwrap(),
            9
        );
        assert_eq!(snapshot(&backup), before);
        assert_eq!(
            knowledge::load(&backup, &study.id, None).unwrap(),
            expected_study
        );
        drop(open(&path).unwrap());
        assert_eq!(backups().len(), 1);
    }
}

#[cfg(test)]
mod game_review_migration_tests {
    use super::*;
    #[test]
    fn v10_wal_upgrade_preserves_document_json_links_tags_and_fsrs_then_supports_analysis() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v10.sqlite3");
        let old = Connection::open(&path).unwrap();
        old.execute_batch("PRAGMA journal_mode=WAL;PRAGMA foreign_keys=ON;")
            .unwrap();
        for sql in [
            include_str!("../migrations/001_initial.sql"),
            include_str!("../migrations/002_legacy_import.sql"),
            include_str!("../migrations/003_statistics.sql"),
            include_str!("../migrations/004_analysis.sql"),
            include_str!("../migrations/005_statistics_quarantine_history.sql"),
            include_str!("../migrations/006_knowledge.sql"),
            include_str!("../migrations/007_game_graph.sql"),
            include_str!("../migrations/008_repertoire_training.sql"),
            include_str!("../migrations/009_adaptive_repertoire_training.sql"),
            include_str!("../migrations/010_knowledge_library.sql"),
        ] {
            old.execute_batch(sql).unwrap();
        }
        let document = crate::knowledge::import(
            &old,
            crate::knowledge::ImportRequest {
                title: "Preserved before reviews".into(),
                kind: "study".into(),
                side: "white".into(),
                pgn: "1.e4 {Plan} e5 (1...c5 $5 2.Nf3) *".into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        let document = crate::knowledge::apply(
            &old,
            &document.id,
            document.revision,
            crate::knowledge::EditOperation::SetTags {
                tags: vec!["Структура".into()],
            },
        )
        .unwrap();
        old.execute("INSERT INTO repertoire_training_srs(decision_id,side,position_key,solution_key,repetitions,interval_days,ease,due_at_ms,last_review_at_ms,stability,difficulty,scheduler_version) VALUES('decision','white','position','e2e4',3,15,2.3,555000,123000,12.4,4.8,6)",[]).unwrap();
        set_setting(&old, "theme", "dark").unwrap();
        let snapshot = |conn: &Connection| -> String {
            conn.query_row("SELECT json_array((SELECT json_group_array(json_array(id,kind,title,side,revision,created_at_ms,updated_at_ms,data_json)) FROM knowledge_documents),(SELECT json_group_array(json_array(document_id,revision,data_json,created_at_ms)) FROM knowledge_document_versions),(SELECT json_group_array(json_array(document_id,chapter_id,node_id,document_revision,position_key,parent_position_key,uci,fen,chapter_title)) FROM knowledge_node_index),(SELECT json_group_array(json_array(tag_key,tag,last_used_at_ms)) FROM knowledge_tag_history),(SELECT json_group_array(json_array(decision_id,due_at_ms,stability,difficulty,scheduler_version)) FROM repertoire_training_srs),(SELECT value FROM settings WHERE key='theme'))",[],|r|r.get(0)).unwrap()
        };
        let before = snapshot(&old);
        let upgraded = open(&path).unwrap();
        assert_eq!(snapshot(&upgraded), before);
        assert_eq!(
            crate::knowledge::load(&upgraded, &document.id, None).unwrap(),
            document
        );
        assert_eq!(
            upgraded
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert_eq!(
            upgraded
                .pragma_query_value::<i64, _>(None, "foreign_keys", |r| r.get(0))
                .unwrap(),
            1
        );
        assert_eq!(
            upgraded
                .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            0
        );
        let analysis = crate::knowledge::create(
            &upgraded,
            crate::knowledge::CreateRequest {
                title: "Analysis".into(),
                kind: "analysis".into(),
                side: "white".into(),
                initial_fen: None,
                links: vec![],
            },
        )
        .unwrap();
        assert_eq!(analysis.kind, "analysis");
        let copies = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .contains("before-game-review-v10")
                    && e.path().extension().is_some_and(|extension| extension == "sqlite3")
            })
            .collect::<Vec<_>>();
        assert_eq!(copies.len(), 1);
        let backup = Connection::open(copies[0].path()).unwrap();
        assert_eq!(snapshot(&backup), before);
        assert_eq!(
            backup
                .pragma_query_value::<i64, _>(None, "user_version", |r| r.get(0))
                .unwrap(),
            10
        );
        drop(open(&path).unwrap());
        assert_eq!(
            std::fs::read_dir(dir.path())
                .unwrap()
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().contains("before-game-review-v10")
                    && e.path().extension().is_some_and(|extension| extension == "sqlite3"))
                .count(),
            1
        );
    }
}

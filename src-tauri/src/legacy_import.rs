//! Explicit one-time import of the retired application's local data.
//! The source is opened read-only. Both existing databases are backed up using
//! SQLite's backup API before the target schema or user data is changed.
use crate::{
    domain::{AppError, Record, SourceKind, Tags},
    pgn, storage,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportReport {
    pub source_games: usize,
    pub unique_games: usize,
    pub games_added: usize,
    pub duplicates_merged: usize,
    pub saved_positions: usize,
    pub files: usize,
    pub file_links_added: usize,
    pub settings_added: usize,
    pub archived_rows: usize,
    pub already_imported: bool,
}

fn read_only(path: &Path) -> Result<Connection, AppError> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

fn backup(conn: &Connection, destination: &Path) -> Result<(), AppError> {
    if destination.exists() {
        return Err(AppError::new("backup", "Резервная копия уже существует"));
    }
    conn.backup("main", destination, None)?;
    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> Result<bool, AppError> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?)
}

fn rows(conn: &Connection, table: &str) -> Result<Vec<Value>, AppError> {
    // Every caller supplies one of the fixed legacy table names below.
    let mut statement = conn.prepare(&format!("SELECT * FROM {table}"))?;
    let names: Vec<_> = statement
        .column_names()
        .iter()
        .map(|s| s.to_string())
        .collect();
    let records = statement.query_map([], |row| {
        let mut object = Map::new();
        for (i, name) in names.iter().enumerate() {
            use rusqlite::types::ValueRef;
            let value = match row.get_ref(i)? {
                ValueRef::Null => Value::Null,
                ValueRef::Integer(v) => Value::from(v),
                ValueRef::Real(v) => Value::from(v),
                ValueRef::Text(v) => Value::String(
                    std::str::from_utf8(v)
                        .map_err(|e| {
                            rusqlite::Error::FromSqlConversionFailure(
                                i,
                                rusqlite::types::Type::Text,
                                Box::new(e),
                            )
                        })?
                        .to_owned(),
                ),
                ValueRef::Blob(v) => Value::Array(v.iter().map(|b| Value::from(*b)).collect()),
            };
            object.insert(name.clone(), value);
        }
        Ok(Value::Object(object))
    })?;
    Ok(records.collect::<Result<Vec<_>, _>>()?)
}

fn text(row: &Value, key: &str) -> String {
    row.get(key)
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned()
}
fn number(row: &Value, key: &str) -> i64 {
    row.get(key).and_then(Value::as_i64).unwrap_or(0)
}
fn row_id(row: &Value, table: &str) -> String {
    match table {
        "app_settings" => "settings".into(),
        "library_file_games" => format!("{}:{}", number(row, "file_id"), number(row, "game_id")),
        _ => row
            .get("id")
            .map(Value::to_string)
            .unwrap_or_else(|| text(row, "key")),
    }
}

fn record(row: &Value) -> Record {
    let mut raw = text(row, "raw_pgn");
    let mut tags: Tags = serde_json::from_str(&text(row, "tags_json")).unwrap_or_default();
    if tags.is_empty() {
        tags = pgn::tags(&raw);
    }
    if raw.trim().is_empty() {
        for (column, tag) in [
            ("event", "Event"),
            ("site", "Site"),
            ("date", "Date"),
            ("white", "White"),
            ("black", "Black"),
            ("result", "Result"),
            ("eco", "ECO"),
            ("opening", "Opening"),
            ("time_control", "TimeControl"),
        ] {
            let value = text(row, column);
            if !value.is_empty() {
                tags.entry(tag.into()).or_insert(value);
            }
        }
        raw = tags
            .iter()
            .map(|(k, v)| {
                format!(
                    "[{k} \"{}\"]\n",
                    v.replace('\\', "\\\\").replace('"', "\\\"")
                )
            })
            .collect::<String>();
        raw.push_str(&format!("\n{}\n", text(row, "moves")));
    }
    let mut record = pgn::record(&raw, tags);
    // Database columns can contain richer metadata than the stored PGN headers.
    let g = &mut record.metadata;
    for (key, field) in [
        ("game_id", &mut g.game_id),
        ("event", &mut g.event),
        ("site", &mut g.site),
        ("date", &mut g.date),
        ("white", &mut g.white),
        ("black", &mut g.black),
        ("result", &mut g.result),
        ("eco", &mut g.eco),
        ("opening", &mut g.opening),
        ("time_control", &mut g.time_control),
        ("termination", &mut g.termination),
        ("moves", &mut g.moves),
        ("perf_type", &mut g.perf_type),
    ] {
        if let Some(value) = row.get(key).and_then(Value::as_str) {
            *field = value.into();
        }
    }
    if row.get("white_elo").is_some() {
        g.white_elo = number(row, "white_elo");
    }
    if row.get("black_elo").is_some() {
        g.black_elo = number(row, "black_elo");
    }
    g.move_count = pgn::record(&format!("\n\n{}", g.moves), Tags::new())
        .metadata
        .move_count;
    // Preserve the exact stored PGN, including comments and variations.
    record.raw_pgn = raw;
    record
}
fn source_kind(row: &Value) -> SourceKind {
    match text(row, "source_kind").as_str() {
        "lichess" | "lichess_user" | "statistics_pgn" => SourceKind::MyGames,
        "zst_model" | "lichess_corpus" => SourceKind::LichessCorpus,
        "master" => SourceKind::Master,
        _ => SourceKind::Local,
    }
}
fn identity(record: &Record) -> String {
    if !record.metadata.game_id.is_empty() {
        format!("lichess:{}", record.metadata.game_id)
    } else {
        format!("pgn:{:x}", Sha256::digest(record.raw_pgn.as_bytes()))
    }
}
fn archive(
    conn: &Connection,
    source: &str,
    table: &str,
    row: &Value,
    game: Option<i64>,
) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO legacy_rows(source_path,table_name,row_id,payload,game_id)
         VALUES(?1,?2,?3,?4,?5)",
        params![
            source,
            table,
            row_id(row, table),
            serde_json::to_string(row)?,
            game
        ],
    )?;
    Ok(())
}

/// Run only from an explicit migration command, never automatically on startup.
/// All legacy rows are retained losslessly; the game_id FK maps deduplicated
/// games and saved positions to the current game table. Existing target games
/// and settings win conflicts, with original legacy values retained in the archive.
pub fn migrate(
    source_path: &Path,
    settings_path: &Path,
    target_path: &Path,
    backup_directory: &Path,
) -> Result<ImportReport, AppError> {
    let source_path = source_path.canonicalize()?;
    if target_path.exists() && source_path == target_path.canonicalize()? {
        return Err(AppError::new(
            "validation",
            "Исходная и целевая базы должны различаться",
        ));
    }
    let source_key = source_path.to_string_lossy().into_owned();
    let source = read_only(&source_path)?;
    if !table_exists(&source, "position_lab_games")? && !table_exists(&source, "games")? {
        return Err(AppError::new("schema", "Не найдены партии прежней версии"));
    }
    // A repeated invocation performs no backup, schema change, or import writes.
    if target_path.exists() {
        let current = read_only(target_path)?;
        if table_exists(&current, "legacy_imports")?
            && let Some(report) = current
                .query_row(
                    "SELECT report FROM legacy_imports WHERE source_path=?1",
                    [&source_key],
                    |r| r.get::<_, String>(0),
                )
                .optional()?
        {
            let mut report: ImportReport = serde_json::from_str(&report)?;
            report.already_imported = true;
            report.games_added = 0;
            report.file_links_added = 0;
            report.settings_added = 0;
            return Ok(report);
        }
    }
    let settings: Value = serde_json::from_slice(&std::fs::read(settings_path)?)?;
    if !settings.is_object() {
        return Err(AppError::new(
            "data",
            "Некорректный файл настроек прежней версии",
        ));
    }
    std::fs::create_dir_all(backup_directory)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| AppError::new("backup", e))?
        .as_nanos();
    let source_backup = backup_directory.join(format!("legacy_{timestamp}.sqlite3"));
    backup(&source, &source_backup)?;
    if target_path.exists() {
        backup(
            &read_only(target_path)?,
            &backup_directory.join(format!("tauri_{timestamp}.sqlite3")),
        )?;
    }
    // Keep a consistent source read snapshot throughout the transfer.
    source.execute_batch("BEGIN;")?;
    let canonical_table = if table_exists(&source, "position_lab_games")? {
        "position_lab_games"
    } else {
        "games"
    };
    let games = rows(&source, canonical_table)?;
    let mut target = storage::open(target_path)?;
    let tx = target.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let mut report = ImportReport::default();
    let mut game_map = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for row in &games {
        let record = record(row);
        let identity = identity(&record);
        report.games_added += tx.execute(
            "INSERT INTO games(identity,metadata,tags,pgn,source,source_kind)
             VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(identity) DO NOTHING",
            params![
                identity,
                serde_json::to_string(&record.metadata)?,
                serde_json::to_string(&record.tags)?,
                record.raw_pgn,
                text(row, "source"),
                serde_json::to_string(&source_kind(row))?
            ],
        )?;
        let id: i64 = tx.query_row("SELECT id FROM games WHERE identity=?1", [&identity], |r| {
            r.get(0)
        })?;
        game_map.insert(number(row, "id"), id);
        identities.insert(identity);
        archive(&tx, &source_key, canonical_table, row, Some(id))?;
        report.archived_rows += 1;
    }
    report.source_games = games.len();
    report.unique_games = identities.len();
    report.duplicates_merged = games.len() - identities.len();
    let mut file_map = BTreeMap::new();
    if table_exists(&source, "library_files")? {
        for row in rows(&source, "library_files")? {
            let path = text(&row, "path");
            tx.execute(
                "INSERT INTO files(path,kind,updated_at)
                 VALUES(?1,?2,COALESCE(unixepoch(?3),unixepoch())) ON CONFLICT(path) DO NOTHING",
                params![path, text(&row, "kind"), text(&row, "updated_at")],
            )?;
            let id = tx.query_row("SELECT id FROM files WHERE path=?1", [path], |r| {
                r.get::<_, i64>(0)
            })?;
            file_map.insert(number(&row, "id"), id);
            archive(&tx, &source_key, "library_files", &row, None)?;
            report.files += 1;
            report.archived_rows += 1;
        }
    }
    if table_exists(&source, "library_file_games")? {
        for row in rows(&source, "library_file_games")? {
            let file = file_map
                .get(&number(&row, "file_id"))
                .ok_or_else(|| AppError::new("data", "Нарушена связь прежнего файла"))?;
            let game = game_map
                .get(&number(&row, "game_id"))
                .ok_or_else(|| AppError::new("data", "Нарушена связь прежней партии"))?;
            report.file_links_added += tx.execute(
                "INSERT OR IGNORE INTO file_games(file_id,game_id) VALUES(?1,?2)",
                params![file, game],
            )?;
            archive(&tx, &source_key, "library_file_games", &row, Some(*game))?;
            report.archived_rows += 1;
        }
    }
    for table in [
        "games",
        "saved_positions",
        "position_reviews",
        "training_cards",
        "storage_meta",
    ] {
        if table == canonical_table || !table_exists(&source, table)? {
            continue;
        }
        for row in rows(&source, table)? {
            let game = if table == "saved_positions" {
                report.saved_positions += 1;
                if let Some(old_id) = row.get("game_ref").and_then(Value::as_i64) {
                    Some(*game_map.get(&old_id).ok_or_else(|| {
                        AppError::new("data", "Нарушена связь сохранённой позиции")
                    })?)
                } else {
                    None
                }
            } else if table == "games" {
                let key = identity(&record(&row));
                tx.query_row("SELECT id FROM games WHERE identity=?1", [key], |r| {
                    r.get(0)
                })
                .optional()?
            } else {
                None
            };
            archive(&tx, &source_key, table, &row, game)?;
            report.archived_rows += 1;
        }
    }
    for key in ["lichess_username", "lichess_api_token", "theme"] {
        if let Some(value) = settings.get(key).and_then(Value::as_str) {
            let value = if key == "theme" && value != "dark" {
                "light"
            } else {
                value
            };
            report.settings_added += tx.execute(
                "INSERT INTO settings(key,value) VALUES(?1,?2) ON CONFLICT(key) DO NOTHING",
                params![key, value],
            )?;
        }
    }
    archive(&tx, &source_key, "app_settings", &settings, None)?;
    report.archived_rows += 1;
    if tx.prepare("PRAGMA foreign_key_check")?.exists([])? {
        return Err(AppError::new(
            "data",
            "Нарушена целостность перенесённых связей",
        ));
    }
    tx.execute(
        "INSERT INTO legacy_imports(source_path,report) VALUES(?1,?2)",
        params![source_key, serde_json::to_string(&report)?],
    )?;
    tx.commit()?;
    Ok(report)
}

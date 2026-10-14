use chesspush_lib::{legacy_import, storage};
use rusqlite::{Connection, OpenFlags, params};
use serde_json::{Value, json};
use std::{fs, path::Path};
use tempfile::TempDir;

fn legacy_database(path: &Path, dangling_position: bool) {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        "CREATE TABLE position_lab_games (
            id INTEGER PRIMARY KEY, fingerprint TEXT, game_id TEXT, site TEXT,
            white TEXT, black TEXT, event TEXT, source TEXT, source_kind TEXT,
            raw_pgn TEXT, tags_json TEXT
         );
         CREATE TABLE games(id INTEGER PRIMARY KEY, game_id TEXT, site TEXT,
            raw_pgn TEXT, note TEXT, user_tag TEXT);
         CREATE TABLE saved_positions(id INTEGER PRIMARY KEY, game_ref INTEGER,
            fen TEXT, ply INTEGER, comment TEXT, tags_json TEXT, arrows_json TEXT,
            highlights_json TEXT, review_count INTEGER);
         CREATE TABLE library_files(id INTEGER PRIMARY KEY, path TEXT,
            original_path TEXT, kind TEXT, updated_at TEXT);
         CREATE TABLE library_file_games(file_id INTEGER, game_id INTEGER);
         CREATE TABLE storage_meta(key TEXT PRIMARY KEY, value TEXT);
         INSERT INTO storage_meta VALUES('legacy_games_migrated_v1','done');",
    )
    .unwrap();
    for (id, game_id, kind) in [
        (1, "AbCd1234", "legacy"),
        (2, "AbCd1234", "legacy"),
        (3, "abcd1234", "lichess"),
    ] {
        let site = format!("https://lichess.org/{game_id}");
        let pgn = format!(
            "[Event \"Blitz\"]\n[Site \"{site}\"]\n[White \"A\"]\n[Black \"B\"]\n\n1. e4 e5 *\n"
        );
        conn.execute(
            "INSERT INTO position_lab_games VALUES(?1,?2,?3,?4,'A','B','Stored richer event','local',?5,?6,?7)",
            params![id, format!("old-fingerprint-{id}"), game_id, site, kind, pgn,
                json!({"Event":"Blitz", "Site":site, "Annotator":"Personal annotation"}).to_string()],
        ).unwrap();
        if id == 1 {
            conn.execute(
                "INSERT INTO games VALUES(10,?1,?2,?3,'Personal note','tag')",
                params![game_id, site, pgn],
            )
            .unwrap();
        }
    }
    conn.execute(
        "INSERT INTO saved_positions VALUES(7,?1,'8/8/8/8/8/8/4K3/7k w - - 0 1',2,
        'Preserve this comment','[\"strategy\"]','[{\"orig\":\"e2\",\"dest\":\"e4\"}]','[]',4)",
        [if dangling_position { 999 } else { 2 }],
    )
    .unwrap();
    conn.execute_batch(
        "INSERT INTO library_files VALUES(5,'C:/personal/import.pgn','D:/original.pgn','import','2026-10-01 12:00:00');
         INSERT INTO library_file_games VALUES(5,1),(5,2),(5,3);"
    ).unwrap();
}

fn settings(path: &Path) {
    fs::write(
        path,
        json!({
            "lichess_username": "fixture-user",
            "lichess_api_token": "fixture-token-not-real",
            "theme": "dark",
            "zst_database_path": "D:/corpus.zst"
        })
        .to_string(),
    )
    .unwrap();
}

#[test]
fn transfers_duplicates_positions_provenance_and_settings_losslessly() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("legacy.sqlite3");
    let target = dir.path().join("new.sqlite3");
    let config = dir.path().join("settings.json");
    let backups = dir.path().join("backups");
    legacy_database(&source, false);
    settings(&config);
    let source_before = fs::read(&source).unwrap();
    // Make an actual v1 target: its backup must precede the v2 schema upgrade.
    let target_conn = Connection::open(&target).unwrap();
    target_conn
        .execute_batch(include_str!("../migrations/001_initial.sql"))
        .unwrap();
    target_conn
        .execute("INSERT INTO settings VALUES('theme','light')", [])
        .unwrap();
    drop(target_conn);

    let report = legacy_import::migrate(&source, &config, &target, &backups).unwrap();
    assert_eq!(report.source_games, 3);
    assert_eq!(report.unique_games, 2);
    assert_eq!(report.duplicates_merged, 1);
    assert_eq!(report.games_added, 2);
    assert_eq!(report.saved_positions, 1);
    assert_eq!(report.files, 1);
    assert_eq!(report.file_links_added, 2);
    assert_eq!(report.settings_added, 2);
    assert!(!report.already_imported);
    assert_eq!(fs::read(&source).unwrap(), source_before);

    let conn = storage::open(&target).unwrap();
    assert_eq!(
        storage::setting(&conn, "theme").unwrap().as_deref(),
        Some("light")
    );
    assert_eq!(
        storage::setting(&conn, "lichess_username")
            .unwrap()
            .as_deref(),
        Some("fixture-user")
    );
    assert_eq!(
        storage::setting(&conn, "lichess_api_token")
            .unwrap()
            .as_deref(),
        Some("fixture-token-not-real")
    );
    assert_eq!(
        conn.query_row("SELECT count(*) FROM games", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let new_game: i64 = conn
        .query_row(
            "SELECT id FROM games WHERE identity='lichess:AbCd1234'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let (position_game, payload): (i64, String) = conn.query_row(
        "SELECT game_id,payload FROM legacy_rows WHERE table_name='saved_positions' AND row_id='7'",
        [], |r| Ok((r.get(0)?,r.get(1)?)),
    ).unwrap();
    assert_eq!(position_game, new_game);
    let position: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(position["comment"], "Preserve this comment");
    assert_eq!(position["game_ref"], 2);
    assert_eq!(position["review_count"], 4);
    assert_eq!(
        serde_json::from_str::<Value>(position["arrows_json"].as_str().unwrap()).unwrap()[0]["dest"],
        "e4"
    );
    let metadata: String = conn
        .query_row("SELECT metadata FROM games WHERE id=?1", [new_game], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&metadata).unwrap()["event"],
        "Stored richer event"
    );
    let old_file: String = conn
        .query_row(
            "SELECT payload FROM legacy_rows WHERE table_name='library_files'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&old_file).unwrap()["original_path"],
        "D:/original.pgn"
    );
    let old_game: String = conn
        .query_row(
            "SELECT payload FROM legacy_rows WHERE table_name='games'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&old_game).unwrap()["note"],
        "Personal note"
    );
    assert!(
        !conn
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .exists([])
            .unwrap()
    );
    drop(conn);

    let paths: Vec<_> = fs::read_dir(&backups)
        .unwrap()
        .map(|r| r.unwrap().path())
        .collect();
    assert_eq!(paths.len(), 2);
    for path in paths {
        let backup = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
        assert_eq!(
            backup
                .query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
        if path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("tauri_")
        {
            assert_eq!(
                backup
                    .query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                1
            );
            assert_eq!(
                backup
                    .query_row("SELECT count(*) FROM games", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                0
            );
        }
    }
    let again = legacy_import::migrate(&source, &config, &target, &backups).unwrap();
    assert!(again.already_imported);
    assert_eq!(again.games_added, 0);
    assert_eq!(again.settings_added, 0);
    assert_eq!(fs::read_dir(&backups).unwrap().count(), 2);
}

#[test]
fn invalid_link_rolls_back_every_imported_row_and_keeps_backups() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("legacy.sqlite3");
    let target = dir.path().join("new.sqlite3");
    let config = dir.path().join("settings.json");
    let backups = dir.path().join("backups");
    legacy_database(&source, true);
    settings(&config);
    let original = fs::read(&source).unwrap();
    let error = legacy_import::migrate(&source, &config, &target, &backups).unwrap_err();
    assert_eq!(error.code, "data");
    assert_eq!(fs::read(&source).unwrap(), original);
    let conn = storage::open(&target).unwrap();
    for table in [
        "games",
        "files",
        "file_games",
        "settings",
        "legacy_rows",
        "legacy_imports",
    ] {
        assert_eq!(
            conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(fs::read_dir(&backups).unwrap().count(), 1);
}

#[test]
fn rejects_same_source_and_target_before_changes() {
    let dir = TempDir::new().unwrap();
    let source = dir.path().join("legacy.sqlite3");
    legacy_database(&source, false);
    let original = fs::read(&source).unwrap();
    let error = legacy_import::migrate(
        &source,
        &dir.path().join("missing.json"),
        &source,
        &dir.path().join("backups"),
    )
    .unwrap_err();
    assert_eq!(error.code, "validation");
    assert_eq!(fs::read(&source).unwrap(), original);
    assert!(!dir.path().join("backups").exists());
}

use chesspush_lib::{
    application::{self, ParseRequest},
    domain::{Filters, Record},
    parser::{Progress, ScanOptions, scan},
    storage,
};
use serde::Deserialize;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures")
        .join(name)
}
#[derive(Deserialize)]
struct Case {
    name: String,
    filters: Filters,
    start_index: u64,
    scan_limit: Option<u64>,
    limit: u64,
    cancel_at: Option<u64>,
    scanned: u64,
    matches: Vec<serde_json::Value>,
}
#[test]
fn python_reference_parity() {
    let data: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(fixture("reference.json")).unwrap()).unwrap();
    let cases: Vec<Case> = serde_json::from_value(data["cases"].clone()).unwrap();
    for case in cases {
        for filename in ["corpus.pgn.zst", "corpus.pgn"] {
            let cancel = AtomicBool::new(false);
            let mut p = Progress::default();
            let mut found = Vec::new();
            scan(&fixture(filename),ScanOptions{filters:Some(&case.filters),start_index:case.start_index,scan_limit:case.scan_limit,match_limit:case.limit},&cancel,&mut p,
                |p| {if case.cancel_at.is_some_and(|n|p.scanned>=n){cancel.store(true,Ordering::Relaxed);}},
                |i,r| {found.push(serde_json::json!({"index":i,"metadata":r.metadata,"tags":r.tags,"raw_pgn":r.raw_pgn}));Ok(())}).unwrap();
            assert_eq!(p.scanned, case.scanned, "{} {filename}", case.name);
            assert_eq!(found, case.matches, "{} {filename}", case.name);
        }
    }
}
fn read(name: &str) -> Result<(Progress, Vec<Record>), chesspush_lib::domain::AppError> {
    let mut p = Progress::default();
    let mut records = Vec::new();
    scan(
        &fixture(name),
        ScanOptions {
            filters: Some(&Filters {
                exclude_bullet: false,
                include_timeout: true,
                ..Filters::default()
            }),
            start_index: 0,
            scan_limit: None,
            match_limit: 100,
        },
        &AtomicBool::new(false),
        &mut p,
        |_| {},
        |_, r| {
            records.push(r.clone());
            Ok(())
        },
    )?;
    Ok((p, records))
}
#[test]
fn corrected_crlf_boundaries() {
    assert_eq!(read("crlf.pgn.zst").unwrap().0.scanned, 12);
}
#[test]
fn truncated_zstd_is_reported() {
    assert_eq!(read("truncated.pgn.zst").unwrap_err().code, "decode");
}
#[test]
fn invalid_utf8_is_reported() {
    assert_eq!(read("invalid-utf8.pgn.zst").unwrap_err().code, "utf8");
}
#[test]
fn malformed_eco_range_does_not_panic() {
    assert!(!chesspush_lib::filters::eco_matches("B12", "B12-"));
}
#[test]
fn persistence_and_partial_cancel() {
    let temp = tempfile::tempdir().unwrap();
    let stop = AtomicBool::new(false);
    let r = application::run(
        ParseRequest {
            source: fixture("corpus.pgn.zst"),
            filters: Filters::default(),
            start_index: 0,
            limit: 2,
            import_all: false,
        },
        temp.path(),
        &stop,
        |_| {},
    )
    .unwrap();
    assert_eq!(r.progress.matched, 2);
    assert!(r.error.is_none());
    let conn = storage::open(&temp.path().join("chesspush.sqlite3")).unwrap();
    assert_eq!(storage::list(&conn).unwrap().len(), 2);
    let count: i64 = conn
        .query_row("SELECT count(*) FROM games", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 2);
    let text = std::fs::read_to_string(r.output.unwrap()).unwrap();
    assert!(text.contains("AbCd0000"));
    stop.store(true, Ordering::Relaxed);
    let cancelled = application::run(
        ParseRequest {
            source: fixture("corpus.pgn.zst"),
            filters: Filters::default(),
            start_index: 0,
            limit: 50,
            import_all: false,
        },
        temp.path(),
        &stop,
        |_| {},
    )
    .unwrap();
    assert!(cancelled.progress.cancelled);
    assert_eq!(cancelled.progress.scanned, 0);
}

#[test]
fn export_failure_still_saves_games() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("exports"), "obstruct directory").unwrap();
    let r = application::run(
        ParseRequest {
            source: fixture("corpus.pgn.zst"),
            filters: Filters::default(),
            start_index: 0,
            limit: 2,
            import_all: false,
        },
        temp.path(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(r.progress.matched, 2);
    assert!(r.warning.is_some());
    assert!(r.output.is_none());
    assert!(r.error.is_none());
    let conn = storage::open(&temp.path().join("chesspush.sqlite3")).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM games", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
}

#[test]
fn cancelled_application_flushes_partial_batch() {
    use std::io::Write;
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("large.pgn.zst");
    let mut enc =
        zstd::stream::write::Encoder::new(std::fs::File::create(&input).unwrap(), 1).unwrap();
    let block = std::fs::read(fixture("corpus.pgn")).unwrap();
    for _ in 0..5000 {
        enc.write_all(&block).unwrap();
    }
    enc.finish().unwrap();
    let stop = AtomicBool::new(false);
    let r = application::run(
        ParseRequest {
            source: input,
            filters: Filters::default(),
            start_index: 0,
            limit: 100000,
            import_all: false,
        },
        temp.path(),
        &stop,
        |p| {
            if p.scanned > 0 {
                stop.store(true, Ordering::Relaxed);
            }
        },
    )
    .unwrap();
    assert!(r.progress.cancelled);
    assert!(r.progress.matched > 0);
    assert!(r.progress.scanned < 60000);
    assert!(r.error.is_none());
    let text = std::fs::read_to_string(r.output.unwrap()).unwrap();
    assert_eq!(text.matches("[Event ").count() as u64, r.progress.matched);
    let conn = storage::open(&temp.path().join("chesspush.sqlite3")).unwrap();
    assert!(!storage::list(&conn).unwrap().is_empty());
}

#[test]
fn import_is_unfiltered_and_id_case_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("personal.pgn");
    std::fs::write(&source, "[Event \"Bullet\"]\n[Site \"https://lichess.org/AbCd0001\"]\n[WhiteElo \"4500\"]\n\n*\n\n[Event \"Bullet\"]\n[Site \"https://lichess.org/abcd0001\"]\n[WhiteElo \"-1\"]\n\n*\n").unwrap();
    let result = application::run(
        ParseRequest {
            source,
            filters: Filters::default(),
            start_index: 99,
            limit: 1,
            import_all: true,
        },
        temp.path(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(result.progress.matched, 2);
    let conn = storage::open(&temp.path().join("chesspush.sqlite3")).unwrap();
    assert_eq!(
        conn.query_row("SELECT count(*) FROM games", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    assert!(result.error.is_none());
}

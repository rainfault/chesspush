//! Real WAL connections reproduce a background writer racing repertoire training.
use chesspush_lib::{
    domain::AppError,
    knowledge::{
        fragments,
        training::{self, Scope, StartRequest},
    },
    storage,
};
use rusqlite::Connection;
use std::{
    cell::RefCell,
    fmt::Debug,
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};

#[derive(Debug, PartialEq, Eq)]
enum Event {
    Busy,
    Completed,
}
struct BusyHandshake {
    events: Sender<Event>,
    release: Receiver<()>,
}
thread_local! {
    static HANDSHAKE: RefCell<Option<BusyHandshake>> = const { RefCell::new(None) };
}

// A bounded handshake, rather than a timed sleep, releases the real background
// transaction only when SQLite asks to wait for its write lock.
fn wait_for_writer(attempt: i32) -> bool {
    if attempt != 0 {
        return false;
    }
    HANDSHAKE.with(|state| {
        let state = state.borrow();
        let handshake = state.as_ref().expect("installed on the worker thread");
        handshake.events.send(Event::Busy).is_ok()
            && handshake
                .release
                .recv_timeout(Duration::from_secs(5))
                .is_ok()
    })
}

fn under_background_write<T, F>(
    writer: &Connection,
    worker: Connection,
    waits: bool,
    action: F,
) -> T
where
    T: Debug + Send + 'static,
    F: FnOnce(&Connection) -> Result<T, AppError> + Send + 'static,
{
    let (events_tx, events_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    writer.execute_batch("BEGIN IMMEDIATE").unwrap();
    storage::set_setting(writer, "test.background-writer", "committed").unwrap();
    let worker_thread = thread::spawn(move || {
        HANDSHAKE.with(|state| {
            *state.borrow_mut() = Some(BusyHandshake {
                events: events_tx.clone(),
                release: release_rx,
            });
        });
        worker.busy_handler(Some(wait_for_writer)).unwrap();
        let result = action(&worker);
        let _ = events_tx.send(Event::Completed);
        result
    });
    let first = events_rx.recv_timeout(Duration::from_secs(5));
    // Always release and join, even on the pre-fix failure path.
    writer.execute_batch("COMMIT").unwrap();
    let _ = release_tx.send(());
    let result = worker_thread.join().unwrap();
    assert_eq!(
        first.unwrap(),
        if waits { Event::Busy } else { Event::Completed },
        "mutations must reserve the writer before reading; overview must remain read-only: {result:?}"
    );
    result.expect("training succeeds without losing progress after the writer commits")
}

fn scope() -> Scope {
    Scope {
        side: "white".into(),
        chapter_id: None,
        node_id: None,
        source_document_id: None,
    }
}
fn request(mode: &str) -> StartRequest {
    StartRequest {
        scope: scope(),
        mode: mode.into(),
        restart: false,
    }
}
fn fixture() -> (tempfile::TempDir, Connection, std::path::PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("training.sqlite3");
    let conn = storage::open(&database).unwrap();
    assert_eq!(
        conn.pragma_query_value::<String, _>(None, "journal_mode", |row| row.get(0))
            .unwrap(),
        "wal"
    );
    let pgn = directory.path().join("repertoire.pgn");
    std::fs::write(&pgn, "1. e4 e5 (1... c5 2. Nf3) 2. Nf3 Nc6 3. Bb5 *").unwrap();
    fragments::attach_file(&conn, &pgn, "white").unwrap();
    (directory, conn, database)
}
fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
        row.get(0)
    })
    .unwrap()
}
fn schedule(conn: &Connection) -> Vec<(String, i64, i64, i64, f64, i64)> {
    conn.prepare("SELECT decision_id,repetitions,interval_days,due_at_ms,ease,last_review_at_ms FROM repertoire_training_srs ORDER BY decision_id")
        .unwrap().query_map([], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?)))
        .unwrap().collect::<Result<Vec<_>,_>>().unwrap()
}

#[test]
fn start_waits_for_an_existing_background_writer() {
    let (_directory, conn, database) = fixture();
    let result = under_background_write(&conn, storage::open(&database).unwrap(), true, |worker| {
        training::start(worker, request("full_drill"))
    });
    assert_eq!(result.completed, 0);
    assert_eq!(result.mistakes, 0);
    assert!(result.total >= 3);
    assert_eq!(count(&conn, "repertoire_training_sessions"), 1);
    assert_eq!(count(&conn, "repertoire_training_reviews"), 0);
    assert!(schedule(&conn).is_empty());
}

#[test]
fn overview_reads_during_a_background_write_without_saving_the_session() {
    let (_directory, conn, database) = fixture();
    let initial = training::start(&conn, request("full_drill")).unwrap();
    // Pin the timestamp so a redundant save is observable without clock sleeps.
    conn.execute(
        "UPDATE repertoire_training_sessions SET updated_at_ms=123",
        [],
    )
    .unwrap();
    let before: (String, i64) = conn
        .query_row(
            "SELECT state_json,updated_at_ms FROM repertoire_training_sessions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let result =
        under_background_write(&conn, storage::open(&database).unwrap(), false, |worker| {
            training::overview(worker, scope())
        });
    let active = result.active_full_drill.unwrap();
    assert_eq!(active.id, initial.id);
    assert_eq!(active.total, initial.total);
    let after: (String, i64) = conn
        .query_row(
            "SELECT state_json,updated_at_ms FROM repertoire_training_sessions",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(before, after);
}

#[test]
fn srs_answer_waits_and_records_the_answer_exactly_once() {
    let (_directory, conn, database) = fixture();
    let initial = training::start(&conn, request("srs")).unwrap();
    let id = initial.id.clone();
    let answered = under_background_write(
        &conn,
        storage::open(&database).unwrap(),
        true,
        move |worker| training::answer(worker, &id, "e2e4"),
    );
    assert_eq!(answered.completed, 1);
    assert_eq!(answered.feedback, "correct");
    assert_eq!(count(&conn, "repertoire_training_reviews"), 1);
    let before = schedule(&conn);
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].1, 1);
    let id = initial.id;
    let duplicate = under_background_write(
        &conn,
        storage::open(&database).unwrap(),
        true,
        move |worker| training::answer(worker, &id, "e2e4"),
    );
    assert_eq!(duplicate.completed, 1);
    assert_eq!(count(&conn, "repertoire_training_reviews"), 1);
    assert_eq!(before, schedule(&conn));
}

#[test]
fn full_drill_hint_waits_and_preserves_srs_schedule() {
    let (_directory, conn, database) = fixture();
    let srs = training::start(&conn, request("srs")).unwrap();
    training::answer(&conn, &srs.id, "e2e4").unwrap();
    let before = schedule(&conn);
    let initial = training::start(&conn, request("full_drill")).unwrap();
    let id = initial.id.clone();
    let hinted = under_background_write(
        &conn,
        storage::open(&database).unwrap(),
        true,
        move |worker| training::hint(worker, &id),
    );
    assert_eq!(hinted.mistakes, 1);
    assert_eq!(hinted.completed, 0);
    assert_eq!(hinted.feedback, "retry");
    let corrected = training::answer(&conn, &initial.id, "e2e4").unwrap();
    assert_eq!(corrected.feedback, "assisted");
    assert_eq!(corrected.mistakes, 1);
    assert_eq!(corrected.completed, 1);
    assert_eq!(before, schedule(&conn));
    let hints: i64 = conn
        .query_row(
            "SELECT count(*) FROM repertoire_training_reviews WHERE session_id=?1 AND kind='hint'",
            [&initial.id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(hints, 1);
}

#[test]
fn full_drill_next_waits_and_keeps_coverage_and_errors() {
    let (_directory, conn, database) = fixture();
    let srs = training::start(&conn, request("srs")).unwrap();
    training::answer(&conn, &srs.id, "e2e4").unwrap();
    let before = schedule(&conn);
    let initial = training::start(&conn, request("full_drill")).unwrap();
    training::answer(&conn, &initial.id, "d2d4").unwrap();
    training::answer(&conn, &initial.id, "e2e4").unwrap();
    let id = initial.id.clone();
    let next = under_background_write(
        &conn,
        storage::open(&database).unwrap(),
        true,
        move |worker| training::next(worker, &id),
    );
    assert_eq!(next.completed, 1);
    assert_eq!(next.mistakes, 1);
    assert_eq!(next.total, initial.total);
    assert_eq!(next.feedback, "awaiting");
    assert!(!next.finished);
    assert_ne!(
        next.prompt.unwrap().decision_id,
        initial.prompt.unwrap().decision_id
    );
    assert_eq!(before, schedule(&conn));
    let overview = training::overview(&conn, scope()).unwrap();
    let resumed = overview.active_full_drill.unwrap();
    assert_eq!(resumed.id, initial.id);
    assert_eq!(resumed.completed, 1);
    assert_eq!(resumed.mistakes, 1);
}

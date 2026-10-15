//! Polling of finished personal games. Creation windows are evidence, not finish-time cursors.
use super::{
    chess,
    store::{self, AccountObservation, PendingGame, SyncRange, SyncStatus},
};
use crate::{
    domain::AppError,
    lichess::{LichessService, VerifiedIdentity},
    storage,
};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    path::Path,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};
const OVERLAP_MS: i64 = 10 * 60 * 1000;
const WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const RECONCILE_MS: i64 = 24 * 60 * 60 * 1000;
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncReport {
    pub status: SyncStatus,
    pub discovered: usize,
    pub imported: usize,
    pub quarantined: usize,
}
/// Unknown/non-played terminal values never schedule analysis.
pub fn is_final(status: &str) -> bool {
    matches!(
        status,
        "mate" | "resign" | "stalemate" | "timeout" | "draw" | "outoftime"
    )
}
fn is_active(status: &str) -> bool {
    matches!(status, "created" | "started")
}
fn check(
    cancel: &AtomicBool,
    service: &LichessService,
    identity: &VerifiedIdentity,
) -> Result<(), AppError> {
    if cancel.load(Ordering::Relaxed) || !service.is_current_identity(identity) {
        Err(AppError::new("cancelled", "Синхронизация остановлена"))
    } else {
        Ok(())
    }
}
pub fn run(
    path: &Path,
    service: &LichessService,
    cancel: &AtomicBool,
) -> Result<SyncReport, AppError> {
    static SEEN: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    let key = format!(
        "{}:{}",
        path.display(),
        service
            .verified_identity()?
            .map(|v| v.id)
            .unwrap_or_default()
    );
    let first = SEEN
        .get_or_init(|| Mutex::new(HashSet::new()))
        .lock()
        .map_err(|_| AppError::new("state", "Состояние синхронизации недоступно"))?
        .insert(key);
    run_at(path, service, cancel, store::now_ms(), first)
}
/// Clock injection and a real HTTP transport keep restart/partial/429 tests deterministic.
pub fn run_at(
    path: &Path,
    service: &LichessService,
    cancel: &AtomicBool,
    now: i64,
    startup: bool,
) -> Result<SyncReport, AppError> {
    let conn = storage::open(path)?;
    let Some(identity) = service.verified_identity()? else {
        return Ok(SyncReport {
            status: store::sync_status(&conn, None)?,
            ..Default::default()
        });
    };
    // Observation is committed by authenticated connect/refresh. A sync snapshot must never
    // re-enable an account after a concurrent disconnect or create a later substitute T0.
    let account = store::account(&conn, &format!("lichess:{}", identity.id))?
        .ok_or_else(|| AppError::new("state", "Наблюдение аккаунта не сохранено"))?;
    if !account.enabled || account.generation != identity.generation {
        return Ok(SyncReport {
            status: store::sync_status(&conn, Some(&account.id))?,
            ..Default::default()
        });
    }
    let state = store::state(&conn, &account.id)?;
    if state.retry_not_before_ms.is_some_and(|retry| retry > now) {
        return Ok(SyncReport {
            status: store::sync_status(&conn, Some(&account.id))?,
            ..Default::default()
        });
    }
    store::mark_running(&conn, &account.id, now)?;
    let mut report = SyncReport::default();
    let result = (|| {
        check(cancel, service, &identity)?;
        // Active export has neither max nor since: correspondence/pre-T0 pending must survive.
        active_scan(
            &conn,
            service,
            &identity,
            &account,
            cancel,
            now,
            &mut report,
        )?;
        if !state.baseline_complete {
            active_scan(
                &conn,
                service,
                &identity,
                &account,
                cancel,
                now,
                &mut report,
            )?;
            store::baseline_complete(&conn, &account.id)?;
        }
        resolve_pending(
            &conn,
            service,
            &identity,
            &account,
            cancel,
            now,
            &mut report,
        )?;
        for range in store::pending_ranges(&conn, &account.id)? {
            check(cancel, service, &identity)?;
            scan_range(
                &conn,
                service,
                &identity,
                &account,
                cancel,
                now,
                &range,
                &mut report,
            )?;
        }
        // Bound each run, preserving unswept creation intervals durably through highwater.
        for _ in 0..4 {
            let state = store::state(&conn, &account.id)?;
            let since = (state.highwater_ms - OVERLAP_MS).max(account.t0_ms);
            let until = now
                .max(account.t0_ms + 1)
                .min(since.saturating_add(WINDOW_MS));
            if until <= since {
                break;
            }
            let range = store::add_range(&conn, &account.id, since, until, "discovery")?;
            scan_range(
                &conn,
                service,
                &identity,
                &account,
                cancel,
                now,
                &range,
                &mut report,
            )?;
            if until >= now {
                break;
            }
        }
        let state = store::state(&conn, &account.id)?;
        if state.reconciliation_upper_ms.is_none()
            && (startup
                || state
                    .last_reconciled_ms
                    .is_none_or(|t| now - t >= RECONCILE_MS))
        {
            store::begin_reconciliation(
                &conn,
                &account.id,
                account.t0_ms,
                now.max(account.t0_ms + 1),
            )?;
        }
        for _ in 0..4 {
            let state = store::state(&conn, &account.id)?;
            let Some(upper) = state.reconciliation_upper_ms else {
                break;
            };
            if state.reconciliation_cursor_ms >= upper {
                store::finish_reconciliation(&conn, &account.id, now)?;
                break;
            }
            let since = state.reconciliation_cursor_ms.max(account.t0_ms);
            let until = upper.min(since.saturating_add(WINDOW_MS));
            let range = store::add_range(&conn, &account.id, since, until, "reconciliation")?;
            scan_range(
                &conn,
                service,
                &identity,
                &account,
                cancel,
                now,
                &range,
                &mut report,
            )?;
            if until >= upper {
                store::finish_reconciliation(&conn, &account.id, now)?;
                break;
            }
        }
        check(cancel, service, &identity)?;
        store::mark_success(&conn, &account.id, now)
    })();
    if let Err(error) = result {
        // Generation invalidation must not publish an old account's state over a new observation.
        if service.is_current_identity(&identity) {
            let delay = if error.code == "rate_limit" {
                service.retry_after_seconds() as i64 * 1000
            } else if error.code == "cancelled" {
                0
            } else {
                30_000
            };
            store::mark_failure(&conn, &account.id, &error, now.saturating_add(delay))?;
        }
        report.status = store::sync_status(&conn, Some(&account.id))?;
        if error.code == "cancelled" {
            return Ok(report);
        }
        // Failures are persisted and returned as status so UI can display progress and retry time.
        return Ok(report);
    }
    report.status = store::sync_status(&conn, Some(&account.id))?;
    Ok(report)
}
fn discovery_query(
    since: Option<i64>,
    until: Option<i64>,
    active: bool,
) -> Vec<(&'static str, String)> {
    let mut q = vec![
        ("sort", "dateAsc".into()),
        ("finished", (!active).to_string()),
        ("ongoing", "true".into()),
        ("moves", "false".into()),
        ("clocks", "false".into()),
        ("evals", "false".into()),
    ];
    if let Some(t) = since {
        q.push(("since", t.to_string()));
    }
    if let Some(t) = until {
        q.push(("until", t.to_string()));
    }
    q
}
fn validate_id(value: &Value) -> Result<&str, AppError> {
    value
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| id.len() == 8 && id.bytes().all(|c| c.is_ascii_alphanumeric()))
        .ok_or_else(|| AppError::new("response", "Некорректный ID партии в ответе Lichess"))
}
fn active_scan(
    conn: &Connection,
    service: &LichessService,
    identity: &VerifiedIdentity,
    account: &AccountObservation,
    cancel: &AtomicBool,
    now: i64,
    report: &mut SyncReport,
) -> Result<(), AppError> {
    service.export_user_games(
        identity,
        &discovery_query(None, None, true),
        cancel,
        |value| {
            validate_id(&value)?;
            let status = value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if is_active(status) {
                store::track_pending(conn, &account.id, &value, now, true, None)?;
                report.discovered += 1;
            }
            Ok(())
        },
    )
}
#[allow(clippy::too_many_arguments)]
fn scan_range(
    conn: &Connection,
    service: &LichessService,
    identity: &VerifiedIdentity,
    account: &AccountObservation,
    cancel: &AtomicBool,
    now: i64,
    range: &SyncRange,
    report: &mut SyncReport,
) -> Result<(), AppError> {
    service.export_user_games(
        identity,
        &discovery_query(Some(range.since_ms), Some(range.until_ms), false),
        cancel,
        |value| {
            validate_id(&value)?;
            let status = value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let id = validate_id(&value)?;
            if !(is_final(status) && store::current_payload(conn, &account.id, id, &value)?) {
                store::track_pending(conn, &account.id, &value, now, is_active(status), None)?;
            }
            report.discovered += 1;
            Ok(())
        },
    )?;
    resolve_pending(conn, service, identity, account, cancel, now, report)?;
    check(cancel, service, identity)?;
    store::close_range(conn, &account.id, range, now)
}
fn resolve_pending(
    conn: &Connection,
    service: &LichessService,
    identity: &VerifiedIdentity,
    account: &AccountObservation,
    cancel: &AtomicBool,
    now: i64,
    report: &mut SyncReport,
) -> Result<(), AppError> {
    for pending in store::pending(conn, &account.id)? {
        check(cancel, service, identity)?;
        if is_active(&pending.status) && pending.last_seen_ms == now {
            continue;
        }
        let mut metadata = serde_json::from_str::<Value>(&pending.raw_metadata)?;
        if !is_final(&pending.status)
            && !matches!(
                pending.status.as_str(),
                "aborted" | "noStart" | "unknownFinish" | "cheat" | "variantEnd"
            )
        {
            let result =
                service.export_game_metadata(identity, &pending.external_id, cancel, |value| {
                    if validate_id(&value)? != pending.external_id {
                        return Err(AppError::new(
                            "response",
                            "ID партии не совпадает с запросом",
                        ));
                    }
                    metadata = value;
                    Ok(())
                });
            if let Err(error) = result {
                if error.code == "not_found" {
                    store::track_pending(
                        conn,
                        &account.id,
                        &metadata,
                        now,
                        false,
                        Some("not_found"),
                    )?;
                    continue;
                }
                return Err(error);
            }
        }
        let status = metadata
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if is_active(status) {
            store::track_pending(conn, &account.id, &metadata, now, true, None)?;
            continue;
        }
        if !is_final(status) {
            store::quarantine(
                conn,
                &account.id,
                &metadata,
                "unconfirmed_or_nonplayed_status",
                now,
            )?;
            if matches!(
                status,
                "aborted" | "noStart" | "unknownFinish" | "cheat" | "variantEnd"
            ) {
                store::untrack(conn, &account.id, &pending.external_id)?;
            }
            report.quarantined += 1;
            continue;
        }
        let mut received = false;
        let exported = service.export_game(identity, &pending.external_id, cancel, |value| {
            if validate_id(&value)? != pending.external_id {
                return Err(AppError::new(
                    "response",
                    "ID партии не совпадает с запросом",
                ));
            }
            let status = value
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if !is_final(status) {
                store::track_pending(
                    conn,
                    &account.id,
                    &value,
                    now,
                    is_active(status),
                    Some("status_changed"),
                )?;
                received = true;
                return Ok(());
            }
            let eligible = eligibility(&value, account, &pending);
            match chess::from_lichess(&value, &account.id, &account.external_id) {
                Ok(game) => {
                    if store::save_game(conn, &account.id, &game, &value.to_string(), eligible)? {
                        report.imported += 1;
                    }
                }
                Err(reason) => {
                    store::quarantine(conn, &account.id, &value, &reason, now)?;
                    store::untrack(conn, &account.id, &pending.external_id)?;
                    report.quarantined += 1;
                }
            }
            received = true;
            Ok(())
        });
        if let Err(error) = exported {
            if error.code == "not_found" {
                store::track_pending(conn, &account.id, &metadata, now, false, Some("not_found"))?;
                continue;
            }
            return Err(error);
        }
        if !received {
            return Err(AppError::new(
                "partial",
                "Ответ Lichess не содержит запрошенную партию",
            ));
        }
    }
    Ok(())
}
fn eligibility(value: &Value, account: &AccountObservation, pending: &PendingGame) -> &'static str {
    if value
        .get("createdAt")
        .and_then(Value::as_i64)
        .is_some_and(|t| t >= account.t0_ms)
        || value
            .get("lastMoveAt")
            .and_then(Value::as_i64)
            .is_some_and(|t| t >= account.t0_ms)
    {
        "eligible"
    } else if pending.active_seen_ms.is_some_and(|t| t >= account.t0_ms) {
        "boundary_uncertain"
    } else {
        "excluded"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::Arc,
        thread,
        time::Duration,
    };
    struct Reply {
        status: u16,
        body: String,
        missing_bytes: usize,
    }
    impl Reply {
        fn json(value: Value) -> Self {
            Self {
                status: 200,
                body: value.to_string(),
                missing_bytes: 0,
            }
        }
        fn ndjson(values: Vec<Value>) -> Self {
            Self {
                status: 200,
                body: values.into_iter().map(|v| format!("{v}\n")).collect(),
                missing_bytes: 0,
            }
        }
    }
    struct Mock {
        url: String,
        stop: Arc<AtomicBool>,
        requests: Arc<Mutex<Vec<String>>>,
        thread: Option<thread::JoinHandle<()>>,
    }
    impl Mock {
        fn new(mut handler: impl FnMut(&str) -> Reply + Send + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.set_nonblocking(true).unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let stop = Arc::new(AtomicBool::new(false));
            let done = stop.clone();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let capture = requests.clone();
            let worker = thread::spawn(move || {
                while !done.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            stream.set_nonblocking(false).unwrap();
                            stream
                                .set_read_timeout(Some(Duration::from_secs(2)))
                                .unwrap();
                            let mut bytes = Vec::new();
                            let mut buffer = [0u8; 2048];
                            while !bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                                let n = stream.read(&mut buffer).unwrap();
                                if n == 0 {
                                    break;
                                }
                                bytes.extend_from_slice(&buffer[..n]);
                                assert!(bytes.len() < 32_768);
                            }
                            let headers = String::from_utf8(bytes).unwrap();
                            let target = headers
                                .lines()
                                .next()
                                .unwrap()
                                .split_whitespace()
                                .nth(1)
                                .unwrap();
                            capture.lock().unwrap().push(target.to_owned());
                            let response = handler(target);
                            write!(stream,"HTTP/1.1 {} Test\r\nContent-Type: application/x-ndjson\r\nRetry-After: 120\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",response.status,response.body.len()+response.missing_bytes,response.body).unwrap();
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2))
                        }
                        Err(e) => panic!("{e}"),
                    }
                }
            });
            Self {
                url,
                stop,
                requests,
                thread: Some(worker),
            }
        }
    }
    impl Drop for Mock {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
            self.thread.take().unwrap().join().unwrap();
        }
    }
    fn fixture(id: &str, created: i64, status: &str) -> Value {
        json!({"id":id,"createdAt":created,"lastMoveAt":created+200,"status":status,"winner":"white","variant":"standard","speed":"blitz","rated":true,"clock":{"initial":300,"increment":2},"players":{"white":{"user":{"id":"syntheticplayer","name":"SyntheticPlayer"},"rating":1500},"black":{"user":{"id":"fixtureopponent","name":"FixtureOpponent"},"rating":1520}},"moves":"e4 e5 Nf3 Nc6"})
    }
    fn server(games: Arc<Mutex<Vec<Value>>>) -> Mock {
        Mock::new(move |target| {
            let url = url::Url::parse(&format!("http://localhost{target}")).unwrap();
            let args: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
            let games = games.lock().unwrap();
            if url.path().starts_with("/game/export/") {
                let id = url.path().rsplit('/').next().unwrap();
                return games
                    .iter()
                    .find(|g| g["id"] == id)
                    .cloned()
                    .map(Reply::json)
                    .unwrap_or(Reply {
                        status: 404,
                        body: "{}".into(),
                        missing_bytes: 0,
                    });
            }
            let active = args.get("finished").is_some_and(|v| v == "false");
            let since = args.get("since").and_then(|v| v.parse::<i64>().ok());
            let until = args.get("until").and_then(|v| v.parse::<i64>().ok());
            Reply::ndjson(
                games
                    .iter()
                    .filter(|g| {
                        (!active || is_active(g["status"].as_str().unwrap()))
                            && since.is_none_or(|t| g["createdAt"].as_i64().unwrap() >= t)
                            && until.is_none_or(|t| g["createdAt"].as_i64().unwrap() < t)
                    })
                    .cloned()
                    .collect(),
            )
        })
    }
    fn setup(endpoint: &str) -> (tempfile::TempDir, std::path::PathBuf, LichessService) {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.sqlite3");
        let service = LichessService::synthetic(&path, endpoint);
        let conn = storage::open(&path).unwrap();
        conn.execute("UPDATE statistics_accounts SET t0_ms=1000", [])
            .unwrap();
        conn.execute(
            "UPDATE statistics_sync_state SET highwater_ms=1000,reconciliation_cursor_ms=1000",
            [],
        )
        .unwrap();
        (directory, path, service)
    }
    #[test]
    fn empty_history_durable_t0_and_no_historical_import() {
        let data = Arc::new(Mutex::new(vec![fixture("oldgame1", 500, "resign")]));
        let mock = server(data);
        let (_tmp, path, service) = setup(&mock.url);
        let r = run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        assert_eq!(r.imported, 0);
        assert_eq!(r.status.status, "idle");
        assert!(r.status.baseline_complete && r.status.boundary_uncertain);
        assert_eq!(r.status.last_reconciled_ms, Some(10_000));
        assert_eq!(
            store::active_account(&storage::open(&path).unwrap())
                .unwrap()
                .unwrap()
                .t0_ms,
            1000
        );
        assert!(
            mock.requests
                .lock()
                .unwrap()
                .iter()
                .all(|p| !p.contains("max="))
        );
    }
    #[test]
    fn equal_timestamps_duplicates_restart_and_first_game() {
        let data = Arc::new(Mutex::new(vec![
            fixture("fixture1", 2000, "resign"),
            fixture("fixture2", 2000, "resign"),
        ]));
        let mock = server(data);
        let (_tmp, path, service) = setup(&mock.url);
        run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        run_at(&path, &service, &AtomicBool::new(false), 20_000, true).unwrap();
        let conn = storage::open(&path).unwrap();
        assert_eq!(store::games(&conn, None).unwrap().len(), 2);
        assert_eq!(
            conn.query_row("SELECT count(*) FROM games", [], |r| r.get::<_, usize>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM statistics_payloads", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            2
        );
        assert!(
            mock.requests
                .lock()
                .unwrap()
                .iter()
                .any(|p| p.contains("since=1000"))
        );
        assert!(
            mock.requests
                .lock()
                .unwrap()
                .iter()
                .all(|p| !p.contains("since=2001"))
        );
        assert_eq!(
            mock.requests
                .lock()
                .unwrap()
                .iter()
                .filter(|p| p.starts_with("/game/export/"))
                .count(),
            2
        );
    }
    #[test]
    fn long_pre_t0_pending_survives_restart_and_late_finish() {
        let data = Arc::new(Mutex::new(vec![fixture("longgame", 500, "started")]));
        let mock = server(data.clone());
        let (_tmp, path, service) = setup(&mock.url);
        run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        let conn = storage::open(&path).unwrap();
        assert_eq!(
            store::pending(&conn, "lichess:syntheticplayer")
                .unwrap()
                .len(),
            1
        );
        assert!(store::games(&conn, None).unwrap().is_empty());
        data.lock().unwrap()[0]["status"] = json!("resign");
        data.lock().unwrap()[0]["lastMoveAt"] = json!(30_000);
        run_at(&path, &service, &AtomicBool::new(false), 40_000, true).unwrap();
        let games = store::games(&conn, None).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].eligibility, "eligible");
        assert!(games[0].finished_at_ms.is_none());
        assert!(
            store::pending(&conn, "lichess:syntheticplayer")
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn pre_t0_active_evidence_remains_boundary_uncertain() {
        let data = Arc::new(Mutex::new(vec![fixture("boundary", 500, "started")]));
        let mock = server(data.clone());
        let (_tmp, path, service) = setup(&mock.url);
        run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        data.lock().unwrap()[0]["status"] = json!("resign");
        run_at(&path, &service, &AtomicBool::new(false), 20_000, false).unwrap();
        assert_eq!(
            store::games(&storage::open(&path).unwrap(), None).unwrap()[0].eligibility,
            "boundary_uncertain"
        );
    }
    #[test]
    fn late_publication_outside_overlap_found_by_full_reconciliation() {
        let data = Arc::new(Mutex::new(Vec::new()));
        let mock = server(data.clone());
        let (_tmp, path, service) = setup(&mock.url);
        run_at(&path, &service, &AtomicBool::new(false), 5_000_000, true).unwrap();
        data.lock()
            .unwrap()
            .push(fixture("latepubl", 2000, "resign"));
        let r = run_at(&path, &service, &AtomicBool::new(false), 6_000_000, true).unwrap();
        assert_eq!(r.imported, 1);
        assert_eq!(
            store::games(&storage::open(&path).unwrap(), None)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn partial_stream_saves_complete_objects_and_keeps_range_pending() {
        let mut discoveries = 0;
        let game = fixture("partial1", 2000, "resign");
        let mock = Mock::new(move |target| {
            if target.contains("finished=false") {
                return Reply::ndjson(vec![]);
            }
            if target.starts_with("/game/export/") {
                return Reply::json(game.clone());
            }
            discoveries += 1;
            if discoveries == 1 {
                Reply {
                    status: 200,
                    body: format!("{}\n{{\"id\":", game),
                    missing_bytes: 100,
                }
            } else {
                Reply::ndjson(vec![game.clone()])
            }
        });
        let (_tmp, path, service) = setup(&mock.url);
        let r = run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        assert_eq!(r.status.status, "error");
        assert_eq!(r.status.highwater_ms, Some(1000));
        assert_eq!(r.status.pending_ranges, 1);
        assert_eq!(r.status.pending_games, 1);
        let r = run_at(&path, &service, &AtomicBool::new(false), 50_000, true).unwrap();
        assert_eq!(r.status.status, "idle");
        assert_eq!(
            store::games(&storage::open(&path).unwrap(), None)
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn rate_limit_persists_retry_and_serializes_retries() {
        let mock = Mock::new(|_| Reply {
            status: 429,
            body: "{}".into(),
            missing_bytes: 0,
        });
        let (_tmp, path, service) = setup(&mock.url);
        let r = run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        assert_eq!(r.status.error_code.as_deref(), Some("rate_limit"));
        assert!(r.status.retry_not_before_ms.unwrap() >= 130_000);
        run_at(&path, &service, &AtomicBool::new(false), 11_000, false).unwrap();
        assert_eq!(mock.requests.lock().unwrap().len(), 1);
    }
    #[test]
    fn offline_is_persisted_without_destroying_pending() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let (_tmp, path, service) = setup(&url);
        let conn = storage::open(&path).unwrap();
        store::track_pending(
            &conn,
            "lichess:syntheticplayer",
            &fixture("offline1", 500, "started"),
            1000,
            true,
            None,
        )
        .unwrap();
        let r = run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        assert_eq!(r.status.error_code.as_deref(), Some("offline"));
        assert_eq!(r.status.pending_games, 1);
        assert_eq!(r.status.highwater_ms, Some(1000));
    }
    #[test]
    fn unsupported_nonplayed_and_invalid_pgn_are_quarantined() {
        let mut invalid = fixture("badpgn01", 2000, "resign");
        invalid["pgn"] = json!("1. e5 1-0");
        let mut variant = fixture("variant1", 2000, "resign");
        variant["variant"] = json!("atomic");
        let data = Arc::new(Mutex::new(vec![
            invalid,
            variant,
            fixture("nostart1", 2000, "noStart"),
            fixture("cheat001", 2000, "cheat"),
        ]));
        let mock = server(data);
        let (_tmp, path, service) = setup(&mock.url);
        let r = run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        assert_eq!(r.status.status, "idle");
        let conn = storage::open(&path).unwrap();
        assert!(store::games(&conn, None).unwrap().is_empty());
        assert_eq!(
            conn.query_row("SELECT count(*) FROM statistics_quarantine", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            4
        );
    }
    #[test]
    fn missing_clocks_and_provider_evals_keep_basic_metrics() {
        let data = Arc::new(Mutex::new(vec![fixture("noclocks", 2000, "resign")]));
        let mock = server(data);
        let (_tmp, path, service) = setup(&mock.url);
        run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        let game = store::games(&storage::open(&path).unwrap(), None)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(game.replay.nodes.len(), 5);
        assert!(
            game.replay
                .nodes
                .iter()
                .all(|n| n.clock_seconds.is_none() && n.spent_seconds.is_none())
        );
        assert!(!game.raw_pgn.is_empty());
    }
    #[test]
    fn stale_identity_snapshot_cannot_reenable_disabled_observation() {
        let mock = server(Arc::new(Mutex::new(Vec::new())));
        let (_tmp, path, service) = setup(&mock.url);
        let conn = storage::open(&path).unwrap();
        store::disable_accounts(&conn).unwrap();
        let result = run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        assert_eq!(result.status.status, "disconnected");
        assert!(store::active_account(&conn).unwrap().is_none());
        assert!(mock.requests.lock().unwrap().is_empty());
    }
    #[test]
    fn stale_network_response_after_disconnect_cannot_commit_or_reenable() {
        let (started, seen) = std::sync::mpsc::channel();
        let (release, ready) = std::sync::mpsc::channel();
        let mut gate = Some(ready);
        let mock = Mock::new(move |_| {
            started.send(()).unwrap();
            if let Some(receiver) = gate.take() {
                receiver.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            Reply::ndjson(vec![fixture("stale001", 500, "started")])
        });
        let (_tmp, path, service) = setup(&mock.url);
        let service = Arc::new(service);
        let running = service.clone();
        let db = path.clone();
        let worker = thread::spawn(move || {
            run_at(&db, &running, &AtomicBool::new(false), 10_000, true).unwrap()
        });
        seen.recv_timeout(Duration::from_secs(5)).unwrap();
        service.disconnect().unwrap();
        release.send(()).unwrap();
        let result = worker.join().unwrap();
        assert_eq!(result.status.status, "disconnected");
        let conn = storage::open(&path).unwrap();
        assert!(store::active_account(&conn).unwrap().is_none());
        assert!(
            store::pending(&conn, "lichess:syntheticplayer")
                .unwrap()
                .is_empty()
        );
        assert!(store::games(&conn, None).unwrap().is_empty());
    }
    #[test]
    fn final_pending_404_does_not_block_new_game_discovery() {
        let incoming = fixture("newgame1", 2000, "resign");
        let mock = Mock::new(move |target| {
            if target.starts_with("/game/export/gone0001") {
                return Reply {
                    status: 404,
                    body: "{}".into(),
                    missing_bytes: 0,
                };
            }
            if target.contains("finished=false") {
                return Reply::ndjson(vec![]);
            }
            if target.starts_with("/game/export/newgame1") {
                return Reply::json(incoming.clone());
            }
            Reply::ndjson(vec![incoming.clone()])
        });
        let (_tmp, path, service) = setup(&mock.url);
        let conn = storage::open(&path).unwrap();
        store::track_pending(
            &conn,
            "lichess:syntheticplayer",
            &fixture("gone0001", 1500, "resign"),
            2000,
            false,
            None,
        )
        .unwrap();
        let report = run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        assert_eq!(report.status.status, "idle");
        assert_eq!(report.status.pending_games, 1);
        assert_eq!(report.status.highwater_ms, Some(10_000));
        let games = store::games(&conn, None).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].external_id, "newgame1");
        assert_eq!(
            store::pending(&conn, "lichess:syntheticplayer").unwrap()[0].external_id,
            "gone0001"
        );
        assert_eq!(
            conn.query_row(
                "SELECT reason FROM statistics_pending WHERE external_id='gone0001'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "not_found"
        );
    }
    #[test]
    fn known_pending_404_survives_and_disconnect_preserves_history() {
        let mock = server(Arc::new(Mutex::new(Vec::new())));
        let (_tmp, path, service) = setup(&mock.url);
        let conn = storage::open(&path).unwrap();
        store::track_pending(
            &conn,
            "lichess:syntheticplayer",
            &fixture("private1", 500, "started"),
            1000,
            true,
            None,
        )
        .unwrap();
        run_at(&path, &service, &AtomicBool::new(false), 10_000, true).unwrap();
        assert_eq!(
            store::pending(&conn, "lichess:syntheticplayer")
                .unwrap()
                .len(),
            1
        );
        service.disconnect().unwrap();
        assert!(!store::accounts(&conn).unwrap()[0].enabled);
        assert_eq!(store::accounts(&conn).unwrap()[0].t0_ms, 1000);
    }
}

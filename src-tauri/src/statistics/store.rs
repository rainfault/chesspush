//! Durable personal ownership and synchronization evidence; corpus provenance stays untouched.
use super::types::StoredGame;
use crate::{domain::AppError, lichess::VerifiedIdentity};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountObservation {
    pub id: String,
    pub external_id: String,
    pub username: String,
    pub t0_ms: i64,
    pub enabled: bool,
    pub generation: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SyncStatus {
    pub account: Option<AccountObservation>,
    pub status: String,
    pub last_success_ms: Option<i64>,
    pub last_reconciled_ms: Option<i64>,
    pub retry_not_before_ms: Option<i64>,
    pub highwater_ms: Option<i64>,
    pub pending_games: usize,
    pub quarantined_games: usize,
    pub pending_ranges: usize,
    pub boundary_uncertain: bool,
    pub baseline_complete: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}
#[derive(Debug, Clone)]
pub struct SyncState {
    pub highwater_ms: i64,
    pub retry_not_before_ms: Option<i64>,
    pub reconciliation_cursor_ms: i64,
    pub reconciliation_upper_ms: Option<i64>,
    pub last_reconciled_ms: Option<i64>,
    pub baseline_complete: bool,
}
#[derive(Debug, Clone)]
pub struct PendingGame {
    pub external_id: String,
    pub created_at_ms: Option<i64>,
    pub active_seen_ms: Option<i64>,
    pub last_seen_ms: i64,
    pub status: String,
    pub raw_metadata: String,
}
#[derive(Debug, Clone)]
pub struct SyncRange {
    pub id: i64,
    pub since_ms: i64,
    pub until_ms: i64,
    pub kind: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatingSnapshot {
    pub account_id: String,
    pub pool: String,
    pub rating: i64,
    pub timestamp_ms: i64,
    pub source: String,
}
pub fn save_initial_rating(
    conn: &Connection,
    account_id: &str,
    pool: &str,
    rating: i64,
    timestamp_ms: i64,
) -> Result<(), AppError> {
    if rating > 0 {
        conn.execute("INSERT OR IGNORE INTO statistics_rating_snapshots(account_id,pool,rating,timestamp_ms,source) VALUES(?1,?2,?3,?4,'initial')",params![account_id,pool,rating,timestamp_ms])?;
    }
    Ok(())
}
pub fn rating_snapshots(
    conn: &Connection,
    account_id: &str,
) -> Result<Vec<RatingSnapshot>, AppError> {
    Ok(conn.prepare("SELECT account_id,pool,rating,timestamp_ms,source FROM statistics_rating_snapshots WHERE account_id=?1 ORDER BY pool")?.query_map([account_id],|r|Ok(RatingSnapshot{account_id:r.get(0)?,pool:r.get(1)?,rating:r.get(2)?,timestamp_ms:r.get(3)?,source:r.get(4)?}))?.collect::<Result<Vec<_>,_>>()?)
}
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
fn account_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AccountObservation> {
    Ok(AccountObservation {
        id: r.get(0)?,
        external_id: r.get(1)?,
        username: r.get(2)?,
        t0_ms: r.get(3)?,
        enabled: r.get(4)?,
        generation: r.get::<_, i64>(5)? as u64,
    })
}
pub fn accounts(conn: &Connection) -> Result<Vec<AccountObservation>, AppError> {
    Ok(conn.prepare("SELECT id,external_id,username,t0_ms,enabled,generation FROM statistics_accounts ORDER BY enabled DESC,t0_ms")?
        .query_map([],account_row)?.collect::<Result<Vec<_>,_>>()?)
}
pub fn active_account(conn: &Connection) -> Result<Option<AccountObservation>, AppError> {
    Ok(conn.query_row("SELECT id,external_id,username,t0_ms,enabled,generation FROM statistics_accounts WHERE enabled=1 LIMIT 1",[],account_row).optional()?)
}
pub fn account(conn: &Connection, id: &str) -> Result<Option<AccountObservation>, AppError> {
    Ok(conn.query_row("SELECT id,external_id,username,t0_ms,enabled,generation FROM statistics_accounts WHERE id=?1",[id],account_row).optional()?)
}
/// Called only after authenticated account verification, never from a display name.
pub fn observe(
    conn: &Connection,
    identity: &VerifiedIdentity,
    now: i64,
) -> Result<AccountObservation, AppError> {
    observe_with_ratings(conn, identity, now, &[])
}
/// T0 and its first available rating observations commit together. Reconnect never backfills a
/// missing initial pool with a current rating.
pub fn observe_with_ratings(
    conn: &Connection,
    identity: &VerifiedIdentity,
    now: i64,
    ratings: &[(&str, i64)],
) -> Result<AccountObservation, AppError> {
    let id = format!("lichess:{}", identity.id);
    let tx = conn.unchecked_transaction()?;
    let first = account(&tx, &id)?.is_none();
    tx.execute(
        "UPDATE statistics_accounts SET enabled=0 WHERE id<>?1",
        [&id],
    )?;
    tx.execute("INSERT INTO statistics_accounts(id,platform,external_id,username,t0_ms,generation) VALUES(?1,'lichess',?2,?3,?4,?5) ON CONFLICT(platform,external_id) DO UPDATE SET username=excluded.username,enabled=1,generation=excluded.generation",params![id,identity.id,identity.username,now,identity.generation as i64])?;
    let observation =
        account(&tx, &id)?.ok_or_else(|| AppError::new("database", "Аккаунт не сохранён"))?;
    tx.execute("INSERT OR IGNORE INTO statistics_sync_state(account_id,highwater_ms,reconciliation_cursor_ms) VALUES(?1,?2,?2)",params![id,observation.t0_ms])?;
    if first {
        for (pool, rating) in ratings {
            save_initial_rating(&tx, &id, pool, *rating, observation.t0_ms)?;
        }
    }
    tx.commit()?;
    Ok(observation)
}
pub fn disable_accounts(conn: &Connection) -> Result<(), AppError> {
    conn.execute("UPDATE statistics_accounts SET enabled=0", [])?;
    conn.execute("UPDATE statistics_sync_state SET status='disconnected' WHERE account_id IN (SELECT id FROM statistics_accounts WHERE enabled=0)",[])?;
    Ok(())
}
pub fn state(conn: &Connection, id: &str) -> Result<SyncState, AppError> {
    Ok(conn.query_row("SELECT highwater_ms,retry_not_before_ms,reconciliation_cursor_ms,reconciliation_upper_ms,last_reconciled_ms,baseline_complete FROM statistics_sync_state WHERE account_id=?1",[id],|r|Ok(SyncState {highwater_ms:r.get(0)?,retry_not_before_ms:r.get(1)?,reconciliation_cursor_ms:r.get(2)?,reconciliation_upper_ms:r.get(3)?,last_reconciled_ms:r.get(4)?,baseline_complete:r.get(5)?}))?)
}
pub fn sync_status(conn: &Connection, id: Option<&str>) -> Result<SyncStatus, AppError> {
    let observation = match id {
        Some(id) => account(conn, id)?,
        None => active_account(conn)?.or(accounts(conn)?.into_iter().next()),
    };
    let Some(observation) = observation else {
        return Ok(SyncStatus {
            status: "disconnected".into(),
            ..Default::default()
        });
    };
    let mut result=conn.query_row("SELECT status,last_success_ms,last_reconciled_ms,retry_not_before_ms,highwater_ms,boundary_uncertain,baseline_complete,error_code,error_message FROM statistics_sync_state WHERE account_id=?1",[&observation.id],|r|Ok(SyncStatus {status:r.get(0)?,last_success_ms:r.get(1)?,last_reconciled_ms:r.get(2)?,retry_not_before_ms:r.get(3)?,highwater_ms:r.get(4)?,boundary_uncertain:r.get(5)?,baseline_complete:r.get(6)?,error_code:r.get(7)?,error_message:r.get(8)?,..Default::default()}))?;
    result.pending_games = conn.query_row(
        "SELECT count(*) FROM statistics_pending WHERE account_id=?1",
        [&observation.id],
        |r| r.get(0),
    )?;
    result.quarantined_games = conn.query_row(
        "SELECT count(*) FROM statistics_quarantine WHERE account_id=?1",
        [&observation.id],
        |r| r.get(0),
    )?;
    result.pending_ranges = conn.query_row(
        "SELECT count(*) FROM statistics_sync_ranges WHERE account_id=?1 AND state='pending'",
        [&observation.id],
        |r| r.get(0),
    )?;
    if !observation.enabled {
        result.status = "disconnected".into();
    }
    result.account = Some(observation);
    Ok(result)
}
pub fn mark_running(conn: &Connection, id: &str, now: i64) -> Result<(), AppError> {
    conn.execute("UPDATE statistics_sync_state SET status='syncing',attempted_at_ms=?2,error_code=NULL,error_message=NULL WHERE account_id=?1",params![id,now])?;
    Ok(())
}
pub fn mark_failure(
    conn: &Connection,
    id: &str,
    error: &AppError,
    retry: i64,
) -> Result<(), AppError> {
    conn.execute("UPDATE statistics_sync_state SET status=?2,error_code=?3,error_message=?4,retry_not_before_ms=?5 WHERE account_id=?1",params![id,if error.code=="cancelled"{"cancelled"}else if error.code=="invalid_token"{"invalid_token"}else{"error"},error.code,error.message,retry])?;
    Ok(())
}
pub fn mark_success(conn: &Connection, id: &str, now: i64) -> Result<(), AppError> {
    conn.execute("UPDATE statistics_sync_state SET status='idle',last_success_ms=?2,retry_not_before_ms=NULL,error_code=NULL,error_message=NULL WHERE account_id=?1",params![id,now])?;
    Ok(())
}
pub fn baseline_complete(conn: &Connection, id: &str) -> Result<(), AppError> {
    conn.execute(
        "UPDATE statistics_sync_state SET baseline_complete=1 WHERE account_id=?1",
        [id],
    )?;
    Ok(())
}
pub fn add_range(
    conn: &Connection,
    id: &str,
    since: i64,
    until: i64,
    kind: &str,
) -> Result<SyncRange, AppError> {
    conn.execute("INSERT INTO statistics_sync_ranges(account_id,since_ms,until_ms,kind,state) VALUES(?1,?2,?3,?4,'pending') ON CONFLICT(account_id,since_ms,until_ms,kind) DO UPDATE SET state='pending'",params![id,since,until,kind])?;
    Ok(conn.query_row("SELECT id,since_ms,until_ms,kind FROM statistics_sync_ranges WHERE account_id=?1 AND since_ms=?2 AND until_ms=?3 AND kind=?4",params![id,since,until,kind],|r|Ok(SyncRange{id:r.get(0)?,since_ms:r.get(1)?,until_ms:r.get(2)?,kind:r.get(3)?}))?)
}
pub fn pending_ranges(conn: &Connection, id: &str) -> Result<Vec<SyncRange>, AppError> {
    Ok(conn.prepare("SELECT id,since_ms,until_ms,kind FROM statistics_sync_ranges WHERE account_id=?1 AND state='pending' ORDER BY since_ms,id")?.query_map([id],|r|Ok(SyncRange{id:r.get(0)?,since_ms:r.get(1)?,until_ms:r.get(2)?,kind:r.get(3)?}))?.collect::<Result<Vec<_>,_>>()?)
}
pub fn close_range(
    conn: &Connection,
    id: &str,
    range: &SyncRange,
    now: i64,
) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE statistics_sync_ranges SET state='complete',checked_at_ms=?2 WHERE id=?1",
        params![range.id, now],
    )?;
    if range.kind == "discovery" {
        tx.execute("UPDATE statistics_sync_state SET highwater_ms=max(highwater_ms,?2) WHERE account_id=?1",params![id,range.until_ms])?;
    } else {
        tx.execute("UPDATE statistics_sync_state SET reconciliation_cursor_ms=max(reconciliation_cursor_ms,?2) WHERE account_id=?1",params![id,range.until_ms])?;
    }
    tx.commit()?;
    Ok(())
}
pub fn begin_reconciliation(
    conn: &Connection,
    id: &str,
    t0: i64,
    upper: i64,
) -> Result<(), AppError> {
    conn.execute("UPDATE statistics_sync_state SET reconciliation_cursor_ms=?2,reconciliation_upper_ms=?3 WHERE account_id=?1",params![id,t0,upper])?;
    Ok(())
}
pub fn finish_reconciliation(conn: &Connection, id: &str, now: i64) -> Result<(), AppError> {
    conn.execute("UPDATE statistics_sync_state SET last_reconciled_ms=?2,reconciliation_upper_ms=NULL WHERE account_id=?1",params![id,now])?;
    Ok(())
}
/// Reconciliation rereads discovery metadata; unchanged complete payloads are not downloaded again.
pub fn current_payload(
    conn: &Connection,
    account_id: &str,
    external_id: &str,
    metadata: &serde_json::Value,
) -> Result<bool, AppError> {
    let previous:Option<String>=conn.query_row("SELECT json_extract(g.data,'$.source_metadata') FROM statistics_games g JOIN statistics_account_games a ON a.game_id=g.game_id WHERE g.source='lichess' AND g.external_id=?1 AND a.account_id=?2",params![external_id,account_id],|r|r.get(0)).optional()?;
    let Some(previous) = previous else {
        return Ok(false);
    };
    let previous: serde_json::Value = serde_json::from_str(&previous)?;
    Ok([
        "status",
        "winner",
        "createdAt",
        "lastMoveAt",
        "variant",
        "rated",
    ]
    .iter()
    .all(|key| {
        metadata
            .get(key)
            .is_none_or(|value| previous.get(key) == Some(value))
    }))
}
pub fn track_pending(
    conn: &Connection,
    id: &str,
    value: &serde_json::Value,
    now: i64,
    active: bool,
    reason: Option<&str>,
) -> Result<(), AppError> {
    let external = value
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::new("response", "В ответе отсутствует ID партии"))?;
    conn.execute("INSERT INTO statistics_pending(account_id,external_id,created_at_ms,first_seen_ms,last_seen_ms,active_seen_ms,status,reason,raw_metadata) VALUES(?1,?2,?3,?4,?4,?5,?6,?7,?8) ON CONFLICT(account_id,external_id) DO UPDATE SET last_seen_ms=excluded.last_seen_ms,active_seen_ms=coalesce(statistics_pending.active_seen_ms,excluded.active_seen_ms),status=excluded.status,reason=excluded.reason,raw_metadata=excluded.raw_metadata",params![id,external,value.get("createdAt").and_then(|v|v.as_i64()),now,if active{Some(now)}else{None},value.get("status").and_then(|v|v.as_str()).unwrap_or("unknown"),reason,value.to_string()])?;
    Ok(())
}
pub fn pending(conn: &Connection, id: &str) -> Result<Vec<PendingGame>, AppError> {
    Ok(conn.prepare("SELECT external_id,created_at_ms,active_seen_ms,last_seen_ms,status,raw_metadata FROM statistics_pending WHERE account_id=?1 ORDER BY first_seen_ms,external_id")?.query_map([id],|r|Ok(PendingGame{external_id:r.get(0)?,created_at_ms:r.get(1)?,active_seen_ms:r.get(2)?,last_seen_ms:r.get(3)?,status:r.get(4)?,raw_metadata:r.get(5)?}))?.collect::<Result<Vec<_>,_>>()?)
}
pub fn untrack(conn: &Connection, id: &str, external: &str) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM statistics_pending WHERE account_id=?1 AND external_id=?2",
        params![id, external],
    )?;
    Ok(())
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarantineRecord {
    pub account_id: String,
    pub external_id: String,
    pub received_at_ms: i64,
    pub reason: String,
    pub raw_json: String,
}
pub fn quarantined(
    conn: &Connection,
    account_id: Option<&str>,
) -> Result<Vec<QuarantineRecord>, AppError> {
    Ok(conn.prepare("SELECT account_id,external_id,received_at_ms,reason,raw_json FROM statistics_quarantine WHERE (?1 IS NULL OR account_id=?1) ORDER BY received_at_ms DESC,external_id")?
        .query_map([account_id], |r| Ok(QuarantineRecord {
            account_id: r.get(0)?, external_id: r.get(1)?, received_at_ms: r.get(2)?,
            reason: r.get(3)?, raw_json: r.get(4)?,
        }))?.collect::<Result<Vec<_>, _>>()?)
}
/// Keep the original quarantine evidence until a successful replacement has been validated.
/// The explicit pending ID is retried independently of creation-window checkpoints.
pub fn retry_quarantined(
    conn: &Connection,
    account_id: &str,
    external_id: &str,
    now: i64,
) -> Result<(), AppError> {
    let tx = conn.unchecked_transaction()?;
    // Acquire the SQLite write lock before checking enabled state. Disconnect may not race
    // this local enqueue into re-enabling an observation or creating a substitute T0.
    let enabled = tx.execute(
        "UPDATE statistics_accounts SET generation=generation WHERE id=?1 AND enabled=1",
        [account_id],
    )?;
    if enabled == 0 {
        return Err(AppError::new("disconnected", "Аккаунт не подключён"));
    }
    let raw: Option<String> = tx
        .query_row(
            "SELECT raw_json FROM statistics_quarantine WHERE account_id=?1 AND external_id=?2",
            params![account_id, external_id],
            |r| r.get(0),
        )
        .optional()?;
    let raw = raw.ok_or_else(|| AppError::new("not_found", "Партия в карантине не найдена"))?;
    let value: serde_json::Value = serde_json::from_str(&raw)?;
    if value.get("id").and_then(|id| id.as_str()) != Some(external_id) {
        return Err(AppError::new(
            "data",
            "ID исходной партии не совпадает с карантином",
        ));
    }
    track_pending(&tx, account_id, &value, now, false, Some("manual_retry"))?;
    // Force a fresh metadata export even for a previously non-played/unknown status.
    tx.execute(
        "UPDATE statistics_pending SET status='retry_requested' WHERE account_id=?1 AND external_id=?2",
        params![account_id, external_id],
    )?;
    tx.commit()?;
    Ok(())
}
/// Preserve every distinct source payload, including input that failed legal replay.
fn archive_quarantine(
    conn: &Connection,
    account_id: &str,
    external_id: &str,
    raw: &str,
    reason: &str,
    received_at_ms: i64,
) -> Result<(), AppError> {
    let hash = format!("{:x}", Sha256::digest(raw.as_bytes()));
    conn.execute("INSERT OR IGNORE INTO statistics_quarantine_history(account_id,external_id,payload_hash,raw_json,reason,received_at_ms) SELECT ?1,?2,?3,?4,?5,?6 WHERE NOT EXISTS(SELECT 1 FROM statistics_quarantine_history WHERE account_id=?1 AND external_id=?2 AND raw_json=?4)", params![account_id,external_id,hash,raw,reason,received_at_ms])?;
    Ok(())
}
fn archive_current_quarantine(
    conn: &Connection,
    account_id: &str,
    external_id: &str,
) -> Result<(), AppError> {
    let old: Option<(String,String,i64)> = conn.query_row(
        "SELECT raw_json,reason,received_at_ms FROM statistics_quarantine WHERE account_id=?1 AND external_id=?2",
        params![account_id,external_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional()?;
    if let Some((raw, reason, received)) = old {
        archive_quarantine(conn, account_id, external_id, &raw, &reason, received)?;
    }
    Ok(())
}
pub fn quarantine(
    conn: &Connection,
    id: &str,
    value: &serde_json::Value,
    reason: &str,
    now: i64,
) -> Result<(), AppError> {
    let external = value
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let raw = value.to_string();
    let tx = conn.unchecked_transaction()?;
    archive_current_quarantine(&tx, id, external)?;
    archive_quarantine(&tx, id, external, &raw, reason, now)?;
    tx.execute("INSERT INTO statistics_quarantine(account_id,external_id,received_at_ms,raw_json,reason) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account_id,external_id) DO UPDATE SET received_at_ms=excluded.received_at_ms,raw_json=excluded.raw_json,reason=excluded.reason", params![id,external,now,raw,reason])?;
    tx.commit()?;
    Ok(())
}
pub fn save_game(
    conn: &Connection,
    account_id: &str,
    game: &StoredGame,
    raw_json: &str,
    eligibility: &str,
) -> Result<bool, AppError> {
    let mut game = game.clone();
    game.account_id = account_id.into();
    game.eligibility = eligibility.into();
    let identity = format!("{}:{}", game.source, game.external_id);
    let record = crate::pgn::record(&game.raw_pgn, crate::pgn::tags(&game.raw_pgn));
    let payload_hash = format!(
        "{:x}",
        Sha256::digest(format!("{raw_json}\n{}", game.raw_pgn).as_bytes())
    );
    let canonical_context = serde_json::to_string(&(
        &game.variant,
        &game.replay.initial_fen,
        game.replay
            .nodes
            .iter()
            .filter_map(|n| n.uci.as_deref())
            .collect::<Vec<_>>(),
    ))?;
    let context_hash = format!("{:x}", Sha256::digest(canonical_context.as_bytes()));
    let tx = conn.unchecked_transaction()?;
    tx.execute("INSERT INTO games(identity,metadata,tags,pgn,source,source_kind) VALUES(?1,?2,?3,?4,'lichess-api','\"my_games\"') ON CONFLICT(identity) DO NOTHING",params![identity,serde_json::to_string(&record.metadata)?,serde_json::to_string(&record.tags)?,game.raw_pgn])?;
    let game_id: i64 =
        tx.query_row("SELECT id FROM games WHERE identity=?1", [&identity], |r| {
            r.get(0)
        })?;
    let previous:Option<(String,i64,i64)>=tx.query_row("SELECT movetext_hash,movetext_revision,payload_revision FROM statistics_games WHERE game_id=?1",[game_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let known_payload: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM statistics_payloads WHERE game_id=?1 AND payload_hash=?2)",
        params![game_id, payload_hash],
        |r| r.get(0),
    )?;
    let revision = previous.as_ref().map_or(1, |(hash, revision, _)| {
        revision + if hash == &context_hash { 0 } else { 1 }
    });
    let payload_revision = previous.as_ref().map_or(1, |(_, _, revision)| {
        revision + if known_payload { 0 } else { 1 }
    });
    tx.execute("INSERT INTO statistics_games(game_id,id,source,external_id,data,movetext_hash,movetext_revision,payload_revision,updated_at_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9) ON CONFLICT(game_id) DO UPDATE SET data=excluded.data,movetext_hash=excluded.movetext_hash,movetext_revision=excluded.movetext_revision,payload_revision=excluded.payload_revision,updated_at_ms=excluded.updated_at_ms",params![game_id,game.id,game.source,game.external_id,serde_json::to_string(&game)?,context_hash,revision,payload_revision,now_ms()])?;
    tx.execute("INSERT INTO statistics_account_games(account_id,game_id,user_color,eligibility,evidence) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(account_id,game_id) DO UPDATE SET user_color=excluded.user_color,eligibility=excluded.eligibility,evidence=excluded.evidence",params![account_id,game_id,game.own_color,eligibility,raw_json])?;
    tx.execute("INSERT OR IGNORE INTO statistics_payloads(game_id,payload_hash,payload_revision,movetext_revision,received_at_ms,raw_json,raw_pgn) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![game_id,payload_hash,payload_revision,revision,now_ms(),raw_json,game.raw_pgn])?;
    tx.execute(
        "DELETE FROM statistics_pending WHERE account_id=?1 AND external_id=?2",
        params![account_id, game.external_id],
    )?;
    archive_current_quarantine(&tx, account_id, &game.external_id)?;
    tx.execute(
        "DELETE FROM statistics_quarantine WHERE account_id=?1 AND external_id=?2",
        params![account_id, game.external_id],
    )?;
    tx.commit()?;
    Ok(previous.is_none())
}
pub fn games(conn: &Connection, account_id: Option<&str>) -> Result<Vec<StoredGame>, AppError> {
    let rows=conn.prepare("SELECT sg.data,ag.account_id,ag.user_color,ag.eligibility FROM statistics_games sg JOIN statistics_account_games ag ON ag.game_id=sg.game_id WHERE (?1 IS NULL OR ag.account_id=?1) ORDER BY sg.updated_at_ms DESC")?.query_map([account_id],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?)))?.collect::<Result<Vec<_>,_>>()?;
    rows.into_iter()
        .map(|(data, account, color, eligibility)| {
            let mut game: StoredGame = serde_json::from_str(&data)?;
            game.account_id = account;
            game.own_color = color;
            game.eligibility = eligibility;
            Ok(game)
        })
        .collect()
}
/// A canonical game is analyzable if any account has confirmed eligible membership.
/// Explicit account-specific views continue to use games(conn,Some(account_id)).
pub fn game(conn: &Connection, id: &str) -> Result<Option<StoredGame>, AppError> {
    let data: Option<String> = conn
        .query_row("SELECT data FROM statistics_games WHERE id=?1", [id], |r| {
            r.get(0)
        })
        .optional()?;
    let Some(data) = data else {
        return Ok(None);
    };
    let mut game: StoredGame = serde_json::from_str(&data)?;
    let membership: Option<(String,String,String)> = conn.query_row(
        "SELECT a.account_id,a.user_color,a.eligibility FROM statistics_account_games a JOIN statistics_games g ON g.game_id=a.game_id JOIN statistics_accounts o ON o.id=a.account_id WHERE g.id=?1 ORDER BY CASE a.eligibility WHEN 'eligible' THEN 0 WHEN 'boundary_uncertain' THEN 1 ELSE 2 END,o.enabled DESC,a.account_id LIMIT 1",
        [id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional()?;
    if let Some((account_id, color, eligibility)) = membership {
        game.account_id = account_id;
        game.own_color = color;
        game.eligibility = eligibility;
    } else {
        // No verified ownership may not inherit an eligible flag from serialized source data.
        game.eligibility = "excluded".into();
    }
    Ok(Some(game))
}
pub fn load_game(conn: &Connection, id: &str) -> Result<Option<StoredGame>, AppError> {
    game(conn, id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::SourceKind,
        statistics::{chess, types::*},
        storage,
    };
    fn identity(id: &str, generation: u64) -> VerifiedIdentity {
        VerifiedIdentity {
            id: id.into(),
            username: id.into(),
            generation,
        }
    }
    fn fixture() -> StoredGame {
        StoredGame {
            id: "lichess:corpus01".into(),
            external_id: "corpus01".into(),
            account_id: "lichess:player1".into(),
            source: "lichess".into(),
            white: Participant {
                id: Some("player1".into()),
                name: "Player1".into(),
                ..Default::default()
            },
            black: Participant {
                id: Some("player2".into()),
                name: "Player2".into(),
                ..Default::default()
            },
            own_color: "white".into(),
            result: "1-0".into(),
            status: "resign".into(),
            variant: "standard".into(),
            raw_pgn: "[Site \"https://lichess.org/corpus01\"]\n\n1. e4 e5 1-0".into(),
            eligibility: "eligible".into(),
            replay: chess::replay_pgn("1. e4 e5 1-0").unwrap(),
            ..Default::default()
        }
    }
    #[test]
    fn canonical_eligibility_uses_any_confirmed_membership() {
        let temp = tempfile::tempdir().unwrap();
        let conn = storage::open(&temp.path().join("db.sqlite3")).unwrap();
        observe(&conn, &identity("player1", 1), 1000).unwrap();
        save_game(&conn, "lichess:player1", &fixture(), "{}", "eligible").unwrap();
        observe(&conn, &identity("player2", 2), 2000).unwrap();
        let mut other = fixture();
        other.account_id = "lichess:player2".into();
        other.own_color = "black".into();
        save_game(&conn, "lichess:player2", &other, "{}", "boundary_uncertain").unwrap();
        let canonical = game(&conn, &other.id).unwrap().unwrap();
        assert_eq!(canonical.eligibility, "eligible");
        assert_eq!(canonical.account_id, "lichess:player1");
        assert_eq!(canonical.own_color, "white");
        assert_eq!(
            games(&conn, Some("lichess:player2")).unwrap()[0].eligibility,
            "boundary_uncertain"
        );
        save_game(&conn, "lichess:player1", &fixture(), "{}", "excluded").unwrap();
        assert_eq!(
            game(&conn, &other.id).unwrap().unwrap().eligibility,
            "boundary_uncertain"
        );
    }
    #[test]
    fn corrected_quarantine_keeps_every_original_payload_immutable() {
        let temp = tempfile::tempdir().unwrap();
        let conn = storage::open(&temp.path().join("db.sqlite3")).unwrap();
        observe(&conn, &identity("player1", 1), 1000).unwrap();
        let first = serde_json::json!({"id":"corpus01","pgn":"first invalid source"});
        let second = serde_json::json!({"id":"corpus01","pgn":"second invalid source"});
        quarantine(&conn, "lichess:player1", &first, "invalid_pgn", 2000).unwrap();
        quarantine(&conn, "lichess:player1", &second, "invalid_pgn", 3000).unwrap();
        quarantine(&conn, "lichess:player1", &second, "invalid_pgn", 4000).unwrap();
        save_game(
            &conn,
            "lichess:player1",
            &fixture(),
            "{\"id\":\"corpus01\",\"validated\":true}",
            "eligible",
        )
        .unwrap();
        assert!(quarantined(&conn, None).unwrap().is_empty());
        let history = conn
            .prepare(
                "SELECT raw_json,received_at_ms FROM statistics_quarantine_history ORDER BY id",
            )
            .unwrap()
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            history,
            vec![(first.to_string(), 2000), (second.to_string(), 3000)]
        );
        assert!(
            conn.execute(
                "UPDATE statistics_quarantine_history SET raw_json='erased'",
                []
            )
            .is_err()
        );
        assert!(
            conn.execute("DELETE FROM statistics_quarantine_history", [])
                .is_err()
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM statistics_quarantine_history",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            2
        );
    }
    #[test]
    fn quarantine_retry_preserves_raw_t0_and_requires_active_known_record() {
        let temp = tempfile::tempdir().unwrap();
        let conn = storage::open(&temp.path().join("db.sqlite3")).unwrap();
        let observation = observe(&conn, &identity("player1", 1), 1000).unwrap();
        let raw = serde_json::json!({"id":"badpgn01","createdAt":500,"status":"noStart","pgn":"broken source"});
        quarantine(&conn, &observation.id, &raw, "invalid_pgn", 2000).unwrap();
        assert_eq!(
            sync_status(&conn, Some(&observation.id))
                .unwrap()
                .quarantined_games,
            1
        );
        retry_quarantined(&conn, &observation.id, "badpgn01", 3000).unwrap();
        retry_quarantined(&conn, &observation.id, "badpgn01", 4000).unwrap();
        let retained = quarantined(&conn, Some(&observation.id)).unwrap();
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].raw_json, raw.to_string());
        assert_eq!(retained[0].received_at_ms, 2000);
        assert_eq!(
            account(&conn, &observation.id).unwrap().unwrap().t0_ms,
            1000
        );
        let retry = pending(&conn, &observation.id).unwrap();
        assert_eq!(retry.len(), 1);
        assert_eq!(retry[0].status, "retry_requested");
        assert_eq!(retry[0].raw_metadata, raw.to_string());
        assert_eq!(
            retry_quarantined(&conn, &observation.id, "unknown1", 5000)
                .unwrap_err()
                .code,
            "not_found"
        );
        disable_accounts(&conn).unwrap();
        assert_eq!(
            retry_quarantined(&conn, &observation.id, "badpgn01", 6000)
                .unwrap_err()
                .code,
            "disconnected"
        );
        assert_eq!(quarantined(&conn, None).unwrap().len(), 1);
    }
    #[test]
    fn immutable_t0_accounts_are_independent_and_disconnect_keeps_history() {
        let temp = tempfile::tempdir().unwrap();
        let conn = storage::open(&temp.path().join("db.sqlite3")).unwrap();
        assert_eq!(
            observe(&conn, &identity("player1", 1), 1000).unwrap().t0_ms,
            1000
        );
        save_game(&conn, "lichess:player1", &fixture(), "{}", "eligible").unwrap();
        assert_eq!(
            observe(&conn, &identity("player2", 2), 2000).unwrap().t0_ms,
            2000
        );
        let mut g = fixture();
        g.account_id = "lichess:player2".into();
        g.own_color = "black".into();
        save_game(&conn, "lichess:player2", &g, "{}", "eligible").unwrap();
        assert_eq!(
            games(&conn, Some("lichess:player1")).unwrap()[0].own_color,
            "white"
        );
        assert_eq!(
            games(&conn, Some("lichess:player2")).unwrap()[0].own_color,
            "black"
        );
        assert_eq!(
            observe(&conn, &identity("player1", 3), 9000).unwrap().t0_ms,
            1000
        );
        disable_accounts(&conn).unwrap();
        assert!(active_account(&conn).unwrap().is_none());
        assert_eq!(games(&conn, Some("lichess:player1")).unwrap().len(), 1);
    }
    #[test]
    fn corpus_identity_gains_membership_without_provenance_change() {
        let temp = tempfile::tempdir().unwrap();
        let mut conn = storage::open(&temp.path().join("db.sqlite3")).unwrap();
        let g = fixture();
        let record = crate::pgn::record(&g.raw_pgn, crate::pgn::tags(&g.raw_pgn));
        let file = storage::remember(&conn, Path::new("fixture.pgn"), "pgn").unwrap();
        storage::save_batch(
            &mut conn,
            &[record],
            &[file],
            Path::new("fixture.pgn"),
            SourceKind::LichessCorpus,
        )
        .unwrap();
        observe(&conn, &identity("player1", 1), 1000).unwrap();
        save_game(&conn, "lichess:player1", &g, "{}", "eligible").unwrap();
        save_game(&conn, "lichess:player1", &g, "{}", "eligible").unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM games", [], |r| r.get::<_, usize>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("SELECT source_kind FROM games", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "\"lichess_corpus\""
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM file_games", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM statistics_account_games", [], |r| {
                r.get::<_, usize>(0)
            })
            .unwrap(),
            1
        );
    }
    #[test]
    fn enrichment_keeps_movetext_revision_and_historical_raw_payloads() {
        let temp = tempfile::tempdir().unwrap();
        let conn = storage::open(&temp.path().join("db.sqlite3")).unwrap();
        observe(&conn, &identity("player1", 1), 1000).unwrap();
        let mut g = fixture();
        save_game(&conn, "lichess:player1", &g, "{\"revision\":1}", "eligible").unwrap();
        g.raw_pgn =
            "[Site \"https://lichess.org/corpus01\"]\n\n1. e4 {[%clk 0:05:00]} e5 1-0".into();
        g.replay.nodes[1].clock_seconds = Some(300.);
        save_game(&conn, "lichess:player1", &g, "{\"revision\":2}", "eligible").unwrap();
        assert_eq!(
            conn.query_row("SELECT movetext_revision FROM statistics_games", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            1
        );
        g.replay = chess::replay_pgn("1. d4 d5 1-0").unwrap();
        g.raw_pgn = "1. d4 d5 1-0".into();
        save_game(&conn, "lichess:player1", &g, "{\"revision\":3}", "eligible").unwrap();
        assert_eq!(
            conn.query_row("SELECT movetext_revision FROM statistics_games", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
            2
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM statistics_payloads", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            3
        );
    }
    use std::path::Path;
}

#[cfg(test)]
mod observation_atomic_tests {
    use super::*;
    #[test]
    fn failed_initial_snapshot_rolls_back_observation_and_other_account_state() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(&dir.path().join("db.sqlite3")).unwrap();
        let old = VerifiedIdentity {
            id: "old".into(),
            username: "Old".into(),
            generation: 0,
        };
        observe(&conn, &old, 10).unwrap();
        conn.execute_batch("CREATE TRIGGER fail_initial BEFORE INSERT ON statistics_rating_snapshots BEGIN SELECT RAISE(ABORT,'synthetic snapshot failure'); END;").unwrap();
        let new = VerifiedIdentity {
            id: "new".into(),
            username: "New".into(),
            generation: 1,
        };
        assert!(observe_with_ratings(&conn, &new, 20, &[("bullet", 1550)]).is_err());
        assert!(account(&conn, "lichess:new").unwrap().is_none());
        assert!(account(&conn, "lichess:old").unwrap().unwrap().enabled);
    }
}

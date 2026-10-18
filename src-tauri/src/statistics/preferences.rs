//! User settings and an explicit new observation period. Primary chess evidence is never erased.
use super::store;
use crate::{domain::AppError, storage};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

const DEPTH_KEY: &str = "statistics.opening_coverage_depth_moves";
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatisticsPreferences {
    pub opening_coverage_depth_moves: u32,
}
impl Default for StatisticsPreferences {
    fn default() -> Self {
        Self {
            opening_coverage_depth_moves: 15,
        }
    }
}
pub fn load(conn: &Connection) -> Result<StatisticsPreferences, AppError> {
    let depth = storage::setting(conn, DEPTH_KEY)?
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| (1..=40).contains(value))
        .unwrap_or(15);
    Ok(StatisticsPreferences {
        opening_coverage_depth_moves: depth,
    })
}
pub fn save(
    conn: &Connection,
    preferences: StatisticsPreferences,
) -> Result<StatisticsPreferences, AppError> {
    if !(1..=40).contains(&preferences.opening_coverage_depth_moves) {
        return Err(AppError::new(
            "validation",
            "Глубина покрытия должна быть от 1 до 40 ходов",
        ));
    }
    storage::set_setting(
        conn,
        DEPTH_KEY,
        &preferences.opening_coverage_depth_moves.to_string(),
    )?;
    Ok(preferences)
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResetResult {
    pub account_id: String,
    pub t0_ms: i64,
    pub removed_games: usize,
}
/// The desktop runtime serializes this operation with sync. Keeping the canonical game, raw
/// payloads, graph index and engine runs allows later explicit import without rebuilding facts.
pub fn reset(conn: &Connection, account_id: &str, now: i64) -> Result<ResetResult, AppError> {
    let tx = conn.unchecked_transaction()?;
    // Acquire a write lock before reading the observation and calculating the new boundary.
    if tx.execute(
        "UPDATE statistics_accounts SET generation=generation WHERE id=?1",
        [account_id],
    )? == 0
    {
        return Err(AppError::new("not_found", "Аккаунт статистики не найден"));
    }
    let observation = store::account(&tx, account_id)?
        .ok_or_else(|| AppError::new("not_found", "Аккаунт статистики не найден"))?;
    let t0_ms = now.max(observation.t0_ms.saturating_add(1));
    let removed_games = tx.execute(
        "DELETE FROM statistics_account_games WHERE account_id=?1",
        [account_id],
    )?;
    tx.execute(
        "DELETE FROM statistics_sync_ranges WHERE account_id=?1",
        [account_id],
    )?;
    tx.execute(
        "DELETE FROM statistics_pending WHERE account_id=?1",
        [account_id],
    )?;
    // Current quarantine is a queue projection. Immutable quarantine_history keeps the source.
    tx.execute(
        "DELETE FROM statistics_quarantine WHERE account_id=?1",
        [account_id],
    )?;
    tx.execute(
        "UPDATE statistics_accounts SET t0_ms=?2 WHERE id=?1",
        params![account_id, t0_ms],
    )?;
    tx.execute(
        "DELETE FROM settings WHERE key=?1",
        [format!("game_graph_view:{account_id}")],
    )?;
    tx.execute("INSERT INTO statistics_sync_state(account_id,highwater_ms,reconciliation_cursor_ms,status) VALUES(?1,?2,?2,?3) ON CONFLICT(account_id) DO UPDATE SET highwater_ms=excluded.highwater_ms,reconciliation_cursor_ms=excluded.reconciliation_cursor_ms,last_success_ms=NULL,retry_not_before_ms=NULL,reconciliation_upper_ms=NULL,last_reconciled_ms=NULL,baseline_complete=0,boundary_uncertain=1,status=excluded.status,error_code=NULL,error_message=NULL,attempted_at_ms=NULL", params![account_id, t0_ms, if observation.enabled { "pending" } else { "disconnected" }])?;
    tx.commit()?;
    Ok(ResetResult {
        account_id: account_id.into(),
        t0_ms,
        removed_games,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        lichess::VerifiedIdentity,
        statistics::{chess, types::StoredGame},
    };
    fn observe(conn: &Connection, name: &str, now: i64) {
        store::observe(
            conn,
            &VerifiedIdentity {
                id: name.into(),
                username: name.into(),
                generation: 7,
            },
            now,
        )
        .unwrap();
    }
    fn game() -> StoredGame {
        StoredGame {
            id: "lichess:reset001".into(),
            external_id: "reset001".into(),
            source: "lichess".into(),
            raw_pgn: "1. e4 e5 1-0".into(),
            own_color: "white".into(),
            result: "1-0".into(),
            variant: "standard".into(),
            status: "resign".into(),
            eligibility: "eligible".into(),
            replay: chess::replay_pgn("1. e4 e5 1-0").unwrap(),
            ..Default::default()
        }
    }
    fn dump(conn: &Connection, table: &str) -> Vec<Vec<rusqlite::types::Value>> {
        let mut query = conn
            .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
            .unwrap();
        let columns = query.column_count();
        query
            .query_map([], |row| (0..columns).map(|index| row.get(index)).collect())
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
    #[test]
    fn depth_is_validated_persisted_and_independent_of_engine_settings() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db.sqlite3");
        let conn = storage::open(&path).unwrap();
        assert_eq!(load(&conn).unwrap().opening_coverage_depth_moves, 15);
        storage::set_setting(&conn, "theme", "dark").unwrap();
        let engine = dump(&conn, "statistics_engine_state");
        for depth in [1, 40, 19] {
            save(
                &conn,
                StatisticsPreferences {
                    opening_coverage_depth_moves: depth,
                },
            )
            .unwrap();
        }
        for depth in [0, 41, u32::MAX] {
            assert_eq!(
                save(
                    &conn,
                    StatisticsPreferences {
                        opening_coverage_depth_moves: depth
                    }
                )
                .unwrap_err()
                .code,
                "validation"
            );
        }
        drop(conn);
        let conn = storage::open(&path).unwrap();
        assert_eq!(load(&conn).unwrap().opening_coverage_depth_moves, 19);
        assert_eq!(
            storage::setting(&conn, "theme").unwrap().as_deref(),
            Some("dark")
        );
        assert_eq!(dump(&conn, "statistics_engine_state"), engine);
        for invalid in ["0", "41", "bad"] {
            storage::set_setting(&conn, DEPTH_KEY, invalid).unwrap();
            assert_eq!(load(&conn).unwrap().opening_coverage_depth_moves, 15);
        }
    }
    #[test]
    fn reset_only_detaches_selected_cohort_and_keeps_all_chess_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("db.sqlite3");
        let conn = storage::open(&path).unwrap();
        observe(&conn, "first", 1000);
        store::save_game(
            &conn,
            "lichess:first",
            &game(),
            "{\"source\":1}",
            "eligible",
        )
        .unwrap();
        observe(&conn, "second", 2000);
        let mut second = game();
        second.own_color = "black".into();
        store::save_game(
            &conn,
            "lichess:second",
            &second,
            "{\"source\":1}",
            "eligible",
        )
        .unwrap();
        store::save_initial_rating(&conn, "lichess:first", "blitz", 1500, 1000).unwrap();
        let pending = serde_json::json!({"id":"active01", "createdAt":500, "status":"started"});
        for account in ["lichess:first", "lichess:second"] {
            store::track_pending(&conn, account, &pending, 3000, true, None).unwrap();
            store::quarantine(
                &conn,
                account,
                &serde_json::json!({"id":"bad00001"}),
                "invalid_pgn",
                3000,
            )
            .unwrap();
            store::add_range(&conn, account, 1000, 3000, "discovery").unwrap();
        }
        conn.execute("UPDATE statistics_sync_state SET status='error',last_success_ms=3000,retry_not_before_ms=4000,last_reconciled_ms=3000,baseline_complete=1,boundary_uncertain=0,error_code='network',error_message='offline',attempted_at_ms=3000,reconciliation_upper_ms=3500", []).unwrap();
        conn.execute("INSERT INTO statistics_analysis_runs(id,game_id,movetext_revision,profile_id,state,data_json,completed_at_ms) VALUES('run1','lichess:reset001',1,'profile1','succeeded','{}',3000)", []).unwrap();
        conn.execute("INSERT INTO statistics_engine_observations VALUES('cache1','context','{}','info',3000)", []).unwrap();
        conn.execute("INSERT INTO statistics_move_evaluations(run_id,ply,best_cache_key,status) VALUES('run1',1,'cache1','complete')", []).unwrap();
        conn.execute("INSERT INTO statistics_analysis_jobs(game_id,movetext_revision,profile_id,config_json,identity_json,state,total,run_id,created_at_ms) VALUES('lichess:reset001',1,'profile1','{}','{}','running',3,'running1',3000)", []).unwrap();
        let document = crate::knowledge::import(
            &conn,
            crate::knowledge::ImportRequest {
                title: "Prepared lines".into(),
                side: "white".into(),
                pgn: "1. e4 e5 (1... c5) *".into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        crate::knowledge::import(
            &conn,
            crate::knowledge::ImportRequest {
                title: "Prepared lines".into(),
                side: "white".into(),
                pgn: "1. d4 d5 *".into(),
                document_id: Some(document.id.clone()),
                expected_revision: Some(document.revision),
            },
        )
        .unwrap();
        crate::game_graph::save_state(
            &conn,
            Some("lichess:first"),
            &serde_json::json!({"history":["old"]}),
        )
        .unwrap();
        crate::game_graph::save_state(
            &conn,
            Some("lichess:second"),
            &serde_json::json!({"history":["other"]}),
        )
        .unwrap();
        let preserved_tables = [
            "games",
            "statistics_games",
            "statistics_payloads",
            "statistics_analysis_jobs",
            "statistics_analysis_runs",
            "statistics_engine_observations",
            "statistics_move_evaluations",
            "statistics_engine_state",
            "game_graph_games",
            "game_occurrences",
            "statistics_quarantine_history",
            "statistics_rating_snapshots",
            "knowledge_documents",
            "knowledge_document_versions",
            "knowledge_node_index",
        ];
        let before: Vec<_> = preserved_tables
            .iter()
            .map(|table| dump(&conn, table))
            .collect();
        let other_account = store::account(&conn, "lichess:second").unwrap().unwrap();
        let other_sync =
            serde_json::to_value(store::sync_status(&conn, Some("lichess:second")).unwrap())
                .unwrap();
        let first = store::account(&conn, "lichess:first").unwrap().unwrap();
        let result = reset(&conn, "lichess:first", 10_000).unwrap();
        assert_eq!(result.removed_games, 1);
        assert_eq!(result.t0_ms, 10_000);
        assert_eq!(
            crate::game_graph::load_state(&conn, Some("lichess:first")).unwrap(),
            None
        );
        assert_eq!(
            crate::game_graph::load_state(&conn, Some("lichess:second")).unwrap(),
            Some(serde_json::json!({"history":["other"]}))
        );
        assert!(
            store::games(&conn, Some("lichess:first"))
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            store::games(&conn, Some("lichess:second")).unwrap().len(),
            1
        );
        assert_eq!(
            serde_json::to_value(store::account(&conn, "lichess:second").unwrap()).unwrap(),
            serde_json::to_value(Some(other_account)).unwrap()
        );
        assert_eq!(
            serde_json::to_value(store::sync_status(&conn, Some("lichess:second")).unwrap())
                .unwrap(),
            other_sync
        );
        let current = store::account(&conn, "lichess:first").unwrap().unwrap();
        assert_eq!(current.enabled, first.enabled);
        assert_eq!(current.generation, first.generation);
        let status = store::sync_status(&conn, Some("lichess:first")).unwrap();
        assert_eq!(status.status, "disconnected");
        assert_eq!(status.highwater_ms, Some(10_000));
        assert_eq!(
            status.pending_games + status.pending_ranges + status.quarantined_games,
            0
        );
        assert!(!status.baseline_complete && status.boundary_uncertain);
        assert!(
            status.last_success_ms.is_none()
                && status.last_reconciled_ms.is_none()
                && status.retry_not_before_ms.is_none()
                && status.error_code.is_none()
        );
        for (table, previous) in preserved_tables.iter().zip(before.iter()) {
            assert_eq!(&dump(&conn, table), previous, "{table} changed");
        }
        assert_eq!(
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |row| {
                row.get::<_, usize>(0)
            })
            .unwrap(),
            0
        );
        drop(conn);
        let conn = storage::open(&path).unwrap();
        observe(&conn, "first", 20_000);
        assert_eq!(
            store::account(&conn, "lichess:first")
                .unwrap()
                .unwrap()
                .t0_ms,
            10_000
        );
        assert!(
            store::games(&conn, Some("lichess:first"))
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn invalid_account_cannot_erase_any_cohort_and_failed_reset_rolls_back() {
        let temp = tempfile::tempdir().unwrap();
        let conn = storage::open(&temp.path().join("db.sqlite3")).unwrap();
        observe(&conn, "first", 1000);
        store::save_game(&conn, "lichess:first", &game(), "{}", "eligible").unwrap();
        assert_eq!(reset(&conn, "missing", 2000).unwrap_err().code, "not_found");
        conn.execute_batch("CREATE TRIGGER reject_reset BEFORE UPDATE OF t0_ms ON statistics_accounts BEGIN SELECT RAISE(ABORT,'synthetic reset failure'); END;").unwrap();
        assert!(reset(&conn, "lichess:first", 2000).is_err());
        assert_eq!(store::games(&conn, Some("lichess:first")).unwrap().len(), 1);
        assert_eq!(
            store::account(&conn, "lichess:first")
                .unwrap()
                .unwrap()
                .t0_ms,
            1000
        );
    }
}

//! Small, read-only opening results projection; no PGN trees or engine runs cross IPC.
use super::{
    chess,
    opening_coverage::{OpeningCoverage, PreparedChoices},
    preferences, store,
};
use crate::domain::AppError;
use rusqlite::Connection;
use serde::Serialize;
use shakmaty::{CastlingMode, Chess, fen::Fen};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Serialize)]
pub struct OpeningSummaryRow {
    pub eco: Option<String>,
    pub name: Option<String>,
    pub color: String,
    pub games: usize,
    pub wins: usize,
    pub draws: usize,
    pub losses: usize,
    pub score: f64,
    /// Exact membership: similarly named openings never bleed into this row's graph.
    pub game_ids: Vec<String>,
    pub preview_fen: Option<String>,
    pub preview_last_move: Option<[String; 2]>,
    pub preview_pgn: Option<String>,
    pub preview_source: Option<String>,
    pub coverage: OpeningCoverage,
}
#[derive(Debug, Serialize)]
pub struct OpeningSummary {
    pub accounts: Vec<store::AccountObservation>,
    pub active_account_id: Option<String>,
    pub selected_account_id: Option<String>,
    pub color: String,
    pub total_games: usize,
    pub wins: usize,
    pub draws: usize,
    pub losses: usize,
    pub score: Option<f64>,
    pub coverage_depth_moves: u32,
    pub coverage_version: &'static str,
    pub rows: Vec<OpeningSummaryRow>,
}

fn last_move(uci: Option<&str>) -> Option<[String; 2]> {
    let uci = uci?;
    (uci.is_ascii() && uci.len() >= 4).then(|| [uci[..2].to_owned(), uci[2..4].to_owned()])
}
fn observed_preview(fen: Option<&str>, expected_key: Option<&str>) -> Option<String> {
    let fen = fen?;
    let pos: Chess = fen
        .parse::<Fen>()
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()?;
    (Some(chess::position_key(&pos).as_str()) == expected_key).then(|| fen.to_owned())
}

pub fn query(
    conn: &Connection,
    account_id: Option<&str>,
    color: &str,
) -> Result<OpeningSummary, AppError> {
    // All groups, prepared revisions and factual occurrences belong to one
    // SQLite read snapshot, even while synchronization or editing commits.
    let transaction = if conn.is_autocommit() {
        Some(conn.unchecked_transaction()?)
    } else {
        None
    };
    let summary = query_snapshot(conn, account_id, color)?;
    if let Some(transaction) = transaction {
        transaction.commit()?;
    }
    Ok(summary)
}
fn query_snapshot(
    conn: &Connection,
    account_id: Option<&str>,
    color: &str,
) -> Result<OpeningSummary, AppError> {
    if !matches!(color, "white" | "black") {
        return Err(AppError::new("validation", "Выберите белые или чёрные"));
    }
    let depth = preferences::load(conn)?.opening_coverage_depth_moves;
    let prepared = PreparedChoices::load(conn, color)?;
    let accounts = store::accounts(conn)?;
    let active_account_id = store::active_account(conn)?.map(|a| a.id);
    let selected_account_id = account_id
        .map(str::to_owned)
        .or_else(|| active_account_id.clone())
        .or_else(|| accounts.first().map(|a| a.id.clone()));
    // Same completed/eligible standard-game cohort as StatisticsFilter. Colour
    // comes from membership, not a canonical game's serialized player colour.
    let mut statement = conn.prepare(
        "SELECT sg.id,nullif(json_extract(sg.data,'$.replay.opening.eco'),''),
                nullif(json_extract(sg.data,'$.replay.opening.name'),''),json_extract(sg.data,'$.result'),
                CASE WHEN json_extract(sg.data,'$.replay.opening.ply')>=0 THEN
                    json_extract(sg.data,'$.replay.nodes['||json_extract(sg.data,'$.replay.opening.ply')||'].fen') END,
                CASE WHEN json_extract(sg.data,'$.replay.opening.ply')>=0 THEN
                    json_extract(sg.data,'$.replay.nodes['||json_extract(sg.data,'$.replay.opening.ply')||'].uci') END,
                json_extract(sg.data,'$.replay.opening.position_key')
         FROM statistics_games sg JOIN statistics_account_games ag ON ag.game_id=sg.game_id
         WHERE ag.account_id=?1 AND ag.user_color=?2 AND ag.eligibility='eligible'
           AND json_extract(sg.data,'$.variant')='standard'
           AND json_extract(sg.data,'$.result') IN ('1-0','0-1','1/2-1/2')
           AND json_array_length(json_extract(sg.data,'$.replay.nodes'))>=2
         ORDER BY sg.id",
    )?;
    let games = statement.query_map(rusqlite::params![selected_account_id, color], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, Option<String>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    let mut grouped = BTreeMap::new();
    let mut total_games = 0;
    for game in games {
        let (id, eco, name, result, fallback_fen, fallback_uci, fallback_key) = game?;
        let row = grouped
            .entry((eco.clone(), name.clone()))
            .or_insert_with(|| {
                let preview = eco
                    .as_deref()
                    .zip(name.as_deref())
                    .and_then(|(eco, name)| chess::opening_preview(eco, name));
                let preview_fen = preview.as_ref().map(|preview| preview.fen.clone());
                let preview_last_move = preview
                    .as_ref()
                    .and_then(|preview| preview.last_move.clone());
                let preview_pgn = preview.as_ref().map(|preview| preview.pgn.clone());
                OpeningSummaryRow {
                    eco,
                    name,
                    color: color.to_owned(),
                    games: 0,
                    wins: 0,
                    draws: 0,
                    losses: 0,
                    score: 0.,
                    game_ids: Vec::new(),
                    preview_source: preview_fen
                        .as_ref()
                        .map(|_| "lichess-chess-openings".to_owned()),
                    preview_fen,
                    preview_last_move,
                    preview_pgn,
                    coverage: OpeningCoverage::summarize(
                        &[],
                        &HashMap::new(),
                        depth,
                        prepared.documents,
                    ),
                }
            });
        if row.preview_fen.is_none() {
            row.preview_fen = observed_preview(fallback_fen.as_deref(), fallback_key.as_deref());
            if row.preview_fen.is_some() {
                row.preview_last_move = last_move(fallback_uci.as_deref());
                row.preview_source = Some("observed-classified-position".into());
            }
        }
        row.games += 1;
        total_games += 1;
        if result == "1/2-1/2" {
            row.draws += 1;
        } else if (result == "1-0") == (color == "white") {
            row.wins += 1;
        } else {
            row.losses += 1;
        }
        row.game_ids.push(id);
    }
    let game_ids = grouped
        .values()
        .flat_map(|row| row.game_ids.iter().cloned())
        .collect::<Vec<_>>();
    let counts = prepared.by_game(conn, &game_ids, color, depth)?;
    let rows: Vec<OpeningSummaryRow> = grouped
        .into_values()
        .map(|mut row| {
            row.score = 100. * (row.wins as f64 + row.draws as f64 * 0.5) / row.games as f64;
            row.coverage =
                OpeningCoverage::summarize(&row.game_ids, &counts, depth, prepared.documents);
            row
        })
        .collect();
    let wins = rows.iter().map(|row| row.wins).sum::<usize>();
    let draws = rows.iter().map(|row| row.draws).sum::<usize>();
    let losses = rows.iter().map(|row| row.losses).sum::<usize>();
    let score =
        (total_games > 0).then(|| 100. * (wins as f64 + draws as f64 * 0.5) / total_games as f64);
    Ok(OpeningSummary {
        accounts,
        active_account_id,
        selected_account_id,
        color: color.to_owned(),
        total_games,
        wins,
        draws,
        losses,
        score,
        coverage_depth_moves: depth,
        coverage_version: super::opening_coverage::VERSION,
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statistics::types::{OpeningMatch, Replay, ReplayNode, StoredGame};
    use rusqlite::params;
    #[test]
    fn results_use_membership_colour_exact_openings_and_confirmed_finished_cohort() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(&dir.path().join("test.sqlite3")).unwrap();
        conn.execute("INSERT INTO statistics_accounts(id,platform,external_id,username,t0_ms) VALUES('a','lichess','a','A',0),('b','lichess','b','B',0)", []).unwrap();
        for i in 0..8 {
            let id = format!("g{i}");
            let game = StoredGame {
                id: id.clone(),
                own_color: "white".into(),
                variant: if i == 7 { "chess960" } else { "standard" }.into(),
                result: match i {
                    0 | 4 | 6 | 7 => "0-1",
                    1 | 3 => "1-0",
                    2 => "1/2-1/2",
                    _ => "*",
                }
                .into(),
                replay: Replay {
                    nodes: vec![ReplayNode::default(); 2],
                    opening: (i != 3).then(|| OpeningMatch {
                        eco: "B20".into(),
                        name: if i == 4 {
                            "Sicilian Defense: Wing Gambit"
                        } else {
                            "Sicilian Defense"
                        }
                        .into(),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ..Default::default()
            };
            conn.execute("INSERT INTO games(id,identity,metadata,tags,pgn,source,source_kind) VALUES(?1,?2,'{}','{}','','','local')",params![i+1,id]).unwrap();
            conn.execute("INSERT INTO statistics_games(game_id,id,source,external_id,data,movetext_hash,updated_at_ms) VALUES(?1,?2,'lichess',?2,?3,'hash',?1)",params![i+1,id,serde_json::to_string(&game).unwrap()]).unwrap();
            conn.execute("INSERT INTO statistics_account_games(account_id,game_id,user_color,eligibility,evidence) VALUES('a',?1,?2,?3,'{}')",params![i+1,if i==4 {"white"}else{"black"},if i==6 {"boundary_uncertain"}else{"eligible"}]).unwrap();
            conn.execute("INSERT INTO statistics_account_games(account_id,game_id,user_color,eligibility,evidence) VALUES('b',?1,'white','eligible','{}')",[i+1]).unwrap();
        }
        let changes_before = conn.total_changes();
        let black = query(&conn, Some("a"), "black").unwrap();
        assert_eq!(black.total_games, 4);
        assert_eq!(
            (black.wins, black.draws, black.losses, black.score),
            (1, 1, 2, Some(37.5))
        );
        let row = black
            .rows
            .iter()
            .find(|r| r.name.as_deref() == Some("Sicilian Defense"))
            .unwrap();
        assert_eq!((row.games, row.wins, row.draws, row.losses), (3, 1, 1, 1));
        assert_eq!(row.score, 50.);
        assert_eq!(row.game_ids, vec!["g0", "g1", "g2"]);
        let unknown = black.rows.iter().find(|r| r.name.is_none()).unwrap();
        assert_eq!((unknown.games, unknown.losses), (1, 1));
        let white = query(&conn, Some("a"), "white").unwrap();
        assert_eq!((white.total_games, white.rows[0].losses), (1, 1));
        assert_eq!(query(&conn, Some("b"), "white").unwrap().total_games, 6);
        assert_eq!(
            query(&conn, Some("unknown"), "black").unwrap().total_games,
            0
        );
        assert!(query(&conn, None, "both").is_err());
        let encoded = serde_json::to_string(&black).unwrap();
        assert!(!encoded.contains("replay"));
        assert!(!encoded.contains("evaluations"));
        assert!(encoded.len() < 4096);
        assert!(!encoded.contains("material"));
        assert_eq!(row.color, "black");
        assert_eq!(white.rows[0].color, "white");
        assert_eq!(conn.total_changes(), changes_before);
    }
    #[test]
    fn same_opening_has_independent_side_rows_coverage_and_canonical_board_previews() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::storage::open(&dir.path().join("sides.sqlite3")).unwrap();
        conn.execute("INSERT INTO statistics_accounts(id,platform,external_id,username,t0_ms) VALUES('a','lichess','a','A',0)", []).unwrap();
        let mut replay = chess::replay_pgn("1.e4 c5 1-0").unwrap();
        replay.opening = chess::classify_opening(&replay);
        for (game_id, id, color) in [(1, "white-example", "white"), (2, "black-example", "black")] {
            let game = StoredGame {
                id: id.into(),
                own_color: "white".into(),
                variant: "standard".into(),
                result: "1-0".into(),
                replay: replay.clone(),
                ..Default::default()
            };
            conn.execute("INSERT INTO games(id,identity,metadata,tags,pgn,source,source_kind) VALUES(?1,?2,'{}','{}','','','local')",params![game_id,id]).unwrap();
            conn.execute("INSERT INTO statistics_games(game_id,id,source,external_id,data,movetext_hash,updated_at_ms) VALUES(?1,?2,'lichess',?2,?3,'hash',1)",params![game_id,id,serde_json::to_string(&game).unwrap()]).unwrap();
            conn.execute("INSERT INTO statistics_account_games(account_id,game_id,user_color,eligibility,evidence) VALUES('a',?1,?2,'eligible','{}')",params![game_id,color]).unwrap();
        }
        let doc = crate::knowledge::import(
            &conn,
            crate::knowledge::ImportRequest {
                title: "White Sicilian".into(),
                side: "white".into(),
                pgn: "1.e4 c5 *".into(),
                document_id: None,
                expected_revision: None,
            },
        )
        .unwrap();
        crate::knowledge::fragments::set_attachment(&conn, &doc.id, doc.revision, true).unwrap();
        let before = conn.total_changes();
        let white = query(&conn, Some("a"), "white").unwrap();
        let black = query(&conn, Some("a"), "black").unwrap();
        assert_eq!(conn.total_changes(), before);
        let (w, b) = (&white.rows[0], &black.rows[0]);
        assert_eq!((&w.eco, &w.name), (&b.eco, &b.name));
        assert_eq!((w.color.as_str(), b.color.as_str()), ("white", "black"));
        assert_eq!(
            (&w.game_ids, &b.game_ids),
            (
                &vec!["white-example".to_string()],
                &vec!["black-example".to_string()]
            )
        );
        assert_eq!((w.score, b.score), (100., 0.));
        assert_eq!(
            (w.coverage.percent, b.coverage.percent),
            (Some(100.), Some(0.))
        );
        assert_eq!(w.preview_fen.as_deref(), Some(replay.nodes[2].fen.as_str()));
        assert_eq!(w.preview_source.as_deref(), Some("lichess-chess-openings"));
        assert_eq!(
            w.preview_last_move,
            Some(["c7".to_owned(), "c5".to_owned()])
        );
        assert_eq!(w.preview_pgn.as_deref(), Some("1. e4 c5"));
        assert_eq!(white.coverage_depth_moves, 15);
        preferences::save(
            &conn,
            preferences::StatisticsPreferences {
                opening_coverage_depth_moves: 1,
            },
        )
        .unwrap();
        assert_eq!(
            query(&conn, Some("a"), "white").unwrap().rows[0]
                .coverage
                .depth_moves,
            1
        );
    }
    #[test]
    fn observed_preview_requires_the_same_legal_classified_position() {
        let replay = chess::replay_pgn("1.e4 c5 1-0").unwrap();
        let n = &replay.nodes[1];
        assert_eq!(
            observed_preview(Some(&n.fen), Some(&n.position_key)),
            Some(n.fen.clone())
        );
        assert!(observed_preview(Some(&n.fen), Some(&replay.nodes[2].position_key)).is_none());
        assert!(observed_preview(Some("broken"), Some(&n.position_key)).is_none());
        assert!(chess::opening_preview("Z00", "Unknown provider label").is_none());
    }
}

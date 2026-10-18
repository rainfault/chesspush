//! Reproducible synthetic-only graph benchmark. Uses a new temporary DB, never user data.
//! cargo run --release --no-default-features --example game_graph_benchmark -- 1000 10000
use chesspush_lib::{
    game_graph::{self, GraphRequest},
    lichess::VerifiedIdentity,
    statistics::{chess, store, types::*},
    storage,
};
use shakmaty::{Chess, Position, san::SanPlus};
use std::time::Instant;
fn fixture(index: usize) -> StoredGame {
    let mut pos = Chess::default();
    let mut state = (index as u64 + 1).wrapping_mul(0x9e3779b97f4a7c15);
    let mut san = Vec::new();
    for ply in 0..60 {
        let legal = pos.legal_moves();
        if legal.is_empty() {
            break;
        }
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let mv = legal[state as usize % legal.len()];
        if ply % 2 == 0 {
            san.push(format!("{}.", ply / 2 + 1));
        }
        san.push(SanPlus::from_move(pos.clone(), mv).to_string());
        pos.play_unchecked(mv);
    }
    let pgn = format!(
        "[Event \"SYNTHETIC GRAPH BENCHMARK\"]\n[White \"SyntheticGraph\"]\n[Black \"SyntheticOpponent\"]\n[Result \"1/2-1/2\"]\n\n{} 1/2-1/2",
        san.join(" ")
    );
    StoredGame {
        id: format!("lichess:synthetic{index:08}"),
        external_id: format!("synthetic{index:08}"),
        account_id: "lichess:syntheticgraph".into(),
        source: "lichess".into(),
        started_at_ms: Some(2000 + index as i64),
        last_move_at_ms: Some(3000 + index as i64),
        white: Participant {
            name: "SyntheticGraph".into(),
            id: Some("syntheticgraph".into()),
            rating: Some(1500),
            ..Default::default()
        },
        black: Participant {
            name: "SyntheticOpponent".into(),
            id: Some("syntheticopponent".into()),
            rating: Some(1500),
            ..Default::default()
        },
        own_color: "white".into(),
        result: "1/2-1/2".into(),
        status: "draw".into(),
        speed: "rapid".into(),
        rated: Some(true),
        variant: "standard".into(),
        initial_seconds: Some(600.),
        increment_seconds: Some(5.),
        replay: chess::replay_pgn(&pgn).unwrap(),
        raw_pgn: pgn,
        eligibility: "eligible".into(),
        ..Default::default()
    }
}
fn main() {
    let sizes = std::env::args()
        .skip(1)
        .map(|s| s.parse::<usize>().expect("size"))
        .collect::<Vec<_>>();
    let sizes = if sizes.is_empty() {
        vec![1000, 10000]
    } else {
        sizes
    };
    let temp = tempfile::tempdir().unwrap();
    let path = if let Some(value) = std::env::var_os("CHESSPUSH_GRAPH_BENCH_DB") {
        let requested = std::path::PathBuf::from(value);
        let qa = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join(".qa")
            .canonicalize()
            .expect(".qa directory must exist");
        let parent = requested
            .parent()
            .expect("benchmark DB parent required")
            .canonicalize()
            .expect("benchmark parent must exist");
        assert!(
            parent.starts_with(&qa),
            "persistent synthetic data must stay inside canonical .qa"
        );
        if requested.exists() {
            assert!(
                requested.canonicalize().unwrap().starts_with(&qa),
                "benchmark DB resolves outside .qa"
            );
        }
        let path = parent.join(requested.file_name().expect("benchmark filename required"));
        validate_existing_fixture(&path);
        path
    } else {
        temp.path().join("synthetic-graph.sqlite3")
    };
    let conn = storage::open(&path).unwrap();
    store::observe(
        &conn,
        &VerifiedIdentity {
            id: "syntheticgraph".into(),
            username: "SyntheticGraph".into(),
            generation: 1,
        },
        1000,
    )
    .unwrap();
    let mut total: usize = conn
        .query_row("SELECT count(*) FROM game_graph_games", [], |r| r.get(0))
        .unwrap();
    for size in sizes {
        let start = Instant::now();
        let mut save_ms = 0.;
        for i in total..size {
            let game = fixture(i);
            let ingest = Instant::now();
            store::save_game(
                &conn,
                "lichess:syntheticgraph",
                &game,
                &format!("{{\"fixture\":true,\"id\":\"{}\"}}", game.external_id),
                "eligible",
            )
            .unwrap();
            save_ms += ingest.elapsed().as_secs_f64() * 1000.;
        }
        let ingest_total_ms = start.elapsed().as_secs_f64() * 1000.;
        total = total.max(size);
        let request = GraphRequest {
            filter: StatisticsFilter {
                account_id: Some("lichess:syntheticgraph".into()),
                ..Default::default()
            },
            max_nodes: Some(120),
            ..Default::default()
        };
        let mut query_ms = Vec::new();
        let mut projection = None;
        let mut cold_ms = 0.;
        for sample in 0..6 {
            let query = Instant::now();
            projection = Some(game_graph::query(&conn, &request).unwrap());
            let ms = query.elapsed().as_secs_f64() * 1000.;
            if sample == 0 {
                cold_ms = ms;
            } else {
                query_ms.push(ms);
            }
        }
        let graph = projection.unwrap();
        let occurrences: i64 = conn
            .query_row("SELECT count(*) FROM game_occurrences", [], |r| r.get(0))
            .unwrap();
        let positions: i64 = conn
            .query_row("SELECT count(*) FROM chess_positions", [], |r| r.get(0))
            .unwrap();
        let bytes = serde_json::to_vec(&graph).unwrap().len();
        query_ms.sort_by(f64::total_cmp);
        println!(
            "{}",
            serde_json::json!({"fixture":"synthetic_only","games":size,"positions":positions,"occurrences":occurrences,
            "source_replay_and_ingest_ms":ingest_total_ms,"transactional_save_and_index_ms":save_ms,"cold_query_ms":cold_ms,"query_p50_ms":query_ms[2],"query_max_ms":query_ms[4],
            "render_nodes":graph.nodes.len(),"render_edges":graph.edges.len(),"projection_bytes":bytes,"hidden_branches":graph.hidden_nodes})
        );
    }
}

fn validate_existing_fixture(path: &std::path::Path) {
    if !path.exists() {
        return;
    }
    let conn =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("read synthetic fixture");
    let tables: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    if tables == 0 {
        return;
    }
    let has_accounts:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='statistics_accounts')",[],|r|r.get(0)).unwrap();
    assert!(
        has_accounts,
        "existing database is not a synthetic graph fixture"
    );
    let account_count: i64 = conn
        .query_row("SELECT count(*) FROM statistics_accounts", [], |r| r.get(0))
        .unwrap();
    let game_count: i64 = conn
        .query_row("SELECT count(*) FROM games", [], |r| r.get(0))
        .unwrap();
    if account_count == 0 && game_count == 0 {
        return;
    }
    let foreign:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM statistics_accounts WHERE id<>'lichess:syntheticgraph') OR EXISTS(SELECT 1 FROM games WHERE identity NOT LIKE 'lichess:synthetic%' OR instr(pgn,'[Event \"SYNTHETIC GRAPH BENCHMARK\"]')=0)",[],|r|r.get(0)).unwrap();
    assert!(
        account_count == 1 && !foreign,
        "refusing to modify a database outside the SyntheticGraph fixture cohort"
    );
}

//! Reproducible Statistics benchmark. Writes only inside an explicit ignored .qa directory.
use chesspush_lib::{
    lichess::VerifiedIdentity,
    statistics::{chess, engine, projection, query, store, types::*},
    storage,
};
use serde_json::json;
use std::{path::PathBuf, sync::atomic::AtomicBool, time::Instant};
#[cfg(windows)]
fn peak_rss() -> Option<usize> {
    #[repr(C)]
    #[derive(Default)]
    struct Counters {
        cb: u32,
        faults: u32,
        peak_working: usize,
        working: usize,
        quota_peak_paged: usize,
        quota_paged: usize,
        quota_peak_nonpaged: usize,
        quota_nonpaged: usize,
        pagefile: usize,
        peak_pagefile: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut Counters,
            size: u32,
        ) -> i32;
    }
    let mut counters = Counters {
        cb: std::mem::size_of::<Counters>() as u32,
        ..Default::default()
    };
    // Both handles and the correctly sized output buffer remain valid throughout the OS call.
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<Counters>() as u32,
        )
    };
    (ok != 0).then_some(counters.peak_working)
}
#[cfg(not(windows))]
fn peak_rss() -> Option<usize> {
    None
}
fn millis(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("isolated .qa directory required")?,
    )
    .canonicalize()?;
    let qa = std::env::current_dir()?.join(".qa").canonicalize()?;
    if !directory.starts_with(&qa) {
        return Err("benchmark data must stay in .qa".into());
    }
    let moves = "1. e4 e5 2. Nf3 d6 3. d4 Bg4 4. dxe5 Bxf3 5. Qxf3 dxe5 6. Bc4 Nf6 7. Qb3 Qe7 8. Nc3 c6 9. Bg5 b5 10. Nxb5 cxb5 11. Bxb5+ Nbd7 12. O-O-O Rd8 13. Rxd7 Rxd7 14. Rd1 Qe6 15. Bxd7+ Nxd7 16. Qb8+ Nxb8 17. Rd8#";
    let pgn = format!(
        "[White \"BenchmarkFixture\"]\n[Black \"FixtureOpponent\"]\n[Result \"1-0\"]\n\n{moves} 1-0\n"
    );
    let first = Instant::now();
    let replay = chess::replay_pgn(&pgn)?;
    let cold = millis(first);
    let mut replay_samples = Vec::new();
    for _ in 0..50 {
        let timer = Instant::now();
        std::hint::black_box(chess::replay_pgn(&pgn)?);
        replay_samples.push(millis(timer));
    }
    replay_samples.sort_by(f64::total_cmp);
    let path = directory.join("chesspush.sqlite3");
    let conn = storage::open(&path).map_err(|e| e.message)?;
    let now = store::now_ms();
    let account = store::observe(
        &conn,
        &VerifiedIdentity {
            id: "benchmark_fixture".into(),
            username: "BenchmarkFixture".into(),
            generation: 0,
        },
        now - 86400000,
    )
    .map_err(|e| e.message)?;
    let clocks = (0..replay.nodes.len() - 1)
        .map(|i| 60000 - i * 150)
        .collect::<Vec<_>>();
    let payload = json!({"id":"bench001","createdAt":now-3600000,"lastMoveAt":now-1800000,"status":"mate","winner":"white","speed":"rapid","rated":true,"variant":"standard","clock":{"initial":600,"increment":2},"players":{"white":{"user":{"id":"benchmark_fixture","name":"BenchmarkFixture"},"rating":1800,"ratingDiff":5},"black":{"user":{"id":"fixtureopponent","name":"FixtureOpponent"},"rating":1800,"ratingDiff":-5}},"pgn":pgn,"clocks":clocks});
    let timer = Instant::now();
    let game = chess::from_lichess(&payload, &account.id, "benchmark_fixture")?;
    store::save_game(&conn, &account.id, &game, &payload.to_string(), "eligible")
        .map_err(|e| e.message)?;
    let ingest_ms = millis(timer);
    let mut analysis = json!({"status":"not_run","reason":"CHESSPUSH_STOCKFISH not supplied"});
    let mut actual_runs = engine::runs(&conn).map_err(|e| e.message)?;
    actual_runs.retain(|r| r.game_id == game.id);
    if !actual_runs.is_empty() {
        analysis = serde_json::from_slice::<serde_json::Value>(&std::fs::read(
            directory.join("statistics-performance.json"),
        )?)?["analysis"]
            .clone();
    }
    if let Ok(binary) = std::env::var("CHESSPUSH_STOCKFISH")
        && actual_runs.is_empty()
    {
        let config = engine::EngineConfig {
            path: binary,
            threads: 1,
            hash_mb: 128,
            profile: "standard".into(),
            ..Default::default()
        };
        let timer = Instant::now();
        let identity = engine::save_configuration(&conn, &config).map_err(|e| e.message)?;
        let validation_ms = millis(timer);
        engine::enqueue(&conn, std::slice::from_ref(&game.id)).map_err(|e| e.message)?;
        let timer = Instant::now();
        engine::worker_tick(&path, &AtomicBool::new(false)).map_err(|e| e.message)?;
        let elapsed = millis(timer);
        actual_runs = engine::runs(&conn).map_err(|e| e.message)?;
        if actual_runs.is_empty() {
            return Err("real standard analysis produced no persisted run".into());
        }
        analysis = json!({"status":"complete","engine":identity.name,"profile_id":identity.profile_id,"profile":"standard","custom_nodes":null,"custom_movetime_ms":null,"threads":1,"hash_mb":128,"plies":game.replay.nodes.len()-1,"validation_ms":validation_ms,"job_ms":elapsed,"evaluations":actual_runs[0].evaluations.len(),"confirmed_moves":actual_runs[0].confirmed_moves.len(),"reported_nodes_in_best_evaluations":actual_runs[0].evaluations.iter().map(|e|e.nodes).sum::<u64>()});
    }
    let mut query_rows = Vec::new();
    for n in [100, 500, 1000] {
        let mut games = Vec::with_capacity(n);
        let mut runs = Vec::new();
        for i in 0..n {
            let mut g = game.clone();
            g.id = format!("synthetic-benchmark-{i}");
            g.external_id = format!("synthetic-{i}");
            g.started_at_ms = Some(now - (i as i64 + 1) * 3600000);
            g.last_move_at_ms = g.started_at_ms.map(|t| t + 1800000);
            g.speed = if i % 2 == 0 { "rapid" } else { "blitz" }.into();
            for r in &actual_runs {
                let mut r = r.clone();
                r.game_id = g.id.clone();
                r.id = format!("synthetic-run-{i}");
                runs.push(r);
            }
            games.push(g);
        }
        let filter = StatisticsFilter {
            account_id: Some(account.id.clone()),
            variant: Some("standard".into()),
            ..Default::default()
        };
        let timer = Instant::now();
        let mut report = query::report(&games, &runs, &filter);
        let build_ms = millis(timer);
        let timer = Instant::now();
        let response_bytes = serde_json::to_vec(&report)?.len();
        let serialize_ms = millis(timer);
        let timer = Instant::now();
        projection::compact(&mut report);
        let compact_ms = millis(timer);
        let timer = Instant::now();
        let compact_response_bytes = serde_json::to_vec(&report)?.len();
        let compact_serialize_ms = millis(timer);
        let timer = Instant::now();
        let tree = query::tree(&games, &filter)?;
        let tree_ms = millis(timer);
        let timer = Instant::now();
        let filtered = query::report(
            &games,
            &runs,
            &StatisticsFilter {
                speed: Some("rapid".into()),
                ..filter.clone()
            },
        );
        let filter_ms = millis(timer);
        let timer = Instant::now();
        for g in &games {
            store::save_game(&conn, &account.id, g, "{}", "eligible").map_err(|e| e.message)?;
        }
        for run in &runs {
            conn.execute("INSERT INTO statistics_analysis_runs(id,game_id,movetext_revision,profile_id,state,data_json,completed_at_ms) VALUES(?1,?2,(SELECT movetext_revision FROM statistics_games WHERE id=?2),?3,'succeeded',?4,?5) ON CONFLICT(id) DO UPDATE SET data_json=excluded.data_json, completed_at_ms=excluded.completed_at_ms",rusqlite::params![run.id,run.game_id,run.profile_id,serde_json::to_string(run)?,run.completed_at_ms])?;
        }
        let batch_save_ms = millis(timer);
        let timer = Instant::now();
        let loaded = store::games(&conn, Some(&account.id)).map_err(|e| e.message)?;
        let sqlite_load_ms = millis(timer);
        query_rows.push(json!({"games":n,"sqlite_stored_games":loaded.len(),"sqlite_load_ms":sqlite_load_ms,"batch_save_ms":batch_save_ms,"plies_per_game":game.replay.nodes.len()-1,"synthetic_repeated_positions":true,"engine_evidence":"clones of one real analysed fixture, not independent chess observations","report_ms":build_ms,"serialize_ms":serialize_ms,"compact_ms":compact_ms,"compact_response_bytes":compact_response_bytes,"compact_serialize_ms":compact_serialize_ms,"response_bytes":response_bytes,"tree_ms":tree_ms,"tree_root_games":tree.games,"filter_ms":filter_ms,"filtered_games":filtered.total_games,"process_peak_rss_bytes":peak_rss()}));
    }
    engine::set_paused(&conn, true).map_err(|e| e.message)?;
    let result = json!({"kind":"synthetic legal PGN and repeated-position datasets; optional real Stockfish","os":std::env::consts::OS,"arch":std::env::consts::ARCH,"available_parallelism":std::thread::available_parallelism()?.get(),"replay_cold_ms":cold,"replay_hot_median_ms":replay_samples[25],"replay_hot_p90_ms":replay_samples[45],"ingest_and_sqlite_save_ms":ingest_ms,"analysis":analysis,"queries":query_rows,"process_peak_rss_bytes":peak_rss(),"memory_scope":"Rust benchmark process only; excludes Stockfish and WebView; high-water cumulative"});
    std::fs::write(
        directory.join("statistics-performance.json"),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

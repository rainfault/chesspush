//! Seeds only an explicitly supplied directory inside this checkout's ignored .qa.
use chesspush_lib::{
    lichess::VerifiedIdentity,
    statistics::{chess, store},
    storage,
};
use serde_json::json;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory =
        std::path::PathBuf::from(std::env::args().nth(1).ok_or("test directory required")?)
            .canonicalize()?;
    let qa = std::env::current_dir()?.join(".qa").canonicalize()?;
    if !directory.starts_with(&qa) {
        return Err("test data must be inside .qa".into());
    }
    let conn = storage::open(&directory.join("chesspush.sqlite3")).map_err(|e| e.message)?;
    let now = store::now_ms();
    let account = store::observe(
        &conn,
        &VerifiedIdentity {
            id: "statistics_fixture".into(),
            username: "StatisticsFixture".into(),
            generation: 0,
        },
        now - 864000000,
    )
    .map_err(|e| e.message)?;
    let mut examples = vec![
        (
            "stOp0001",
            "1. e4 e5 2. Nf3 d6 3. d4 Bg4 4. dxe5 Bxf3 5. Qxf3 dxe5 6. Bc4 Nf6 7. Qb3 Qe7 8. Nc3 c6 9. Bg5 b5 10. Nxb5 cxb5 11. Bxb5+ Nbd7 12. O-O-O Rd8 13. Rxd7 Rxd7 14. Rd1 Qe6 15. Bxd7+ Nxd7 16. Qb8+ Nxb8 17. Rd8#",
            "mate",
            Some("white"),
            false,
        ),
        (
            "stTr0001",
            "1. Nf3 Nf6 2. g3 g6 3. Bg2 Bg7",
            "resign",
            Some("white"),
            false,
        ),
        (
            "stTr0002",
            "1. g3 g6 2. Nf3 Nf6 3. Bg2 Bg7",
            "draw",
            None,
            true,
        ),
        (
            "stFo0001",
            "1. f3 e5 2. g4 Qh4#",
            "mate",
            Some("black"),
            false,
        ),
    ];
    if std::env::args().any(|arg| arg == "--with-cycle") {
        examples.push((
            "stCy0001",
            "1. Nf3 Nf6 2. Ng1 Ng8 3. Nf3 Nf6 4. Ng1 Ng8",
            "draw",
            None,
            false,
        ));
    }
    for (i, (id, moves, status, winner, black)) in examples.iter().enumerate() {
        let result = match winner {
            Some("white") => "1-0",
            Some("black") => "0-1",
            _ => "1/2-1/2",
        };
        let white = if *black {
            "Opponent"
        } else {
            "StatisticsFixture"
        };
        let black_name = if *black {
            "StatisticsFixture"
        } else {
            "Opponent"
        };
        let pgn = format!(
            "[Event \"Statistics test\"]\n[Site \"https://lichess.org/{id}\"]\n[White \"{white}\"]\n[Black \"{black_name}\"]\n[Result \"{result}\"]\n\n{moves} {result}\n"
        );
        let replay = chess::replay_pgn(&pgn)?;
        let clocks = (0..replay.nodes.len() - 1)
            .map(|p| 60000 - p as i64 * 250)
            .collect::<Vec<_>>();
        let payload = json!({"id":id,"createdAt":now-(i as i64+1)*86400000,"lastMoveAt":now-(i as i64+1)*86400000+1800000,"status":status,"winner":winner,"speed":if i==3{"blitz"}else{"rapid"},"variant":"standard","rated":true,"players":{"white":{"user":{"id":if *black{"opponent"}else{"statistics_fixture"},"name":white},"rating":1800,"ratingDiff":5},"black":{"user":{"id":if *black{"statistics_fixture"}else{"opponent"},"name":black_name},"rating":1750,"ratingDiff":-5}},"clock":{"initial":600,"increment":2},"pgn":pgn,"clocks":clocks});
        let game = chess::from_lichess(&payload, &account.id, "statistics_fixture")?;
        store::save_game(&conn, &account.id, &game, &payload.to_string(), "eligible")
            .map_err(|e| e.message)?;
    }
    store::quarantine(
        &conn,
        &account.id,
        &json!({"id":"stBad001","moves":"e4 broken","status":"resign"}),
        "Некорректная главная линия PGN",
        now,
    )
    .map_err(|e| e.message)?;
    store::save_initial_rating(&conn, &account.id, "rapid", 1790, now - 864000000)
        .map_err(|e| e.message)?;
    println!(
        "Seeded {} legal completed test games in isolated profile",
        store::games(&conn, Some(&account.id))
            .map_err(|e| e.message)?
            .len()
    );
    Ok(())
}

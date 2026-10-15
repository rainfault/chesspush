//! Legal main-line replay. Source annotations survive in raw PGN; derived fields are versioned.
use super::types::*;
use pgn_reader::{RawComment, RawTag, Reader, SanPlus, Visitor};
use serde_json::Value;
use sha2::{Digest, Sha256};
use shakmaty::{
    CastlingMode, Chess, Color, EnPassantMode, Position, Role, Square, fen::Fen, uci::UciMove,
};
use std::{collections::HashMap, ops::ControlFlow, sync::OnceLock};

pub const INITIAL_FEN: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
pub fn color_name(c: Color) -> &'static str {
    if c == Color::White { "white" } else { "black" }
}
pub fn terminal_outcome(fen: &str) -> Option<String> {
    let pos: Chess = fen
        .parse::<Fen>()
        .ok()?
        .into_position(CastlingMode::Standard)
        .ok()?;
    pos.outcome().known().map(|o| {
        o.winner()
            .map(|c| color_name(c).to_owned())
            .unwrap_or_else(|| "draw".into())
    })
}
pub fn position_key(pos: &Chess) -> String {
    // Legal EP, rather than pseudo-legal/FEN EP, gives identical keys for actual transpositions.
    let fen = Fen::from_position(pos, EnPassantMode::Legal).to_string();
    format!(
        "standard|{}",
        fen.split_whitespace().take(4).collect::<Vec<_>>().join(" ")
    )
}
fn material(pos: &Chess) -> MaterialSnapshot {
    let mut m = MaterialSnapshot::default();
    for (i, role) in [
        Role::Pawn,
        Role::Knight,
        Role::Bishop,
        Role::Rook,
        Role::Queen,
    ]
    .iter()
    .enumerate()
    {
        m.white[i] =
            (pos.board().by_role(*role) & pos.board().by_color(Color::White)).count() as u8;
        m.black[i] =
            (pos.board().by_role(*role) & pos.board().by_color(Color::Black)).count() as u8;
        m.balance += (m.white[i] as i32 - m.black[i] as i32) * [1, 3, 3, 5, 9][i];
    }
    let n = m.white[1] + m.black[1];
    let b = m.white[2] + m.black[2];
    let r = m.white[3] + m.black[3];
    let q = m.white[4] + m.black[4];
    m.ending = if n + b + r + q == 0 {
        "pawn"
    } else if n + b + q == 0 {
        "rook"
    } else if n + b + r == 0 {
        "queen"
    } else if r + q == 0 && n == 0 && m.white[2] == 1 && m.black[2] == 1 {
        let bishops: Vec<_> = (pos.board().by_role(Role::Bishop)).into_iter().collect();
        let a = (u32::from(bishops[0].file()) + u32::from(bishops[0].rank())) % 2;
        let z = (u32::from(bishops[1].file()) + u32::from(bishops[1].rank())) % 2;
        if a == z {
            "same_bishops"
        } else {
            "opposite_bishops"
        }
    } else if r + q == 0 {
        "minor"
    } else {
        "mixed"
    }
    .into();
    m
}
fn node(pos: &Chess, ply: u32) -> ReplayNode {
    ReplayNode {
        ply,
        absolute_ply: (pos.fullmoves().get() - 1) * 2 + u32::from(pos.turn() == Color::Black),
        fen: Fen::from_position(pos, EnPassantMode::Always).to_string(),
        position_key: position_key(pos),
        phase: "opening".into(),
        material: material(pos),
        legal_moves: pos.legal_moves().len() as u32,
        clock_quality: "missing".into(),
        ..Default::default()
    }
}
struct Line {
    pos: Chess,
    replay: Replay,
}
struct ReplayVisitor;
impl Visitor for ReplayVisitor {
    type Tags = HashMap<String, String>;
    type Movetext = Line;
    type Output = Result<Replay, String>;
    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(HashMap::new())
    }
    fn tag(
        &mut self,
        tags: &mut Self::Tags,
        name: &[u8],
        value: RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        tags.insert(
            String::from_utf8_lossy(name).into_owned(),
            value.decode_utf8_lossy().into_owned(),
        );
        ControlFlow::Continue(())
    }
    fn begin_movetext(&mut self, tags: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        if let Some(v) = tags.get("Variant")
            && !["Standard", "From Position", "standard", "fromPosition"].contains(&v.as_str())
        {
            return ControlFlow::Break(Err(format!("Unsupported variant: {v}")));
        }
        let fen = tags.get("FEN").map(String::as_str).unwrap_or(INITIAL_FEN);
        let pos = match fen.parse::<Fen>().map_err(|e| e.to_string()).and_then(|f| {
            f.into_position(CastlingMode::Standard)
                .map_err(|e| e.to_string())
        }) {
            Ok(p) => p,
            Err(e) => return ControlFlow::Break(Err(e)),
        };
        let replay = Replay {
            source_result: tags.get("Result").cloned(),
            initial_fen: fen.into(),
            nodes: vec![node(&pos, 0)],
            phase_version: "scalachess-divider-e3f7eb8f73a268f668d70e594e8f2426d8614881".into(),
            replay_version: "shakmaty-0.30.0-pgn-reader-0.29.1-v1".into(),
            catalogue_version: catalogue_version().to_owned(),
            ..Default::default()
        };
        ControlFlow::Continue(Line { pos, replay })
    }
    fn san(&mut self, line: &mut Line, san: SanPlus) -> ControlFlow<Self::Output> {
        let mv = match san.san.to_move(&line.pos) {
            Ok(m) => m,
            Err(e) => {
                return ControlFlow::Break(Err(format!(
                    "Illegal move at ply {}: {san}: {e}",
                    line.replay.nodes.len()
                )));
            }
        };
        let actor = color_name(line.pos.turn()).to_owned();
        let uci = UciMove::from_move(mv, CastlingMode::Standard).to_string();
        line.pos.play_unchecked(mv);
        let mut n = node(&line.pos, line.replay.nodes.len() as u32);
        n.actor = Some(actor);
        n.uci = Some(uci);
        n.san = Some(san.to_string());
        line.replay.nodes.push(n);
        ControlFlow::Continue(())
    }
    fn comment(&mut self, line: &mut Line, comment: RawComment<'_>) -> ControlFlow<Self::Output> {
        let n = line.replay.nodes.last_mut().unwrap();
        let text = String::from_utf8_lossy(comment.0);
        if !n.comment.is_empty() {
            n.comment.push('\n');
        }
        n.comment.push_str(&text);
        if let Some(start) = text.find("[%clk ")
            && let Some(end) = text[start + 6..].find(']')
        {
            n.clock_seconds = parse_clock(&text[start + 6..start + 6 + end]);
        }
        ControlFlow::Continue(())
    }
    fn outcome(
        &mut self,
        line: &mut Line,
        outcome: shakmaty::Outcome,
    ) -> ControlFlow<Self::Output> {
        let result = outcome.to_string();
        if result != "*"
            && line
                .replay
                .source_result
                .as_deref()
                .is_some_and(|r| r != "*" && r != result)
        {
            return ControlFlow::Break(Err("PGN header and movetext results disagree".into()));
        }
        if result != "*" {
            line.replay.source_result = Some(result);
        }
        ControlFlow::Continue(())
    }
    fn end_game(&mut self, line: Line) -> Self::Output {
        Ok(line.replay)
    }
}
fn parse_clock(s: &str) -> Option<f64> {
    let parts: Vec<_> = s.trim().split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    let value = parts[0].parse::<f64>().ok()? * 3600.
        + parts[1].parse::<f64>().ok()? * 60.
        + parts[2].parse::<f64>().ok()?;
    (value.is_finite() && value >= 0.).then_some(value)
}
pub fn replay_pgn(pgn: &str) -> Result<Replay, String> {
    let expected_moves = validate_mainline_tokens(pgn)?;
    let mut reader = Reader::new(pgn.as_bytes());
    let mut replay = reader
        .read_game(&mut ReplayVisitor)
        .map_err(|e| e.to_string())?
        .ok_or("PGN contains no game")??;
    if reader
        .read_game(&mut ReplayVisitor)
        .map_err(|e| e.to_string())?
        .is_some()
    {
        return Err("Expected one game".into());
    }
    if replay.nodes.len() != expected_moves + 1 {
        return Err("PGN token stream and legal replay disagree".into());
    }
    assign_phases(&mut replay);
    Ok(replay)
}
// Port of lichess-org/scalachess Divider e3f7eb8f73a268f668d70e594e8f2426d8614881.
// Copyright (c) 2012-2014 Thibault Duplessis; MIT, see licenses/SCALACHESS-MIT.txt.
// Board array includes initial board; coincident middle/end retain upstream semantics.
fn mixed_score(y: i32, w: usize, b: usize) -> i32 {
    match (w, b) {
        (0, 1) => 1 + y,
        (0, 2) if y < 6 => 2 + 6 - y,
        (0, 3 | 4) if y < 7 => 3 + 7 - y,
        (1, 0) => 1 + 8 - y,
        (1, 1) => 5 + (4 - y).abs(),
        (1, 2) => 4 + 7 - y,
        (1, 3) => 5 + 7 - y,
        (2, 0) if y > 2 => 2 + y - 2,
        (2, 1) => 4 + y - 1,
        (2, 2) => 7,
        (3 | 4, 0) if y > 1 => 3 + y - 1,
        (3, 1) => 5 + y - 1,
        _ => 0,
    }
}
fn divider_facts(fen: &str) -> (usize, bool, i32) {
    let pos: Chess = fen
        .parse::<Fen>()
        .unwrap()
        .into_position(CastlingMode::Standard)
        .unwrap();
    let board = pos.board();
    let majors =
        (board.occupied() & !(board.by_role(Role::Pawn) | board.by_role(Role::King))).count();
    let mut home = [0, 0];
    let mut mix = 0;
    for sq in board.by_color(Color::White) {
        if u32::from(sq.rank()) == 0 {
            home[0] += 1;
        }
    }
    for sq in board.by_color(Color::Black) {
        if u32::from(sq.rank()) == 7 {
            home[1] += 1;
        }
    }
    for y in 0..7 {
        for x in 0..7 {
            let (mut w, mut b) = (0, 0);
            for dy in 0..2 {
                for dx in 0..2 {
                    let sq = Square::new(x + dx + 8 * (y + dy));
                    match board.color_at(sq) {
                        Some(Color::White) => w += 1,
                        Some(Color::Black) => b += 1,
                        None => {}
                    }
                }
            }
            mix += mixed_score(y as i32 + 1, w, b);
        }
    }
    (majors, home[0] < 4 || home[1] < 4, mix)
}
fn assign_phases(replay: &mut Replay) {
    let facts: Vec<_> = replay.nodes.iter().map(|n| divider_facts(&n.fen)).collect();
    let middle = facts
        .iter()
        .position(|(n, s, m)| *n <= 10 || *s || *m > 150);
    let end = middle.and_then(|_| facts.iter().position(|(n, _, _)| *n <= 6));
    replay.middle_ply = middle
        .filter(|m| end.is_none_or(|e| *m < e))
        .map(|x| x as u32);
    replay.end_ply = end.map(|x| x as u32);
    // Nonstandard starting endings have no observed transition. Preserve Divider boundaries,
    // but label material-defined initial endings with a distinct, explicit method.
    if replay.initial_fen != INITIAL_FEN && facts.first().is_some_and(|(n, _, _)| *n <= 6) {
        replay
            .phase_version
            .push_str("+initial-material-endgame-v1");
        for n in &mut replay.nodes {
            n.phase = "endgame".into();
        }
        return;
    }
    // As upstream, no detected middle means no meaningful per-phase attribution, even if end coincides.
    for n in &mut replay.nodes {
        n.phase = match replay.middle_ply {
            Some(m) if n.ply >= m => {
                if replay.end_ply.is_some_and(|e| n.ply >= e) {
                    "endgame"
                } else {
                    "middlegame"
                }
            }
            _ => "opening",
        }
        .into();
    }
}
#[derive(serde::Deserialize)]
struct CatalogueEntry {
    eco: String,
    name: String,
    pgn: String,
}
fn catalogue_version() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| {
        format!(
            "lichess-chess-openings-sha256:{:x}",
            Sha256::digest(include_bytes!("../../../public/openings.json"))
        )
    })
}
fn catalogue() -> &'static HashMap<String, Vec<OpeningMatch>> {
    static MAP: OnceLock<HashMap<String, Vec<OpeningMatch>>> = OnceLock::new();
    MAP.get_or_init(|| {
        let entries: Vec<CatalogueEntry> =
            serde_json::from_str(include_str!("../../../public/openings.json"))
                .expect("bundled CC0 opening catalogue");
        let mut map: HashMap<String, Vec<OpeningMatch>> = HashMap::new();
        for entry in entries {
            if let Ok(r) = replay_pgn(&(entry.pgn + " *"))
                && let Some(n) = r.nodes.last()
            {
                let (family, variation) = entry.name.split_once(": ").unwrap_or((&entry.name, ""));
                map.entry(n.position_key.clone())
                    .or_default()
                    .push(OpeningMatch {
                        eco: entry.eco,
                        name: entry.name.clone(),
                        family: family.into(),
                        variation: variation.into(),
                        ply: n.ply,
                        position_key: n.position_key.clone(),
                    });
            }
        }
        map
    })
}
pub fn classify_opening(replay: &Replay) -> Option<OpeningMatch> {
    replay.nodes.iter().rev().find_map(|n| {
        catalogue()
            .get(&n.position_key)
            .and_then(|v| v.iter().max_by_key(|o| o.ply))
            .map(|o| {
                let mut x = o.clone();
                x.ply = n.ply;
                x
            })
    })
}
fn participant(v: &Value) -> Participant {
    Participant {
        id: v
            .pointer("/user/id")
            .and_then(Value::as_str)
            .map(str::to_lowercase),
        name: v
            .pointer("/user/name")
            .or_else(|| v.get("name"))
            .and_then(Value::as_str)
            .unwrap_or("Anonymous")
            .into(),
        rating: v.get("rating").and_then(Value::as_i64).map(|x| x as i32),
        rating_diff: v
            .get("ratingDiff")
            .and_then(Value::as_i64)
            .map(|x| x as i32),
        provisional: v
            .get("provisional")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}
pub fn from_lichess(
    v: &Value,
    account_id: &str,
    stable_external_id: &str,
) -> Result<StoredGame, String> {
    let external_id = v
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or("Missing game ID")?;
    let variant = v
        .get("variant")
        .and_then(Value::as_str)
        .unwrap_or("standard");
    if !["standard", "fromPosition"].contains(&variant) {
        return Err(format!("Unsupported variant: {variant}"));
    }
    let status = v.get("status").and_then(Value::as_str).unwrap_or("unknown");
    if ![
        "mate",
        "resign",
        "stalemate",
        "timeout",
        "draw",
        "outoftime",
    ]
    .contains(&status)
    {
        return Err("Game is not confirmed finished".into());
    }
    let white = participant(&v["players"]["white"]);
    let black = participant(&v["players"]["black"]);
    let own_color = if white.id.as_deref() == Some(&stable_external_id.to_lowercase()) {
        "white"
    } else if black.id.as_deref() == Some(&stable_external_id.to_lowercase()) {
        "black"
    } else {
        return Err("Game does not belong to the observed account".into());
    };
    let result = match v.get("winner").and_then(Value::as_str) {
        Some("white") => "1-0",
        Some("black") => "0-1",
        _ if ["draw", "stalemate"].contains(&status) => "1/2-1/2",
        _ => return Err("No supported terminal result".into()),
    };
    let raw_pgn = if let Some(s) = v.get("pgn").and_then(Value::as_str) {
        s.to_owned()
    } else {
        // Some export formats supply SAN + raw JSON rather than PGN. This is a lossless mainline reconstruction; original JSON retained.
        let moves = v
            .get("moves")
            .and_then(Value::as_str)
            .ok_or("Missing PGN and moves")?;
        let fen = v
            .get("initialFen")
            .and_then(Value::as_str)
            .filter(|s| *s != "startpos")
            .map(|s| format!("[SetUp \"1\"]\n[FEN \"{s}\"]\n"))
            .unwrap_or_default();
        format!(
            "[Site \"https://lichess.org/{external_id}\"]\n[Result \"{result}\"]\n{fen}\n{moves} {result}"
        )
    };
    let mut replay = replay_pgn(&raw_pgn)?;
    if replay.nodes.len() < 2 {
        return Err("Finished game has no legal moves".into());
    }
    if replay
        .source_result
        .as_deref()
        .is_some_and(|r| r != "*" && r != result)
    {
        return Err("PGN and API results disagree".into());
    }
    if let Some(out) = terminal_outcome(&replay.nodes.last().unwrap().fen) {
        let legal_result = match out.as_str() {
            "white" => "1-0",
            "black" => "0-1",
            _ => "1/2-1/2",
        };
        if legal_result != result {
            return Err("Terminal board and API result disagree".into());
        }
    }
    if v.get("pgn").is_some()
        && let Some(moves) = v.get("moves").and_then(Value::as_str)
    {
        let fen = v
            .get("initialFen")
            .and_then(Value::as_str)
            .filter(|s| *s != "startpos")
            .unwrap_or(&replay.initial_fen);
        let check = replay_pgn(&format!("[FEN \"{fen}\"]\n\n{moves} {result}"))?;
        if !check.nodes.iter().skip(1).map(|n| &n.uci).eq(replay
            .nodes
            .iter()
            .skip(1)
            .map(|n| &n.uci))
        {
            return Err("PGN and API move lists disagree".into());
        }
    }
    if let Some(clocks) = v.get("clocks").and_then(Value::as_array) {
        for (i, n) in replay.nodes.iter_mut().skip(1).enumerate() {
            if let Some(c) = clocks
                .get(i)
                .and_then(Value::as_f64)
                .filter(|c| c.is_finite() && *c >= 0.)
            {
                n.clock_seconds = Some(c / 100.);
            }
        }
    }
    let initial_seconds = v.pointer("/clock/initial").and_then(Value::as_f64);
    let increment_seconds = v.pointer("/clock/increment").and_then(Value::as_f64);
    restore_times(&mut replay, increment_seconds, v, status);
    replay.opening = classify_opening(&replay);
    Ok(StoredGame {
        id: format!("lichess:{external_id}"),
        external_id: external_id.into(),
        account_id: account_id.into(),
        source: "lichess".into(),
        started_at_ms: v.get("createdAt").and_then(Value::as_i64),
        last_move_at_ms: v.get("lastMoveAt").and_then(Value::as_i64),
        finished_at_ms: None,
        end_time_quality: "unknown".into(),
        white,
        black,
        own_color: own_color.into(),
        result: result.into(),
        status: status.into(),
        speed: v
            .get("speed")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .into(),
        rated: v.get("rated").and_then(Value::as_bool),
        variant: variant.into(),
        initial_seconds,
        increment_seconds,
        raw_pgn,
        source_metadata: v.clone(),
        eligibility: "eligible".into(),
        replay,
    })
}
fn restore_times(replay: &mut Replay, increment: Option<f64>, source: &Value, status: &str) {
    let last = replay.nodes.len() - 1;
    let mut previous: [Option<(u32, f64)>; 2] = [None, None];
    for i in 1..replay.nodes.len() {
        let n = &mut replay.nodes[i];
        n.remaining_before = None;
        n.spent_seconds = None;
        let side = usize::from(n.actor.as_deref() == Some("black"));
        if let Some(clock) = n.clock_seconds {
            n.clock_quality = "remaining".into();
            if let Some((ply, prev)) = previous[side]
                && n.ply == ply + 2
            {
                n.remaining_before = Some(prev);
                let berserk =
                    source["players"][if side == 0 { "white" } else { "black" }]["berserk"]
                        .as_bool()
                        .unwrap_or(false);
                let inc = if berserk { Some(0.) } else { increment };
                // No increment is granted after a move ending the game automatically.
                let terminal = terminal_outcome(&n.fen).is_some();
                let inc = if i == last && terminal {
                    Some(0.)
                } else if i == last && status == "draw" {
                    None
                } else {
                    inc
                };
                if let Some(inc) = inc {
                    let spent = prev + inc - clock;
                    if spent >= -0.02 {
                        n.spent_seconds = Some(spent.max(0.));
                        n.clock_quality = "recovered_centiseconds".into();
                    } else {
                        n.clock_quality = "invalid_negative".into();
                    }
                }
            }
            previous[side] = Some((n.ply, clock));
        } else {
            previous[side] = None;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legal_replay_and_annotations() {
        let r = replay_pgn("1. e4 {[%clk 0:04:59.90] comment} e5 (1... c5) 2. Nf3 Nc6 *").unwrap();
        assert_eq!(r.nodes.len(), 5);
        assert_eq!(r.nodes[1].uci.as_deref(), Some("e2e4"));
        assert_eq!(r.nodes[1].clock_seconds, Some(299.9));
        assert!(r.nodes[1].comment.contains("comment"));
        assert!(replay_pgn("1. e5 *").is_err());
    }
    #[test]
    fn transposition_and_rule_context() {
        let a = replay_pgn("1. Nf3 Nf6 2. g3 g6 *").unwrap();
        let b = replay_pgn("1. g3 g6 2. Nf3 Nf6 *").unwrap();
        assert_eq!(a.nodes[4].position_key, b.nodes[4].position_key);
        assert_ne!(a.nodes[4].fen, b.nodes[4].fen);
    }
    #[test]
    fn phantom_and_pinned_ep_not_identity() {
        let a: Chess = "4r1k1/8/8/3pP3/8/8/8/4K3 w - d6 0 1"
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        let b: Chess = "4r1k1/8/8/3pP3/8/8/8/4K3 w - - 0 1"
            .parse::<Fen>()
            .unwrap()
            .into_position(CastlingMode::Standard)
            .unwrap();
        assert_eq!(position_key(&a), position_key(&b));
    }
    #[test]
    fn terminal_mate_and_stalemate() {
        assert_eq!(
            terminal_outcome("7k/6Q1/6K1/8/8/8/8/8 b - - 0 1").as_deref(),
            Some("white")
        );
        assert_eq!(
            terminal_outcome("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1").as_deref(),
            Some("draw")
        );
    }
    #[test]
    fn clock_first_gap_and_increment() {
        let mut r = replay_pgn("1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 *").unwrap();
        for (i, c) in [300., 300., 297., 298., 295., 297.].iter().enumerate() {
            r.nodes[i + 1].clock_seconds = Some(*c);
        }
        restore_times(&mut r, Some(2.), &Value::Null, "resign");
        assert_eq!(r.nodes[1].spent_seconds, None);
        assert_eq!(r.nodes[3].spent_seconds, Some(5.));
        r.nodes[3].clock_seconds = None;
        restore_times(&mut r, Some(2.), &Value::Null, "resign");
        assert_eq!(r.nodes[5].spent_seconds, None);
    }
    #[test]
    fn from_position_keeps_absolute_ply() {
        let r = replay_pgn("[FEN \"8/8/8/8/8/4k3/8/4K2R w K - 0 23\"]\n\n1. Rh3+ *");
        assert!(r.unwrap().nodes[0].fen.contains(" w K "));
        let r = replay_pgn("[FEN \"8/8/8/8/8/4k3/8/4K2R w - - 0 23\"]\n\n23. Rh3+ *").unwrap();
        assert_eq!(r.nodes[0].absolute_ply, 44);
    }
}

pub fn line_position(line: &[String]) -> Result<ReplayNode, String> {
    line_position_from(INITIAL_FEN, line)
}
pub fn line_position_from(initial_fen: &str, line: &[String]) -> Result<ReplayNode, String> {
    if line.len() > 2048 {
        return Err("Line too long".into());
    }
    let mut pos: Chess = initial_fen
        .parse::<Fen>()
        .map_err(|e| e.to_string())?
        .into_position(CastlingMode::Standard)
        .map_err(|e| e.to_string())?;
    for s in line {
        let uci = s.parse::<UciMove>().map_err(|e| e.to_string())?;
        let mv = uci.to_move(&pos).map_err(|e| e.to_string())?;
        pos.play_unchecked(mv);
    }
    Ok(node(&pos, line.len() as u32))
}

#[cfg(test)]
mod ingestion_tests {
    use super::*;
    fn payload() -> Value {
        serde_json::json!({"id":"testGame","createdAt":1000,"lastMoveAt":3000,"status":"mate","winner":"white","variant":"standard","speed":"blitz","rated":true,"players":{"white":{"user":{"id":"me","name":"Me"},"rating":1500},"black":{"user":{"id":"other","name":"Other"},"rating":1500}},"clock":{"initial":300,"increment":2},"moves":"e4 e5 Qh5 Nc6 Bc4 Nf6 Qxf7#","clocks":[30000,30000,29900,29900,29800,29800,29400]})
    }
    #[test]
    fn ingest_membership_clock_and_no_fake_end_time() {
        let mut p = payload();
        let g = from_lichess(&p, "lichess:me", "me").unwrap();
        assert_eq!(g.finished_at_ms, None);
        assert_eq!(g.last_move_at_ms, Some(3000));
        assert_eq!(g.replay.nodes[7].spent_seconds, Some(4.));
        assert_eq!(g.replay.nodes[1].spent_seconds, None);
        assert!(g.replay.opening.is_some());
        p["players"]["white"]["user"]["id"] = "stranger".into();
        assert!(from_lichess(&p, "lichess:me", "me").is_err());
    }
    #[test]
    fn source_contradictions_are_rejected() {
        let mut p = payload();
        p["winner"] = "black".into();
        assert!(from_lichess(&p, "lichess:me", "me").is_err());
        p = payload();
        p["pgn"] = "[Result \"0-1\"]\n\n1. e4 e5 0-1".into();
        assert!(from_lichess(&p, "lichess:me", "me").is_err());
        p = payload();
        p["pgn"] = "1. d4 d5 1-0".into();
        assert!(from_lichess(&p, "lichess:me", "me").is_err());
    }
}

// pgn-reader deliberately tolerates unknown tokens. Ingestion requires an independently
// validated token stream, so a damaged PGN can never silently become a shorter valid game.
fn validate_mainline_tokens(pgn: &str) -> Result<usize, String> {
    let b = pgn.as_bytes();
    let mut i = 0;
    let mut depth = 0usize;
    let mut moves = 0;
    while i < b.len() {
        match b[i] {
            c if c.is_ascii_whitespace() => i += 1,
            b'[' => {
                i += 1;
                let mut quoted = false;
                let mut escaped = false;
                let mut closed = false;
                while i < b.len() {
                    let c = b[i];
                    i += 1;
                    if escaped {
                        escaped = false;
                        continue;
                    }
                    if c == b'\\' && quoted {
                        escaped = true;
                    } else if c == b'"' {
                        quoted = !quoted;
                    } else if c == b']' && !quoted {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    return Err("Unterminated PGN tag".into());
                }
            }
            b'{' => {
                i += 1;
                while i < b.len() && b[i] != b'}' {
                    i += 1;
                }
                if i == b.len() {
                    return Err("Unterminated PGN comment".into());
                }
                i += 1;
            }
            b';' | b'%' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                if depth == 0 {
                    return Err("Unmatched PGN variation".into());
                }
                depth -= 1;
                i += 1;
            }
            b'$' => {
                i += 1;
                let start = i;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if i == start {
                    return Err("Invalid PGN NAG".into());
                }
            }
            b'}' | b']' => return Err("Unexpected PGN delimiter".into()),
            _ => {
                let start = i;
                while i < b.len() && !b[i].is_ascii_whitespace() && !b"{}[]();$".contains(&b[i]) {
                    i += 1;
                }
                if i == start {
                    return Err("Invalid PGN token".into());
                }
                if depth > 0 {
                    continue;
                }
                let mut token = &pgn[start..i];
                if ["1-0", "0-1", "1/2-1/2", "*", "e.p."].contains(&token) {
                    continue;
                }
                let skip = if token.starts_with("0-0") {
                    0
                } else {
                    token
                        .bytes()
                        .take_while(|c| c.is_ascii_digit() || *c == b'.')
                        .count()
                };
                if skip > 0 {
                    token = &token[skip..];
                }
                token = token.trim_end_matches(['!', '?']);
                if token.is_empty() {
                    continue;
                }
                if SanPlus::from_ascii(token.as_bytes()).is_err() {
                    return Err(format!("Invalid PGN move token: {token}"));
                }
                moves += 1;
            }
        }
    }
    if depth != 0 {
        return Err("Unterminated PGN variation".into());
    }
    Ok(moves)
}

#[cfg(test)]
mod method_fixtures {
    use super::*;
    #[test]
    fn damaged_tokens_and_unterminated_annotations_are_not_silently_accepted() {
        for pgn in [
            "1. e4 garbage e5 *",
            "1. e4 {broken",
            "1. e4 (1... e5",
            "[Event \"x\"\n\n1. e4 *",
            "1. e4 e5junk *",
        ] {
            assert!(replay_pgn(pgn).is_err(), "{pgn}");
        }
    }
    #[test]
    fn divider_pinned_boundary_and_custom_endgame_are_distinct() {
        let start = divider_facts(INITIAL_FEN);
        assert_eq!(start, (14, false, 0));
        assert_eq!(mixed_score(1, 4, 0), 0);
        assert_eq!(mixed_score(4, 1, 1), 5);
        assert_eq!(mixed_score(7, 0, 4), 0);
        let r = replay_pgn("[FEN \"8/8/8/8/8/4k3/8/4K2R w - - 0 23\"]\n\n23. Rh3+ *").unwrap();
        assert_eq!(r.middle_ply, None);
        assert_eq!(r.end_ply, Some(0));
        assert!(r.phase_version.ends_with("+initial-material-endgame-v1"));
        assert_eq!(r.nodes[1].phase, "endgame");
    }
}

#[cfg(test)]
mod catalogue_fixture {
    use super::*;
    #[test]
    fn every_bundled_opening_has_a_legal_replay() {
        let entries: Vec<CatalogueEntry> =
            serde_json::from_str(include_str!("../../../public/openings.json")).unwrap();
        assert_eq!(
            catalogue().values().map(Vec::len).sum::<usize>(),
            entries.len()
        );
    }
}

pub fn opening_at(replay: &Replay, ply: u32) -> Option<OpeningMatch> {
    replay
        .nodes
        .iter()
        .take(ply as usize + 1)
        .rev()
        .find_map(|n| {
            catalogue()
                .get(&n.position_key)
                .and_then(|v| v.iter().max_by_key(|o| o.ply))
                .map(|o| {
                    let mut x = o.clone();
                    x.ply = n.ply;
                    x
                })
        })
}

pub fn legal_continuations(fen: &str) -> Result<Vec<LegalContinuation>, String> {
    let pos: Chess = fen
        .parse::<Fen>()
        .map_err(|e| e.to_string())?
        .into_position(CastlingMode::Standard)
        .map_err(|e| e.to_string())?;
    let mut moves: Vec<_> = pos
        .legal_moves()
        .iter()
        .map(|mv| LegalContinuation {
            uci: UciMove::from_move(*mv, CastlingMode::Standard).to_string(),
            san: SanPlus::from_move(pos.clone(), *mv).to_string(),
        })
        .collect();
    moves.sort_by(|a, b| a.san.cmp(&b.san));
    Ok(moves)
}

/// Versioned paths built only from names in the pinned local catalogue. The rule
/// describes literal label qualifiers, not a universal semantic chess taxonomy.
/// Unknown/provider names remain one label; no inferred parent is invented.
pub fn opening_name_path(name: &str) -> Vec<String> {
    static PATHS: OnceLock<HashMap<String, Vec<String>>> = OnceLock::new();
    let paths = PATHS.get_or_init(|| {
        catalogue()
            .values()
            .flatten()
            .map(|label| {
                let name = &label.name;
                let path = if let Some((family, tail)) = name.split_once(": ") {
                    let mut path = vec![family.to_owned()];
                    let mut full = family.to_owned();
                    for (i, part) in tail.split(", ").enumerate() {
                        full.push_str(if i == 0 { ": " } else { ", " });
                        full.push_str(part);
                        path.push(full.clone());
                    }
                    path
                } else {
                    vec![name.clone()]
                };
                (name.clone(), path)
            })
            .collect()
    });
    paths
        .get(name)
        .cloned()
        .unwrap_or_else(|| vec![name.to_owned()])
}
pub fn opening_label_id(color: &str, path: &[String]) -> String {
    format!(
        "{color}:catalog-name-path-v1:{:x}",
        Sha256::digest(path.join("\u{1f}").as_bytes())
    )
}
pub fn opening_label_matches(game: &StoredGame, id: &str) -> bool {
    let path = game
        .replay
        .opening
        .as_ref()
        .map(|o| opening_name_path(&o.name))
        .unwrap_or_else(|| vec!["Не классифицировано".into()]);
    (1..=path.len()).any(|n| opening_label_id(&game.own_color, &path[..n]) == id)
}

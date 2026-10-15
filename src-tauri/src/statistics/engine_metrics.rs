//! Versioned, evidence-first quality, opportunity and endgame analytics.
use super::{chess, metrics, types::*};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shakmaty::{CastlingMode, Chess, Color, Position, fen::Fen};
use shakmaty_syzygy::Tablebase;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::Path,
};

pub const VERSION: &str = "qes-evidence-v3";
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalysisCase {
    pub metric_id: String,
    pub position_ply: u32,
    pub game_id: String,
    pub ply: u32,
    pub run_id: String,
    pub label: String,
    pub value: Option<f64>,
    pub unit: String,
    pub phase: String,
    pub details: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Episode {
    pub game_id: String,
    pub run_id: String,
    pub kind: String,
    pub start_ply: u32,
    pub end_ply: u32,
    pub confirmation_ply: u32,
    pub exit_ply: Option<u32>,
    pub terminal_ply: Option<u32>,
    pub status: String,
    pub own_decisions: usize,
    pub first_loss_ply: Option<u32>,
    pub total_loss: f64,
    pub phase: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LossBin {
    pub lo: f64,
    pub hi: Option<f64>,
    pub decisions: usize,
    pub game_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrajectoryPoint {
    pub game_id: String,
    pub run_id: String,
    pub ply: u32,
    pub cp: Option<i32>,
    pub mate: Option<i32>,
    pub wdl: Option<[u32; 3]>,
    pub q: Option<f64>,
    pub pov: String,
    pub status: String,
    pub accuracy: Option<f64>,
    pub advice: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EngineDetails {
    pub version: String,
    pub cases: Vec<AnalysisCase>,
    pub episodes: Vec<Episode>,
    pub histogram: Vec<LossBin>,
    pub tablebase_status: String,
    pub trajectory: Vec<TrajectoryPoint>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TablebaseObservation {
    pub ply: u32,
    pub wdl: Option<i32>,
    pub dtz: Option<i32>,
    pub rounded: bool,
    pub status: String,
    pub manifest_id: String,
    pub rule50: bool,
    pub context_hash: String,
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
/// Metadata manifest identifies the exact local set; no directory traversal or table downloads.
pub fn tablebase_manifest(path: &str) -> Result<String, String> {
    let dir = Path::new(path);
    if !dir.is_absolute() || !dir.is_dir() {
        return Err("Укажите абсолютный путь к локальным таблицам Syzygy".into());
    }
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let p = entry.path();
        if p.extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s == "rtbw" || s == "rtbz")
        {
            let m = entry.metadata().map_err(|e| e.to_string())?;
            if m.is_file() {
                files.push((
                    entry.file_name().to_string_lossy().into_owned(),
                    m.len(),
                    m.modified()
                        .ok()
                        .and_then(|v| v.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|v| v.as_nanos()),
                ));
            }
        }
    }
    files.sort();
    if files.is_empty() {
        return Err("В каталоге нет файлов .rtbw / .rtbz".into());
    }
    Ok(hash(
        serde_json::to_string(&("syzygy-local-v1", files))
            .map_err(|e| e.to_string())?
            .as_bytes(),
    ))
}
/// Conservative history policy: classify exact transitions only after irreversible zeroing.
/// Nonzero halfmove contexts are retained as unavailable, not silently treated as draws.
pub fn probe_tablebases(game: &StoredGame, path: Option<&str>) -> Vec<TablebaseObservation> {
    let manifest = path
        .and_then(|p| tablebase_manifest(p).ok())
        .unwrap_or_default();
    let mut tables = Tablebase::<Chess>::new();
    let enabled = path.is_some_and(|p| tables.add_directory(p).is_ok()) && !manifest.is_empty();
    game.replay
        .nodes
        .iter()
        .map(|node| {
            let context = serde_json::to_string(&(
                &game.replay.initial_fen,
                game.replay
                    .nodes
                    .iter()
                    .take(node.ply as usize + 1)
                    .filter_map(|n| n.uci.as_deref())
                    .collect::<Vec<_>>(),
                &manifest,
                "rule50-zeroing-context-v1",
            ))
            .unwrap_or_default();
            let mut o = TablebaseObservation {
                ply: node.ply,
                manifest_id: manifest.clone(),
                rule50: true,
                context_hash: hash(context.as_bytes()),
                status: if enabled {
                    "missing_tables"
                } else {
                    "not_configured"
                }
                .into(),
                ..Default::default()
            };
            if !enabled {
                return o;
            }
            let Some(pos) = node
                .fen
                .parse::<Fen>()
                .ok()
                .and_then(|f| f.into_position::<Chess>(CastlingMode::Standard).ok())
            else {
                o.status = "invalid_position".into();
                return o;
            };
            if let Some(outcome) = chess::terminal_outcome(&node.fen) {
                o.wdl = Some(match outcome.as_str() {
                    "white" => 2,
                    "black" => -2,
                    _ => 0,
                });
                o.dtz = Some(0);
                o.status = "available".into();
                return o;
            }
            if pos.castles().any() {
                o.status = "castling_rights".into();
                return o;
            }
            if pos.board().occupied().count() > 7 {
                o.status = "out_of_coverage".into();
                return o;
            }
            if pos.halfmoves() != 0 {
                o.status = "history_sensitive".into();
                return o;
            }
            match (tables.probe_wdl(&pos), tables.probe_dtz(&pos)) {
                (Ok(w), Ok(d)) => {
                    if let Some(w) = w.unambiguous() {
                        let sign = if pos.turn() == Color::White { 1 } else { -1 };
                        o.wdl = Some(sign * i32::from(w));
                        o.rounded = d.precise().is_none();
                        o.dtz = Some(sign * d.ignore_rounding().0);
                        o.status = "available".into();
                    } else {
                        o.status = "rounded_ambiguous".into();
                    }
                }
                _ => o.status = "missing_tables".into(),
            }
            o
        })
        .collect()
}
fn phase_name(v: &str) -> &str {
    match v {
        "opening" => "Дебют",
        "middlegame" => "Миттельшпиль",
        "endgame" => "Эндшпиль",
        _ => v,
    }
}
fn ending_name(v: &str) -> &str {
    match v {
        "pawn" => "Пешечные",
        "rook" => "Ладейные",
        "queen" => "Ферзевые",
        "minor" => "Лёгкие фигуры",
        "same_bishops" => "Одноцветные слоны",
        "opposite_bishops" => "Разноцветные слоны",
        "mixed" => "Смешанные",
        _ => v,
    }
}
fn actor_sign(color: &str) -> f64 {
    if color == "white" { 1. } else { -1. }
}
fn usable(e: &PositionEvaluation) -> bool {
    e.complete && e.bound.is_none() && e.mate != Some(0)
}
fn q(e: &PositionEvaluation, color: &str) -> Option<f64> {
    metrics::eval_q(e, color)
}
fn cp(e: &PositionEvaluation, color: &str) -> Option<f64> {
    if !usable(e) {
        return None;
    }
    // Exact terminal draw has the conventional score zero; raw ACPL still excludes missing CP.
    if e.terminal_outcome.as_deref() == Some("draw") {
        return Some(0.);
    }
    e.cp.map(|v| actor_sign(color) * v as f64)
}

fn mate(e: &PositionEvaluation, color: &str) -> Option<i32> {
    if !usable(e) {
        return None;
    }
    if let Some(m) = e.mate {
        return Some(if color == "white" { m } else { -m });
    }
    match e.terminal_outcome.as_deref() {
        Some("white") => Some(if color == "white" { 1 } else { -1 }),
        Some("black") => Some(if color == "black" { 1 } else { -1 }),
        _ => None,
    }
}

fn prob_raw(cp: f64) -> f64 {
    100. / (1. + (-0.00368208 * cp).exp())
}
fn capped_score(e: &PositionEvaluation, color: &str) -> Option<f64> {
    if let Some(c) = cp(e, color) {
        return Some(c.clamp(-1000., 1000.));
    }
    if let Some(m) = mate(e, color) {
        return Some(if m > 0 { 1000. } else { -1000. });
    }
    e.terminal_outcome.as_deref().map(|o| {
        if o == "draw" {
            0.
        } else if o == color {
            1000.
        } else {
            -1000.
        }
    })
}
/// Lila Advice policy, CP path deliberately not clamped; mate branches are separate.
pub fn advice(
    before: &PositionEvaluation,
    after: &PositionEvaluation,
    color: &str,
) -> Option<&'static str> {
    if !usable(before) || !usable(after) {
        return None;
    }
    if let Some((a, b)) = cp(before, color).zip(cp(after, color)) {
        let loss = prob_raw(a) - prob_raw(b);
        return if loss >= 15. {
            Some("blunder")
        } else if loss >= 10. {
            Some("mistake")
        } else if loss >= 5. {
            Some("inaccuracy")
        } else {
            None
        };
    }
    let created = cp(before, color).is_some() && mate(after, color).is_some_and(|m| m < 0);
    let lost = mate(before, color).is_some_and(|m| m > 0)
        && (cp(after, color).is_some() || mate(after, color).is_some_and(|m| m < 0));
    if created {
        let p = cp(before, color).unwrap();
        Some(if p < -999. {
            "inaccuracy"
        } else if p < -700. {
            "mistake"
        } else {
            "blunder"
        })
    } else if lost {
        let p = cp(after, color).unwrap_or(0.);
        Some(if p > 999. {
            "inaccuracy"
        } else if p > 700. {
            "mistake"
        } else {
            "blunder"
        })
    } else {
        None
    }
}
#[allow(clippy::too_many_arguments)]
fn case(
    details: &mut EngineDetails,
    id: &str,
    g: &StoredGame,
    r: &EngineRun,
    ply: u32,
    label: &str,
    value: Option<f64>,
    unit: &str,
    description: impl ToString,
) {
    details.cases.push(AnalysisCase {
        metric_id: id.into(),
        position_ply: if id == "S03"
            || id == "Q06"
            || id == "E01"
            || id.starts_with("S01")
            || id.starts_with("S04_")
        {
            ply
        } else {
            ply.saturating_sub(1)
        },
        game_id: g.id.clone(),
        ply,
        run_id: r.id.clone(),
        label: label.into(),
        value,
        unit: unit.into(),
        phase: g
            .replay
            .nodes
            .get(ply as usize)
            .map(|n| n.phase.clone())
            .unwrap_or_default(),
        details: description.to_string(),
    });
}
#[allow(clippy::too_many_arguments)]
fn row(
    rows: &mut Vec<MetricValue>,
    id: &str,
    name: &str,
    unit: &str,
    value: Option<f64>,
    sample: usize,
    eligible: usize,
    definition: &str,
    ids: Vec<String>,
) {
    rows.push(MetricValue {
        id: id.into(),
        name: name.into(),
        unit: unit.into(),
        value,
        sample,
        eligible,
        definition: definition.into(),
        method: VERSION.into(),
        game_ids: ids,
        status: if value.is_some() {
            "available"
        } else if sample == 0 {
            "missing_input"
        } else {
            "no_sample"
        }
        .into(),
        ..Default::default()
    });
}
fn pct(n: usize, d: usize) -> Option<f64> {
    (d > 0).then(|| 100. * n as f64 / d as f64)
}
fn sd(v: &[f64]) -> Option<f64> {
    metrics::mean(v)
        .map(|m| (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt())
}
fn selected_phase(node: &ReplayNode, filter: &StatisticsFilter) -> bool {
    filter.phase.as_ref().is_none_or(|p| *p == node.phase)
}
fn complete_comparison(c: &MoveComparison, color: &str) -> bool {
    c.complete
        && c.method == "same_root_searchmoves"
        && q(&c.best, color)
            .zip(q(&c.played, color))
            .is_some_and(|(a, b)| metrics::q_delta(b, a) <= 0.02)
}
fn full(g: &StoredGame, r: &EngineRun) -> bool {
    g.replay.nodes.iter().all(|n| {
        r.evaluations
            .iter()
            .find(|e| e.ply == n.ply)
            .and_then(|e| q(e, &g.own_color))
            .is_some()
    }) && g.replay.nodes.iter().skip(1).all(|n| {
        r.confirmed_moves
            .iter()
            .find(|c| c.ply == n.ply)
            .is_some_and(|c| complete_comparison(c, n.actor.as_deref().unwrap_or(&g.own_color)))
    })
}
fn episodes(g: &StoredGame, r: &EngineRun, advantage: bool) -> Vec<Episode> {
    let mut output = Vec::new();
    let mut pending = None;
    let mut active: Option<Episode> = None;
    for n in g.replay.nodes.iter().skip(1) {
        let before = r
            .evaluations
            .iter()
            .find(|e| e.ply + 1 == n.ply)
            .and_then(|e| q(e, &g.own_color));
        let after = r
            .evaluations
            .iter()
            .find(|e| e.ply == n.ply)
            .and_then(|e| q(e, &g.own_color));
        let own = n.actor.as_deref() == Some(&g.own_color);
        let valid = !own
            || r.confirmed_moves
                .iter()
                .find(|c| c.ply == n.ply)
                .is_some_and(|c| complete_comparison(c, &g.own_color));
        // Available node WDL cannot fill an unobserved/unstable own decision.
        if !valid {
            pending = None;
            if let Some(mut e) = active.take() {
                e.end_ply = n.ply.saturating_sub(1);
                e.status = "incomplete".into();
                output.push(e);
            }
            continue;
        }
        if own && active.is_none() {
            let entry = before.is_some_and(|v| if advantage { v >= 0.90 } else { v <= 0.10 });
            if entry && valid {
                if let Some(start) = pending {
                    active = Some(Episode {
                        game_id: g.id.clone(),
                        run_id: r.id.clone(),
                        kind: if advantage { "advantage" } else { "defense" }.into(),
                        start_ply: start,
                        confirmation_ply: n.ply,
                        status: "incomplete".into(),
                        phase: g.replay.nodes[start.saturating_sub(1) as usize]
                            .phase
                            .clone(),
                        ..Default::default()
                    });
                    pending = None;
                } else {
                    pending = Some(n.ply);
                }
            } else {
                pending = None;
            }
        }
        let exit = after.is_some_and(|v| if advantage { v < 0.80 } else { v > 0.20 });
        if exit || after.is_none() {
            pending = None;
            if let Some(mut e) = active.take() {
                e.end_ply = if exit { n.ply } else { n.ply.saturating_sub(1) };
                e.exit_ply = exit.then_some(n.ply);
                e.status = if exit { "exited" } else { "incomplete" }.into();
                output.push(e);
            }
        }
    }
    if let Some(mut e) = active {
        e.terminal_ply = g.replay.nodes.last().map(|n| n.ply);
        e.end_ply = e.terminal_ply.unwrap_or(e.start_ply);
        e.status = "terminal".into();
        output.push(e);
    }
    let gm = metrics::game_metrics(g, Some(r));
    for e in &mut output {
        let end = e.end_ply;
        let own: Vec<_> = gm
            .decisions
            .iter()
            .filter(|d| d.ply >= e.start_ply && d.ply <= end)
            .collect();
        e.own_decisions = own.len();
        e.first_loss_ply = own
            .iter()
            .find(|d| d.loss_q.is_some_and(|v| v >= 10.))
            .map(|d| d.ply);
        e.total_loss = own.iter().filter_map(|d| d.loss_q).sum();
    }
    output
}

pub fn append(
    report: &mut StatisticsReport,
    games: &[&StoredGame],
    runs: &HashMap<String, &EngineRun>,
    filter: &StatisticsFilter,
) {
    let mut rows = Vec::new();
    let mut d = EngineDetails {
        version: VERSION.into(),
        tablebase_status: "not_configured".into(),
        ..Default::default()
    };
    let ids = games.iter().map(|g| g.id.clone()).collect::<Vec<_>>();
    let eligible =
        games
            .iter()
            .flat_map(|g| {
                g.replay.nodes.iter().skip(1).filter(|n| {
                    n.actor.as_deref() == Some(&g.own_color) && selected_phase(n, filter)
                })
            })
            .count();
    let mut losses = Vec::new();
    let mut finite = Vec::new();
    let mut p_losses = Vec::new();
    let mut capped = Vec::new();
    let mut accuracies = Vec::new();
    let mut game_accuracies = Vec::new();
    let mut game_iqrs = Vec::new();
    let mut game_sds = Vec::new();
    let mut game_p90s = Vec::new();
    let mut trajectory_sample = (0usize, 0usize);
    let mut acceptable = (0, 0);
    let mut advice_counts = BTreeMap::<&str, usize>::new();
    let mut advice_n = 0;
    let mut windows = Vec::new();
    let mut opportunity_n = 0;
    let mut gains = Vec::new();
    let mut responses = Vec::new();
    let mut restorative = Vec::new();
    let mut mate_n = 0;
    let mut mate_lost = 0;
    let mut mate_candidates = 0;
    let mut rechecked = 0;
    let mut decisions = Vec::<(&StoredGame, DecisionEvent)>::new();
    let mut complete_games = HashSet::new();
    for &g in games {
        let Some(&r) = runs.get(&g.id) else {
            continue;
        };
        let gm = metrics::game_metrics(g, Some(r));
        if full(g, r) {
            complete_games.insert(g.id.clone());
        }
        if let Some(a) = metrics::game_accuracy(g, r, filter.phase.as_deref()).0 {
            game_accuracies.push(a);
            case(
                &mut d,
                "Q06",
                g,
                r,
                0,
                "Accuracy партии",
                Some(a),
                "%",
                "Взвешенное среднее и гармоническое; окна исходной последовательности",
            );
        }
        for n in &g.replay.nodes {
            let evaluation = r.evaluations.iter().find(|e| e.ply == n.ply);
            let previous = n
                .ply
                .checked_sub(1)
                .and_then(|p| r.evaluations.iter().find(|e| e.ply == p));
            let valid = evaluation.is_some_and(usable);
            if selected_phase(n, filter) {
                trajectory_sample.1 += 1;
                if evaluation.and_then(|e| q(e, &g.own_color)).is_some() {
                    trajectory_sample.0 += 1;
                }
            }
            let sign = if g.own_color == "white" { 1 } else { -1 };
            d.trajectory.push(TrajectoryPoint {
                game_id: g.id.clone(),
                run_id: r.id.clone(),
                ply: n.ply,
                cp: evaluation
                    .filter(|_| valid)
                    .and_then(|e| e.cp)
                    .map(|v| v * sign),
                mate: evaluation
                    .filter(|_| valid)
                    .and_then(|e| e.mate)
                    .map(|v| v * sign),
                wdl: evaluation
                    .filter(|_| valid)
                    .and_then(|e| e.wdl)
                    .map(|w| if sign == 1 { w } else { [w[2], w[1], w[0]] }),
                q: evaluation.and_then(|e| q(e, &g.own_color)),
                pov: g.own_color.clone(),
                status: if valid {
                    "available"
                } else {
                    "missing_or_bound"
                }
                .into(),
                accuracy: gm
                    .move_accuracies
                    .iter()
                    .find(|(p, _)| *p == n.ply)
                    .map(|(_, v)| *v),
                advice: if n.actor.as_deref() == Some(&g.own_color) {
                    previous
                        .zip(evaluation)
                        .and_then(|(a, b)| advice(a, b, &g.own_color))
                        .map(str::to_owned)
                } else {
                    None
                },
            });
        }
        let mut own_losses = Vec::new();
        for n in g
            .replay
            .nodes
            .iter()
            .skip(1)
            .filter(|n| selected_phase(n, filter))
        {
            let c = r
                .confirmed_moves
                .iter()
                .find(|c| c.ply == n.ply && c.complete);
            if n.actor.as_deref() == Some(&g.own_color) {
                if let Some(e) = gm.decisions.iter().find(|e| e.ply == n.ply) {
                    if let Some(l) = e.loss_q {
                        losses.push(l);
                        own_losses.push((n.ply, l));
                    }
                    if let Some(v) = e.cp_loss {
                        finite.push(v);
                    }
                    if let Some(v) = e.loss_pl {
                        p_losses.push(v);
                    }
                    if let Some((a, b)) = e.before_q.zip(e.after_q)
                        && e.loss_q.is_some()
                        && a >= 0.5
                    {
                        acceptable.1 += 1;
                        if b >= 0.48 {
                            acceptable.0 += 1;
                        }
                    }
                    decisions.push((g, e.clone()));
                }
                if let Some((_, a)) = gm.move_accuracies.iter().find(|(p, _)| *p == n.ply) {
                    accuracies.push(*a);
                    case(
                        &mut d,
                        "Q05",
                        g,
                        r,
                        n.ply,
                        "Accuracy хода",
                        Some(*a),
                        "%",
                        "Соседние оценки, upstream начальная cp=15 и uncertainty bonus +1",
                    );
                }
                let before = r.evaluations.iter().find(|e| e.ply + 1 == n.ply);
                let after = r.evaluations.iter().find(|e| e.ply == n.ply);
                if let Some((a, b)) = before.zip(after).filter(|(a, b)| usable(a) && usable(b)) {
                    advice_n += 1;
                    if let Some(label) = advice(a, b, &g.own_color) {
                        *advice_counts.entry(label).or_default() += 1;
                        case(
                            &mut d,
                            "Q08",
                            g,
                            r,
                            n.ply,
                            label,
                            None,
                            "",
                            format!(
                                "raw before cp={:?},mate={:?}; after cp={:?},mate={:?}; consecutive/local scores",
                                a.cp, a.mate, b.cp, b.mate
                            ),
                        );
                    }
                    let initial = if n.ply == 1 && g.replay.initial_fen == chess::INITIAL_FEN {
                        Some(15. * actor_sign(&g.own_color))
                    } else {
                        capped_score(a, &g.own_color)
                    };
                    if let Some((a, b)) = initial.zip(capped_score(b, &g.own_color)) {
                        capped.push((a - b).max(0.));
                    }
                }
                if let Some(c) = c
                    && mate(&c.best, &g.own_color).is_some_and(|m| m > 0)
                {
                    mate_n += 1;
                    let lost = mate(&c.played, &g.own_color).is_none_or(|m| m <= 0);
                    if lost {
                        mate_candidates += 1;
                        if let Some(check) = r
                            .mate_rechecks
                            .iter()
                            .find(|x| x.ply == n.ply && x.complete)
                        {
                            rechecked += 1;
                            if mate(&check.best, &g.own_color).is_some_and(|m| m > 0)
                                && mate(&check.played, &g.own_color).is_none_or(|m| m <= 0)
                            {
                                mate_lost += 1;
                                case(
                                    &mut d,
                                    "E05_mate",
                                    g,
                                    r,
                                    n.ply,
                                    "Матовый путь не найден при перепроверке",
                                    None,
                                    "",
                                    "Независимая same-root перепроверка с двойным бюджетом; отсутствие оценки mate не доказывает отсутствие любого форсированного мата",
                                );
                            }
                        }
                    }
                }
            } else if let Some(c) = c
                && let Some((a, b)) = q(&c.best, &g.own_color).zip(q(&c.played, &g.own_color))
            {
                opportunity_n += 1;
                let gain = metrics::q_delta(b, a);
                if gain >= 0.10 {
                    gains.push(gain * 100.);
                    case(
                        &mut d,
                        "E01",
                        g,
                        r,
                        n.ply,
                        "Возможность",
                        Some(gain * 100.),
                        "п.п.",
                        "Same-root улучшение после хода соперника",
                    );
                    if let Some(reply) = r
                        .confirmed_moves
                        .iter()
                        .find(|x| x.ply == n.ply + 1 && x.complete)
                        && let Some((root, after)) =
                            q(&reply.best, &g.own_color).zip(q(&reply.played, &g.own_color))
                        && metrics::q_delta(root, b).abs() <= 0.05
                        && metrics::q_delta(after, root) <= 0.02
                    {
                        let used = metrics::q_delta(root, after).max(0.) <= 0.02;
                        let share = (metrics::q_delta(after, a) / gain).clamp(0., 1.) * 100.;
                        responses.push((used, share));
                        if a < 0.5 {
                            restorative.push((used, after >= 0.5));
                        }
                        case(
                            &mut d,
                            "E03",
                            g,
                            r,
                            n.ply + 1,
                            "Сохранено улучшения",
                            Some(share),
                            "%",
                            format!("q0={a:.4},q1={b:.4},q2={after:.4}"),
                        );
                    }
                }
            }
        }
        let values = own_losses.iter().map(|(_, v)| *v).collect::<Vec<_>>();
        if let Some(iqr) = metrics::quantile(&values, 0.75)
            .zip(metrics::quantile(&values, 0.25))
            .map(|(a, b)| a - b)
        {
            game_iqrs.push(iqr);
            case(
                &mut d,
                "Q11",
                g,
                r,
                0,
                "Внутрипартийный IQR потерь",
                Some(iqr),
                "п.п.",
                "Квантили пригодных личных Lq одной партии",
            );
        }
        if let Some(v) = sd(&values) {
            game_sds.push(v);
        }
        if let Some(v) = metrics::quantile(&values, 0.9) {
            game_p90s.push(v);
            case(
                &mut d,
                "Q11_p90",
                g,
                r,
                0,
                "Внутрипартийный P90 потерь",
                Some(v),
                "п.п.",
                "P90 пригодных собственных решений одной партии; единицы не смешаны между играми",
            );
        }
        for w in own_losses.windows(3) {
            if w.windows(2).all(|p| p[1].0 == p[0].0 + 2) {
                let sum = w.iter().map(|(_, l)| l).sum::<f64>();
                windows.push(sum);
                case(
                    &mut d,
                    "Q11_window",
                    g,
                    r,
                    w[0].0,
                    "Потери за три решения",
                    Some(sum),
                    "п.п.",
                    "Только три последовательных пригодных личных решения; gap не склеивается",
                );
            }
        }
        d.episodes.extend(episodes(g, r, true));
        d.episodes.extend(episodes(g, r, false));
    }
    row(
        &mut rows,
        "Q01",
        "ACPL",
        "cp",
        metrics::mean(&finite),
        finite.len(),
        eligible,
        "Среднее finite same-root max(0,cp_best−cp_played); mate/bounds/unstable исключены",
        ids.clone(),
    );
    row(
        &mut rows,
        "Q01_capped",
        "ACPL · capped adjacent",
        "cp",
        metrics::mean(&capped),
        capped.len(),
        eligible,
        "Отдельная серия: соседние white-POV scores, cp ceiling ±1000, mate→ceiling, стандартная начальная cp=15; не raw same-root",
        ids.clone(),
    );
    for (id, name, v) in [
        ("Q02", "Потеря ожидаемого очка", metrics::mean(&losses)),
        (
            "Q02_median",
            "Потеря ожидаемого очка · медиана",
            metrics::quantile(&losses, 0.5),
        ),
        (
            "Q02_p90",
            "Потеря ожидаемого очка · P90",
            metrics::quantile(&losses, 0.9),
        ),
    ] {
        row(
            &mut rows,
            id,
            name,
            "п.п.",
            v,
            losses.len(),
            eligible,
            "Lq=100 max(0,q_best−q_played); единый same-root профиль",
            ids.clone(),
        );
    }
    row(
        &mut rows,
        "Q03",
        "Потеря Lichess WinPercent",
        "п.п.",
        metrics::mean(&p_losses),
        p_losses.len(),
        eligible,
        "Same-root pL с ceiling ±1000; не WDL q",
        ids.clone(),
    );
    row(
        &mut rows,
        "Q03_median",
        "Потеря WinPercent · медиана",
        "п.п.",
        metrics::quantile(&p_losses, 0.5),
        p_losses.len(),
        eligible,
        "Медиана same-root pL loss",
        ids.clone(),
    );
    let (valid_nodes, node_n) = trajectory_sample;
    row(
        &mut rows,
        "Q04",
        "Покрытие WDL по позициям",
        "%",
        pct(valid_nodes, node_n),
        valid_nodes,
        node_n,
        "Каждая позиция имеет отдельные W/D/L,q,POV и status в trajectory; terminal из правил, bounds не точечная оценка",
        ids.clone(),
    );
    row(
        &mut rows,
        "Q05",
        "Accuracy хода · среднее",
        "%",
        metrics::mean(&accuracies),
        accuracies.len(),
        eligible,
        "Среднее индивидуальных Accuracy; каждое значение доступно по ходу",
        ids.clone(),
    );
    row(
        &mut rows,
        "Q06",
        "Accuracy партии · среднее",
        "%",
        metrics::mean(&game_accuracies),
        game_accuracies.len(),
        games.len(),
        "Среднее готовых game Accuracy; не Accuracy группы Insights",
        ids.clone(),
    );
    row(
        &mut rows,
        "Q07",
        "Accuracy · группа Insights",
        "%",
        metrics::harmonic(&accuracies),
        accuracies.len(),
        eligible,
        "H(a)=n/Σ(1/max(1,a)) от выбранных личных решений",
        ids.clone(),
    );
    for label in ["inaccuracy", "mistake", "blunder"] {
        row(
            &mut rows,
            &format!("Q08_{label}_count"),
            &format!(
                "Число {}",
                match label {
                    "inaccuracy" => "неточностей",
                    "mistake" => "ошибок",
                    _ => "зевков",
                }
            ),
            "случаев",
            (advice_n > 0).then_some(*advice_counts.get(label).unwrap_or(&0) as f64),
            advice_n,
            eligible,
            "Count конкретного класса Advice; старший класс один на ход",
            ids.clone(),
        );
        row(
            &mut rows,
            &format!("Q08_{label}"),
            match label {
                "inaccuracy" => "Неточности Lichess Advice",
                "mistake" => "Ошибки Lichess Advice",
                _ => "Зевки Lichess Advice",
            },
            "на 100 решений",
            pct(*advice_counts.get(label).unwrap_or(&0), advice_n),
            advice_n,
            eligible,
            "Соседние raw local scores; winningChances пороги 0.1/0.2/0.3 без ceiling; отдельные upstream mate branches",
            ids.clone(),
        );
    }
    for t in [5, 10, 20] {
        let events = losses
            .iter()
            .copied()
            .filter(|l| *l >= t as f64)
            .collect::<Vec<_>>();
        let id = if t == 10 {
            "Q09".into()
        } else {
            format!("Q09_{t}")
        };
        row(
            &mut rows,
            &format!("{id}_count"),
            &format!("Число потерь ≥{t} п.п."),
            "случаев",
            (!losses.is_empty()).then_some(events.len() as f64),
            losses.len(),
            eligible,
            "Число событий Lq заданного порога среди пригодных личных решений",
            ids.clone(),
        );
        row(
            &mut rows,
            &id,
            &format!("Потери ≥{t} п.п."),
            "на 100 решений",
            pct(events.len(), losses.len()),
            losses.len(),
            eligible,
            "Пороговый фильтр Lq; forced legal choices сохранены в evidence",
            ids.clone(),
        );
        if t == 10 {
            row(
                &mut rows,
                "Q09_sum",
                "Сумма существенных потерь",
                "п.п.",
                (!losses.is_empty()).then(|| events.iter().sum()),
                events.len(),
                losses.len(),
                "Сумма Lq событий ≥10; величина не аддитивная вероятность поражения",
                ids.clone(),
            );
            row(
                &mut rows,
                "Q09_median",
                "Существенная потеря · медиана",
                "п.п.",
                metrics::quantile(&events, 0.5),
                events.len(),
                losses.len(),
                "Медиана Lq среди событий ≥10",
                ids.clone(),
            );
        }
    }
    for (forced, id, label) in [
        (
            false,
            "Q09_choices",
            "Существенные потери при наличии выбора",
        ),
        (
            true,
            "Q09_forced",
            "Существенные потери при единственном легальном ходе",
        ),
    ] {
        let selected = decisions
            .iter()
            .filter(|(g, e)| {
                g.replay
                    .nodes
                    .get(e.ply.saturating_sub(1) as usize)
                    .is_some_and(|n| {
                        if forced {
                            n.legal_moves == 1
                        } else {
                            n.legal_moves > 1
                        }
                    })
            })
            .filter_map(|(_, e)| e.loss_q)
            .collect::<Vec<_>>();
        row(
            &mut rows,
            id,
            label,
            "на 100 решений",
            pct(
                selected.iter().filter(|l| **l >= 10.).count(),
                selected.len(),
            ),
            selected.len(),
            eligible,
            "Раздельный legal-choice срез по числу легальных ходов ДО решения; единственный legal move не объявляется трудностью",
            ids.clone(),
        );
    }
    row(
        &mut rows,
        "Q10",
        "Сохранено приемлемое состояние",
        "%",
        pct(acceptable.0, acceptable.1),
        acceptable.1,
        eligible,
        "q_after≥0.48 только среди q_before≥0.50; epsilon=.02",
        ids.clone(),
    );
    for (id, name, v) in [
        (
            "Q11_iqr",
            "Внутрипартийный IQR потерь · среднее",
            metrics::mean(&game_iqrs),
        ),
        (
            "Q11_sd",
            "Внутрипартийный SD потерь · среднее",
            metrics::mean(&game_sds),
        ),
        (
            "Q11_p90",
            "Внутрипартийный P90 потерь · среднее",
            metrics::mean(&game_p90s),
        ),
        (
            "Q11_max",
            "Наибольшая потеря",
            losses.iter().copied().reduce(f64::max),
        ),
        (
            "Q11_window",
            "Максимум потерь за три решения",
            windows.iter().copied().reduce(f64::max),
        ),
    ] {
        row(
            &mut rows,
            id,
            name,
            "п.п.",
            v,
            if id == "Q11_window" {
                windows.len()
            } else if id == "Q11_iqr" {
                game_iqrs.len()
            } else if id == "Q11_sd" {
                game_sds.len()
            } else if id == "Q11_p90" {
                game_p90s.len()
            } else {
                losses.len()
            },
            eligible,
            "k=3 последовательных личных решения; SD population; техническая неоднородность, не психологический диагноз",
            ids.clone(),
        );
    }
    append_forced_dispersion(&mut rows, &mut d, games, runs, &decisions, eligible);
    for (lo, hi) in [
        (0., Some(1.)),
        (1., Some(5.)),
        (5., Some(10.)),
        (10., Some(20.)),
        (20., None),
    ] {
        let cases = decisions
            .iter()
            .filter(|(_, e)| {
                e.loss_q
                    .is_some_and(|l| l >= lo && hi.is_none_or(|h| l < h))
            })
            .collect::<Vec<_>>();
        d.histogram.push(LossBin {
            lo,
            hi,
            decisions: cases.len(),
            game_ids: cases
                .iter()
                .map(|(g, _)| g.id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect(),
        });
    }
    append_opportunities(
        &mut rows,
        &ids,
        eligible,
        opportunity_n,
        &gains,
        &responses,
        &restorative,
        mate_n,
        mate_lost,
        rechecked,
        mate_candidates,
    );
    append_episodes(
        &mut rows,
        &mut d,
        games,
        runs,
        &complete_games,
        &decisions,
        filter,
    );
    append_phases_endings(&mut rows, &mut d, games, runs, &decisions, filter);
    report.metrics.retain(|m| {
        !m.id.starts_with('Q')
            && !m.id.starts_with('E')
            && !(m.id.starts_with('S')
                && m.id[1..]
                    .chars()
                    .take(2)
                    .collect::<String>()
                    .parse::<u32>()
                    .is_ok_and(|n| n <= 8))
    });
    report.metrics.extend(rows);
    report.analysis = d;
}

#[allow(clippy::too_many_arguments)]
fn append_opportunities(
    rows: &mut Vec<MetricValue>,
    ids: &[String],
    eligible: usize,
    opponent_n: usize,
    gains: &[f64],
    responses: &[(bool, f64)],
    restorative: &[(bool, bool)],
    mate_n: usize,
    mate_lost: usize,
    rechecked: usize,
    mate_candidates: usize,
) {
    row(
        rows,
        "E01",
        "Возможности после ошибок соперника",
        "случаев",
        (opponent_n > 0).then_some(gains.len() as f64),
        opponent_n,
        opponent_n,
        "G=q_after−q_before≥0.10 на пригодном ходе соперника; события с ответом и без ответа отдельно",
        ids.to_vec(),
    );
    row(
        rows,
        "E01_rate",
        "Возможности на 100 ходов соперника",
        "на 100 ходов",
        pct(gains.len(), opponent_n),
        opponent_n,
        opponent_n,
        "Число G≥0.10 / все пригодные same-root ходы соперника",
        ids.to_vec(),
    );
    row(
        rows,
        "E01_gain",
        "Полученное улучшение · медиана",
        "п.п.",
        metrics::quantile(gains, 0.5),
        gains.len(),
        opponent_n,
        "Медиана 100G среди возможностей",
        ids.to_vec(),
    );
    for (suffix, label, value) in [
        ("mean", "среднее", metrics::mean(gains)),
        ("p90", "P90", metrics::quantile(gains, 0.9)),
    ] {
        row(
            rows,
            &format!("E01_gain_{suffix}"),
            &format!("Полученное улучшение · {label}"),
            "п.п.",
            value,
            gains.len(),
            opponent_n,
            "Распределение 100G среди возможностей G≥.10; каждый ход соперника один раз",
            ids.to_vec(),
        );
    }
    row(
        rows,
        "E02",
        "Использовано возможностей",
        "%",
        pct(
            responses.iter().filter(|(used, _)| *used).count(),
            responses.len(),
        ),
        responses.len(),
        gains.len(),
        "R≤0.02 среди G≥0.10 с пригодным ответом; root согласие ≤0.05; без ответа исключено",
        ids.to_vec(),
    );
    row(
        rows,
        "E01_unanswered",
        "Возможности без пригодного ответа",
        "случаев",
        (opponent_n > 0).then_some(gains.len().saturating_sub(responses.len()) as f64),
        gains.len(),
        gains.len(),
        "Включает завершение до ответа и missing/unstable оценки ответа",
        ids.to_vec(),
    );
    let shares = responses.iter().map(|(_, v)| *v).collect::<Vec<_>>();
    row(
        rows,
        "E03",
        "Сохранённая доля улучшения",
        "%",
        metrics::mean(&shares),
        shares.len(),
        gains.len(),
        "100 clamp((q2−q0)/(q1−q0),0,1); G≥0.10",
        ids.to_vec(),
    );
    row(
        rows,
        "E03_median",
        "Сохранённая доля улучшения · медиана",
        "%",
        metrics::quantile(&shares, 0.5),
        shares.len(),
        gains.len(),
        "Медиана сохранённой доли G≥0.10",
        ids.to_vec(),
    );
    row(
        rows,
        "E04",
        "Использовано восстановительных возможностей",
        "%",
        pct(
            restorative.iter().filter(|(u, _)| *u).count(),
            restorative.len(),
        ),
        restorative.len(),
        responses.len(),
        "E02 при q0<0.50; улучшение худшей позиции не означает выигрыш",
        ids.to_vec(),
    );
    row(
        rows,
        "E04_equal",
        "Восстановительные ответы к q≥0.50",
        "%",
        pct(
            restorative.iter().filter(|(_, equal)| *equal).count(),
            restorative.len(),
        ),
        restorative.len(),
        responses.len(),
        "Доля q2≥0.50 после G≥0.10 при q0<0.50",
        ids.to_vec(),
    );
    row(
        rows,
        "E05_mate_rate",
        "Утрата матового пути на 100 возможностей",
        "%",
        (rechecked == mate_candidates)
            .then(|| pct(mate_lost, mate_n))
            .flatten(),
        mate_n.saturating_sub(mate_candidates) + rechecked,
        mate_n,
        "Подтверждённые потери / пригодные собственные mate roots; при непроверенном кандидате результат отсутствует",
        ids.to_vec(),
    );
    row(
        rows,
        "E05_mate",
        "Матовый путь не найден при перепроверке",
        "случаев",
        (mate_n > 0 && rechecked == mate_candidates).then_some(mate_lost as f64),
        mate_n.saturating_sub(mate_candidates) + rechecked,
        mate_n,
        "Лишь повторно подтверждённые own mate→finite/opponent mate из одного корня с двойным бюджетом; не доказательство отсутствия любого мата",
        ids.to_vec(),
    );
    let _ = eligible;
}
fn append_episodes(
    rows: &mut Vec<MetricValue>,
    d: &mut EngineDetails,
    games: &[&StoredGame],
    runs: &HashMap<String, &EngineRun>,
    complete: &HashSet<String>,
    decisions: &[(&StoredGame, DecisionEvent)],
    filter: &StatisticsFilter,
) {
    let ids = games.iter().map(|g| g.id.clone()).collect::<Vec<_>>();
    for (kind, id) in [("advantage", "E06"), ("defense", "E10_frequency")] {
        let entered = d
            .episodes
            .iter()
            .filter(|e| {
                e.kind == kind
                    && complete.contains(&e.game_id)
                    && filter.phase.as_ref().is_none_or(|p| *p == e.phase)
            })
            .map(|e| e.game_id.clone())
            .collect::<HashSet<_>>();
        row(
            rows,
            id,
            if kind == "advantage" {
                "Партии с устойчивым преимуществом"
            } else {
                "Партии с трудным состоянием"
            },
            "%",
            pct(entered.len(), complete.len()),
            complete.len(),
            games.len(),
            "Уникальные игры с подтверждённым входом / полностью анализированные игры; два личных решения, после каждого хода проверяется выход",
            entered.into_iter().collect(),
        );
    }
    for (kind, prefix, entry_name) in [
        ("advantage", "E08", "Реализация преимущества"),
        ("defense", "E10", "Спасено трудных партий"),
    ] {
        let eps = d
            .episodes
            .iter()
            .filter(|e| e.kind == kind && filter.phase.as_ref().is_none_or(|p| *p == e.phase))
            .collect::<Vec<_>>();
        let entered = eps
            .iter()
            .map(|e| e.game_id.clone())
            .collect::<HashSet<_>>();
        let entered_games = games
            .iter()
            .filter(|g| entered.contains(&g.id))
            .copied()
            .collect::<Vec<_>>();
        outcome_rows(rows, prefix, entry_name, &entered_games, games.len());
        let success = entered_games
            .iter()
            .filter(|g| {
                if kind == "advantage" {
                    metrics::outcome(g) == "win"
                } else {
                    metrics::outcome(g) != "loss"
                }
            })
            .count();
        let scores = entered_games
            .iter()
            .filter_map(|g| metrics::own_score(g))
            .map(|s| 100. * s)
            .collect::<Vec<_>>();
        row(
            rows,
            prefix,
            entry_name,
            "%",
            pct(success, entered_games.len()),
            entered_games.len(),
            games.len(),
            "Одна завершённая игра с подтверждённым входом в знаменателе один раз; преимущество W/N, защита (W+D)/N",
            entered.iter().cloned().collect(),
        );
        row(
            rows,
            &format!("{prefix}_score"),
            if kind == "advantage" {
                "Score после преимущества"
            } else {
                "Score после трудного состояния"
            },
            "%",
            metrics::mean(&scores),
            scores.len(),
            entered_games.len(),
            "100(W+0.5D)/N среди игр с подтверждённым входом",
            entered.iter().cloned().collect(),
        );
        row(
            rows,
            &format!("{prefix}_episodes"),
            if kind == "advantage" {
                "Эпизоды преимущества"
            } else {
                "Защитные эпизоды"
            },
            "эпизодов",
            (!complete.is_empty() || !eps.is_empty()).then_some(eps.len() as f64),
            eps.len(),
            games.len(),
            "Эпизоды считаются отдельно от уникальных игр",
            entered.iter().cloned().collect(),
        );
        for phase in ["opening", "middlegame", "endgame"] {
            let first = entered_games
                .iter()
                .filter(|g| {
                    eps.iter()
                        .filter(|e| e.game_id == g.id)
                        .min_by_key(|e| e.start_ply)
                        .is_some_and(|e| e.phase == phase)
                })
                .copied()
                .collect::<Vec<_>>();
            let succeeded = first
                .iter()
                .filter(|g| {
                    if kind == "advantage" {
                        metrics::outcome(g) == "win"
                    } else {
                        metrics::outcome(g) != "loss"
                    }
                })
                .count();
            row(
                rows,
                &format!("{prefix}_{phase}"),
                &format!("{entry_name} · {}", phase_name(phase)),
                "%",
                pct(succeeded, first.len()),
                first.len(),
                entered_games.len(),
                "Фаза первого подтверждённого входа; один game один раз",
                first.iter().map(|g| g.id.clone()).collect(),
            );
        }
    }
    let advantage = d
        .episodes
        .iter()
        .filter(|e| e.kind == "advantage" && filter.phase.as_ref().is_none_or(|p| *p == e.phase))
        .collect::<Vec<_>>();
    let fully = advantage
        .iter()
        .filter(|e| e.status != "incomplete")
        .copied()
        .collect::<Vec<_>>();
    let retained = fully.iter().filter(|e| e.exit_ply.is_none()).count();
    row(
        rows,
        "E07",
        "Преимущество сохранено до завершения",
        "%",
        pct(retained, fully.len()),
        fully.len(),
        advantage.len(),
        "Полностью наблюдаемые подтверждённые episodes без q<0.80 / fully observed episodes; incomplete исключены",
        fully.iter().map(|e| e.game_id.clone()).collect(),
    );
    let lengths = fully
        .iter()
        .map(|e| e.own_decisions as f64)
        .collect::<Vec<_>>();
    row(
        rows,
        "E07_length",
        "Длина эпизода преимущества · медиана",
        "решений",
        metrics::quantile(&lengths, 0.5),
        lengths.len(),
        advantage.len(),
        "Число личных решений от start до exit/terminal включительно",
        ids.clone(),
    );
    for (kind, id) in [("advantage", "E09"), ("defense", "E11")] {
        let mut seen = HashSet::new();
        let mut vals = Vec::new();
        let matched = d
            .episodes
            .iter()
            .filter(|e| e.kind == kind && filter.phase.as_ref().is_none_or(|p| *p == e.phase))
            .cloned()
            .collect::<Vec<_>>();
        for e in matched {
            let end = e.end_ply;
            for (g, ev) in decisions
                .iter()
                .filter(|(g, ev)| g.id == e.game_id && ev.ply >= e.start_ply && ev.ply <= end)
            {
                if seen.insert((g.id.clone(), ev.ply))
                    && let Some(v) = ev.loss_q
                {
                    vals.push(v);
                    if v >= 10.
                        && let Some(r) = runs.get(&g.id)
                    {
                        case(
                            d,
                            id,
                            g,
                            r,
                            ev.ply,
                            "Потеря внутри эпизода",
                            Some(v),
                            "п.п.",
                            "Личное решение внутри подтверждённого эпизода",
                        );
                    }
                }
            }
        }
        row(
            rows,
            id,
            if kind == "advantage" {
                "Потери при реализации"
            } else {
                "Защитные потери"
            },
            "п.п.",
            metrics::mean(&vals),
            vals.len(),
            decisions.len(),
            "Среднее Lq личных решений внутри эпизода; выходное решение включено",
            ids.clone(),
        );
        row(
            rows,
            &format!("{id}_median"),
            if kind == "advantage" {
                "Потери при реализации · медиана"
            } else {
                "Защитные потери · медиана"
            },
            "п.п.",
            metrics::quantile(&vals, 0.5),
            vals.len(),
            decisions.len(),
            "Медиана личных Lq внутри эпизодов",
            ids.clone(),
        );
        row(
            rows,
            &format!("{id}_errors"),
            if kind == "advantage" {
                "Ошибки при реализации"
            } else {
                "Защитные ошибки"
            },
            "на 100 решений",
            pct(vals.iter().filter(|v| **v >= 10.).count(), vals.len()),
            vals.len(),
            decisions.len(),
            "Lq≥10 на 100 пригодных решений эпизода",
            ids.clone(),
        );
    }
    let defenses = d
        .episodes
        .iter()
        .filter(|e| {
            e.kind == "defense"
                && complete.contains(&e.game_id)
                && filter.phase.as_ref().is_none_or(|p| *p == e.phase)
        })
        .map(|e| e.game_id.clone())
        .collect::<HashSet<_>>();
    let mut recovery_actor = BTreeMap::<String, usize>::new();
    for g in games.iter().filter(|g| defenses.contains(&g.id)) {
        let Some(r) = runs.get(&g.id) else {
            continue;
        };
        let start = d
            .episodes
            .iter()
            .filter(|e| {
                e.kind == "defense"
                    && e.game_id == g.id
                    && filter.phase.as_ref().is_none_or(|p| *p == e.phase)
            })
            .map(|e| e.start_ply)
            .min()
            .unwrap_or(u32::MAX);
        if let Some(e) = r
            .evaluations
            .iter()
            .filter(|e| e.ply >= start && q(e, &g.own_color).is_some_and(|q| q >= 0.5))
            .min_by_key(|e| e.ply)
        {
            let actor = g
                .replay
                .nodes
                .get(e.ply as usize)
                .and_then(|n| n.actor.as_deref());
            let key = if actor == Some(g.own_color.as_str()) {
                "own"
            } else {
                "opponent"
            };
            *recovery_actor.entry(key.into()).or_default() += 1;
            case(
                d,
                "E11_recovery",
                g,
                r,
                e.ply,
                "Первое наблюдаемое восстановление",
                q(e, &g.own_color).map(|v| v * 100.),
                "%",
                format!(
                    "actor={key}; q≥.50 впервые после входа; актор последнего хода обозначает наблюдаемую последовательность, а не доказанную причину"
                ),
            );
            if let Some(c) = d.cases.last_mut() {
                c.position_ply = e.ply;
            }
        }
    }
    for key in ["own", "opponent"] {
        row(
            rows,
            &format!("E11_recovery_{key}"),
            if key == "own" {
                "Возврат наблюдался после собственного хода"
            } else {
                "Возврат наблюдался после хода соперника"
            },
            "партий",
            (!defenses.is_empty()).then_some(*recovery_actor.get(key).unwrap_or(&0) as f64),
            defenses.len(),
            games.len(),
            "Первый узел q≥.50 после defense entry; только fully observed entered games, без причинной интерпретации",
            defenses.iter().cloned().collect(),
        );
    }
    let recovered = games
        .iter()
        .filter(|g| defenses.contains(&g.id))
        .filter(|g| {
            runs.get(&g.id).is_some_and(|r| {
                let start = d
                    .episodes
                    .iter()
                    .filter(|e| {
                        e.kind == "defense"
                            && e.game_id == g.id
                            && filter.phase.as_ref().is_none_or(|p| *p == e.phase)
                    })
                    .map(|e| e.start_ply)
                    .min()
                    .unwrap_or(u32::MAX);
                r.evaluations
                    .iter()
                    .any(|e| e.ply >= start && q(e, &g.own_color).is_some_and(|q| q >= 0.5))
            })
        })
        .count();
    row(
        rows,
        "E11_recovery",
        "Возврат к q≥0.50",
        "%",
        pct(recovered, defenses.len()),
        defenses.len(),
        games.len(),
        "Только полностью наблюдаемые entered games; последующий q≥0.50, без причинного вывода",
        defenses.into_iter().collect(),
    );
}
#[derive(Clone)]
struct Segment<'a> {
    game: &'a StoredGame,
    run: Option<&'a EngineRun>,
    start: u32,
    end: u32,
    kind: String,
}
fn segments<'a>(
    games: &[&'a StoredGame],
    runs: &HashMap<String, &'a EngineRun>,
) -> Vec<Segment<'a>> {
    let mut all = Vec::new();
    for &g in games {
        let mut active: Option<Segment> = None;
        for n in &g.replay.nodes {
            if n.phase != "endgame" {
                if let Some(s) = active.take() {
                    all.push(s);
                }
                continue;
            }
            if active.as_ref().is_some_and(|s| s.kind != n.material.ending) {
                all.push(active.take().unwrap());
            }
            if let Some(s) = &mut active {
                s.end = n.ply;
            } else {
                active = Some(Segment {
                    game: g,
                    run: runs.get(&g.id).copied(),
                    start: n.ply,
                    end: n.ply,
                    kind: n.material.ending.clone(),
                });
            }
        }
        if let Some(s) = active {
            all.push(s);
        }
    }
    all
}
fn append_phases_endings(
    rows: &mut Vec<MetricValue>,
    d: &mut EngineDetails,
    games: &[&StoredGame],
    runs: &HashMap<String, &EngineRun>,
    decisions: &[(&StoredGame, DecisionEvent)],
    filter: &StatisticsFilter,
) {
    let ids = games.iter().map(|g| g.id.clone()).collect::<Vec<_>>();
    for phase in ["opening", "middlegame", "endgame"] {
        if filter.phase.as_ref().is_some_and(|p| *p != phase) {
            continue;
        }
        let reached = games
            .iter()
            .filter(|g| g.replay.nodes.iter().any(|n| n.phase == phase))
            .copied()
            .collect::<Vec<_>>();
        let lens = reached
            .iter()
            .map(|g| {
                g.replay
                    .nodes
                    .iter()
                    .skip(1)
                    .filter(|n| n.phase == phase && n.actor.as_deref() == Some(&g.own_color))
                    .count() as f64
            })
            .collect::<Vec<_>>();
        row(
            rows,
            &format!("S01_{phase}"),
            &format!("Достигнута фаза · {}", phase_name(phase)),
            "%",
            pct(reached.len(), games.len()),
            games.len(),
            games.len(),
            "Уникальные игры с наблюдаемой фазой / games с legal replay",
            reached.iter().map(|g| g.id.clone()).collect(),
        );
        row(
            rows,
            &format!("S01_{phase}_length"),
            &format!("Длина фазы · {}", phase_name(phase)),
            "решений",
            metrics::quantile(&lens, 0.5),
            lens.len(),
            games.len(),
            "Медиана числа личных решений в достигнутой фазе; отсутствующая фаза исключена",
            reached.iter().map(|g| g.id.clone()).collect(),
        );
        let starts = reached
            .iter()
            .filter_map(|g| {
                g.replay
                    .nodes
                    .iter()
                    .find(|n| n.phase == phase)
                    .map(|n| n.ply as f64)
            })
            .collect::<Vec<_>>();
        row(
            rows,
            &format!("S01_{phase}_start"),
            &format!("Начало фазы · {}", phase_name(phase)),
            "полуходов",
            metrics::quantile(&starts, 0.5),
            starts.len(),
            games.len(),
            "Медиана локального ply первой позиции достигнутой фазы; исходный FEN не заменяется стартовой позицией",
            reached.iter().map(|g| g.id.clone()).collect(),
        );
        let vals = decisions
            .iter()
            .filter(|(_, e)| e.phase == phase)
            .filter_map(|(_, e)| e.loss_q)
            .collect::<Vec<_>>();
        row(
            rows,
            &format!("S02_{phase}"),
            &format!("Потеря ожидаемого очка · {}", phase_name(phase)),
            "п.п.",
            metrics::mean(&vals),
            vals.len(),
            lens.iter().sum::<f64>() as usize,
            "Среднее same-root Lq в фазе",
            ids.clone(),
        );
        row(
            rows,
            &format!("S02_{phase}_errors"),
            &format!("Существенные ошибки · {}", phase_name(phase)),
            "на 100 решений",
            pct(vals.iter().filter(|l| **l >= 10.).count(), vals.len()),
            vals.len(),
            decisions.len(),
            "Lq≥10 / пригодные решения фазы",
            ids.clone(),
        );
        let mut opponent_n = 0;
        let mut opportunities = 0;
        let mut replies = 0;
        let mut used = 0;
        for &g in games {
            if let Some(r) = runs.get(&g.id) {
                for node in g.replay.nodes.iter().skip(1).filter(|n| {
                    n.phase == phase
                        && selected_phase(n, filter)
                        && n.actor.as_deref() != Some(&g.own_color)
                }) {
                    if let Some(c) = r
                        .confirmed_moves
                        .iter()
                        .find(|c| c.ply == node.ply && c.complete)
                        && let Some((a, b)) =
                            q(&c.best, &g.own_color).zip(q(&c.played, &g.own_color))
                    {
                        opponent_n += 1;
                        if metrics::q_delta(b, a) >= 0.10 {
                            opportunities += 1;
                            if let Some(reply) = r
                                .confirmed_moves
                                .iter()
                                .find(|c| c.ply == node.ply + 1 && c.complete)
                                && let Some((root, after)) =
                                    q(&reply.best, &g.own_color).zip(q(&reply.played, &g.own_color))
                                && metrics::q_delta(root, b).abs() <= 0.05
                                && metrics::q_delta(after, root) <= 0.02
                            {
                                replies += 1;
                                if metrics::q_delta(root, after).max(0.) <= 0.02 {
                                    used += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
        row(
            rows,
            &format!("S02_{phase}_opportunities"),
            &format!("Возможности · {}", phase_name(phase)),
            "на 100 ходов",
            pct(opportunities, opponent_n),
            opponent_n,
            opponent_n,
            "E01 по фазе хода соперника; ответ может пересекать границу фазы",
            ids.clone(),
        );
        row(
            rows,
            &format!("S02_{phase}_used"),
            &format!("Использовано возможностей · {}", phase_name(phase)),
            "%",
            pct(used, replies),
            replies,
            opportunities,
            "E02 в фазе возникновения возможности; без пригодного ответа исключено",
            ids.clone(),
        );
        let accuracy = games
            .iter()
            .filter_map(|g| {
                runs.get(&g.id)
                    .and_then(|r| metrics::game_accuracy(g, r, Some(phase)).0)
            })
            .collect::<Vec<_>>();
        row(
            rows,
            &format!("S02_{phase}_accuracy"),
            &format!("Accuracy фазы · {}", phase_name(phase)),
            "%",
            metrics::mean(&accuracy),
            accuracy.len(),
            reached.len(),
            "Game algorithm на фазовом срезе с предыдущей оценкой; среднее отдельных фазовых процентов",
            ids.clone(),
        );
    }
    let segs = if filter.phase.as_deref().is_none_or(|p| p == "endgame") {
        segments(games, runs)
    } else {
        Vec::new()
    };
    let kinds = segs.iter().map(|s| s.kind.clone()).collect::<HashSet<_>>();
    for kind in kinds {
        let ss = segs.iter().filter(|s| s.kind == kind).collect::<Vec<_>>();
        let gset = ss.iter().map(|s| s.game.id.clone()).collect::<HashSet<_>>();
        let selected = games
            .iter()
            .filter(|g| gset.contains(&g.id))
            .copied()
            .collect::<Vec<_>>();
        outcome_rows(
            rows,
            &format!("S03_{kind}"),
            &format!("Окончание · {}", ending_name(&kind)),
            &selected,
            games.len(),
        );
        let scores = selected
            .iter()
            .filter_map(|g| metrics::own_score(g))
            .map(|v| 100. * v)
            .collect::<Vec<_>>();
        row(
            rows,
            &format!("S03_{kind}_games"),
            &format!("Окончание · {}", ending_name(&kind)),
            "партий",
            Some(gset.len() as f64),
            gset.len(),
            games.len(),
            "Уникальные игры; одна игра может содержать несколько типов",
            gset.iter().cloned().collect(),
        );
        row(
            rows,
            &format!("S03_{kind}_segments"),
            &format!("Сегменты · {}", ending_name(&kind)),
            "сегментов",
            Some(ss.len() as f64),
            ss.len(),
            gset.len(),
            "Непрерывные интервалы одного material grouping; повторный вход отдельный сегмент",
            gset.iter().cloned().collect(),
        );
        row(
            rows,
            &format!("S03_{kind}_score"),
            &format!("Score окончания · {}", ending_name(&kind)),
            "%",
            metrics::mean(&scores),
            scores.len(),
            gset.len(),
            "W/D/L по уникальным завершённым играм этого типа",
            gset.iter().cloned().collect(),
        );
        append_ending_stratum(
            rows,
            d,
            &format!("S04_{kind}"),
            ending_name(&kind),
            &ss,
            decisions,
            games.len(),
        );
        for axis in ["q", "material", "remaining"] {
            let bins = ss
                .iter()
                .filter_map(|seg| segment_bin(seg, axis).map(|b| b.0))
                .collect::<std::collections::BTreeSet<_>>();
            for bin in bins {
                let subset = ss
                    .iter()
                    .copied()
                    .filter(|seg| segment_bin(seg, axis).is_some_and(|b| b.0 == bin))
                    .collect::<Vec<_>>();
                let label = segment_bin(subset[0], axis).unwrap().1;
                append_ending_stratum(
                    rows,
                    d,
                    &format!("S04_context_{kind}_{axis}_{bin}"),
                    &format!("{} · {label}", ending_name(&kind)),
                    &subset,
                    decisions,
                    games.len(),
                );
            }
        }
    }

    let ending = decisions
        .iter()
        .filter(|(g, e)| {
            g.replay
                .nodes
                .get(e.ply.saturating_sub(1) as usize)
                .is_some_and(|n| n.phase == "endgame")
                && selected_phase(&g.replay.nodes[e.ply as usize], filter)
        })
        .collect::<Vec<_>>();
    let equal = ending
        .iter()
        .filter(|(_, e)| {
            e.loss_q.is_some()
                && e.before_q.is_some_and(|v| (0.45..=0.55).contains(&v))
                && e.after_q.is_some()
        })
        .copied()
        .collect::<Vec<_>>();
    let retained = equal
        .iter()
        .filter(|(_, e)| e.after_q.unwrap() >= 0.45)
        .count();
    row(
        rows,
        "S05_engine",
        "Сохранено не хуже диапазона равенства",
        "%",
        pct(retained, equal.len()),
        equal.len(),
        ending.len(),
        "Отдельная ENGINE серия: before q∈[.45,.55], after q≥.45; не доказанная табличная ничья",
        ids.clone(),
    );
    let excess = ending
        .iter()
        .filter(|(g, e)| {
            let n = &g.replay.nodes[e.ply.saturating_sub(1) as usize];
            actor_sign(&g.own_color) * n.material.balance as f64 > 0.
        })
        .copied()
        .collect::<Vec<_>>();
    let loss = excess
        .iter()
        .filter_map(|(_, e)| e.loss_q)
        .collect::<Vec<_>>();
    row(
        rows,
        "S07",
        "Потери при материальном перевесе в окончании",
        "п.п.",
        metrics::mean(&loss),
        loss.len(),
        excess.len(),
        "Signed material>0 перед ходом; P1/N3/B3/R5/Q9, exact signature в evidence; не engine advantage",
        ids.clone(),
    );
    row(
        rows,
        "S07_errors",
        "Ошибки при материальном перевесе",
        "на 100 решений",
        pct(loss.iter().filter(|v| **v >= 10.).count(), loss.len()),
        loss.len(),
        excess.len(),
        "Lq≥10 / пригодные решения с material>0",
        ids.clone(),
    );
    let entered_excess = d
        .episodes
        .iter()
        .filter(|e| e.kind == "advantage")
        .filter(|e| {
            games
                .iter()
                .find(|g| g.id == e.game_id)
                .and_then(|g| {
                    g.replay
                        .nodes
                        .get(e.start_ply.saturating_sub(1) as usize)
                        .map(|n| (*g, n))
                })
                .is_some_and(|(g, n)| {
                    n.phase == "endgame"
                        && filter.phase.as_ref().is_none_or(|p| *p == n.phase)
                        && actor_sign(&g.own_color) * n.material.balance as f64 > 0.
                })
        })
        .map(|e| e.game_id.clone())
        .collect::<HashSet<_>>();
    let converted_excess = games
        .iter()
        .filter(|g| entered_excess.contains(&g.id) && metrics::outcome(g) == "win")
        .count();
    row(
        rows,
        "S07_conversion",
        "Реализация преимущества при материальном перевесе",
        "%",
        pct(converted_excess, entered_excess.len()),
        entered_excess.len(),
        games.len(),
        "E08 с подтверждённым входом в эндшпиле и signed material>0 при входе; не любой материальный перевес объявляется выигрышем",
        entered_excess.into_iter().collect(),
    );
    append_material_strata(rows, d, &excess, &segs, runs);
    let mut first = HashSet::new();
    for (g, e) in &equal {
        if e.after_q.is_some_and(|v| v <= 0.10)
            && first.insert(g.id.clone())
            && let Some(r) = runs.get(&g.id)
        {
            case(
                d,
                "S08",
                g,
                r,
                e.ply,
                "Равное → трудное окончание",
                e.loss_q,
                "п.п.",
                "Первый собственный same-root переход q∈[.45,.55]→q≤.10",
            );
        }
    }
    row(
        rows,
        "S08",
        "Переходы из равного в трудное окончание",
        "на 100 решений",
        pct(first.len(), equal.len()),
        equal.len(),
        ending.len(),
        "Первый переход на игру / пригодные band decisions; equal=[.45,.55], difficult≤.10",
        first.into_iter().collect(),
    );
    let mut tb_n = 0;
    let mut drawn = 0;
    let mut held = 0;
    let mut worsened = 0;
    let mut winlost = 0;
    let mut tb_wins = 0;
    let mut dtz_growth = 0;
    let mut seen_tb = false;
    for &g in games {
        let Some(r) = runs.get(&g.id) else {
            continue;
        };
        if r.tablebases.iter().any(|o| o.status != "not_configured") {
            seen_tb = true;
        }
        for n in g
            .replay
            .nodes
            .iter()
            .skip(1)
            .filter(|n| n.actor.as_deref() == Some(&g.own_color) && selected_phase(n, filter))
        {
            let a = r
                .tablebases
                .iter()
                .find(|o| o.ply + 1 == n.ply && o.status == "available" && o.rule50);
            let b = r
                .tablebases
                .iter()
                .find(|o| o.ply == n.ply && o.status == "available" && o.rule50);
            let Some((a, b)) = a
                .zip(b)
                .filter(|(a, b)| a.manifest_id == b.manifest_id && !a.manifest_id.is_empty())
            else {
                continue;
            };
            let pair_dtz = a.dtz.zip(b.dtz);
            let Some((a, b)) = a.wdl.zip(b.wdl) else {
                continue;
            };
            let outcome = |w: i32| {
                let w = if g.own_color == "white" { w } else { -w };
                if w == 2 {
                    1
                } else if w == -2 {
                    -1
                } else {
                    0
                }
            };
            let before = outcome(a);
            let after = outcome(b);
            tb_n += 1;
            if before == 1 {
                tb_wins += 1;
            }
            if before == 1
                && after == 1
                && let Some((da, db)) = pair_dtz
                && db.abs() > da.abs()
            {
                dtz_growth += 1;
                case(
                    d,
                    "S06_dtz_growth",
                    g,
                    r,
                    n.ply,
                    "Рост DTZ при сохранённом W",
                    Some((db.abs() - da.abs()) as f64),
                    "полуходов",
                    "W→W не ухудшение WDL; DTZ не расстояние до мата",
                );
            }
            if before == 0 {
                drawn += 1;
                if after >= 0 {
                    held += 1;
                }
            }
            if before > after {
                worsened += 1;
                if before == 1 {
                    winlost += 1;
                }
                case(
                    d,
                    "S06",
                    g,
                    r,
                    n.ply,
                    "Табличное ухудшение результата",
                    Some((before - after) as f64),
                    "классов",
                    "Успешные exact root+continuation probes, одинаковый manifest, rule50 и безопасный zeroing context; рост DTZ W→W не ошибка",
                );
            }
        }
    }
    d.tablebase_status = if tb_n > 0 {
        "available"
    } else if seen_tb {
        "missing_tables_or_rule_context"
    } else {
        "not_configured"
    }
    .into();
    row(
        rows,
        "S05_tb",
        "Сохранены табличные ничьи",
        "%",
        pct(held, drawn),
        drawn,
        tb_n,
        "Только exact D roots и успешный continuation; cursed/blessed treated D under50; отсутствующие таблицы не ничья",
        ids.clone(),
    );
    row(
        rows,
        "S06",
        "Табличные ухудшения результата",
        "на 100 решений",
        pct(worsened, tb_n),
        tb_n,
        ending.len(),
        "Actor-POV W→D/L и D→L; only exact safe-context probes; никаких UCI tbhits доказательств",
        ids.clone(),
    );
    row(
        rows,
        "S06_count",
        "Число табличных ухудшений",
        "случаев",
        (tb_n > 0).then_some(worsened as f64),
        tb_n,
        tb_n,
        "Число exact W→D/L и D→L; тот же знаменатель, что S06",
        ids.clone(),
    );
    row(
        rows,
        "S06_dtz_growth",
        "Рост DTZ при сохранении W",
        "случаев",
        (tb_n > 0).then_some(dtz_growth as f64),
        tb_wins,
        tb_n,
        "Отдельные W→W изменения DTZ; не потеря табличного исхода",
        ids.clone(),
    );
    row(
        rows,
        "E05_tb_rate",
        "Утрата табличного выигрыша на 100 W",
        "%",
        pct(winlost, tb_wins),
        tb_wins,
        tb_n,
        "Число W→D/L / пригодные exact TB-W решения, не все TB решения",
        ids.clone(),
    );
    row(
        rows,
        "E05_tb",
        "Утрата табличного выигрыша",
        "случаев",
        (tb_wins > 0).then_some(winlost as f64),
        tb_wins,
        tb_n,
        "Exact W→D/L отдельно от mateloss, manifest и policy сохранены",
        ids,
    );
    for id in [
        "S05_tb",
        "S06",
        "S06_count",
        "S06_dtz_growth",
        "E05_tb",
        "E05_tb_rate",
    ] {
        if let Some(m) = rows.iter_mut().find(|m| m.id == id && m.value.is_none()) {
            m.status = "blocked_dependency".into();
        }
    }
}

fn outcome_rows(
    rows: &mut Vec<MetricValue>,
    prefix: &str,
    label: &str,
    games: &[&StoredGame],
    eligible: usize,
) {
    for (suffix, outcome, name) in [
        ("wins", "win", "Победы"),
        ("draws", "draw", "Ничьи"),
        ("losses", "loss", "Поражения"),
    ] {
        row(
            rows,
            &format!("{prefix}_{suffix}"),
            &format!("{label} · {name}"),
            "партий",
            (!games.is_empty()).then_some(
                games
                    .iter()
                    .filter(|g| metrics::outcome(g) == outcome)
                    .count() as f64,
            ),
            games.len(),
            eligible,
            "Исходы уникальных завершённых игр данного условия; один game один раз",
            games.iter().map(|g| g.id.clone()).collect(),
        );
    }
}
fn append_forced_dispersion(
    rows: &mut Vec<MetricValue>,
    d: &mut EngineDetails,
    games: &[&StoredGame],
    runs: &HashMap<String, &EngineRun>,
    decisions: &[(&StoredGame, DecisionEvent)],
    eligible: usize,
) {
    let mut per_game = HashMap::<&str, Vec<&DecisionEvent>>::new();
    for (g, e) in decisions {
        per_game.entry(&g.id).or_default().push(e);
    }
    for forced in [false, true] {
        let suffix = if forced { "forced" } else { "choices" };
        let label = if forced {
            "единственный легальный ход"
        } else {
            "есть выбор"
        };
        let mut iqrs = Vec::new();
        let mut sds = Vec::new();
        let mut p90s = Vec::new();
        let mut all = Vec::new();
        let mut sums = Vec::new();
        for &g in games {
            let vals = per_game
                .get(g.id.as_str())
                .into_iter()
                .flatten()
                .filter(|e| {
                    g.replay
                        .nodes
                        .get(e.ply.saturating_sub(1) as usize)
                        .is_some_and(|n| {
                            if forced {
                                n.legal_moves == 1
                            } else {
                                n.legal_moves > 1
                            }
                        })
                })
                .filter_map(|e| e.loss_q.map(|v| (e.ply, v)))
                .collect::<Vec<_>>();
            let losses = vals.iter().map(|(_, v)| *v).collect::<Vec<_>>();
            all.extend(losses.iter().copied());
            if let Some(v) = metrics::quantile(&losses, 0.75)
                .zip(metrics::quantile(&losses, 0.25))
                .map(|(a, b)| a - b)
            {
                iqrs.push(v);
            }
            if let Some(v) = sd(&losses) {
                sds.push(v);
            }
            if let Some(v) = metrics::quantile(&losses, 0.9) {
                p90s.push(v);
                if let Some(r) = runs.get(&g.id) {
                    case(
                        d,
                        &format!("Q11_{suffix}_p90"),
                        g,
                        r,
                        0,
                        &format!("P90 · {label}"),
                        Some(v),
                        "п.п.",
                        "P90 отдельной партии на legal-choice срезе; отсутствующие решения не нули",
                    );
                }
            }
            for w in vals
                .windows(3)
                .filter(|w| w.windows(2).all(|p| p[1].0 == p[0].0 + 2))
            {
                sums.push(w.iter().map(|(_, v)| v).sum::<f64>());
            }
        }
        let ids = games.iter().map(|g| g.id.clone()).collect::<Vec<_>>();
        for (stat, name, value, n) in [
            (
                "iqr",
                "Средний внутрипартийный IQR",
                metrics::mean(&iqrs),
                iqrs.len(),
            ),
            (
                "sd",
                "Средний внутрипартийный SD",
                metrics::mean(&sds),
                sds.len(),
            ),
            (
                "p90",
                "Средний внутрипартийный P90",
                metrics::mean(&p90s),
                p90s.len(),
            ),
            (
                "max",
                "Максимальная потеря",
                all.iter().copied().reduce(f64::max),
                all.len(),
            ),
            (
                "window",
                "Максимум окна трёх решений",
                sums.iter().copied().reduce(f64::max),
                sums.len(),
            ),
        ] {
            row(
                rows,
                &format!("Q11_{suffix}_{stat}"),
                &format!("{name} · {label}"),
                "п.п.",
                value,
                n,
                eligible,
                "Legal-choice срез; IQR/SD/P90 сначала внутри каждой игры, затем среднее; окно не пересекает пропуски или решения другого среза",
                ids.clone(),
            );
        }
    }
}
fn q_bin(v: f64) -> (&'static str, &'static str) {
    if v <= 0.10 {
        ("difficult", "q входа ≤10%")
    } else if v < 0.45 {
        ("worse", "q входа 10–45%")
    } else if v <= 0.55 {
        ("equal", "q входа 45–55%")
    } else if v < 0.90 {
        ("better", "q входа 55–90%")
    } else {
        ("advantage", "q входа ≥90%")
    }
}
fn segment_bin(s: &Segment<'_>, axis: &str) -> Option<(String, String)> {
    match axis {
        "q" => s
            .run
            .and_then(|r| r.evaluations.iter().find(|e| e.ply == s.start))
            .and_then(|e| q(e, &s.game.own_color))
            .map(|v| {
                let (a, b) = q_bin(v);
                (a.into(), b.into())
            }),
        "material" => {
            let signed = actor_sign(&s.game.own_color)
                * s.game.replay.nodes[s.start as usize].material.balance as f64;
            Some(if signed > 0. {
                ("ahead".into(), "Материал при входе: перевес".into())
            } else if signed < 0. {
                ("behind".into(), "Материал при входе: недостаток".into())
            } else {
                ("equal".into(), "Материал при входе: равенство".into())
            })
        }
        "remaining" => s
            .game
            .replay
            .nodes
            .iter()
            .find(|n| {
                n.actor.as_deref() == Some(&s.game.own_color)
                    && n.ply.saturating_sub(1) >= s.start
                    && n.ply.saturating_sub(1) <= s.end
            })
            .and_then(|n| n.remaining_before)
            .filter(|v| v.is_finite() && *v >= 0.)
            .map(|v| {
                if v < 30. {
                    ("under30".into(), "Часы первого решения: <30 с".into())
                } else if v < 60. {
                    ("30to60".into(), "Часы первого решения: 30–60 с".into())
                } else if v < 180. {
                    ("60to180".into(), "Часы первого решения: 60–180 с".into())
                } else {
                    ("180plus".into(), "Часы первого решения: ≥180 с".into())
                }
            }),
        _ => None,
    }
}
#[allow(clippy::too_many_arguments)]
fn append_ending_stratum(
    rows: &mut Vec<MetricValue>,
    d: &mut EngineDetails,
    prefix: &str,
    label: &str,
    ss: &[&Segment<'_>],
    decisions: &[(&StoredGame, DecisionEvent)],
    eligible: usize,
) {
    let mut intervals = HashMap::<&str, Vec<(u32, u32)>>::new();
    let mut run_index = HashMap::<&str, (&StoredGame, Option<&EngineRun>)>::new();
    for seg in ss {
        intervals
            .entry(&seg.game.id)
            .or_default()
            .push((seg.start, seg.end));
        run_index.insert(&seg.game.id, (seg.game, seg.run));
    }
    let contains = |game: &str, ply: u32| {
        intervals.get(game).is_some_and(|ranges| {
            ranges
                .iter()
                .any(|(start, end)| ply >= *start && ply <= *end)
        })
    };
    let mut by_game = BTreeMap::<String, &StoredGame>::new();
    for s in ss {
        by_game.insert(s.game.id.clone(), s.game);
    }
    let games = by_game.values().copied().collect::<Vec<_>>();
    let ids = by_game.keys().cloned().collect::<Vec<_>>();
    let vals = decisions
        .iter()
        .filter(|(g, e)| contains(&g.id, e.ply.saturating_sub(1)))
        .filter_map(|(_, e)| e.loss_q)
        .collect::<Vec<_>>();
    let nodes = games
        .iter()
        .flat_map(|g| {
            g.replay
                .nodes
                .iter()
                .filter(|n| {
                    n.actor.as_deref() == Some(&g.own_color)
                        && contains(&g.id, n.ply.saturating_sub(1))
                })
                .map(|n| (*g, n))
        })
        .collect::<Vec<_>>();
    let spent = nodes
        .iter()
        .filter_map(|(_, n)| n.spent_seconds)
        .collect::<Vec<_>>();
    let shares = games
        .iter()
        .filter_map(|g| {
            let total = g
                .replay
                .nodes
                .iter()
                .filter(|n| n.actor.as_deref() == Some(&g.own_color))
                .filter_map(|n| n.spent_seconds)
                .sum::<f64>();
            let selected = nodes
                .iter()
                .filter(|(a, _)| a.id == g.id)
                .filter_map(|(_, n)| n.spent_seconds)
                .collect::<Vec<_>>();
            (total > 0. && !selected.is_empty())
                .then(|| 100. * selected.iter().sum::<f64>() / total)
        })
        .collect::<Vec<_>>();
    let entry_q = ss
        .iter()
        .filter_map(|s| {
            s.run
                .and_then(|r| r.evaluations.iter().find(|e| e.ply == s.start))
                .and_then(|e| q(e, &s.game.own_color))
                .map(|v| v * 100.)
        })
        .collect::<Vec<_>>();
    let remaining = ss
        .iter()
        .filter_map(|s| {
            s.game
                .replay
                .nodes
                .iter()
                .find(|n| {
                    n.actor.as_deref() == Some(&s.game.own_color)
                        && n.ply.saturating_sub(1) >= s.start
                        && n.ply.saturating_sub(1) <= s.end
                })
                .and_then(|n| n.remaining_before)
        })
        .collect::<Vec<_>>();
    for (suffix, name, unit, value, n) in [
        ("loss", "Потери", "п.п.", metrics::mean(&vals), vals.len()),
        (
            "loss_median",
            "Медиана потерь",
            "п.п.",
            metrics::quantile(&vals, 0.5),
            vals.len(),
        ),
        (
            "loss_p90",
            "P90 потерь",
            "п.п.",
            metrics::quantile(&vals, 0.9),
            vals.len(),
        ),
        (
            "errors",
            "Существенные потери",
            "на 100 решений",
            pct(vals.iter().filter(|v| **v >= 10.).count(), vals.len()),
            vals.len(),
        ),
        (
            "time",
            "Время решения · медиана",
            "с",
            metrics::quantile(&spent, 0.5),
            spent.len(),
        ),
        (
            "time_total",
            "Известный расход времени",
            "с",
            (!spent.is_empty()).then(|| spent.iter().sum()),
            spent.len(),
        ),
        (
            "time_share",
            "Доля известного игрового расхода",
            "%",
            metrics::mean(&shares),
            shares.len(),
        ),
        (
            "time_coverage",
            "Покрытие расхода времени",
            "%",
            pct(spent.len(), nodes.len()),
            spent.len(),
        ),
        (
            "entry_q",
            "Ожидаемое очко при входе · медиана",
            "%",
            metrics::quantile(&entry_q, 0.5),
            entry_q.len(),
        ),
        (
            "remaining",
            "Часы первого решения · медиана",
            "с",
            metrics::quantile(&remaining, 0.5),
            remaining.len(),
        ),
    ] {
        row(
            rows,
            &format!("{prefix}_{suffix}"),
            &format!("{name} · {label}"),
            unit,
            value,
            n,
            if suffix == "time_coverage" {
                nodes.len()
            } else {
                ss.len()
            },
            "Сегменты данного типа и entry-context; решения по составу ДО хода. Доля часов: сначала sum(known type spent)/sum(known all-game own spent) в каждой игре, затем среднее; неизвестные часы исключены. Контекст — условная связь, не причина",
            ids.clone(),
        );
    }
    let advantages = d
        .episodes
        .iter()
        .filter(|e| e.kind == "advantage" && contains(&e.game_id, e.start_ply.saturating_sub(1)))
        .map(|e| e.game_id.clone())
        .collect::<HashSet<_>>();
    let converted = games
        .iter()
        .filter(|g| advantages.contains(&g.id) && metrics::outcome(g) == "win")
        .count();
    row(
        rows,
        &format!("{prefix}_conversion"),
        &format!("Реализация входа · {label}"),
        "%",
        pct(converted, advantages.len()),
        advantages.len(),
        games.len(),
        "E08: подтверждённый вход в преимущество именно внутри этих material/context segments; одна игра один раз",
        advantages.iter().cloned().collect(),
    );
    let entered_games = games
        .iter()
        .copied()
        .filter(|g| advantages.contains(&g.id))
        .collect::<Vec<_>>();
    outcome_rows(
        rows,
        &format!("{prefix}_conversion"),
        label,
        &entered_games,
        eligible,
    );
    let defense_losses = decisions
        .iter()
        .filter(|(g, e)| {
            contains(&g.id, e.ply.saturating_sub(1))
                && d.episodes.iter().any(|ep| {
                    ep.kind == "defense"
                        && ep.game_id == g.id
                        && e.ply >= ep.start_ply
                        && e.ply <= ep.end_ply
                })
        })
        .filter_map(|(_, e)| e.loss_q)
        .collect::<Vec<_>>();
    row(
        rows,
        &format!("{prefix}_defense"),
        &format!("Защитные потери · {label}"),
        "п.п.",
        metrics::mean(&defense_losses),
        defense_losses.len(),
        nodes.len(),
        "E11: собственные решения в защитном эпизоде и данном сегменте",
        ids.clone(),
    );
    row(
        rows,
        &format!("{prefix}_defense_errors"),
        &format!("Защитные ошибки · {label}"),
        "на 100 решений",
        pct(
            defense_losses.iter().filter(|v| **v >= 10.).count(),
            defense_losses.len(),
        ),
        defense_losses.len(),
        nodes.len(),
        "Lq≥10 / пригодные собственные defensive decisions данного сегмента",
        ids.clone(),
    );
    let defenses = d
        .episodes
        .iter()
        .filter(|e| e.kind == "defense" && contains(&e.game_id, e.start_ply.saturating_sub(1)))
        .filter(|e| {
            run_index
                .get(e.game_id.as_str())
                .is_some_and(|(g, r)| r.is_some_and(|r| full(g, r)))
        })
        .collect::<Vec<_>>();
    let entered = defenses
        .iter()
        .map(|e| e.game_id.clone())
        .collect::<HashSet<_>>();
    let recovered = entered
        .iter()
        .filter(|id| {
            run_index.get(id.as_str()).is_some_and(|(g, run)| {
                run.is_some_and(|r| {
                    r.evaluations.iter().any(|v| {
                        contains(id, v.ply)
                            && defenses
                                .iter()
                                .any(|e| e.game_id == **id && v.ply >= e.start_ply)
                            && q(v, &g.own_color).is_some_and(|v| v >= 0.5)
                    })
                })
            })
        })
        .count();
    row(
        rows,
        &format!("{prefix}_recovery"),
        &format!("Возврат к q≥50% внутри типа · {label}"),
        "%",
        pct(recovered, entered.len()),
        entered.len(),
        games.len(),
        "E11: fully observed entered games; возврат к q≥.5 после входа и ещё внутри выбранного material/context segment; источник улучшения не объявляется собственной заслугой",
        entered.into_iter().collect(),
    );
    for s in ss {
        let empty = EngineRun::default();
        let r = s.run.unwrap_or(&empty);
        let signature = &s.game.replay.nodes[s.start as usize].material;
        case(
            d,
            if prefix.starts_with("S04_context_") {
                prefix
            } else {
                "S03"
            },
            s.game,
            r,
            s.start,
            &format!("Сегмент · {label}"),
            None,
            "",
            format!(
                "end={}; white={:?}; black={:?}; entry_q={:?}; entry_material={:?}; entry_remaining={:?}; grouping по наблюдаемому контексту, без причинного вывода",
                s.end,
                signature.white,
                signature.black,
                segment_bin(s, "q"),
                segment_bin(s, "material"),
                segment_bin(s, "remaining")
            ),
        );
    }
}
fn append_material_strata(
    rows: &mut Vec<MetricValue>,
    d: &mut EngineDetails,
    excess: &[&(&StoredGame, DecisionEvent)],
    segs: &[Segment<'_>],
    runs: &HashMap<String, &EngineRun>,
) {
    for axis in ["signature", "entry_q"] {
        let mut groups = BTreeMap::<String, (String, Vec<&(&StoredGame, DecisionEvent)>)>::new();
        for &event in excess {
            let (g, e) = event;
            let key = if axis == "signature" {
                let m = &g.replay.nodes[e.ply.saturating_sub(1) as usize].material;
                let label = format!("Белые P/N/B/R/Q {:?}; чёрные {:?}", m.white, m.black);
                Some((hash(label.as_bytes())[..12].to_string(), label))
            } else {
                segs.iter()
                    .find(|s| {
                        s.game.id == g.id
                            && e.ply.saturating_sub(1) >= s.start
                            && e.ply.saturating_sub(1) <= s.end
                    })
                    .and_then(|s| segment_bin(s, "q"))
            };
            if let Some((key, label)) = key {
                groups
                    .entry(key)
                    .or_insert_with(|| (label, Vec::new()))
                    .1
                    .push(event);
            }
        }
        for (key, (label, events)) in groups {
            let prefix = format!("S07_{axis}_{key}");
            let ids = events
                .iter()
                .map(|(g, _)| g.id.clone())
                .collect::<HashSet<_>>();
            let vals = events
                .iter()
                .filter_map(|(_, e)| e.loss_q)
                .collect::<Vec<_>>();
            for (suffix, name, unit, v) in [
                ("loss", "Потери", "п.п.", metrics::mean(&vals)),
                (
                    "median",
                    "Медиана потерь",
                    "п.п.",
                    metrics::quantile(&vals, 0.5),
                ),
                (
                    "errors",
                    "Существенные потери",
                    "на 100 решений",
                    pct(vals.iter().filter(|v| **v >= 10.).count(), vals.len()),
                ),
            ] {
                row(
                    rows,
                    &format!("{prefix}_{suffix}"),
                    &format!("{name} при перевесе · {label}"),
                    unit,
                    v,
                    vals.len(),
                    events.len(),
                    "S07 страта: exact signature ДО решения либо q входа в material segment; signed material>0, личный POV; без вывода о причинности",
                    ids.iter().cloned().collect(),
                );
            }
            let entered = d
                .episodes
                .iter()
                .filter(|e| {
                    e.kind == "advantage"
                        && events
                            .iter()
                            .any(|(g, n)| g.id == e.game_id && n.ply == e.start_ply)
                })
                .map(|e| e.game_id.clone())
                .collect::<HashSet<_>>();
            let won = events
                .iter()
                .map(|(g, _)| *g)
                .filter(|g| entered.contains(&g.id) && metrics::outcome(g) == "win")
                .map(|g| g.id.clone())
                .collect::<HashSet<_>>();
            row(
                rows,
                &format!("{prefix}_conversion"),
                &format!("Реализация при перевесе · {label}"),
                "%",
                pct(won.len(), entered.len()),
                entered.len(),
                ids.len(),
                "E08 среди игр с подтверждённым входом в эту exact material/entry-q страту",
                entered.into_iter().collect(),
            );
            for (g, e) in events {
                if let Some(r) = runs.get(&g.id) {
                    case(
                        d,
                        &prefix,
                        g,
                        r,
                        e.ply,
                        &label,
                        e.loss_q,
                        "п.п.",
                        "S07: наблюдаемые исходный материал и entry q; не объяснение причины ошибки",
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod qes_tests {
    use super::*;
    fn eval(ply: u32, units: u32) -> PositionEvaluation {
        PositionEvaluation {
            ply,
            cp: Some(units as i32 - 500),
            wdl: Some([units, 0, 1000 - units]),
            complete: true,
            ..Default::default()
        }
    }
    fn fixture(states: &[u32]) -> (StoredGame, EngineRun) {
        let mut replay = chess::replay_pgn("1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 1-0").unwrap();
        replay.nodes.truncate(states.len());
        let g = StoredGame {
            id: "game".into(),
            own_color: "white".into(),
            result: "1-0".into(),
            replay,
            ..Default::default()
        };
        let r = EngineRun {
            id: "run".into(),
            game_id: g.id.clone(),
            profile_id: "profile".into(),
            evaluations: states
                .iter()
                .enumerate()
                .map(|(i, v)| eval(i as u32, *v))
                .collect(),
            confirmed_moves: states
                .windows(2)
                .enumerate()
                .map(|(i, v)| MoveComparison {
                    ply: i as u32 + 1,
                    best: eval(i as u32, v[0]),
                    played: eval(i as u32, v[1]),
                    complete: true,
                    method: "same_root_searchmoves".into(),
                })
                .collect(),
            ..Default::default()
        };
        (g, r)
    }
    fn report(g: &StoredGame, r: &EngineRun) -> StatisticsReport {
        let mut out = StatisticsReport::default();
        append(
            &mut out,
            &[g],
            &HashMap::from([(g.id.clone(), r)]),
            &StatisticsFilter::default(),
        );
        out
    }
    fn value(r: &StatisticsReport, id: &str) -> Option<f64> {
        r.metrics
            .iter()
            .find(|m| m.id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
            .value
    }

    #[test]
    fn counts_and_within_game_p90_keep_their_own_denominators() {
        let (g, r) = fixture(&[500, 400, 500, 300, 500]);
        let out = report(&g, &r);
        assert_eq!(value(&out, "Q08_inaccuracy_count"), Some(1.));
        assert_eq!(value(&out, "Q08_blunder_count"), Some(1.));
        assert_eq!(value(&out, "Q09_count"), Some(2.));
        assert_eq!(value(&out, "Q09_20_count"), Some(1.));
        assert_eq!(value(&out, "Q11_p90"), Some(19.));
        assert_eq!(value(&out, "Q11_choices_p90"), Some(19.));
        assert_eq!(value(&out, "Q11_forced_p90"), None);
        let (mut g2, mut r2) = fixture(&[500, 500, 500]);
        g2.id = "second".into();
        r2.game_id = g2.id.clone();
        let mut combined = StatisticsReport::default();
        append(
            &mut combined,
            &[&g, &g2],
            &HashMap::from([(g.id.clone(), &r), (g2.id.clone(), &r2)]),
            &StatisticsFilter::default(),
        );
        assert_eq!(value(&combined, "Q11_p90"), Some(9.5)); // average game P90, not pooled P90=18
        let mut forced = g.clone();
        forced.replay.nodes[0].legal_moves = 1;
        let out = report(&forced, &r);
        assert_eq!(value(&out, "Q11_forced_p90"), Some(10.));
        assert_eq!(value(&out, "Q11_choices_p90"), Some(20.));
    }
    #[test]
    fn mate_and_synthetic_tablebase_rates_use_eligible_winning_roots() {
        let (g, mut r) = fixture(&[900, 500, 900, 900, 900]);
        for i in [0, 2] {
            r.confirmed_moves[i].best.cp = None;
            r.confirmed_moves[i].best.mate = Some(2);
        }
        r.confirmed_moves[2].played.cp = None;
        r.confirmed_moves[2].played.mate = Some(3);
        assert_eq!(value(&report(&g, &r), "E05_mate_rate"), None);
        let mut check = r.confirmed_moves[0].clone();
        check.method = "same_root_mate_recheck_double_budget_v1".into();
        r.mate_rechecks.push(check);
        assert_eq!(value(&report(&g, &r), "E05_mate_rate"), Some(50.));
        // Synthetic persisted observations exercise aggregation only, not successful disk probing.
        r.tablebases = [2, 0, 0, -2, -2]
            .into_iter()
            .enumerate()
            .map(|(p, w)| TablebaseObservation {
                ply: p as u32,
                wdl: Some(w),
                dtz: Some(2),
                status: "available".into(),
                manifest_id: "synthetic-manifest".into(),
                rule50: true,
                ..Default::default()
            })
            .collect();
        let out = report(&g, &r);
        assert_eq!(value(&out, "S06_count"), Some(2.));
        assert_eq!(value(&out, "S06"), Some(100.));
        assert_eq!(value(&out, "E05_tb"), Some(1.));
        assert_eq!(value(&out, "E05_tb_rate"), Some(100.));
        assert_eq!(
            out.metrics
                .iter()
                .find(|m| m.id == "E05_tb")
                .unwrap()
                .sample,
            1
        );
        r.tablebases[1].wdl = Some(2);
        r.tablebases[0].dtz = Some(2);
        r.tablebases[1].dtz = Some(5);
        let out = report(&g, &r);
        assert_eq!(value(&out, "S06_count"), Some(1.));
        assert_eq!(value(&out, "S06_dtz_growth"), Some(1.));
        assert_eq!(value(&out, "E05_tb_rate"), Some(0.));
    }
    #[test]
    fn missing_same_root_decision_ends_episode_as_incomplete_even_with_node_wdl() {
        let (g, mut r) = fixture(&[900, 900, 900, 900, 900, 900, 900, 900, 900]);
        r.confirmed_moves.retain(|c| c.ply != 5);
        let es = episodes(&g, &r, true);
        assert_eq!(es[0].status, "incomplete");
        assert_eq!(es[0].end_ply, 4);
        let out = report(&g, &r);
        assert_eq!(value(&out, "E07"), None);
        assert_eq!(value(&out, "E06"), None);
    }
    #[test]
    fn phase_filter_applies_to_episode_retention_recovery_and_ending_rows() {
        let (mut g, r) = fixture(&[900, 900, 900, 850, 900, 790, 950, 950, 950]);
        for n in &mut g.replay.nodes {
            n.phase = if n.ply < 5 { "opening" } else { "endgame" }.into();
        }
        let mut out = StatisticsReport::default();
        let filter = StatisticsFilter {
            phase: Some("endgame".into()),
            ..Default::default()
        };
        append(
            &mut out,
            &[&g],
            &HashMap::from([(g.id.clone(), &r)]),
            &filter,
        );
        assert_eq!(value(&out, "E07"), None);
        assert!(out.metrics.iter().all(|m| !m.id.starts_with("S02_opening")));
        let (mut g, r) = fixture(&[100, 100, 100, 100, 500, 500, 500]);
        for n in &mut g.replay.nodes {
            n.phase = if n.ply < 4 { "opening" } else { "endgame" }.into();
        }
        assert_eq!(value(&report(&g, &r), "E11_recovery"), Some(100.));
        append(
            &mut out,
            &[&g],
            &HashMap::from([(g.id.clone(), &r)]),
            &filter,
        );
        assert_eq!(value(&out, "E11_recovery"), None);
        let filter = StatisticsFilter {
            phase: Some("middlegame".into()),
            ..Default::default()
        };
        append(
            &mut out,
            &[&g],
            &HashMap::from([(g.id.clone(), &r)]),
            &filter,
        );
        assert!(
            out.metrics
                .iter()
                .all(|m| !m.id.starts_with("S03_") && !m.id.starts_with("S04_"))
        );
        assert_eq!(value(&out, "Q04"), None);
    }

    #[test]
    fn exact_wdl_quantum_thresholds_and_black_pov() {
        assert_eq!(metrics::q_delta(0.5, 0.4), 0.10);
        assert_eq!(metrics::q_delta(0.6, 0.58), 0.02);
        let black = PositionEvaluation {
            wdl: Some([100, 0, 900]),
            complete: true,
            ..Default::default()
        };
        assert_eq!(q(&black, "black"), Some(0.90));
        let (g, r) = fixture(&[500, 400, 500, 300, 500]);
        let out = report(&g, &r);
        assert_eq!(value(&out, "Q09"), Some(100.));
        assert_eq!(value(&out, "Q09_20"), Some(50.));
        assert_eq!(value(&out, "Q09_sum"), Some(30.));
        assert_eq!(value(&out, "Q09_median"), Some(15.));
    }
    #[test]
    fn opportunity_gain_and_reply_at_exact_boundaries() {
        let (g, r) = fixture(&[400, 400, 500, 480, 500]);
        let out = report(&g, &r);
        assert_eq!(value(&out, "E01"), Some(1.));
        assert_eq!(value(&out, "E01_rate"), Some(50.));
        assert_eq!(value(&out, "E02"), Some(100.));
        assert!((value(&out, "E03").unwrap() - 80.).abs() < 1e-10);
        assert_eq!(value(&out, "E04"), Some(100.));
        assert_eq!(value(&out, "E04_equal"), Some(0.));
    }
    #[test]
    fn acceptable_state_excludes_originally_worse_and_preserves_computed_zero() {
        let (g, r) = fixture(&[500, 480, 400, 100, 500]);
        let out = report(&g, &r);
        assert_eq!(value(&out, "Q10"), Some(100.));
        assert_eq!(
            out.metrics.iter().find(|m| m.id == "Q10").unwrap().sample,
            1
        );
        let (g, r) = fixture(&[500, 470, 400]);
        assert_eq!(value(&report(&g, &r), "Q10"), Some(0.));
        let (g, r) = fixture(&[400, 390, 400]);
        assert_eq!(value(&report(&g, &r), "Q10"), None);
    }
    #[test]
    fn advice_has_raw_cp_and_explicit_mate_branches() {
        let cp = |v| PositionEvaluation {
            cp: Some(v),
            complete: true,
            ..Default::default()
        };
        let mt = |v| PositionEvaluation {
            mate: Some(v),
            complete: true,
            ..Default::default()
        };
        assert_eq!(advice(&cp(0), &cp(-100), "white"), Some("inaccuracy"));
        assert_eq!(advice(&cp(0), &cp(-200), "white"), Some("blunder"));
        assert_eq!(advice(&cp(-1000), &mt(-2), "white"), Some("inaccuracy"));
        assert_eq!(advice(&cp(-800), &mt(-2), "white"), Some("mistake"));
        assert_eq!(advice(&cp(-700), &mt(-2), "white"), Some("blunder"));
        assert_eq!(advice(&mt(2), &cp(1000), "white"), Some("inaccuracy"));
        assert_eq!(advice(&mt(2), &cp(800), "white"), Some("mistake"));
        assert_eq!(advice(&mt(2), &mt(-3), "white"), Some("blunder"));
        assert_eq!(advice(&mt(2), &mt(4), "white"), None);
        assert_eq!(advice(&cp(0), &mt(0), "white"), None);
        let draw = PositionEvaluation {
            terminal_outcome: Some("draw".into()),
            complete: true,
            ..Default::default()
        };
        assert_eq!(advice(&mt(2), &draw, "white"), Some("blunder"));
        assert_eq!(advice(&cp(200), &draw, "white"), Some("blunder"));
    }
    #[test]
    fn episode_is_confirmed_twice_exits_and_does_not_bridge_missing_state() {
        let (g, r) = fixture(&[900, 900, 900, 850, 900, 790, 950, 950, 950]);
        let es = episodes(&g, &r, true);
        assert_eq!(es.len(), 1);
        assert_eq!(
            (es[0].start_ply, es[0].confirmation_ply, es[0].exit_ply),
            (1, 3, Some(5))
        );
        assert_eq!(es[0].own_decisions, 3);
        assert_eq!(es[0].status, "exited");
        let (tg, tr) = fixture(&[900, 900, 900, 900, 900]);
        assert_eq!(episodes(&tg, &tr, true)[0].status, "terminal");
        let (g, mut r) = fixture(&[900, 900, 900, 900, 900, 900, 900]);
        r.evaluations[4].complete = false;
        let es = episodes(&g, &r, true);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].status, "incomplete");
        assert_eq!(es[0].end_ply, 3);
        assert_eq!(es[0].own_decisions, 2);
        let out = report(&g, &r);
        assert_eq!(value(&out, "E06"), None);
        assert_eq!(value(&out, "E07"), None);
    }
    #[test]
    fn dispersion_is_computed_inside_games_and_windows_do_not_cross_gaps() {
        let (g, r) = fixture(&[500, 400, 500, 450, 500, 490, 500, 300, 500]);
        let out = report(&g, &r);
        assert_eq!(value(&out, "Q11_max"), Some(20.));
        assert_eq!(value(&out, "Q11_window"), Some(26.));
        assert_eq!(value(&out, "Q11_iqr"), Some(8.5));
        assert!(value(&out, "Q11_sd").unwrap() > 7.);
        let mut r = r;
        r.confirmed_moves[2].complete = false;
        let out = report(&g, &r);
        assert_eq!(value(&out, "Q11_window"), None);
    }
    #[test]
    fn trajectory_uses_personal_pov_and_exposes_move_accuracy() {
        let (mut g, r) = fixture(&[600, 550, 500]);
        g.own_color = "black".into();
        let out = report(&g, &r);
        assert_eq!(out.analysis.trajectory[0].pov, "black");
        assert_eq!(out.analysis.trajectory[0].wdl, Some([400, 0, 600]));
        assert_eq!(out.analysis.trajectory[0].q, Some(0.4));
        assert_eq!(out.analysis.trajectory[0].cp, Some(-100));
        assert!(out.analysis.trajectory[2].accuracy.is_some());
    }
    #[test]
    fn no_analysis_and_missing_tables_are_not_zero_results() {
        let (g, _) = fixture(&[500, 400, 500]);
        let mut out = StatisticsReport::default();
        append(
            &mut out,
            &[&g],
            &HashMap::new(),
            &StatisticsFilter::default(),
        );
        assert_eq!(value(&out, "Q01"), None);
        assert_eq!(value(&out, "E01"), None);
        assert_eq!(value(&out, "S06"), None);
        assert_eq!(
            out.metrics.iter().find(|m| m.id == "S06").unwrap().status,
            "blocked_dependency"
        );
        assert!(
            probe_tablebases(&g, None)
                .iter()
                .all(|o| o.wdl.is_none() && o.status == "not_configured")
        );
    }
    #[test]
    fn syzygy_manifest_requires_local_files_and_records_set_identity() {
        let dir = tempfile::tempdir().unwrap();
        assert!(tablebase_manifest(dir.path().to_str().unwrap()).is_err());
        std::fs::write(
            dir.path().join("KQvK.rtbw"),
            b"test-fixture-not-a-tablebase",
        )
        .unwrap();
        let first = tablebase_manifest(dir.path().to_str().unwrap()).unwrap();
        std::fs::write(dir.path().join("KQvK.rtbz"), b"another-manifest-entry").unwrap();
        assert_ne!(
            first,
            tablebase_manifest(dir.path().to_str().unwrap()).unwrap()
        );
    }
    #[test]
    fn mate_loss_requires_an_actual_separate_recheck() {
        let (g, mut r) = fixture(&[900, 500, 500]);
        r.confirmed_moves[0].best.mate = Some(2);
        r.confirmed_moves[0].best.cp = None;
        let out = report(&g, &r);
        assert_eq!(value(&out, "E05_mate"), None);
        let mut recheck = r.confirmed_moves[0].clone();
        recheck.method = "same_root_mate_recheck_double_budget_v1".into();
        r.mate_rechecks.push(recheck);
        assert_eq!(value(&report(&g, &r), "E05_mate"), Some(1.));
    }
}
#[cfg(test)]
mod game_quality_goldens {
    use super::*;
    #[test]
    fn weighted_game_accuracy_and_capped_acpl_have_independent_numerical_goldens() {
        let game = StoredGame {
            id: "golden".into(),
            own_color: "white".into(),
            replay: chess::replay_pgn("1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 1-0").unwrap(),
            ..Default::default()
        };
        let cp = [15, -100, -50, -150, -50, -100, 20, -200, 0];
        let run = EngineRun {
            game_id: game.id.clone(),
            evaluations: cp
                .iter()
                .enumerate()
                .map(|(i, c)| PositionEvaluation {
                    ply: i as u32,
                    cp: Some(*c),
                    complete: true,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let (accuracy, moves) = metrics::game_accuracy(&game, &run, None);
        assert!((accuracy.unwrap() - 58.44338129880295).abs() < 1e-10);
        assert_eq!(moves.len(), 4);
        let mut report = StatisticsReport::default();
        append(
            &mut report,
            &[&game],
            &HashMap::from([(game.id.clone(), &run)]),
            &StatisticsFilter::default(),
        );
        assert!(
            (report
                .metrics
                .iter()
                .find(|m| m.id == "Q06")
                .unwrap()
                .value
                .unwrap()
                - 58.44338129880295)
                .abs()
                < 1e-10
        );
        assert_eq!(
            report
                .metrics
                .iter()
                .find(|m| m.id == "Q01_capped")
                .unwrap()
                .value,
            Some(121.25)
        );
        assert!(
            report
                .analysis
                .cases
                .iter()
                .any(|c| c.metric_id == "Q05" && c.position_ply == 0)
        );
    }
    #[test]
    fn endgame_same_root_band_material_and_first_transition_denominators() {
        let pgn = "[SetUp \"1\"]\n[FEN \"7k/8/8/8/8/8/R7/K7 w - - 0 1\"]\n\n1. Ra3 Kg7 2. Ra7+ Kf6 3. Ra6+ Ke5 4. Ra5+ Kd4 1/2-1/2";
        let mut game = StoredGame {
            id: "ending".into(),
            own_color: "white".into(),
            result: "1/2-1/2".into(),
            replay: chess::replay_pgn(pgn).unwrap(),
            ..Default::default()
        };
        assert!(game.replay.nodes.iter().all(|n| n.phase == "endgame"));
        for n in &mut game.replay.nodes {
            if n.actor.as_deref() == Some("white") {
                n.spent_seconds = Some(10. * n.ply as f64);
            }
        }
        let states = [500, 50, 500, 490, 500, 450, 500, 500, 500];
        let eval = |p, q| PositionEvaluation {
            ply: p,
            wdl: Some([q, 0, 1000 - q]),
            complete: true,
            ..Default::default()
        };
        let run = EngineRun {
            game_id: game.id.clone(),
            evaluations: states
                .iter()
                .enumerate()
                .map(|(i, q)| eval(i as u32, *q))
                .collect(),
            confirmed_moves: states
                .windows(2)
                .enumerate()
                .map(|(i, v)| MoveComparison {
                    ply: i as u32 + 1,
                    best: eval(i as u32, v[0]),
                    played: eval(i as u32, v[1]),
                    complete: true,
                    method: "same_root_searchmoves".into(),
                })
                .collect(),
            ..Default::default()
        };
        let mut report = StatisticsReport::default();
        append(
            &mut report,
            &[&game],
            &HashMap::from([(game.id.clone(), &run)]),
            &StatisticsFilter::default(),
        );
        let get = |id: &str| report.metrics.iter().find(|m| m.id == id).unwrap().value;
        assert_eq!(get("S01_endgame"), Some(100.));
        assert_eq!(get("S01_endgame_length"), Some(4.));
        assert_eq!(get("S03_rook_segments"), Some(1.));
        assert_eq!(get("S03_rook_score"), Some(50.));
        assert_eq!(get("S05_engine"), Some(75.));
        assert_eq!(get("S07"), Some(12.75));
        assert_eq!(get("S07_errors"), Some(25.));
        assert_eq!(get("S08"), Some(25.));
        assert_eq!(get("S03_rook_draws"), Some(1.));
        assert_eq!(get("S03_rook_wins"), Some(0.));
        assert_eq!(get("S04_rook_time_total"), Some(160.));
        assert_eq!(get("S04_rook_time_share"), Some(100.));
        assert_eq!(get("S04_rook_time_coverage"), Some(100.));
        assert_eq!(get("S04_context_rook_q_equal_loss"), Some(12.75));
        assert_eq!(
            get("S04_context_rook_material_ahead_time_total"),
            Some(160.)
        );
        assert_eq!(get("S07_entry_q_equal_loss"), Some(12.75));
        let signature = report
            .metrics
            .iter()
            .find(|m| m.id.starts_with("S07_signature_") && m.id.ends_with("_loss"))
            .unwrap();
        assert_eq!(signature.value, Some(12.75));
        assert_eq!(signature.sample, 4);
        assert!(report.analysis.cases.iter().any(|c|c.metric_id.starts_with("S04_context_rook_q_equal")&&c.position_ply==0));
        // A type slice receives 150s of the game's known 160s after the first position segment.
        let mut sliced = game.clone();
        for n in &mut sliced.replay.nodes {
            if n.ply < 2 {
                n.phase = "opening".into();
            }
        }
        let mut out = StatisticsReport::default();
        append(
            &mut out,
            &[&sliced],
            &HashMap::from([(sliced.id.clone(), &run)]),
            &StatisticsFilter::default(),
        );
        assert_eq!(
            out.metrics
                .iter()
                .find(|m| m.id == "S04_rook_time_total")
                .unwrap()
                .value,
            Some(150.)
        );
        assert_eq!(
            out.metrics
                .iter()
                .find(|m| m.id == "S04_rook_time_share")
                .unwrap()
                .value,
            Some(93.75)
        );
    }
}

#[cfg(test)]
mod ending_context_goldens {
    use super::*;
    #[test]
    fn ending_recovery_wdl_and_entry_clock_bins_preserve_missing_data() {
        let pgn = "[SetUp \"1\"]\n[FEN \"7k/8/8/8/8/8/R7/K7 w - - 0 1\"]\n\n1. Ra3 Kg7 2. Ra7+ Kf6 3. Ra6+ Ke5 4. Ra5+ Kd4 1/2-1/2";
        let mut g = StoredGame {
            id: "defense-ending".into(),
            own_color: "white".into(),
            result: "1/2-1/2".into(),
            replay: chess::replay_pgn(pgn).unwrap(),
            ..Default::default()
        };
        for n in &mut g.replay.nodes {
            if n.actor.as_deref() == Some("white") {
                n.spent_seconds = Some(10.);
                n.remaining_before = Some(120.);
            }
        }
        let states = [100, 100, 100, 100, 500, 500, 500, 500, 500];
        let eval = |p, units| PositionEvaluation {
            ply: p,
            wdl: Some([units, 0, 1000 - units]),
            complete: true,
            ..Default::default()
        };
        let r = EngineRun {
            game_id: g.id.clone(),
            evaluations: states
                .iter()
                .enumerate()
                .map(|(p, u)| eval(p as u32, *u))
                .collect(),
            confirmed_moves: states
                .windows(2)
                .enumerate()
                .map(|(p, v)| MoveComparison {
                    ply: p as u32 + 1,
                    best: eval(p as u32, v[0]),
                    played: eval(p as u32, v[1]),
                    complete: true,
                    method: "same_root_searchmoves".into(),
                })
                .collect(),
            ..Default::default()
        };
        let mut out = StatisticsReport::default();
        append(
            &mut out,
            &[&g],
            &HashMap::from([(g.id.clone(), &r)]),
            &StatisticsFilter::default(),
        );
        let v = |id: &str| out.metrics.iter().find(|m| m.id == id).unwrap().value;
        assert_eq!(v("S04_rook_recovery"), Some(100.));
        assert_eq!(v("S04_context_rook_q_difficult_recovery"), Some(100.));
        assert_eq!(
            v("S04_context_rook_remaining_60to180_time_total"),
            Some(40.)
        );
        assert_eq!(v("E10_draws"), Some(1.));
        assert_eq!(v("E10_wins"), Some(0.));
        assert_eq!(v("E10"), Some(100.));
        assert_eq!(v("E10_score"), Some(50.));
        g.replay.nodes[1].remaining_before = None; // Later known clocks cannot fabricate the entry clock.
        append(
            &mut out,
            &[&g],
            &HashMap::from([(g.id.clone(), &r)]),
            &StatisticsFilter::default(),
        );
        assert!(
            out.metrics
                .iter()
                .all(|m| !m.id.starts_with("S04_context_rook_remaining_"))
        );
        for n in &mut g.replay.nodes {
            n.spent_seconds = None;
        }
        append(
            &mut out,
            &[&g],
            &HashMap::from([(g.id.clone(), &r)]),
            &StatisticsFilter::default(),
        );
        assert_eq!(
            out.metrics
                .iter()
                .find(|m| m.id == "S04_rook_time_total")
                .unwrap()
                .value,
            None
        );
        assert_eq!(
            out.metrics
                .iter()
                .find(|m| m.id == "S04_rook_time_share")
                .unwrap()
                .value,
            None
        );
        assert_eq!(
            out.metrics
                .iter()
                .find(|m| m.id == "S04_rook_time_coverage")
                .unwrap()
                .value,
            Some(0.)
        );
    }
}

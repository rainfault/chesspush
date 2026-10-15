//! Versioned metrics computed from persisted evidence. WDL q and Lichess pL remain separate.
//! Accuracy adaptation: lila ad760137cf9941cadd93126433695d6933209b0a, AGPL-3.0-or-later.
//! Adapted 2026-10-15; see THIRD_PARTY.md and licenses/LILA-AGPL-3.0.txt.
use super::types::*;
use std::collections::HashMap;

pub const METRIC_VERSION: &str = "statistics-metrics-v1";
pub fn mean(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| v.iter().sum::<f64>() / v.len() as f64)
}
pub fn quantile(v: &[f64], p: f64) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    let mut x = v.to_vec();
    x.sort_by(f64::total_cmp);
    let k = (x.len() - 1) as f64 * p.clamp(0., 1.);
    let i = k.floor() as usize;
    Some(x[i] + (x[k.ceil() as usize] - x[i]) * (k - i as f64))
}
pub fn wilson(k: usize, n: usize) -> Option<(f64, f64)> {
    if n == 0 {
        return None;
    }
    let p = k as f64 / n as f64;
    let z = 1.959963984540054;
    let d = 1. + z * z / n as f64;
    let c = (p + z * z / (2. * n as f64)) / d;
    let h = z * (p * (1. - p) / n as f64 + z * z / (4. * (n * n) as f64)).sqrt() / d;
    Some((100. * (c - h), 100. * (c + h)))
}
pub fn harmonic(v: &[f64]) -> Option<f64> {
    (!v.is_empty()).then(|| v.len() as f64 / v.iter().map(|x| 1. / x.max(1.)).sum::<f64>())
}
pub fn pl(cp: f64) -> f64 {
    100. / (1. + (-0.00368208 * cp.clamp(-1000., 1000.)).exp())
}
pub fn move_accuracy(before: f64, after: f64) -> f64 {
    if after >= before {
        100.
    } else {
        (103.1668100711649 * (-0.04354415386753951 * (before - after)).exp() - 3.166924740191411
            + 1.)
            .clamp(0., 100.)
    }
}
pub fn outcome(g: &StoredGame) -> &'static str {
    match (g.result.as_str(), g.own_color.as_str()) {
        ("1/2-1/2", _) => "draw",
        ("1-0", "white") | ("0-1", "black") => "win",
        ("1-0", "black") | ("0-1", "white") => "loss",
        _ => "unknown",
    }
}
pub fn own_score(g: &StoredGame) -> Option<f64> {
    match outcome(g) {
        "win" => Some(1.),
        "draw" => Some(0.5),
        "loss" => Some(0.),
        _ => None,
    }
}
/// WDL expected-score units are integer half-permille; preserve that quantum in deltas.
pub fn q_delta(before: f64, after: f64) -> f64 {
    ((before - after) * 2000.).round() / 2000.
}
pub fn eval_q(e: &PositionEvaluation, color: &str) -> Option<f64> {
    if !e.complete || e.bound.is_some() {
        return None;
    }
    let units = if let Some(out) = e.terminal_outcome.as_deref() {
        match out {
            "white" => {
                if color == "white" {
                    2000
                } else {
                    0
                }
            }
            "black" => {
                if color == "black" {
                    2000
                } else {
                    0
                }
            }
            "draw" => 1000,
            _ => return None,
        }
    } else {
        let [w, d, l] = e.wdl?;
        if w > 1000 || d > 1000 || l > 1000 || w + d + l != 1000 {
            return None;
        }
        if color == "white" {
            2 * w + d
        } else {
            2 * l + d
        }
    };
    Some(units as f64 / 2000.)
}

fn eval_pl(e: &PositionEvaluation) -> Option<f64> {
    if !e.complete || e.bound.is_some() {
        return None;
    }
    if let Some(c) = e.cp {
        return Some(pl(c as f64));
    }
    if let Some(out) = e.terminal_outcome.as_deref() {
        return match out {
            "white" => Some(pl(1000.)),
            "black" => Some(pl(-1000.)),
            "draw" => Some(50.),
            _ => None,
        };
    }
    e.mate
        .filter(|m| *m != 0)
        .map(|m| pl(if m > 0 { 1000. } else { -1000. }))
}
#[derive(Default, Clone)]
pub struct GameMetrics {
    pub decisions: Vec<DecisionEvent>,
    pub events: Vec<DecisionEvent>,
    pub accuracy: Option<f64>,
    pub move_accuracies: Vec<(u32, f64)>,
    pub loss_q: Vec<f64>,
    pub cp_losses: Vec<f64>,
    pub opportunities: usize,
    pub opportunity_total: usize,
    pub used_opportunities: usize,
    pub conversion: bool,
    pub defense: bool,
    pub conversion_episodes: usize,
    pub defense_episodes: usize,
    pub exit_q: Option<f64>,
    pub defensive_decisions: Vec<DecisionEvent>,
    pub advantage_decisions: Vec<DecisionEvent>,
    pub defense_recovered: bool,
}
pub fn game_metrics(g: &StoredGame, run: Option<&EngineRun>) -> GameMetrics {
    let Some(run) = run else {
        return GameMetrics::default();
    };
    let mut m = GameMetrics::default();
    let comparisons: HashMap<_, _> = run.confirmed_moves.iter().map(|e| (e.ply, e)).collect();
    for n in g.replay.nodes.iter().skip(1) {
        let Some(c) = comparisons
            .get(&n.ply)
            .filter(|c| c.complete && c.method == "same_root_searchmoves")
        else {
            continue;
        };
        let Some(actor) = n.actor.as_deref() else {
            continue;
        };
        let before = eval_q(&c.best, actor);
        let after = eval_q(&c.played, actor);
        // Independent searches disagreeing substantially are not silently converted to zero loss.
        let unstable = matches!((before,after),(Some(a),Some(b)) if q_delta(b,a)>0.02);
        let loss_q = if unstable {
            None
        } else {
            before.zip(after).map(|(a, b)| 100. * q_delta(a, b).max(0.))
        };
        let cp_loss = c
            .best
            .cp
            .zip(c.played.cp)
            .filter(|_| {
                c.best.bound.is_none()
                    && c.played.bound.is_none()
                    && c.best.complete
                    && c.played.complete
            })
            .and_then(|(a, b)| {
                let diff = if actor == "white" {
                    a as f64 - b as f64
                } else {
                    b as f64 - a as f64
                };
                (diff >= -20.).then_some(diff.max(0.))
            });
        let loss_pl = eval_pl(&c.best).zip(eval_pl(&c.played)).map(|(a, b)| {
            if actor == "white" {
                (a - b).max(0.)
            } else {
                (b - a).max(0.)
            }
        });
        let event = DecisionEvent {
            game_id: g.id.clone(),
            ply: n.ply,
            kind: if unstable {
                "unstable"
            } else if loss_q.is_some_and(|l| l >= 10.) {
                "significant"
            } else {
                "decision"
            }
            .into(),
            actor: actor.into(),
            phase: n.phase.clone(),
            san: n.san.clone().unwrap_or_default(),
            loss_q,
            loss_pl,
            cp_loss,
            before_q: before,
            after_q: after,
            clock_seconds: n.remaining_before,
            spent_seconds: n.spent_seconds,
            run_id: run.id.clone(),
            profile_id: run.profile_id.clone(),
            method: c.method.clone(),
            best_move: c.best.best_move.clone(),
            fen: g.replay.nodes[n.ply as usize - 1].fen.clone(),
        };
        if actor == g.own_color {
            if let Some(v) = loss_q {
                m.loss_q.push(v);
            }
            if let Some(v) = cp_loss {
                m.cp_losses.push(v);
            }
            m.decisions.push(event.clone());
            if event.kind != "decision" {
                m.events.push(event);
            }
        }
    }
    let (accuracy, moves) = game_accuracy(g, run, None);
    m.accuracy = accuracy;
    m.move_accuracies = moves;
    if let Some(p) = g.replay.middle_ply {
        m.exit_q = run
            .evaluations
            .iter()
            .find(|e| e.ply == p)
            .and_then(|e| eval_q(e, &g.own_color));
    }
    // Opportunity gain is confirmed at the opponent's root; response retention at the following root.
    for n in g
        .replay
        .nodes
        .iter()
        .skip(1)
        .filter(|n| n.actor.as_deref() != Some(&g.own_color))
    {
        let Some(c) = comparisons.get(&n.ply).filter(|c| c.complete) else {
            continue;
        };
        let Some((q0, q1)) = eval_q(&c.best, &g.own_color).zip(eval_q(&c.played, &g.own_color))
        else {
            continue;
        };
        if q_delta(q1, q0) < 0.10 {
            continue;
        }
        m.opportunity_total += 1;
        let Some(reply) = comparisons.get(&(n.ply + 1)).filter(|r| r.complete) else {
            continue;
        };
        let Some((root, q2)) =
            eval_q(&reply.best, &g.own_color).zip(eval_q(&reply.played, &g.own_color))
        else {
            continue;
        };
        if q_delta(root, q1).abs() > 0.05 || q_delta(q2, root) > 0.02 {
            continue;
        }
        m.opportunities += 1;
        let used = q_delta(root, q2).max(0.) <= 0.02;
        if used {
            m.used_opportunities += 1;
        }
        if let Some(base) = m.decisions.iter().find(|d| d.ply == n.ply + 1) {
            let mut event = base.clone();
            event.kind = if used {
                "opportunity_used"
            } else {
                "opportunity_missed"
            }
            .into();
            event.before_q = Some(q1);
            event.after_q = Some(q2);
            m.events.push(event);
        }
    }
    let state: HashMap<_, _> = run
        .evaluations
        .iter()
        .map(|e| (e.ply, eval_q(e, &g.own_color)))
        .collect();
    for (advantage, target) in [
        (true, &mut m.conversion_episodes),
        (false, &mut m.defense_episodes),
    ] {
        let mut first: Option<u32> = None;
        let mut active = false;
        for n in g.replay.nodes.iter().skip(1) {
            let before = state.get(&(n.ply - 1)).copied().flatten();
            if n.actor.as_deref() == Some(&g.own_color) {
                if !m
                    .decisions
                    .iter()
                    .any(|d| d.ply == n.ply && d.loss_q.is_some())
                {
                    first = None;
                    active = false;
                    continue;
                }
                if let Some(q) = before {
                    let entry = if advantage { q >= 0.90 } else { q <= 0.10 };
                    if !active && entry && comparisons.get(&n.ply).is_some_and(|c| c.complete) {
                        if let Some(start) = first {
                            active = true;
                            *target += 1;
                            if let Some(base) = m.decisions.iter().find(|e| e.ply == start) {
                                let mut event = base.clone();
                                event.kind = if advantage {
                                    "advantage_entry"
                                } else {
                                    "defense_entry"
                                }
                                .into();
                                event.phase = g.replay.nodes[start.saturating_sub(1) as usize]
                                    .phase
                                    .clone();
                                m.events.push(event);
                            }
                        } else {
                            first = Some(n.ply);
                        }
                    } else if !active && !entry {
                        first = None;
                    }
                } else {
                    first = None;
                    active = false;
                }
            }
            match state.get(&n.ply).copied().flatten() {
                Some(q) => {
                    let exit = if advantage { q < 0.80 } else { q > 0.20 };
                    if exit {
                        first = None;
                        active = false;
                    }
                }
                None => {
                    first = None;
                    active = false;
                }
            }
        }
    }
    for (kind, advantage) in [("advantage_entry", true), ("defense_entry", false)] {
        let starts: Vec<_> = m
            .events
            .iter()
            .filter(|e| e.kind == kind)
            .map(|e| e.ply)
            .collect();
        let mut seen = std::collections::HashSet::new();
        for start in starts {
            if !advantage
                && run
                    .evaluations
                    .iter()
                    .any(|e| e.ply >= start && eval_q(e, &g.own_color).is_some_and(|q| q >= 0.50))
            {
                m.defense_recovered = true;
            }
            for n in g.replay.nodes.iter().filter(|n| n.ply >= start) {
                if n.actor.as_deref() == Some(&g.own_color)
                    && !m
                        .decisions
                        .iter()
                        .any(|d| d.ply == n.ply && d.loss_q.is_some())
                {
                    break;
                }
                if let Some(e) = m
                    .decisions
                    .iter()
                    .find(|e| e.ply == n.ply && e.loss_q.is_some())
                    && seen.insert(e.ply)
                {
                    if advantage {
                        m.advantage_decisions.push(e.clone());
                    } else {
                        m.defensive_decisions.push(e.clone());
                    }
                }
                match state.get(&n.ply).copied().flatten() {
                    Some(q) => {
                        if if advantage { q < 0.80 } else { q > 0.20 } {
                            break;
                        }
                    }
                    None => break,
                }
            }
        }
    }
    m.conversion = m.conversion_episodes > 0;
    m.defense = m.defense_episodes > 0;
    m
}
pub fn game_accuracy(
    g: &StoredGame,
    run: &EngineRun,
    phase: Option<&str>,
) -> (Option<f64>, Vec<(u32, f64)>) {
    let values: HashMap<_, _> = run
        .evaluations
        .iter()
        .map(|e| (e.ply, eval_pl(e)))
        .collect();
    let nodes: Vec<_> = g
        .replay
        .nodes
        .iter()
        .skip(1)
        .filter(|n| phase.is_none_or(|p| n.phase == p))
        .collect();
    if nodes.is_empty() {
        return (None, vec![]);
    }
    let first = nodes[0].ply;
    let initial = if first == 1 && g.replay.initial_fen == super::chess::INITIAL_FEN {
        Some(pl(15.))
    } else {
        values.get(&(first - 1)).copied().flatten()
    };
    let mut all = vec![initial];
    all.extend(nodes.iter().map(|n| values.get(&n.ply).copied().flatten()));
    let window = (nodes.len() / 10).clamp(2, 8).min(all.len());
    let mut windows: Vec<&[Option<f64>]> = vec![&all[..window]; window.saturating_sub(2)];
    windows.extend(all.windows(window));
    let mut weighted = vec![];
    let mut moves = vec![];
    for (i, (pair, win)) in all.windows(2).zip(windows).enumerate() {
        let n = nodes[i];
        if n.actor.as_deref() != Some(&g.own_color) {
            continue;
        }
        let Some((a, b)) = pair[0].zip(pair[1]) else {
            continue;
        };
        let Some(values) = win.iter().copied().collect::<Option<Vec<_>>>() else {
            continue;
        };
        let avg = mean(&values).unwrap();
        let sd = (values.iter().map(|v| (v - avg).powi(2)).sum::<f64>() / values.len() as f64)
            .sqrt()
            .clamp(0.5, 12.);
        let accuracy = if g.own_color == "white" {
            move_accuracy(a, b)
        } else {
            move_accuracy(100. - a, 100. - b)
        };
        weighted.push((accuracy, sd));
        moves.push((n.ply, accuracy));
    }
    if weighted.is_empty() {
        return (None, moves);
    }
    let w = weighted.iter().map(|(a, b)| a * b).sum::<f64>()
        / weighted.iter().map(|(_, b)| b).sum::<f64>();
    let h = harmonic(&weighted.iter().map(|(a, _)| *a).collect::<Vec<_>>()).unwrap();
    (Some((w + h) / 2.), moves)
}
/// Deterministic whole-game bootstrap; never treats score with draws as Bernoulli wins.
pub fn bootstrap_mean(values: &[f64]) -> Option<(f64, f64)> {
    if values.len() < 2 {
        return None;
    }
    let mut seed = 0x9e3779b97f4a7c15u64;
    let mut samples = Vec::with_capacity(1000);
    for _ in 0..1000 {
        let mut sum = 0.;
        for _ in values {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            sum += values[(seed % values.len() as u64) as usize];
        }
        samples.push(sum / values.len() as f64);
    }
    let low = quantile(&samples, 0.025)?;
    let high = quantile(&samples, 0.975)?;
    // An empirical bootstrap cannot establish certainty from a constant sample
    // (or a numerically collapsed interval). Keep the observation and N, omit CI.
    (low.is_finite() && high.is_finite() && high > low).then_some((low, high))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn probability_scales_and_pov() {
        let e = PositionEvaluation {
            wdl: Some([200, 700, 100]),
            complete: true,
            ..Default::default()
        };
        assert_eq!(eval_q(&e, "white"), Some(0.55));
        assert!((eval_q(&e, "black").unwrap() - 0.45).abs() < 1e-12);
        let b = PositionEvaluation {
            bound: Some("lower".into()),
            ..e
        };
        assert_eq!(eval_q(&b, "white"), None);
        assert!((pl(0.) - 50.).abs() < 1e-12);
    }
    #[test]
    fn upstream_accuracy_golden_coefficients() {
        assert_eq!(move_accuracy(50., 50.), 100.);
        assert!((move_accuracy(60., 50.) - 64.5798284537207).abs() < 1e-9);
        assert!((move_accuracy(70., 50.) - 41.0168187414171).abs() < 1e-9);
    }
    #[test]
    fn wilson_keeps_uncertainty_when_no_losses() {
        let (lo, hi) = wilson(3, 3).unwrap();
        assert!(lo > 43. && lo < 44.);
        assert!((hi - 100.).abs() < 1e-10);
        assert_eq!(wilson(0, 0), None);
    }
    #[test]
    fn quantiles_and_game_bootstrap() {
        assert_eq!(quantile(&[0., 2., 4., 6.], 0.5), Some(3.));
        let (lo, hi) = bootstrap_mean(&[0., 0.5, 1.]).unwrap();
        assert!(lo < 0.5 && hi > 0.5);
    }
}

#[cfg(test)]
mod evidence_tests {
    use super::*;
    fn fixture(states: &[u32]) -> (StoredGame, EngineRun) {
        let replay =
            super::super::chess::replay_pgn("1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 1-0")
                .unwrap();
        let mut g = StoredGame {
            id: "lichess:fixture".into(),
            own_color: "white".into(),
            result: "1-0".into(),
            replay,
            ..Default::default()
        };
        g.replay.nodes.truncate(states.len());
        let eval = |ply: u32, q: u32| PositionEvaluation {
            ply,
            cp: Some(q as i32 - 500),
            wdl: Some([q, 0, 1000 - q]),
            complete: true,
            ..Default::default()
        };
        let evaluations: Vec<_> = states
            .iter()
            .enumerate()
            .map(|(i, q)| eval(i as u32, *q))
            .collect();
        let confirmed_moves = states
            .windows(2)
            .enumerate()
            .map(|(i, p)| MoveComparison {
                ply: i as u32 + 1,
                best: eval(i as u32, p[0]),
                played: eval(i as u32, p[1]),
                complete: true,
                method: "same_root_searchmoves".into(),
            })
            .collect();
        (
            g,
            EngineRun {
                id: "run".into(),
                profile_id: "profile".into(),
                evaluations,
                confirmed_moves,
                ..Default::default()
            },
        )
    }
    #[test]
    fn opportunity_response_and_advantage_are_distinct() {
        let (g, r) = fixture(&[500, 500, 900, 890, 950, 940, 950, 940, 940]);
        let m = game_metrics(&g, Some(&r));
        assert_eq!(m.opportunities, 1);
        assert_eq!(m.used_opportunities, 1);
        assert!(m.conversion);
        assert_eq!(m.conversion_episodes, 1);
        assert!(
            m.events
                .iter()
                .any(|e| e.kind == "opportunity_used" && e.ply == 3)
        );
        assert!(
            m.events
                .iter()
                .any(|e| e.kind == "advantage_entry" && e.ply == 3)
        );
    }
    #[test]
    fn own_loss_resets_pending_advantage_before_new_opponent_error() {
        let (g, r) = fixture(&[500, 500, 950, 700, 950, 940, 950]);
        let m = game_metrics(&g, Some(&r));
        assert!(!m.conversion);
        assert!(
            m.events
                .iter()
                .any(|e| e.kind == "significant" && e.ply == 3)
        );
    }
    #[test]
    fn bounds_and_mates_do_not_become_raw_cp_zero() {
        let (g, mut r) = fixture(&[500, 400, 500]);
        r.confirmed_moves[0].played.cp = None;
        r.confirmed_moves[0].played.mate = Some(-2);
        let m = game_metrics(&g, Some(&r));
        assert!(m.cp_losses.is_empty());
        assert_eq!(m.loss_q.len(), 1);
        r.confirmed_moves[0].best.bound = Some("lower".into());
        let m = game_metrics(&g, Some(&r));
        assert!(m.loss_q.is_empty());
    }
    #[test]
    fn substantial_negative_search_difference_is_unavailable() {
        let (g, r) = fixture(&[500, 650, 650]);
        let m = game_metrics(&g, Some(&r));
        assert!(m.loss_q.is_empty());
        assert_eq!(m.events[0].kind, "unstable");
    }
}

#[cfg(test)]
mod defense_fixture {
    use super::*;
    #[test]
    fn confirmed_defense_tracks_own_losses_and_later_recovery() {
        let g = StoredGame {
            id: "defense".into(),
            own_color: "white".into(),
            result: "1/2-1/2".into(),
            replay: super::super::chess::replay_pgn(
                "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 1/2-1/2",
            )
            .unwrap(),
            ..Default::default()
        };
        let states = [500, 100, 100, 50, 100, 50, 700, 700, 700];
        let e = |ply, q| PositionEvaluation {
            ply,
            complete: true,
            wdl: Some([q, 0, 1000 - q]),
            ..Default::default()
        };
        let r = EngineRun {
            evaluations: states
                .iter()
                .enumerate()
                .map(|(i, q)| e(i as u32, *q))
                .collect(),
            confirmed_moves: states
                .windows(2)
                .enumerate()
                .map(|(i, q)| MoveComparison {
                    ply: i as u32 + 1,
                    best: e(i as u32, q[0]),
                    played: e(i as u32, q[1]),
                    method: "same_root_searchmoves".into(),
                    complete: true,
                })
                .collect(),
            ..Default::default()
        };
        let m = game_metrics(&g, Some(&r));
        assert!(m.defense);
        assert!(m.defense_recovered);
        assert_eq!(m.defense_episodes, 1);
        assert_eq!(m.defensive_decisions.len(), 2);
        assert!(
            (mean(
                &m.defensive_decisions
                    .iter()
                    .filter_map(|e| e.loss_q)
                    .collect::<Vec<_>>()
            )
            .unwrap()
                - 5.)
                .abs()
                < 1e-9
        );
    }
}

#[cfg(test)]
mod bootstrap_degenerate_fixtures {
    use super::*;
    #[test]
    fn constant_outcomes_retain_the_mean_without_a_false_exact_interval() {
        for values in [
            &[100., 100.][..],
            &[0., 0.][..],
            &[50., 50., 50.][..],
            &[42.25, 42.25][..],
        ] {
            assert_eq!(bootstrap_mean(values), None);
            assert_eq!(mean(values), Some(values[0]));
        }
        let (low, high) = bootstrap_mean(&[0., 50., 100.]).unwrap();
        assert!(low < 50. && high > 50.);
    }
    #[test]
    fn numerically_collapsed_quantiles_do_not_produce_an_interval() {
        let mut values = vec![0.; 100];
        values[0] = f64::from_bits(1);
        assert_ne!(values[0], values[1]);
        assert_eq!(bootstrap_mean(&values), None);
    }
}

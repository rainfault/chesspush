//! Opening analytics from legal visits and persisted, compatible engine evidence.
//! Catalogue hierarchy is a literal naming path, never an invented chess taxonomy.
use super::{
    chess,
    metrics::{self, GameMetrics},
    query,
    types::*,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
type Visits<'a> = Vec<(&'a StoredGame, &'a ReplayNode)>;
type VisitStrata<'a> = BTreeMap<String, (Visits<'a>, Visits<'a>)>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningCase {
    pub game_id: String,
    pub ply: u32,
    pub run_id: Option<String>,
    pub profile_id: Option<String>,
    pub fen: String,
    pub position_key: String,
    pub san: Option<String>,
    pub uci: Option<String>,
    pub actor: String,
    pub own_color: String,
    pub timestamp_ms: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningTimeGame {
    pub game_id: String,
    pub known_seconds: Option<f64>,
    pub known_decisions: usize,
    pub decisions: usize,
    pub coverage: Option<f64>,
    pub cases: Vec<OpeningCase>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningQuality {
    pub decisions: usize,
    pub wdl_sample: usize,
    pub cp_sample: usize,
    pub accuracy_sample: usize,
    pub mean_loss: Option<f64>,
    pub median_loss: Option<f64>,
    pub p90_loss: Option<f64>,
    pub acpl: Option<f64>,
    pub accuracy: Option<f64>,
    pub significant: usize,
    pub significant_per_100: Option<f64>,
    pub spent_sample: usize,
    pub median_spent: Option<f64>,
    pub total_spent: Option<f64>,
    pub mean_spent_per_game: Option<f64>,
    pub spent_game_sample: usize,
    pub clock_coverage: Option<f64>,
    pub time_games: Vec<OpeningTimeGame>,
    pub cases: Vec<OpeningCase>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningTemporal {
    pub current_visits: usize,
    pub previous_visits: usize,
    pub current_total: usize,
    pub previous_total: usize,
    pub current_share: Option<f64>,
    pub previous_share: Option<f64>,
    pub share_delta: Option<f64>,
    pub first_seen_ms: Option<i64>,
    pub first_seen: Option<OpeningCase>,
    pub previous_cases: Vec<OpeningCase>,
    pub new_paths: usize,
    pub repeated_paths: usize,
    pub unknown_paths: usize,
    pub new_path_percent: Option<f64>,
    pub repeated_path_percent: Option<f64>,
    pub comparison: Vec<MetricValue>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningHierarchyNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub level: String,
    pub label: String,
    pub path: Vec<String>,
    pub color: String,
    pub frequency: Option<f64>,
    pub result: ResultRow,
    pub previous_result: ResultRow,
    pub quality: OpeningQuality,
    pub previous_quality: OpeningQuality,
    pub rating_difference_median: Option<f64>,
    pub rating_difference_sample: usize,
    pub comparison: Vec<MetricValue>,
    pub cases: Vec<OpeningCase>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningPositionContext {
    pub context_key: String,
    pub cases: Vec<OpeningCase>,
    pub evaluation: Option<PositionEvaluation>,
    pub tablebase: Option<super::engine_metrics::TablebaseObservation>,
    pub q: Option<f64>,
    pub perspective: String,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningNodeAnalysis {
    pub contexts: Vec<OpeningPositionContext>,
    pub unique_contexts: usize,
    pub usable_contexts: usize,
    pub q_median: Option<f64>,
    pub q_p25: Option<f64>,
    pub q_p75: Option<f64>,
    pub perspective: String,
    pub definition: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningBoundary {
    pub case: Option<OpeningCase>,
    pub game_id: String,
    pub kind: String,
    pub ply: Option<u32>,
    pub cp: Option<i32>,
    pub mate: Option<i32>,
    pub wdl: Option<[u32; 3]>,
    pub q: Option<f64>,
    pub pov: String,
    pub terminal_outcome: Option<String>,
    pub status: String,
    pub method: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningBoundarySummary {
    pub kind: String,
    pub eligible: usize,
    pub sample: usize,
    pub q_median: Option<f64>,
    pub q_mean: Option<f64>,
    pub q_p25: Option<f64>,
    pub q_p75: Option<f64>,
    pub advantage_percent: Option<f64>,
    pub hard_percent: Option<f64>,
    pub cases: Vec<OpeningBoundary>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningFirstLoss {
    pub game_id: String,
    pub case: Option<OpeningCase>,
    pub loss_q: Option<f64>,
    pub status: String,
    pub decisions: usize,
    pub analysed: usize,
    pub result: String,
    pub exit_q: Option<f64>,
    pub opening: Option<OpeningMatch>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningResponseWindow {
    pub response: OpeningCase,
    pub decisions: Vec<OpeningCase>,
    pub mean_loss: Option<f64>,
    pub significant: usize,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningOpponentResponse {
    pub position_key: String,
    pub played_uci: String,
    pub san: String,
    pub visits: usize,
    pub games: usize,
    pub frequency: Option<f64>,
    pub window_size: usize,
    pub eligible_windows: usize,
    pub excluded_windows: usize,
    pub mean_loss: Option<f64>,
    pub significant_per_100: Option<f64>,
    pub cases: Vec<OpeningResponseWindow>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningBlocker {
    pub id: String,
    pub status: String,
    pub dependency: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningAnalysis {
    pub method: String,
    pub hierarchy: Vec<OpeningHierarchyNode>,
    pub node: OpeningNodeAnalysis,
    pub boundaries: Vec<OpeningBoundarySummary>,
    pub first_losses: Vec<OpeningFirstLoss>,
    pub opponent_responses: Vec<OpeningOpponentResponse>,
    pub comparison: Vec<MetricValue>,
    pub blockers: Vec<OpeningBlocker>,
}

fn own_actor(g: &StoredGame, n: &ReplayNode) -> bool {
    n.actor.as_deref() == Some(&g.own_color)
}
fn case(g: &StoredGame, n: &ReplayNode, run: Option<&EngineRun>) -> OpeningCase {
    OpeningCase {
        game_id: g.id.clone(),
        ply: n.ply,
        run_id: run.map(|r| r.id.clone()),
        profile_id: run.map(|r| r.profile_id.clone()),
        fen: n.fen.clone(),
        position_key: n.position_key.clone(),
        san: n.san.clone(),
        uci: n.uci.clone(),
        actor: n.actor.clone().unwrap_or_else(|| {
            n.fen
                .split_whitespace()
                .nth(1)
                .map(|c| if c == "w" { "white" } else { "black" })
                .unwrap_or("unknown")
                .into()
        }),
        own_color: g.own_color.clone(),
        timestamp_ms: query::game_time(g),
    }
}
fn run_map<'a>(runs: &'a [EngineRun], profile: Option<&str>) -> HashMap<String, &'a EngineRun> {
    let mut map = HashMap::new();
    for r in runs
        .iter()
        .filter(|r| Some(r.profile_id.as_str()) == profile)
    {
        let entry = map.entry(r.game_id.clone()).or_insert(r);
        if r.completed_at_ms > entry.completed_at_ms {
            *entry = r;
        }
    }
    map
}
fn select<'a>(games: &'a [StoredGame], f: &StatisticsFilter) -> Vec<&'a StoredGame> {
    let mut seen = HashSet::new();
    games
        .iter()
        .filter(|g| query::eligible(g, f) && seen.insert(g.id.clone()))
        .collect()
}
fn refs<'a>(
    games: &'a [&'a StoredGame],
    opening_only: bool,
) -> Vec<(&'a StoredGame, &'a ReplayNode)> {
    games
        .iter()
        .flat_map(|g| {
            g.replay
                .nodes
                .iter()
                .skip(1)
                .filter(move |n| own_actor(g, n) && (!opening_only || n.phase == "opening"))
                .map(|n| (*g, n))
        })
        .collect()
}
fn quality(
    entries: &[(&StoredGame, &ReplayNode)],
    q: &HashMap<String, GameMetrics>,
    runs: &HashMap<String, &EngineRun>,
) -> OpeningQuality {
    let decisions: Vec<_> = entries
        .iter()
        .filter_map(|(g, n)| q.get(&g.id)?.decisions.iter().find(|e| e.ply == n.ply))
        .collect();
    let loss: Vec<_> = decisions.iter().filter_map(|e| e.loss_q).collect();
    let cp: Vec<_> = decisions.iter().filter_map(|e| e.cp_loss).collect();
    let acc: Vec<_> = entries
        .iter()
        .filter_map(|(g, n)| {
            q.get(&g.id)?
                .move_accuracies
                .iter()
                .find(|(ply, _)| *ply == n.ply)
                .map(|(_, a)| *a)
        })
        .collect();
    let spent: Vec<_> = entries
        .iter()
        .filter_map(|(_, n)| n.spent_seconds)
        .collect();
    let mut sums: HashMap<&str, f64> = HashMap::new();
    for (g, n) in entries {
        if let Some(s) = n.spent_seconds {
            *sums.entry(&g.id).or_default() += s;
        }
    }
    let game_sums: Vec<_> = sums.values().copied().collect();
    let mut per_game: BTreeMap<&str, Vec<(&StoredGame, &ReplayNode)>> = BTreeMap::new();
    for (g, n) in entries {
        per_game.entry(&g.id).or_default().push((*g, *n));
    }
    let time_games = per_game
        .into_iter()
        .map(|(id, refs)| {
            let known_decisions = refs
                .iter()
                .filter(|(_, n)| n.spent_seconds.is_some())
                .count();
            OpeningTimeGame {
                game_id: id.into(),
                known_seconds: sums.get(id).copied(),
                known_decisions,
                decisions: refs.len(),
                coverage: (!refs.is_empty())
                    .then(|| 100. * known_decisions as f64 / refs.len() as f64),
                cases: refs
                    .iter()
                    .map(|(g, n)| case(g, n, runs.get(&g.id).copied()))
                    .collect(),
            }
        })
        .collect();
    let significant = loss.iter().filter(|l| **l >= 10.).count();
    OpeningQuality {
        decisions: entries.len(),
        wdl_sample: loss.len(),
        cp_sample: cp.len(),
        accuracy_sample: acc.len(),
        mean_loss: metrics::mean(&loss),
        median_loss: metrics::quantile(&loss, 0.5),
        p90_loss: metrics::quantile(&loss, 0.9),
        acpl: metrics::mean(&cp),
        accuracy: metrics::harmonic(&acc),
        significant,
        significant_per_100: (!loss.is_empty())
            .then(|| 100. * significant as f64 / loss.len() as f64),
        spent_sample: spent.len(),
        median_spent: metrics::quantile(&spent, 0.5),
        total_spent: (!spent.is_empty()).then(|| spent.iter().sum()),
        mean_spent_per_game: metrics::mean(&game_sums),
        spent_game_sample: game_sums.len(),
        clock_coverage: (!entries.is_empty())
            .then(|| 100. * spent.len() as f64 / entries.len() as f64),
        time_games,
        cases: entries
            .iter()
            .map(|(g, n)| case(g, n, runs.get(&g.id).copied()))
            .collect(),
    }
}
fn scoped_quality(q: &HashMap<String, GameMetrics>) -> HashMap<String, GameMetrics> {
    q.iter()
        .map(|(id, m)| {
            let mut x = m.clone();
            x.decisions.retain(|e| e.phase == "opening");
            x.events.retain(|e| e.phase == "opening");
            x.loss_q = x.decisions.iter().filter_map(|e| e.loss_q).collect();
            x.cp_losses = x.decisions.iter().filter_map(|e| e.cp_loss).collect();
            let own: HashSet<_> = x.decisions.iter().map(|e| e.ply).collect();
            x.move_accuracies.retain(|(ply, _)| own.contains(ply));
            (id.clone(), x)
        })
        .collect()
}
fn hierarchy(
    games: &[&StoredGame],
    q: &HashMap<String, GameMetrics>,
    runs: &HashMap<String, &EngineRun>,
    previous: &[&StoredGame],
    previous_q: &HashMap<String, GameMetrics>,
) -> Vec<OpeningHierarchyNode> {
    let mut groups: BTreeMap<(String, Vec<String>), Vec<&StoredGame>> = BTreeMap::new();
    for g in games {
        let path = chess::opening_name_path(
            g.replay
                .opening
                .as_ref()
                .map(|o| o.name.as_str())
                .unwrap_or("Не классифицировано"),
        );
        for depth in 1..=path.len() {
            groups
                .entry((g.own_color.clone(), path[..depth].to_vec()))
                .or_default()
                .push(g);
        }
    }
    for g in previous {
        let path = chess::opening_name_path(
            g.replay
                .opening
                .as_ref()
                .map(|o| o.name.as_str())
                .unwrap_or("Не классифицировано"),
        );
        for depth in 1..=path.len() {
            groups
                .entry((g.own_color.clone(), path[..depth].to_vec()))
                .or_default();
        }
    }
    let sq = scoped_quality(q);
    let psq = scoped_quality(previous_q);
    groups
        .into_iter()
        .map(|((color, path), group)| {
            let id = chess::opening_label_id(&color, &path);
            let parent_id =
                (path.len() > 1).then(|| chess::opening_label_id(&color, &path[..path.len() - 1]));
            let peers = games.iter().filter(|g| g.own_color == color).count();
            let entries = refs(&group, true);
            let stats = quality(&entries, q, runs);
            let diffs: Vec<_> = group
                .iter()
                .filter_map(|g| {
                    let (a, b) = if g.own_color == "white" {
                        (&g.white, &g.black)
                    } else {
                        (&g.black, &g.white)
                    };
                    a.rating.zip(b.rating).map(|(a, b)| (a - b) as f64)
                })
                .collect();
            let prev: Vec<_> = previous
                .iter()
                .filter(|g| g.own_color == color && chess::opening_label_matches(g, &id))
                .copied()
                .collect();
            OpeningHierarchyNode {
                id,
                parent_id,
                level: ["family", "opening", "variation", "subvariation"]
                    [path.len().saturating_sub(1).min(3)]
                .into(),
                label: path.last().cloned().unwrap_or_default(),
                path: path.clone(),
                color,
                frequency: (peers > 0).then(|| 100. * group.len() as f64 / peers as f64),
                result: query::result_row(
                    path.last().cloned().unwrap_or_default(),
                    path.last().cloned().unwrap_or_default(),
                    &group,
                    &sq,
                ),
                previous_result: query::result_row(
                    path.last().cloned().unwrap_or_default(),
                    path.last().cloned().unwrap_or_default(),
                    &prev,
                    &psq,
                ),
                quality: stats.clone(),
                previous_quality: quality(&refs(&prev, true), previous_q, runs),
                rating_difference_median: metrics::quantile(&diffs, 0.5),
                rating_difference_sample: diffs.len(),
                comparison: compare(&group, &prev, q, previous_q),
                cases: stats.cases,
            }
        })
        .collect()
}
fn node_visits<'a>(
    games: &'a [&'a StoredGame],
    tree: &OpeningTree,
    actor: Option<&str>,
) -> Vec<(&'a StoredGame, &'a ReplayNode)> {
    let mut visits = vec![];
    for g in games {
        for (i, n) in g.replay.nodes.iter().enumerate() {
            let matches = if tree.mode == "line" {
                g.replay.initial_fen == tree.initial_fen
                    && i == tree.line.len()
                    && g.replay
                        .nodes
                        .iter()
                        .skip(1)
                        .take(i)
                        .filter_map(|n| n.uci.as_deref())
                        .eq(tree.line.iter().map(String::as_str))
            } else {
                n.position_key == tree.position_key
            };
            if !matches {
                continue;
            }
            if let Some(role) = actor.filter(|a| *a != "all") {
                let actual = g
                    .replay
                    .nodes
                    .get(i + 1)
                    .map(|n| if own_actor(g, n) { "own" } else { "opponent" });
                if actual != Some(role) {
                    continue;
                }
            }
            visits.push((*g, n));
        }
    }
    visits
}
fn context_key(g: &StoredGame, n: &ReplayNode) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            format!(
                "{}|{}",
                g.replay.initial_fen,
                g.replay
                    .nodes
                    .iter()
                    .skip(1)
                    .take(n.ply as usize)
                    .filter_map(|n| n.uci.as_deref())
                    .collect::<Vec<_>>()
                    .join(" ")
            )
            .as_bytes()
        )
    )
}
fn node_analysis(
    visits: &[(&StoredGame, &ReplayNode)],
    runs: &HashMap<String, &EngineRun>,
    fen: &str,
) -> OpeningNodeAnalysis {
    let perspective = if fen.split_whitespace().nth(1) == Some("b") {
        "black"
    } else {
        "white"
    }
    .to_owned();
    let mut contexts: BTreeMap<String, OpeningPositionContext> = BTreeMap::new();
    for (g, n) in visits {
        let run = runs.get(&g.id).copied();
        let evaluation = run
            .and_then(|r| r.evaluations.iter().find(|e| e.ply == n.ply))
            .cloned();
        let tablebase = run
            .and_then(|r| r.tablebases.iter().find(|t| t.ply == n.ply))
            .cloned();
        let q = evaluation
            .as_ref()
            .and_then(|e| metrics::eval_q(e, &perspective));
        let status = match evaluation.as_ref() {
            None if tablebase.as_ref().is_some_and(|t| t.status == "available") => "tablebase_only",
            None => "no_analysis",
            Some(e) if e.bound.is_some() => "bound",
            Some(e) if !e.complete => "incomplete",
            Some(_) if q.is_none() => "no_wdl",
            _ => "available",
        };
        let key = context_key(g, n);
        let entry = contexts
            .entry(key.clone())
            .or_insert_with(|| OpeningPositionContext {
                context_key: key,
                cases: vec![],
                evaluation: evaluation.clone(),
                tablebase: tablebase.clone(),
                q,
                perspective: perspective.clone(),
                status: status.into(),
            });
        if let (Some(before), Some(after)) = (entry.evaluation.as_ref(), evaluation.as_ref()) {
            if (
                before.cp,
                before.mate,
                before.wdl,
                before.bound.as_deref(),
                before.complete,
            ) != (
                after.cp,
                after.mate,
                after.wdl,
                after.bound.as_deref(),
                after.complete,
            ) {
                entry.q = None;
                entry.status = "conflicting_observations".into();
            }
        } else if entry.evaluation.is_none() && evaluation.is_some() {
            entry.evaluation = evaluation;
            entry.q = q;
            entry.status = status.into();
        }
        if entry.tablebase.is_none() {
            entry.tablebase = tablebase;
        }
        entry.cases.push(case(g, n, run));
    }
    let contexts: Vec<_> = contexts.into_values().collect();
    let values: Vec<_> = contexts.iter().filter_map(|c| c.q).collect();
    OpeningNodeAnalysis{unique_contexts:contexts.len(),usable_contexts:values.len(),q_median:metrics::quantile(&values,0.5),q_p25:metrics::quantile(&values,0.25),q_p75:metrics::quantile(&values,0.75),perspective,contexts,definition:"Раздельные full initial FEN + UCI history contexts; одна согласованная оценка на history context выбранного профиля. q в POV стороны хода; cp/mate в evaluation канонически White. Конфликтующие оценки одного context не усредняются. Kpos не подменяет repetition/rule50 context.".into()}
}
fn boundary(g: &StoredGame, kind: &str, runs: &HashMap<String, &EngineRun>) -> OpeningBoundary {
    let ply = match kind {
        "phase" => g.replay.middle_ply,
        "last_named" => g.replay.opening.as_ref().map(|o| o.ply),
        _ => g.replay.nodes.get(20).map(|n| n.ply),
    };
    let run = runs.get(&g.id).copied();
    let node = ply.and_then(|p| g.replay.nodes.get(p as usize));
    let e = node.and_then(|n| run?.evaluations.iter().find(|e| e.ply == n.ply));
    let usable = e.filter(|e| e.complete && e.bound.is_none());
    let white = g.own_color == "white";
    let wdl = usable
        .and_then(|e| e.wdl)
        .map(|[w, d, l]| if white { [w, d, l] } else { [l, d, w] });
    OpeningBoundary {
        case: node.map(|n| case(g, n, run)),
        game_id: g.id.clone(),
        kind: kind.into(),
        ply,
        cp: usable
            .and_then(|e| e.cp)
            .and_then(|v| if white { Some(v) } else { v.checked_neg() }),
        mate: usable
            .and_then(|e| e.mate)
            .and_then(|v| if white { Some(v) } else { v.checked_neg() }),
        wdl,
        q: usable.and_then(|e| metrics::eval_q(e, &g.own_color)),
        pov: g.own_color.clone(),
        terminal_outcome: usable.and_then(|e| e.terminal_outcome.clone()),
        status: if ply.is_none() {
            "no_boundary"
        } else if e.is_none() {
            "no_analysis"
        } else if usable.is_none() {
            "incomplete_or_bound"
        } else {
            "available"
        }
        .into(),
        method: if kind == "phase" {
            g.replay.phase_version.clone()
        } else if kind == "last_named" {
            g.replay.catalogue_version.clone()
        } else {
            "fixed_20_ply_v1".into()
        },
    }
}
fn boundaries(
    games: &[&StoredGame],
    runs: &HashMap<String, &EngineRun>,
) -> Vec<OpeningBoundarySummary> {
    ["phase", "last_named", "fixed_20"]
        .into_iter()
        .map(|kind| {
            let cases: Vec<_> = games.iter().map(|g| boundary(g, kind, runs)).collect();
            let values: Vec<_> = cases.iter().filter_map(|c| c.q).collect();
            OpeningBoundarySummary {
                kind: kind.into(),
                eligible: cases.len(),
                sample: values.len(),
                q_median: metrics::quantile(&values, 0.5),
                q_mean: metrics::mean(&values),
                q_p25: metrics::quantile(&values, 0.25),
                q_p75: metrics::quantile(&values, 0.75),
                advantage_percent: (!values.is_empty()).then(|| {
                    100. * values.iter().filter(|q| **q >= 0.90).count() as f64
                        / values.len() as f64
                }),
                hard_percent: (!values.is_empty()).then(|| {
                    100. * values.iter().filter(|q| **q <= 0.10).count() as f64
                        / values.len() as f64
                }),
                cases,
            }
        })
        .collect()
}
fn first_losses(
    games: &[&StoredGame],
    q: &HashMap<String, GameMetrics>,
    runs: &HashMap<String, &EngineRun>,
) -> Vec<OpeningFirstLoss> {
    games
        .iter()
        .map(|g| {
            let entries: Vec<_> = g
                .replay
                .nodes
                .iter()
                .skip(1)
                .filter(|n| own_actor(g, n) && n.phase == "opening")
                .collect();
            let m = &q[&g.id];
            let analysed = entries
                .iter()
                .filter(|n| {
                    m.decisions
                        .iter()
                        .any(|e| e.ply == n.ply && e.loss_q.is_some())
                })
                .count();
            let first = m
                .decisions
                .iter()
                .filter(|e| e.phase == "opening" && e.loss_q.is_some_and(|l| l >= 10.))
                .min_by_key(|e| e.ply);
            let status = if entries.is_empty() {
                "no_opening_sample"
            } else if let Some(first) = first {
                if entries.iter().filter(|n| n.ply < first.ply).all(|n| {
                    m.decisions
                        .iter()
                        .any(|e| e.ply == n.ply && e.loss_q.is_some())
                }) {
                    "confirmed_first_loss"
                } else {
                    "first_observed_loss_partial"
                }
            } else if analysed == entries.len() {
                "complete_no_loss"
            } else if analysed == 0 {
                "no_analysis"
            } else {
                "partial_no_loss"
            };
            OpeningFirstLoss {
                game_id: g.id.clone(),
                case: first
                    .and_then(|e| g.replay.nodes.get(e.ply as usize))
                    .map(|n| case(g, n, runs.get(&g.id).copied())),
                loss_q: first.and_then(|e| e.loss_q),
                status: status.into(),
                decisions: entries.len(),
                analysed,
                result: g.result.clone(),
                exit_q: m.exit_q,
                opening: g.replay.opening.clone(),
            }
        })
        .collect()
}
fn opponent_responses(
    games: &[&StoredGame],
    q: &HashMap<String, GameMetrics>,
    runs: &HashMap<String, &EngineRun>,
) -> Vec<OpeningOpponentResponse> {
    let mut groups: BTreeMap<(String, String), Vec<(&StoredGame, &ReplayNode)>> = BTreeMap::new();
    let mut totals: HashMap<String, usize> = HashMap::new();
    for g in games {
        for n in g
            .replay
            .nodes
            .iter()
            .skip(1)
            .filter(|n| n.phase == "opening" && !own_actor(g, n))
        {
            let root = &g.replay.nodes[n.ply as usize - 1];
            *totals.entry(root.position_key.clone()).or_default() += 1;
            groups
                .entry((root.position_key.clone(), n.uci.clone().unwrap_or_default()))
                .or_default()
                .push((g, n));
        }
    }
    let mut rows = vec![];
    for ((key, uci), visits) in groups {
        let mut windows = vec![];
        let mut losses = vec![];
        let mut eligible = 0;
        let mut ids = HashSet::new();
        for (g, n) in &visits {
            ids.insert(g.id.clone());
            let own: Vec<_> = g
                .replay
                .nodes
                .iter()
                .filter(|next| next.ply > n.ply && own_actor(g, next))
                .take(3)
                .collect();
            let decisions: Vec<_> = own
                .iter()
                .map(|next| case(g, next, runs.get(&g.id).copied()))
                .collect();
            let values: Vec<_> = own
                .iter()
                .filter_map(|next| {
                    q[&g.id]
                        .decisions
                        .iter()
                        .find(|e| e.ply == next.ply)?
                        .loss_q
                })
                .collect();
            let status = if own.len() < 3 {
                "short_game"
            } else if values.len() < 3 {
                "incomplete_analysis"
            } else {
                "available"
            };
            if status == "available" {
                eligible += 1;
                losses.extend(values.iter().copied());
            }
            windows.push(OpeningResponseWindow {
                response: case(g, n, runs.get(&g.id).copied()),
                decisions,
                mean_loss: (status == "available")
                    .then(|| metrics::mean(&values))
                    .flatten(),
                significant: if status == "available" {
                    values.iter().filter(|v| **v >= 10.).count()
                } else {
                    0
                },
                status: status.into(),
            });
        }
        rows.push(OpeningOpponentResponse {
            frequency: totals
                .get(&key)
                .filter(|n| **n > 0)
                .map(|n| 100. * visits.len() as f64 / *n as f64),
            position_key: key,
            played_uci: uci,
            san: visits[0].1.san.clone().unwrap_or_default(),
            visits: visits.len(),
            games: ids.len(),
            window_size: 3,
            eligible_windows: eligible,
            excluded_windows: visits.len() - eligible,
            mean_loss: metrics::mean(&losses),
            significant_per_100: (!losses.is_empty()).then(|| {
                100. * losses.iter().filter(|v| **v >= 10.).count() as f64 / losses.len() as f64
            }),
            cases: windows,
        });
    }
    rows.sort_by(|a, b| {
        b.visits
            .cmp(&a.visits)
            .then(a.position_key.cmp(&b.position_key))
    });
    rows
}
fn compare(
    current: &[&StoredGame],
    previous: &[&StoredGame],
    q: &HashMap<String, GameMetrics>,
    pq: &HashMap<String, GameMetrics>,
) -> Vec<MetricValue> {
    if previous.is_empty() {
        return vec![];
    }
    let mut groups: BTreeMap<String, (Vec<&StoredGame>, Vec<&StoredGame>)> = BTreeMap::new();
    for g in current {
        groups.entry(query::stratum(g)).or_default().0.push(g);
    }
    for g in previous {
        groups.entry(query::stratum(g)).or_default().1.push(g);
    }
    let groups: Vec<_> = groups
        .into_values()
        .filter(|(a, b)| !a.is_empty() && !b.is_empty())
        .collect();
    let mut rows = vec![];
    for (id, name, unit) in [
        ("O02_score_delta", "Δ очков в ветви", "п.п."),
        ("O07_loss_delta", "Δ дебютных потерь", "п.п."),
        ("O10_spent_delta", "Δ времени дебютного решения", "с"),
    ] {
        let measure =
            |games: &[&StoredGame], quality: &HashMap<String, GameMetrics>| -> Option<f64> {
                match id {
                    "O02_score_delta" => metrics::mean(
                        &games
                            .iter()
                            .filter_map(|g| metrics::own_score(g).map(|v| 100. * v))
                            .collect::<Vec<_>>(),
                    ),
                    "O07_loss_delta" => metrics::mean(
                        &games
                            .iter()
                            .flat_map(|g| {
                                quality[&g.id]
                                    .decisions
                                    .iter()
                                    .filter(|e| e.phase == "opening")
                                    .filter_map(|e| e.loss_q)
                            })
                            .collect::<Vec<_>>(),
                    ),
                    _ => metrics::quantile(
                        &refs(games, true)
                            .iter()
                            .filter_map(|(_, n)| n.spent_seconds)
                            .collect::<Vec<_>>(),
                        0.5,
                    ),
                }
            };
        let mut value = 0.;
        let mut weights = 0.;
        let mut ids = vec![];
        let mut sample = 0;
        let mut eligible = 0;
        for (a, b) in &groups {
            if let Some((av, bv)) = measure(a, q).zip(measure(b, pq)) {
                let w = (a.len() + b.len()) as f64;
                value += w * (av - bv);
                weights += w;
                sample += a.len();
                eligible += b.len();
                ids.extend(a.iter().map(|g| g.id.clone()));
            }
        }
        let common: Vec<_> = groups.iter().map(|(a, b)| (a, b)).collect();
        let overlap = current
            .iter()
            .any(|g| previous.iter().any(|p| p.id == g.id));
        let confidence = if overlap {
            None
        } else {
            query::bootstrap_stratified_difference(
                if id == "O02_score_delta" {
                    "G03"
                } else if id == "O07_loss_delta" {
                    "Q02"
                } else {
                    "T01"
                },
                &common,
                q,
                pq,
                Some("opening"),
            )
            .filter(|(low, high)| high > low)
        };
        rows.push(MetricValue{low:confidence.map(|v|v.0),high:confidence.map(|v|v.1),id:id.into(),name:name.into(),unit:unit.into(),value:(weights>0.).then(||value/weights),sample,eligible,definition:"Разница по общим source/perf/variant/control/color/rated/rating strata; фиксированные pooled game веса, один engine profile. 400 bootstrap whole-game resamples where nondegenerate and independent; no causal/significance claim.".into(),method:"opening_common_strata_pooled_v1".into(),game_ids:ids,status:if overlap{"no_independent_comparison"}else if weights>0.{"descriptive"}else{"no_common_strata"}.into()});
    }
    rows
}
fn path_key(g: &StoredGame, n: &ReplayNode) -> String {
    format!(
        "{}|{}",
        g.replay.initial_fen,
        g.replay
            .nodes
            .iter()
            .skip(1)
            .take(n.ply as usize)
            .filter_map(|n| n.uci.as_deref())
            .collect::<Vec<_>>()
            .join(" ")
    )
}
fn enrich_tree(
    report: &mut StatisticsReport,
    selected: &[&StoredGame],
    previous: &[&StoredGame],
    history: &[&StoredGame],
    pq: &HashMap<String, GameMetrics>,
    all_actor_q: &HashMap<String, GameMetrics>,
    runs: &HashMap<String, &EngineRun>,
) -> OpeningNodeAnalysis {
    let visits = node_visits(
        selected,
        &report.opening_tree,
        report.filter.opening_actor.as_deref(),
    );
    let old_visits = node_visits(
        previous,
        &report.opening_tree,
        report.filter.opening_actor.as_deref(),
    );
    let historical = node_visits(
        history,
        &report.opening_tree,
        report.filter.opening_actor.as_deref(),
    );
    let current_total = visits
        .iter()
        .filter(|(g, n)| g.replay.nodes.get(n.ply as usize + 1).is_some())
        .count();
    let previous_total = old_visits
        .iter()
        .filter(|(g, n)| g.replay.nodes.get(n.ply as usize + 1).is_some())
        .count();
    let existing: HashSet<_> = report
        .opening_tree
        .edges
        .iter()
        .map(|e| e.uci.clone())
        .collect();
    let mut added = HashSet::new();
    for (g, n) in &old_visits {
        if let Some(next) = g.replay.nodes.get(n.ply as usize + 1)
            && let Some(uci) = next.uci.as_ref()
            && !existing.contains(uci)
            && added.insert(uci.clone())
        {
            report.opening_tree.edges.push(OpeningEdge {
                uci: uci.clone(),
                san: next.san.clone().unwrap_or_default(),
                fen: next.fen.clone(),
                position_key: next.position_key.clone(),
                result: query::result_row(
                    uci.clone(),
                    next.san.clone().unwrap_or_default(),
                    &[],
                    all_actor_q,
                ),
                frequency: 0.,
                ..Default::default()
            });
        }
    }
    report
        .opening_tree
        .unplayed
        .retain(|m| !added.contains(&m.uci));
    for edge in &mut report.opening_tree.edges {
        let entries: Vec<_> = visits
            .iter()
            .filter_map(|(g, n)| {
                g.replay
                    .nodes
                    .get(n.ply as usize + 1)
                    .filter(|next| next.uci.as_deref() == Some(&edge.uci))
                    .map(|next| (*g, next))
            })
            .collect();
        let prev: Vec<_> = old_visits
            .iter()
            .filter_map(|(g, n)| {
                g.replay
                    .nodes
                    .get(n.ply as usize + 1)
                    .filter(|next| next.uci.as_deref() == Some(&edge.uci))
                    .map(|next| (*g, next))
            })
            .collect();
        let mut earlier: Vec<_> = historical
            .iter()
            .filter_map(|(g, n)| {
                g.replay
                    .nodes
                    .get(n.ply as usize + 1)
                    .filter(|next| next.uci.as_deref() == Some(&edge.uci))
                    .map(|next| (*g, next))
            })
            .collect();
        earlier.sort_by_key(|(g, n)| (query::game_time(g), g.id.clone(), n.ply));
        let role_entries = if entries.is_empty() { &prev } else { &entries };
        let roles: HashSet<_> = role_entries
            .iter()
            .map(|(g, n)| if own_actor(g, n) { "own" } else { "opponent" })
            .collect();
        edge.actor = if roles.len() == 1 {
            roles.into_iter().next().unwrap()
        } else {
            "mixed"
        }
        .into();
        edge.cases = entries
            .iter()
            .map(|(g, n)| case(g, n, runs.get(&g.id).copied()))
            .collect();
        edge.quality = quality(&entries, all_actor_q, runs);
        let current_paths: HashSet<_> = entries.iter().map(|(g, n)| path_key(g, n)).collect();
        let mut path_first: HashMap<String, i64> = HashMap::new();
        for (g, n) in &earlier {
            if let Some(t) = query::game_time(g) {
                path_first
                    .entry(path_key(g, n))
                    .and_modify(|v| *v = (*v).min(t))
                    .or_insert(t);
            }
        }
        let (mut new_paths, mut repeated_paths, mut unknown_paths) = (0, 0, 0);
        for path in current_paths {
            match path_first.get(&path) {
                Some(t) if report.filter.since_ms.is_some_and(|s| *t < s) => repeated_paths += 1,
                Some(_) => new_paths += 1,
                None => unknown_paths += 1,
            }
        }
        let first = earlier
            .iter()
            .find(|(g, _)| query::game_time(g).is_some())
            .map(|(g, n)| case(g, n, runs.get(&g.id).copied()));
        let share = (current_total > 0).then(|| 100. * entries.len() as f64 / current_total as f64);
        let old_share =
            (previous_total > 0).then(|| 100. * prev.len() as f64 / previous_total as f64);
        edge.temporal = OpeningTemporal {
            current_visits: entries.len(),
            previous_visits: prev.len(),
            current_total,
            previous_total,
            current_share: share,
            previous_share: old_share,
            share_delta: share.zip(old_share).map(|(a, b)| a - b),
            first_seen_ms: first.as_ref().and_then(|c| c.timestamp_ms),
            first_seen: first,
            previous_cases: prev
                .iter()
                .map(|(g, n)| case(g, n, runs.get(&g.id).copied()))
                .collect(),
            new_paths,
            repeated_paths,
            unknown_paths,
            new_path_percent: (new_paths + repeated_paths > 0)
                .then(|| 100. * new_paths as f64 / (new_paths + repeated_paths) as f64),
            repeated_path_percent: (new_paths + repeated_paths > 0)
                .then(|| 100. * repeated_paths as f64 / (new_paths + repeated_paths) as f64),
            comparison: compare_edge(&entries, &prev, all_actor_q, pq),
        };
    }
    report.opening_tree.edges.sort_by(|a, b| {
        b.visits
            .cmp(&a.visits)
            .then(b.temporal.previous_visits.cmp(&a.temporal.previous_visits))
            .then(a.uci.cmp(&b.uci))
    });
    node_analysis(&visits, runs, &report.opening_tree.fen)
}

pub(crate) fn append(
    report: &mut StatisticsReport,
    games: &[StoredGame],
    runs: &[EngineRun],
    current_q: &HashMap<String, GameMetrics>,
) {
    let selected = select(games, &report.filter);
    let map = run_map(runs, report.selected_profile_id.as_deref());
    let mut previous_filter = report.filter.clone();
    previous_filter.since_ms = report.filter.comparison_since_ms;
    previous_filter.until_ms = report.filter.comparison_until_ms;
    previous_filter.comparison_since_ms = None;
    previous_filter.comparison_until_ms = None;
    let previous = if report.filter.comparison_since_ms.is_some()
        && report.filter.comparison_until_ms.is_some()
    {
        select(games, &previous_filter)
    } else {
        vec![]
    };
    let previous_q: HashMap<_, _> = previous
        .iter()
        .map(|g| {
            (
                g.id.clone(),
                metrics::game_metrics(g, map.get(&g.id).copied()),
            )
        })
        .collect();
    let mut history_filter = report.filter.clone();
    history_filter.since_ms = None;
    history_filter.until_ms = None;
    let history = select(games, &history_filter);
    let scoped = scoped_quality(current_q);
    for row in &mut report.openings {
        let group: Vec<_> = selected
            .iter()
            .filter(|g| row.result.game_ids.contains(&g.id))
            .copied()
            .collect();
        let fixed = query::result_row(
            row.result.key.clone(),
            row.result.label.clone(),
            &group,
            &scoped,
        );
        row.result.mean_loss = fixed.mean_loss;
        row.result.mean_accuracy = fixed.mean_accuracy;
        row.result.analysed = fixed.analysed;
    }
    for problem in &mut report.problems {
        let mut cases = vec![];
        let mut losses = vec![];
        for g in &selected {
            for e in current_q[&g.id]
                .decisions
                .iter()
                .filter(|e| e.phase == "opening" && e.loss_q.is_some_and(|v| v >= 10.))
            {
                let root = &g.replay.nodes[e.ply as usize - 1];
                let played = &g.replay.nodes[e.ply as usize];
                if root.position_key == problem.position_key
                    && played.uci.as_deref() == Some(&problem.played_uci)
                {
                    cases.push(case(g, played, map.get(&g.id).copied()));
                    losses.push(e.loss_q.unwrap());
                }
            }
        }
        problem.cases = cases;
        problem.median_loss = metrics::quantile(&losses, 0.5);
        let selected_edges = matching_edges(&selected, &problem.position_key, &problem.played_uci);
        let old_edges = matching_edges(&previous, &problem.position_key, &problem.played_uci);
        problem.comparison = compare_edge(&selected_edges, &old_edges, current_q, &previous_q);
    }
    let mut all_actor_q = current_q.clone();
    for (id, m) in &previous_q {
        all_actor_q.entry(id.clone()).or_insert_with(|| m.clone());
    }
    let mut processed = HashSet::new();
    for g in selected
        .iter()
        .chain(previous.iter())
        .filter(|g| processed.insert(g.id.clone()))
    {
        let mut other = (*g).clone();
        other.own_color = if g.own_color == "white" {
            "black"
        } else {
            "white"
        }
        .into();
        let opposite = metrics::game_metrics(&other, map.get(&g.id).copied());
        let entry = all_actor_q.entry(g.id.clone()).or_default();
        entry.decisions.extend(opposite.decisions);
        entry.move_accuracies.extend(opposite.move_accuracies);
    }
    let node = enrich_tree(
        report,
        &selected,
        &previous,
        &history,
        &all_actor_q,
        &all_actor_q,
        &map,
    );
    report.opening_analysis=OpeningAnalysis{method:"opening_analytics_v2:catalog-name-path-v1; visits=all; results=unique-games; O12=full-next-3-own-decisions".into(),hierarchy:hierarchy(&selected,current_q,&map,&previous,&previous_q),node,boundaries:boundaries(&selected,&map),first_losses:first_losses(&selected,current_q,&map),opponent_responses:opponent_responses(&selected,current_q,&map),comparison:compare(&selected,&previous,current_q,&previous_q),blockers:vec![OpeningBlocker{id:"O14".into(),status:"BLOCKED".into(),dependency:"Требуется предоставленный пользователем versioned PGN-репертуар, allowed-move graph и явно выбранный position/path режим. Модуля репертуара сейчас нет.".into()},OpeningBlocker{id:"O15".into(),status:"BLOCKED".into(),dependency:"Требуется тот же предоставленный versioned PGN-репертуар для проверки покрытия ответов; личные партии не доказывают знание репертуара.".into()}]};
}

fn unique_games<'a>(values: &[(&'a StoredGame, &ReplayNode)]) -> Vec<&'a StoredGame> {
    let mut seen = HashSet::new();
    values
        .iter()
        .filter_map(|(g, _)| seen.insert(g.id.clone()).then_some(*g))
        .collect()
}

fn compare_edge(
    current: &[(&StoredGame, &ReplayNode)],
    previous: &[(&StoredGame, &ReplayNode)],
    q: &HashMap<String, GameMetrics>,
    pq: &HashMap<String, GameMetrics>,
) -> Vec<MetricValue> {
    let mut rows = vec![];
    if previous.is_empty() {
        return rows;
    }
    for (id, name, unit) in [
        (
            "O07_edge_loss_delta",
            "Δ потерь выбранного продолжения",
            "п.п.",
        ),
        (
            "O10_edge_spent_delta",
            "Δ расхода выбранного продолжения",
            "с",
        ),
        (
            "O11_edge_error_delta",
            "Δ частоты существенных потерь продолжения",
            "п.п.",
        ),
    ] {
        let values = |entries: &[(&StoredGame, &ReplayNode)],
                      map: &HashMap<String, GameMetrics>|
         -> Option<f64> {
            let values: Vec<_> = entries
                .iter()
                .filter_map(|(g, n)| {
                    if id == "O10_edge_spent_delta" {
                        n.spent_seconds
                    } else {
                        map.get(&g.id)?
                            .decisions
                            .iter()
                            .find(|e| e.ply == n.ply)?
                            .loss_q
                    }
                })
                .collect();
            if id == "O10_edge_spent_delta" {
                metrics::quantile(&values, 0.5)
            } else if id == "O11_edge_error_delta" {
                (!values.is_empty()).then(|| {
                    100. * values.iter().filter(|v| **v >= 10.).count() as f64 / values.len() as f64
                })
            } else {
                metrics::mean(&values)
            }
        };
        let mut strata: VisitStrata<'_> = BTreeMap::new();
        for (g, n) in current {
            strata.entry(query::stratum(g)).or_default().0.push((g, n));
        }
        for (g, n) in previous {
            strata.entry(query::stratum(g)).or_default().1.push((g, n));
        }
        let (mut sum, mut weight) = (0., 0.);
        for (a, b) in strata.values() {
            if let Some((av, bv)) = values(a, q).zip(values(b, pq)) {
                let w = (unique_games(a).len() + unique_games(b).len()) as f64;
                sum += w * (av - bv);
                weight += w;
            }
        }
        rows.push(MetricValue{id:id.into(),name:name.into(),unit:unit.into(),value:(weight>0.).then(||sum/weight),sample:current.len(),eligible:previous.len(),definition:"Только посещения данного Kpos/played edge, разные периоды и общие source/perf/control/color/rating strata; описательное сравнение без причинного вывода".into(),method:"opening_edge_same_context_period_v1".into(),game_ids:unique_games(current).iter().map(|g|g.id.clone()).collect(),status:if weight>0.{"descriptive"}else{"no_common_strata"}.into(),..Default::default()});
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;
    fn game(id: &str, moves: &str, color: &str, time: i64) -> StoredGame {
        let mut replay = chess::replay_pgn(&format!("{moves} 1-0")).unwrap();
        replay.opening = chess::classify_opening(&replay);
        StoredGame {
            id: id.into(),
            source: "lichess".into(),
            account_id: "me".into(),
            own_color: color.into(),
            result: "1-0".into(),
            rated: Some(true),
            speed: "rapid".into(),
            variant: "standard".into(),
            eligibility: "eligible".into(),
            started_at_ms: Some(time),
            initial_seconds: Some(600.),
            increment_seconds: Some(0.),
            white: Participant {
                rating: Some(1500),
                ..Default::default()
            },
            black: Participant {
                rating: Some(1600),
                ..Default::default()
            },
            replay,
            ..Default::default()
        }
    }
    fn eval(ply: u32, q: u32) -> PositionEvaluation {
        PositionEvaluation {
            ply,
            complete: true,
            cp: Some(q as i32 - 500),
            wdl: Some([q, 0, 1000 - q]),
            ..Default::default()
        }
    }
    fn run(g: &StoredGame, changes: &[(u32, u32, u32)]) -> EngineRun {
        EngineRun {
            id: format!("run:{}", g.id),
            game_id: g.id.clone(),
            profile_id: "p".into(),
            evaluations: g.replay.nodes.iter().map(|n| eval(n.ply, 500)).collect(),
            confirmed_moves: changes
                .iter()
                .map(|(ply, best, played)| MoveComparison {
                    ply: *ply,
                    best: eval(ply - 1, *best),
                    played: eval(ply - 1, *played),
                    method: "same_root_searchmoves".into(),
                    complete: true,
                })
                .collect(),
            ..Default::default()
        }
    }
    #[test]
    fn literal_catalogue_path_has_real_parents_and_color_denominators() {
        let name = "Benoni Defense: Fianchetto Variation, Hastings Defense, Main Line";
        let path = chess::opening_name_path(name);
        assert_eq!(path.len(), 4);
        assert_eq!(path[0], "Benoni Defense");
        assert_eq!(
            path[2],
            "Benoni Defense: Fianchetto Variation, Hastings Defense"
        );
        let source: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../public/openings.json")).unwrap();
        let entry = source.iter().find(|e| e["name"] == name).unwrap();
        let moves = entry["pgn"].as_str().unwrap();
        let mut a = game("a", moves, "white", 1);
        a.replay.opening.as_mut().unwrap().name = name.into();
        a.replay.opening.as_mut().unwrap().family = "Benoni Defense".into();
        let mut b = a.clone();
        b.id = "b".into();
        let mut c = a.clone();
        c.id = "c".into();
        c.own_color = "black".into();
        let r = query::report(&[a, b, c], &[], &StatisticsFilter::default());
        let white: Vec<_> = r
            .opening_analysis
            .hierarchy
            .iter()
            .filter(|n| n.color == "white")
            .collect();
        assert_eq!(white.len(), 4);
        for node in &white {
            assert_eq!(node.result.games, 2);
            assert_eq!(node.frequency, Some(100.));
            if let Some(parent) = &node.parent_id {
                assert!(white.iter().any(|n| &n.id == parent));
            }
        }
        let leaf = white.iter().find(|n| n.path.len() == 4).unwrap();
        assert_eq!(leaf.rating_difference_median, Some(-100.));
        let filter = StatisticsFilter {
            opening_label_id: Some(leaf.id.clone()),
            ..Default::default()
        };
        assert_eq!(r.games.iter().filter(|g| g.own_color == "white").count(), 2);
        assert!(chess::opening_label_matches(
            &game("g", moves, "white", 1),
            &chess::opening_label_id("white", &path[..1])
        ));
        assert!(filter.opening_label_id.is_some());
    }
    #[test]
    fn actor_filter_and_repeat_visits_preserve_unique_game_results() {
        let a = game("a", "1. Nf3 Nf6 2. Ng1 Ng8 3. e4 e5", "white", 1);
        let mut b = a.clone();
        b.id = "b".into();
        b.own_color = "black".into();
        let f = StatisticsFilter {
            opening_actor: Some("own".into()),
            ..Default::default()
        };
        let r = query::report(&[a.clone(), b], &[], &f);
        assert_eq!(r.opening_tree.games, 1);
        assert_eq!(r.opening_tree.visits, 2);
        assert_eq!(r.opening_tree.distinct_paths, 2);
        assert_eq!(
            r.opening_tree
                .edges
                .iter()
                .map(|e| e.frequency)
                .sum::<f64>(),
            100.
        );
        assert!(
            r.opening_tree
                .edges
                .iter()
                .all(|e| e.result.games == 1 && e.actor == "own" && e.cases.len() == 1)
        );
        let opposite = query::report(
            &[a],
            &[],
            &StatisticsFilter {
                opening_actor: Some("opponent".into()),
                ..Default::default()
            },
        );
        assert_eq!(opposite.opening_tree.games, 0);
    }
    #[test]
    fn position_state_keeps_history_contexts_and_rejects_conflicting_duplicates() {
        let a = game("a", "1. Nf3 Nf6 2. g3 g6 3. Bg2", "white", 1);
        let b = game("b", "1. g3 g6 2. Nf3 Nf6 3. Bg2", "white", 2);
        let mut ar = run(&a, &[]);
        ar.evaluations[4] = eval(4, 500);
        let mut br = run(&b, &[]);
        br.evaluations[4] = eval(4, 700);
        let f = StatisticsFilter {
            position_key: Some(a.replay.nodes[4].position_key.clone()),
            ..Default::default()
        };
        let r = query::report(&[a.clone(), b], &[ar.clone(), br], &f);
        assert_eq!(r.opening_analysis.node.unique_contexts, 2);
        assert!((r.opening_analysis.node.q_median.unwrap() - 0.6).abs() < 1e-9);
        let mut clone = a.clone();
        clone.id = "c".into();
        let mut cr = run(&clone, &[]);
        cr.evaluations[4] = eval(4, 900);
        let r = query::report(&[a, clone], &[ar, cr], &f);
        assert_eq!(r.opening_analysis.node.unique_contexts, 1);
        assert_eq!(r.opening_analysis.node.usable_contexts, 0);
        assert_eq!(
            r.opening_analysis.node.contexts[0].status,
            "conflicting_observations"
        );
    }
    #[test]
    fn opening_quality_excludes_later_loss_and_has_actual_clocks() {
        let mut g = game("g", "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6", "white", 1);
        g.replay.nodes[5].phase = "middlegame".into();
        g.replay.nodes[1].spent_seconds = Some(2.);
        g.replay.nodes[3].spent_seconds = Some(4.);
        let r = run(&g, &[(1, 500, 450), (3, 500, 500), (5, 500, 100)]);
        let report = query::report(&[g], &[r], &StatisticsFilter::default());
        let row = &report.openings[0];
        assert!((row.result.mean_loss.unwrap() - 2.5).abs() < 1e-9);
        let quality = &report.opening_analysis.hierarchy[0].quality;
        assert_eq!(quality.decisions, 2);
        assert_eq!(quality.mean_spent_per_game, Some(6.));
        assert_eq!(quality.median_spent, Some(3.));
        assert_eq!(quality.clock_coverage, Some(100.));
        assert_eq!(quality.time_games[0].known_seconds, Some(6.));
        assert_eq!(quality.time_games[0].known_decisions, 2);
    }
    #[test]
    fn missing_clocks_and_exact_tablebase_evidence_are_not_fake_zeros() {
        let mut a = game("a", "1. e4 e5 2. Nf3 Nc6", "white", 1);
        let b = game("b", "1. e4 e5 2. Nf3 Nc6", "white", 2);
        a.replay.nodes[1].spent_seconds = Some(0.);
        let mut ar = run(&a, &[]);
        ar.evaluations.clear();
        ar.tablebases
            .push(super::super::engine_metrics::TablebaseObservation {
                ply: 0,
                wdl: Some(0),
                dtz: Some(0),
                status: "available".into(),
                manifest_id: "fixture-manifest".into(),
                context_hash: "fixture-history".into(),
                rule50: true,
                ..Default::default()
            });
        let r = query::report(&[a, b], &[ar], &StatisticsFilter::default());
        let quality = &r.opening_analysis.hierarchy[0].quality;
        assert_eq!(quality.mean_spent_per_game, Some(0.));
        assert_eq!(quality.spent_game_sample, 1);
        assert_eq!(
            quality
                .time_games
                .iter()
                .find(|g| g.game_id == "a")
                .unwrap()
                .known_seconds,
            Some(0.)
        );
        assert_eq!(
            quality
                .time_games
                .iter()
                .find(|g| g.game_id == "b")
                .unwrap()
                .known_seconds,
            None
        );
        let context = &r.opening_analysis.node.contexts[0];
        assert_eq!(context.status, "tablebase_only");
        assert_eq!(context.tablebase.as_ref().unwrap().wdl, Some(0));
        assert_eq!(context.q, None);
        assert_eq!(r.opening_analysis.node.q_median, None);
    }
    #[test]
    fn boundaries_are_separate_and_use_player_pov_without_fake_fixed_slice() {
        let mut a = game("a", "1. e4 e5 2. Nf3 Nc6", "white", 1);
        a.replay.middle_ply = Some(3);
        let mut b = a.clone();
        b.id = "b".into();
        b.own_color = "black".into();
        let mut ar = run(&a, &[]);
        ar.evaluations[3] = eval(3, 900);
        let mut br = run(&b, &[]);
        br.evaluations[3] = eval(3, 900);
        let r = query::report(&[a, b], &[ar, br], &StatisticsFilter::default());
        let phase = r
            .opening_analysis
            .boundaries
            .iter()
            .find(|b| b.kind == "phase")
            .unwrap();
        assert_eq!(phase.sample, 2);
        assert_eq!(phase.advantage_percent, Some(50.));
        assert_eq!(phase.hard_percent, Some(50.));
        assert!(
            phase
                .cases
                .iter()
                .any(|c| c.pov == "black" && c.cp == Some(-400))
        );
        let fixed = r
            .opening_analysis
            .boundaries
            .iter()
            .find(|b| b.kind == "fixed_20")
            .unwrap();
        assert_eq!(fixed.sample, 0);
        assert!(
            fixed
                .cases
                .iter()
                .all(|c| c.status == "no_boundary" && c.q.is_none())
        );
    }
    #[test]
    fn first_loss_distinguishes_first_observed_from_confirmed_first() {
        let g = game("g", "1. e4 e5 2. Nf3 Nc6", "white", 1);
        let incomplete = run(&g, &[(3, 500, 300)]);
        let r = query::report(
            std::slice::from_ref(&g),
            &[incomplete],
            &StatisticsFilter::default(),
        );
        assert_eq!(
            r.opening_analysis.first_losses[0].status,
            "first_observed_loss_partial"
        );
        let complete = run(&g, &[(1, 500, 500), (3, 500, 300)]);
        let r = query::report(
            std::slice::from_ref(&g),
            &[complete],
            &StatisticsFilter::default(),
        );
        assert_eq!(
            r.opening_analysis.first_losses[0].status,
            "confirmed_first_loss"
        );
        let clean = run(&g, &[(1, 500, 500), (3, 500, 500)]);
        let r = query::report(&[g], &[clean], &StatisticsFilter::default());
        assert_eq!(
            r.opening_analysis.first_losses[0].status,
            "complete_no_loss"
        );
    }
    #[test]
    fn opponent_response_requires_three_real_analysed_personal_decisions() {
        let mut g = game("g", "1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. O-O Nf6", "white", 1);
        for n in &mut g.replay.nodes {
            n.phase = "opening".into();
        }
        let r = run(&g, &[(3, 500, 400), (5, 500, 300), (7, 500, 500)]);
        let report = query::report(&[g], &[r], &StatisticsFilter::default());
        let full = report
            .opening_analysis
            .opponent_responses
            .iter()
            .find(|r| r.san == "e5")
            .unwrap();
        assert_eq!(full.eligible_windows, 1);
        assert!((full.mean_loss.unwrap() - 10.).abs() < 1e-9);
        assert!((full.significant_per_100.unwrap() - 200. / 3.).abs() < 1e-9);
        let short = report
            .opening_analysis
            .opponent_responses
            .iter()
            .find(|r| r.san == "Nc6")
            .unwrap();
        assert_eq!(short.eligible_windows, 0);
        assert_eq!(short.cases[0].status, "short_game");
        assert_eq!(short.mean_loss, None);
    }
    #[test]
    fn periods_use_all_saved_history_for_first_seen_and_repertoire_is_blocked() {
        let old = game("old", "1. e4 e5", "white", 1000);
        let previous = game("previous", "1. d4 d5", "white", 7000);
        let current = game("current", "1. e4 c5", "white", 20000);
        let f = StatisticsFilter {
            since_ms: Some(15000),
            until_ms: Some(30000),
            comparison_since_ms: Some(5000),
            comparison_until_ms: Some(10000),
            ..Default::default()
        };
        let r = query::report(&[old, previous, current], &[], &f);
        let edge = r.opening_tree.edges.iter().find(|e| e.san == "e4").unwrap();
        assert_eq!(edge.temporal.first_seen_ms, Some(1000));
        assert_eq!(edge.temporal.previous_share, Some(0.));
        assert_eq!(edge.temporal.current_share, Some(100.));
        assert_eq!(edge.temporal.repeated_paths, 1);
        assert_eq!(edge.temporal.new_paths, 0);
        assert_eq!(r.opening_analysis.blockers.len(), 2);
        assert!(
            r.opening_analysis
                .blockers
                .iter()
                .all(|b| b.status == "BLOCKED" && ["O14", "O15"].contains(&b.id.as_str()))
        );
    }
}

fn matching_edges<'a>(
    games: &[&'a StoredGame],
    position: &str,
    uci: &str,
) -> Vec<(&'a StoredGame, &'a ReplayNode)> {
    games
        .iter()
        .flat_map(|g| {
            g.replay
                .nodes
                .windows(2)
                .filter(|p| {
                    p[0].position_key == position
                        && p[1].uci.as_deref() == Some(uci)
                        && own_actor(g, &p[1])
                        && p[1].phase == "opening"
                })
                .map(|p| (*g, &p[1]))
        })
        .collect()
}

#[cfg(test)]
mod period_and_actor_tests {
    use super::*;
    fn fixture(id: &str, time: i64) -> StoredGame {
        let mut replay =
            chess::replay_pgn("1. e4 e5 2. Nf3 Nc6 3. Bc4 Bc5 4. O-O Nf6 1-0").unwrap();
        for node in &mut replay.nodes {
            node.phase = "opening".into();
        }
        replay.opening = chess::classify_opening(&replay);
        StoredGame {
            id: id.into(),
            source: "lichess".into(),
            own_color: "white".into(),
            variant: "standard".into(),
            speed: "rapid".into(),
            result: "1-0".into(),
            rated: Some(true),
            eligibility: "eligible".into(),
            started_at_ms: Some(time),
            initial_seconds: Some(600.),
            increment_seconds: Some(0.),
            replay,
            ..Default::default()
        }
    }
    fn e(q: u32) -> PositionEvaluation {
        PositionEvaluation {
            complete: true,
            wdl: Some([q, 0, 1000 - q]),
            ..Default::default()
        }
    }
    fn run(g: &StoredGame, changes: &[(u32, u32, u32)]) -> EngineRun {
        EngineRun {
            id: format!("run:{}", g.id),
            game_id: g.id.clone(),
            profile_id: "p".into(),
            confirmed_moves: changes
                .iter()
                .map(|(ply, a, b)| MoveComparison {
                    ply: *ply,
                    best: e(*a),
                    played: e(*b),
                    complete: true,
                    method: "same_root_searchmoves".into(),
                })
                .collect(),
            ..Default::default()
        }
    }
    #[test]
    fn problem_period_difference_includes_good_visits_of_the_same_played_edge() {
        let old = fixture("old", 1000);
        let current_bad = fixture("bad", 20000);
        let current_good = fixture("good", 21000);
        let runs = [
            run(&old, &[(1, 500, 500)]),
            run(&current_bad, &[(1, 500, 300)]),
            run(&current_good, &[(1, 500, 500)]),
        ];
        let report = query::report(
            &[old, current_bad, current_good],
            &runs,
            &StatisticsFilter {
                since_ms: Some(15000),
                until_ms: Some(30000),
                comparison_since_ms: Some(0),
                comparison_until_ms: Some(10000),
                ..Default::default()
            },
        );
        let problem = &report.problems[0];
        assert_eq!(problem.occurrences, 1);
        assert_eq!(problem.cases.len(), 1);
        let delta = problem
            .comparison
            .iter()
            .find(|m| m.id == "O07_edge_loss_delta")
            .unwrap();
        assert_eq!(delta.value, Some(10.));
        let frequency = problem
            .comparison
            .iter()
            .find(|m| m.id == "O11_edge_error_delta")
            .unwrap();
        assert_eq!(frequency.value, Some(50.));
    }
    #[test]
    fn opponent_edge_quality_uses_opponent_pov_and_missing_windows_stay_missing() {
        let g = fixture("game", 1);
        let r = run(&g, &[(2, 500, 600), (3, 500, 400), (7, 500, 500)]);
        let report = query::report(
            std::slice::from_ref(&g),
            &[r],
            &StatisticsFilter {
                line: Some(vec!["e2e4".into()]),
                opening_actor: Some("opponent".into()),
                tree_mode: Some("line".into()),
                ..Default::default()
            },
        );
        let edge = &report.opening_tree.edges[0];
        assert_eq!(edge.actor, "opponent");
        assert_eq!(edge.quality.mean_loss, Some(10.));
        assert_eq!(edge.quality.wdl_sample, 1);
        let response = report
            .opening_analysis
            .opponent_responses
            .iter()
            .find(|r| r.san == "e5")
            .unwrap();
        assert_eq!(response.eligible_windows, 0);
        assert_eq!(response.cases[0].status, "incomplete_analysis");
        assert_eq!(response.mean_loss, None);
    }
    #[test]
    fn previous_only_continuation_is_not_a_fake_current_result() {
        for (color, actor) in [("white", "own"), ("black", "opponent")] {
            let mut old = fixture("old", 1000);
            old.own_color = color.into();
            let current = fixture("current", 20000);
            let mut other = current.clone();
            other.replay = chess::replay_pgn("1. d4 d5 1-0").unwrap();
            other.replay.opening = chess::classify_opening(&other.replay);
            let report = query::report(
                &[old, other],
                &[],
                &StatisticsFilter {
                    since_ms: Some(15000),
                    until_ms: Some(30000),
                    comparison_since_ms: Some(0),
                    comparison_until_ms: Some(10000),
                    ..Default::default()
                },
            );
            let old_edge = report
                .opening_tree
                .edges
                .iter()
                .find(|e| e.san == "e4")
                .unwrap();
            assert_eq!(old_edge.actor, actor);
            assert_eq!(old_edge.result.games, 0);
            assert_eq!(old_edge.result.score, None);
            assert_eq!(old_edge.temporal.current_share, Some(0.));
            assert_eq!(old_edge.temporal.previous_share, Some(100.));
            assert_eq!(old_edge.temporal.previous_cases.len(), 1);
            assert!(!report.opening_tree.unplayed.iter().any(|m| m.uci == "e2e4"));
            assert!(
                report
                    .opening_analysis
                    .hierarchy
                    .iter()
                    .any(|h| h.result.games == 0 && h.previous_result.games > 0)
            );
        }
    }
}

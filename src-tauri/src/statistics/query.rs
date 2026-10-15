//! Statistics application queries: all chess calculations stay in Rust, GUI consumes DTOs.
use super::{
    chess,
    metrics::{self, GameMetrics},
    types::*,
};
use std::collections::{BTreeMap, HashMap, HashSet};

pub fn game_time(g: &StoredGame) -> Option<i64> {
    g.finished_at_ms.or(g.last_move_at_ms).or(g.started_at_ms)
}
pub fn control(g: &StoredGame) -> String {
    match (g.initial_seconds, g.increment_seconds) {
        (Some(a), Some(b)) => format!("{a}+{b}"),
        _ => "unknown".into(),
    }
}
fn players(g: &StoredGame) -> (&Participant, &Participant) {
    if g.own_color == "white" {
        (&g.white, &g.black)
    } else {
        (&g.black, &g.white)
    }
}
pub(crate) fn eligible(g: &StoredGame, f: &StatisticsFilter) -> bool {
    if !["1-0", "0-1", "1/2-1/2"].contains(&g.result.as_str()) || g.replay.nodes.len() < 2 {
        return false;
    }
    match g.eligibility.as_str() {
        "eligible" => {}
        "boundary_uncertain" if f.include_uncertain.unwrap_or(false) => {}
        _ => return false,
    }
    if f.account_id.as_ref().is_some_and(|x| *x != g.account_id)
        || f.source.as_ref().is_some_and(|x| *x != g.source)
        || f.color.as_ref().is_some_and(|x| *x != g.own_color)
        || f.speed.as_ref().is_some_and(|x| *x != g.speed)
        || f.variant.as_ref().is_some_and(|x| *x != g.variant)
        || f.rated.is_some_and(|x| Some(x) != g.rated)
        || f.control.as_ref().is_some_and(|x| *x != control(g))
    {
        return false;
    }
    if f.since_ms
        .is_some_and(|x| game_time(g).is_none_or(|t| t < x))
        || f.until_ms
            .is_some_and(|x| game_time(g).is_none_or(|t| t >= x))
    {
        return false;
    }
    let opponent = players(g).1;
    if f.opponent_rating_min
        .is_some_and(|x| opponent.rating.is_none_or(|r| r < x))
        || f.opponent_rating_max
            .is_some_and(|x| opponent.rating.is_none_or(|r| r > x))
    {
        return false;
    }
    if f.opening_label_id
        .as_ref()
        .is_some_and(|id| !chess::opening_label_matches(g, id))
    {
        return false;
    }
    if let Some(o) = &f.opening {
        let needle = o.to_lowercase();
        if !g.replay.opening.as_ref().is_some_and(|x| {
            x.name.to_lowercase().contains(&needle)
                || x.eco.to_lowercase() == needle
                || x.family.to_lowercase() == needle
        }) {
            return false;
        }
    }
    if f.game_ids.as_ref().is_some_and(|ids| !ids.contains(&g.id)) {
        return false;
    }
    true
}
fn dedup_games<'a>(games: &'a [StoredGame], filter: &StatisticsFilter) -> Vec<&'a StoredGame> {
    let mut seen = HashSet::new();
    games
        .iter()
        .filter(|g| eligible(g, filter) && seen.insert(g.id.clone()))
        .collect()
}
pub(crate) fn result_row(
    key: String,
    label: String,
    games: &[&StoredGame],
    quality: &HashMap<String, GameMetrics>,
) -> ResultRow {
    let wins = games
        .iter()
        .filter(|g| metrics::outcome(g) == "win")
        .count();
    let draws = games
        .iter()
        .filter(|g| metrics::outcome(g) == "draw")
        .count();
    let losses = games
        .iter()
        .filter(|g| metrics::outcome(g) == "loss")
        .count();
    let loss: Vec<_> = games
        .iter()
        .filter_map(|g| quality.get(&g.id))
        .flat_map(|m| m.loss_q.iter().copied())
        .collect();
    let ratings: Vec<_> = games
        .iter()
        .filter_map(|g| players(g).1.rating.map(|r| r as f64))
        .collect();
    let score_interval = metrics::bootstrap_mean(
        &games
            .iter()
            .filter_map(|g| metrics::own_score(g).map(|s| s * 100.))
            .collect::<Vec<_>>(),
    );
    let accuracies: Vec<_> = games
        .iter()
        .filter_map(|g| quality.get(&g.id))
        .flat_map(|m| m.move_accuracies.iter().map(|(_, a)| *a))
        .collect();
    ResultRow {
        key,
        label,
        games: games.len(),
        wins,
        draws,
        losses,
        score: (!games.is_empty())
            .then(|| 100. * (wins as f64 + 0.5 * draws as f64) / games.len() as f64),
        game_ids: games.iter().map(|g| g.id.clone()).collect(),
        mean_loss: metrics::mean(&loss),
        mean_accuracy: metrics::harmonic(&accuracies),
        analysed: loss.len(),
        opponent_rating_median: metrics::quantile(&ratings, 0.5),
        opponent_rating_p25: metrics::quantile(&ratings, 0.25),
        opponent_rating_p75: metrics::quantile(&ratings, 0.75),
        rating_sample: ratings.len(),
        provisional_opponents: games.iter().filter(|g| players(g).1.provisional).count(),
        score_low: score_interval.map(|x| x.0),
        score_high: score_interval.map(|x| x.1),
    }
}
pub fn tree(games: &[StoredGame], filter: &StatisticsFilter) -> Result<OpeningTree, String> {
    let selected = dedup_games(games, filter);
    let line = filter.line.clone().unwrap_or_default();
    let initial_fen = chess::line_position_from(
        filter
            .opening_initial_fen
            .as_deref()
            .unwrap_or(chess::INITIAL_FEN),
        &[],
    )?
    .fen;
    let generated = chess::line_position_from(&initial_fen, &line)?;
    let key = filter
        .position_key
        .clone()
        .unwrap_or_else(|| generated.position_key.clone());
    let mode = filter
        .tree_mode
        .as_deref()
        .unwrap_or(if filter.position_key.is_some() {
            "position"
        } else if filter.line.is_some() {
            "line"
        } else {
            "position"
        });
    let generated_matches = generated.position_key == key;
    let mut fen = generated.fen;
    let mut edge_spent: HashMap<String, Vec<f64>> = HashMap::new();
    let mut edges: BTreeMap<String, (String, String, String, Vec<&StoredGame>, usize)> =
        BTreeMap::new();
    let mut visited = HashSet::new();
    let mut paths = HashSet::new();
    let mut visits = 0;
    for g in &selected {
        for (i, n) in g.replay.nodes.iter().enumerate() {
            let matches = if mode == "line" {
                g.replay.initial_fen == initial_fen
                    && i == line.len()
                    && g.replay
                        .nodes
                        .iter()
                        .skip(1)
                        .take(i)
                        .map(|n| n.uci.as_deref().unwrap_or(""))
                        .eq(line.iter().map(String::as_str))
            } else {
                n.position_key == key
            };
            if !matches {
                continue;
            }
            if let Some(actor) = filter.opening_actor.as_deref().filter(|s| *s != "all") {
                let role = g
                    .replay
                    .nodes
                    .get(i + 1)
                    .and_then(|next| next.actor.as_deref())
                    .map(|color| {
                        if color == g.own_color {
                            "own"
                        } else {
                            "opponent"
                        }
                    });
                if role != Some(actor) {
                    continue;
                }
            }
            if !generated_matches {
                fen = n.fen.clone();
            }
            visits += 1;
            visited.insert(g.id.clone());
            paths.insert(format!(
                "{}|{}",
                g.replay.initial_fen,
                g.replay
                    .nodes
                    .iter()
                    .skip(1)
                    .take(i)
                    .filter_map(|n| n.uci.as_deref())
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
            if let Some(next) = g.replay.nodes.get(i + 1) {
                let uci = next.uci.clone().unwrap_or_default();
                if let Some(spent) = next.spent_seconds {
                    edge_spent.entry(uci.clone()).or_default().push(spent);
                }
                let edge = edges.entry(uci).or_insert_with(|| {
                    (
                        next.san.clone().unwrap_or_default(),
                        next.fen.clone(),
                        next.position_key.clone(),
                        vec![],
                        0,
                    )
                });
                edge.4 += 1;
                if !edge.3.iter().any(|x| x.id == g.id) {
                    edge.3.push(g);
                }
            }
        }
    }
    let total = edges.values().map(|e| e.4).sum::<usize>();
    let mut moves: Vec<_> = edges
        .into_iter()
        .map(
            |(uci, (san, fen, position_key, games, visits))| OpeningEdge {
                actor: String::new(),
                cases: vec![],
                quality: Default::default(),
                temporal: Default::default(),
                spent_median: edge_spent.get(&uci).and_then(|v| metrics::quantile(v, 0.5)),
                spent_sample: edge_spent.get(&uci).map_or(0, Vec::len),
                result: result_row(uci.clone(), san.clone(), &games, &HashMap::new()),
                uci,
                san,
                fen,
                position_key,
                frequency: if total > 0 {
                    100. * visits as f64 / total as f64
                } else {
                    0.
                },
                visits,
            },
        )
        .collect();
    moves.sort_by(|a, b| b.visits.cmp(&a.visits).then_with(|| a.uci.cmp(&b.uci)));
    let mut game_ids: Vec<_> = visited.into_iter().collect();
    game_ids.sort();
    let played: HashSet<_> = moves.iter().map(|e| e.uci.clone()).collect();
    let unplayed = chess::legal_continuations(&fen)?
        .into_iter()
        .filter(|m| !played.contains(&m.uci))
        .collect();
    Ok(OpeningTree {
        initial_fen,
        distinct_paths: paths.len(),
        unplayed,
        mode: mode.into(),
        fen,
        position_key: key,
        line,
        games: game_ids.len(),
        visits,
        edges: moves,
        game_ids,
    })
}
pub(crate) fn time_row(
    key: &str,
    label: &str,
    entries: &[(&StoredGame, &ReplayNode)],
    quality: &HashMap<String, GameMetrics>,
) -> TimeRow {
    let spent: Vec<_> = entries
        .iter()
        .filter_map(|(_, n)| n.spent_seconds)
        .collect();
    let ids: HashSet<_> = entries.iter().map(|(g, _)| g.id.clone()).collect();
    let events: Vec<_> = entries
        .iter()
        .filter_map(|(g, n)| {
            quality
                .get(&g.id)?
                .decisions
                .iter()
                .find(|e| e.ply == n.ply)
        })
        .collect();
    let loss: Vec<_> = events.iter().filter_map(|e| e.loss_q).collect();
    let mut game_ids: Vec<_> = ids.into_iter().collect();
    game_ids.sort();
    TimeRow {
        key: key.into(),
        label: label.into(),
        decisions: entries.len(),
        games: game_ids.len(),
        clock_sample: entries
            .iter()
            .filter(|(_, n)| n.remaining_before.is_some())
            .count(),
        spent_sample: spent.len(),
        median_spent: metrics::quantile(&spent, 0.5),
        p25_spent: metrics::quantile(&spent, 0.25),
        p75_spent: metrics::quantile(&spent, 0.75),
        p90_spent: metrics::quantile(&spent, 0.9),
        total_spent: (!spent.is_empty()).then(|| spent.iter().sum()),
        mean_loss: metrics::mean(&loss),
        significant: loss.iter().filter(|v| **v >= 10.).count(),
        analysed: loss.len(),
        game_ids,
    }
}
#[allow(clippy::too_many_arguments)]
fn value(
    id: &str,
    name: &str,
    unit: &str,
    v: Option<f64>,
    sample: usize,
    eligible: usize,
    definition: &str,
    ids: Vec<String>,
) -> MetricValue {
    MetricValue {
        id: id.into(),
        name: name.into(),
        unit: unit.into(),
        value: v,
        sample,
        eligible,
        definition: definition.into(),
        method: metrics::METRIC_VERSION.into(),
        game_ids: ids,
        status: if v.is_some() {
            "ready"
        } else if eligible == 0 {
            "no_sample"
        } else {
            "no_data"
        }
        .into(),
        ..Default::default()
    }
}
pub fn report(
    games: &[StoredGame],
    runs: &[EngineRun],
    filter: &StatisticsFilter,
) -> StatisticsReport {
    let selected = dedup_games(games, filter);
    let ids: Vec<_> = selected.iter().map(|g| g.id.clone()).collect();
    let n = selected.len();
    let profiles: HashSet<_> = runs
        .iter()
        .filter(|r| ids.contains(&r.game_id))
        .map(|r| r.profile_id.clone())
        .collect();
    let mut profile_ids: Vec<_> = profiles.into_iter().collect();
    profile_ids.sort();
    let profile_labels = runs
        .iter()
        .filter(|r| profile_ids.contains(&r.profile_id))
        .map(|r| (r.profile_id.clone(), profile_label(r)))
        .collect();
    let selected_profile = filter.profile_id.clone().or_else(|| {
        runs.iter()
            .filter(|r| ids.contains(&r.game_id))
            .max_by_key(|r| r.completed_at_ms)
            .map(|r| r.profile_id.clone())
    });
    let mut run_map: HashMap<String, &EngineRun> = HashMap::new();
    for run in runs
        .iter()
        .filter(|r| Some(&r.profile_id) == selected_profile.as_ref())
    {
        let entry = run_map.entry(run.game_id.clone()).or_insert(run);
        if run.completed_at_ms > entry.completed_at_ms {
            *entry = run;
        }
    }
    let mut quality: HashMap<String, GameMetrics> = HashMap::new();
    for g in &selected {
        let mut m = metrics::game_metrics(g, run_map.get(&g.id).copied());
        if let Some(phase) = &filter.phase {
            m.decisions.retain(|e| e.phase == *phase);
            m.events.retain(|e| e.phase == *phase);
            m.defensive_decisions.retain(|e| e.phase == *phase);
            m.advantage_decisions.retain(|e| e.phase == *phase);
            m.opportunities = m
                .events
                .iter()
                .filter(|e| e.kind == "opportunity_used" || e.kind == "opportunity_missed")
                .count();
            m.opportunity_total = m.opportunities;
            m.used_opportunities = m
                .events
                .iter()
                .filter(|e| e.kind == "opportunity_used")
                .count();
            m.conversion_episodes = m
                .events
                .iter()
                .filter(|e| e.kind == "advantage_entry")
                .count();
            m.defense_episodes = m
                .events
                .iter()
                .filter(|e| e.kind == "defense_entry")
                .count();
            m.conversion = m.conversion_episodes > 0;
            m.defense = m.defense_episodes > 0;
            m.loss_q = m.decisions.iter().filter_map(|e| e.loss_q).collect();
            m.cp_losses = m.decisions.iter().filter_map(|e| e.cp_loss).collect();
            if let Some(r) = run_map.get(&g.id) {
                let (a, moves) = metrics::game_accuracy(g, r, Some(phase));
                m.accuracy = a;
                m.move_accuracies = moves;
            }
        }
        quality.insert(g.id.clone(), m);
    }
    let entries: Vec<_> = selected
        .iter()
        .flat_map(|g| {
            g.replay
                .nodes
                .iter()
                .skip(1)
                .filter(|n| {
                    n.actor.as_deref() == Some(&g.own_color)
                        && filter.phase.as_ref().is_none_or(|p| *p == n.phase)
                })
                .map(|n| (*g, n))
        })
        .collect();
    let decisions = entries.len();
    let all_losses: Vec<_> = quality
        .values()
        .flat_map(|m| m.loss_q.iter().copied())
        .collect();
    let all_cp: Vec<_> = quality
        .values()
        .flat_map(|m| m.cp_losses.iter().copied())
        .collect();
    let all_pl: Vec<_> = quality
        .values()
        .flat_map(|m| m.decisions.iter().filter_map(|e| e.loss_pl))
        .collect();
    let accuracy_moves: Vec<_> = quality
        .values()
        .flat_map(|m| m.move_accuracies.iter().map(|(_, a)| *a))
        .collect();
    let scores: Vec<_> = selected
        .iter()
        .filter_map(|g| metrics::own_score(g).map(|s| s * 100.))
        .collect();
    let wins = selected
        .iter()
        .filter(|g| metrics::outcome(g) == "win")
        .count();
    let mut mv = vec![
        value(
            "G01",
            "Партий",
            "партий",
            Some(n as f64),
            n,
            n,
            "N=W+D+L; подтверждённые личные завершённые партии",
            ids.clone(),
        ),
        value(
            "G02",
            "Победы",
            "%",
            (n > 0).then(|| 100. * wins as f64 / n as f64),
            n,
            n,
            "100 × W / N; интервал Wilson 95%",
            ids.clone(),
        ),
        value(
            "G03",
            "Набранные очки",
            "%",
            metrics::mean(&scores),
            n,
            n,
            "100 × (W + 0.5D) / N; bootstrap целых партий",
            ids.clone(),
        ),
    ];
    if let Some((lo, hi)) = metrics::wilson(wins, n) {
        mv[1].low = Some(lo);
        mv[1].high = Some(hi);
    }
    if let Some((lo, hi)) = metrics::bootstrap_mean(&scores) {
        mv[2].low = Some(lo);
        mv[2].high = Some(hi);
    } else {
        mv[2].status = if scores.len() < 2 {
            "insufficient_interval_sample"
        } else {
            "descriptive_degenerate_interval"
        }
        .into();
    }
    let mut grouped: BTreeMap<String, Vec<&StoredGame>> = BTreeMap::new();
    for g in &selected {
        for key in [
            format!("color:{}", g.own_color),
            format!(
                "rated:{}",
                g.rated
                    .map(|v| if v { "rated" } else { "casual" })
                    .unwrap_or("unknown")
            ),
            format!("control:{} {} {}", g.source, g.speed, control(g)),
            format!("variant:{}", g.variant),
            format!("termination:{}", g.status),
        ] {
            grouped.entry(key).or_default().push(g);
        }
        let (own, opp) = players(g);
        let diff = own.rating.zip(opp.rating).map(|(a, b)| a - b);
        let bin = match diff {
            Some(d) if d < -200 => "<-200",
            Some(d) if d < -50 => "-200…-51",
            Some(d) if d <= 50 => "-50…50",
            Some(d) if d <= 200 => "51…200",
            Some(_) => ">200",
            None => "unknown",
        };
        grouped
            .entry(format!("rating_diff:{bin}"))
            .or_default()
            .push(g);
    }
    let results = grouped
        .into_iter()
        .map(|(key, g)| result_row(key.clone(), key, &g, &quality))
        .collect();
    let rating_pools: HashSet<_> = selected
        .iter()
        .map(|g| format!("{}:{}:{}", g.source, g.speed, g.variant))
        .collect();
    let ratings: Vec<_> = selected
        .iter()
        .filter_map(|g| players(g).1.rating.map(|r| r as f64))
        .collect();
    let lengths: Vec<_> = selected
        .iter()
        .map(|g| (g.replay.nodes.len() - 1) as f64)
        .collect();
    mv.push(value(
        "G06",
        "Рейтинг соперников · медиана",
        "пунктов",
        if rating_pools.len() <= 1 {
            metrics::quantile(&ratings, 0.5)
        } else {
            None
        },
        ratings.len(),
        n,
        "Медиана предыгровых рейтингов соперников; разные source/perf фильтровать отдельно",
        ids.clone(),
    ));
    mv.push(value(
        "G10",
        "Длина партии · медиана",
        "полуходов",
        metrics::quantile(&lengths, 0.5),
        n,
        n,
        "Медиана числа легально воспроизведённых полуходов",
        ids.clone(),
    ));
    let days: HashSet<_> = selected
        .iter()
        .filter_map(|g| game_time(g).map(|t| (t + 3 * 3600 * 1000).div_euclid(86400000)))
        .collect();
    mv.push(value("G11","Дней с партиями","дней",Some(days.len()as f64),n,n,"Дни с сохранёнными партиями, Europe/Moscow; не доказывает отсутствие игры при разрыве sync",ids.clone()));
    mv.push(value(
        "Q01",
        "ACPL",
        "cp",
        metrics::mean(&all_cp),
        all_cp.len(),
        decisions,
        "Среднее max(0,best cp − played cp), same-root; mate и bounds исключены",
        ids.clone(),
    ));
    mv.push(value(
        "Q02",
        "Потеря ожидаемого очка",
        "п.п.",
        metrics::mean(&all_losses),
        all_losses.len(),
        decisions,
        "Среднее 100 × max(0,q_best − q_played); q=(W+0.5D)/1000; same-root",
        ids.clone(),
    ));
    mv.push(value(
        "Q02_median",
        "Потеря ожидаемого очка · медиана",
        "п.п.",
        metrics::quantile(&all_losses, 0.5),
        all_losses.len(),
        decisions,
        "Медиана Lq пригодных решений",
        ids.clone(),
    ));
    mv.push(value(
        "Q02_p90",
        "Потеря ожидаемого очка · P90",
        "п.п.",
        metrics::quantile(&all_losses, 0.9),
        all_losses.len(),
        decisions,
        "90-й процентиль Lq пригодных решений",
        ids.clone(),
    ));
    mv.push(value(
        "Q03",
        "Потеря Lichess WinPercent",
        "п.п.",
        metrics::mean(&all_pl),
        all_pl.len(),
        decisions,
        "Потеря pL=100/(1+exp(-0.00368208×cp)); same-root; отдельно от WDL q",
        ids.clone(),
    ));
    mv.push(value("Q07","Accuracy · группа Insights","%",metrics::harmonic(&accuracy_moves),accuracy_moves.len(),decisions,"H(a)=n/Σ(1/max(1,a)); соседние оценки и upstream начальная cp=15 стандартной партии; uncertainty bonus +1",ids.clone()));
    for threshold in [5, 10, 20] {
        let count = all_losses
            .iter()
            .filter(|l| **l >= threshold as f64)
            .count();
        mv.push(value(
            if threshold == 10 {
                "Q09"
            } else if threshold == 5 {
                "Q09_5"
            } else {
                "Q09_20"
            },
            &format!("Потери ≥{threshold} п.п."),
            "на 100 решений",
            (!all_losses.is_empty()).then(|| 100. * count as f64 / all_losses.len() as f64),
            all_losses.len(),
            decisions,
            "100 × count(Lq ≥ порог) / пригодные собственные решения; порог проектный",
            ids.clone(),
        ));
    }
    let acceptable: Vec<_> = quality
        .values()
        .flat_map(|m| m.decisions.iter())
        .filter(|e| e.before_q.is_some_and(|q| q >= 0.5) && e.after_q.is_some())
        .collect();
    let retained = acceptable
        .iter()
        .filter(|e| e.after_q.unwrap() >= 0.48)
        .count();
    mv.push(value(
        "Q10",
        "Сохранено приемлемое состояние",
        "%",
        (!acceptable.is_empty()).then(|| 100. * retained as f64 / acceptable.len() as f64),
        acceptable.len(),
        decisions,
        "q_after ≥0.48 среди q_before≥0.50; epsilon=0.02",
        ids.clone(),
    ));
    let opportunity_total: usize = quality.values().map(|m| m.opportunity_total).sum();
    let opportunities = quality.values().map(|m| m.opportunities).sum::<usize>();
    let used = quality
        .values()
        .map(|m| m.used_opportunities)
        .sum::<usize>();
    mv.push(value("E01","Возможности после ответа соперника","случаев",(!all_losses.is_empty()).then_some(opportunity_total as f64),opportunity_total,decisions,"G=q_after_opponent−q_before_opponent ≥0.10; подтверждённые same-root пары и согласие соседних roots ≤0.05",ids.clone()));
    mv.push(value(
        "E02",
        "Использовано возможностей",
        "%",
        (opportunities > 0).then(|| 100. * used as f64 / opportunities as f64),
        opportunities,
        opportunities,
        "Собственная потеря ответа ≤0.02 среди возможностей G≥0.10",
        ids.clone(),
    ));
    mv.push(value("E01_unanswered","Возможности без пригодного ответа","случаев",(!all_losses.is_empty()).then_some(opportunity_total.saturating_sub(opportunities)as f64),opportunity_total,opportunity_total,"Подтверждённые улучшения G≥0.10 без ответа либо без согласованной оценки ответа; исключены из знаменателя E02",ids.clone()));
    let conversion: Vec<_> = selected
        .iter()
        .filter(|g| quality[&g.id].conversion)
        .copied()
        .collect();
    let converted = conversion
        .iter()
        .filter(|g| metrics::outcome(g) == "win")
        .count();
    mv.push(value("E08","Реализация преимущества","%",(!conversion.is_empty()).then(||100.*converted as f64/conversion.len()as f64),conversion.len(),conversion.len(),"Победы / игры с двумя последовательными пригодными личными решениями q≥0.90; выход q<0.80 после каждого хода",conversion.iter().map(|g|g.id.clone()).collect()));
    let defense: Vec<_> = selected
        .iter()
        .filter(|g| quality[&g.id].defense)
        .copied()
        .collect();
    let saved = defense
        .iter()
        .filter(|g| ["win", "draw"].contains(&metrics::outcome(g)))
        .count();
    mv.push(value(
        "E10",
        "Спасено трудных партий",
        "%",
        (!defense.is_empty()).then(|| 100. * saved as f64 / defense.len() as f64),
        defense.len(),
        n,
        "(W+D) / игры с двумя личными решениями q≤0.10; выход q>0.20",
        defense.iter().map(|g| g.id.clone()).collect(),
    ));
    let defensive: Vec<_> = quality
        .values()
        .flat_map(|m| m.defensive_decisions.iter().filter_map(|e| e.loss_q))
        .collect();
    mv.push(value("E11","Защитные потери","п.п.",metrics::mean(&defensive),defensive.len(),decisions,"Среднее Lq собственных решений внутри подтверждённых defense_v1 эпизодов; включая собственный выход",defense.iter().map(|g|g.id.clone()).collect()));
    let recovered = defense
        .iter()
        .filter(|g| quality[&g.id].defense_recovered)
        .count();
    mv.push(value("E11_recovery","Возврат к q≥0.50","%",(!defense.is_empty()).then(||100.*recovered as f64/defense.len()as f64),defense.len(),defense.len(),"Игры с последующим наблюдаемым q≥0.50 после defense_v1 входа / игры с подтверждённым входом",defense.iter().map(|g|g.id.clone()).collect()));
    let advantage: Vec<_> = quality
        .values()
        .flat_map(|m| m.advantage_decisions.iter().filter_map(|e| e.loss_q))
        .collect();
    mv.push(value(
        "E09",
        "Потери при реализации",
        "п.п.",
        metrics::mean(&advantage),
        advantage.len(),
        decisions,
        "Среднее Lq собственных решений внутри advantage_v1 эпизодов; включая собственный выход",
        conversion.iter().map(|g| g.id.clone()).collect(),
    ));
    let exits: Vec<_> = quality.values().filter_map(|m| m.exit_q).collect();
    mv.push(value("O08","Выход из дебюта · ожидаемое очко","%",metrics::mean(&exits).map(|q|q*100.),exits.len(),n,"Среднее q на обнаруженной границе middlegame Divider; короткие партии без границы исключены",ids.clone()));
    let spent: Vec<_> = entries
        .iter()
        .filter_map(|(_, n)| n.spent_seconds)
        .collect();
    mv.push(value("T01","Время хода · медиана","с",metrics::quantile(&spent,0.5),spent.len(),decisions,"Медиана восстановленного расхода часов; первый elapsed, gaps и неизвестный increment исключены",ids.clone()));
    let clock_entries: Vec<_> = entries
        .iter()
        .filter(|(_, n)| n.remaining_before.is_some())
        .copied()
        .collect();
    let low_clock = clock_entries
        .iter()
        .filter(|(_, n)| n.remaining_before.unwrap() < 30.)
        .count();
    mv.push(value(
        "T04",
        "Решения при остатке <30 с",
        "%",
        (!clock_entries.is_empty()).then(|| 100. * low_clock as f64 / clock_entries.len() as f64),
        clock_entries.len(),
        decisions,
        "Доля решений с известным remaining_before<30с; точный контроль и increment сохраняются",
        ids.clone(),
    ));
    mv.push(value("T02","Известный расход часов","с",(!spent.is_empty()).then(||spent.iter().sum()),spent.len(),decisions,"Сумма известного spent; распределение по фазам в таблице phases, пропуски не считаются нулём",ids.clone()));
    let remaining: Vec<_> = clock_entries
        .iter()
        .filter_map(|(_, n)| n.remaining_before)
        .collect();
    mv.push(value(
        "T03",
        "Остаток часов · медиана",
        "с",
        metrics::quantile(&remaining, 0.5),
        remaining.len(),
        decisions,
        "Медиана remaining_before; не подмена clock_after или spent",
        ids.clone(),
    ));
    for threshold in [10, 60] {
        let low = remaining.iter().filter(|v| **v < threshold as f64).count();
        mv.push(value(
            &format!("T04_{threshold}"),
            &format!("Решения при остатке <{threshold} с"),
            "%",
            (!remaining.is_empty()).then(|| 100. * low as f64 / remaining.len() as f64),
            remaining.len(),
            decisions,
            "100 × count(remaining_before < порог) / решения с известным remaining_before",
            ids.clone(),
        ));
    }
    let relative: Vec<_> = clock_entries
        .iter()
        .filter_map(|(g, n)| {
            let initial = g.initial_seconds?;
            let berserk = g.source_metadata["players"][&g.own_color]["berserk"]
                .as_bool()
                .unwrap_or(false);
            if initial <= 0. || berserk {
                return None;
            }
            Some(n.remaining_before? / initial * 100.)
        })
        .collect();
    mv.push(value("T03_relative","Относительный остаток · медиана","%",metrics::quantile(&relative,0.5),relative.len(),decisions,"100 × remaining_before / effective_initial; неизвестный berserk initial исключён, значения >100% сохраняются",ids.clone()));
    for threshold in [5, 10] {
        let low = relative.iter().filter(|v| **v < threshold as f64).count();
        mv.push(value(&format!("T04_relative_{threshold}"),&format!("Решения при остатке <{threshold}% начального"),"%",(!relative.is_empty()).then(||100.*low as f64/relative.len()as f64),relative.len(),decisions,"Доля решений с относительным остатком ниже операционного порога; initial и increment известны",ids.clone()));
    }
    let flagged = selected
        .iter()
        .filter(|g| g.status == "outoftime" && metrics::outcome(g) == "loss")
        .count();
    mv.push(value("T_flag","Поражения по времени","партий",Some(flagged as f64),n,n,"Личное поражение при API termination outoftime; timeout означает отключение, не подтверждённый флаг",ids.clone()));
    mv.push(value(
        "P04",
        "Покрытие движкового анализа",
        "%",
        (decisions > 0).then(|| 100. * all_losses.len() as f64 / decisions as f64),
        all_losses.len(),
        decisions,
        "Пригодные same-root WDL решения / личные решения в фильтре; один analysis profile",
        ids.clone(),
    ));
    let mut opening_groups: BTreeMap<String, Vec<&StoredGame>> = BTreeMap::new();
    for g in &selected {
        opening_groups
            .entry(
                g.replay
                    .opening
                    .as_ref()
                    .map(|o| format!("{}|{}", o.eco, o.name))
                    .unwrap_or_else(|| "unknown".into()),
            )
            .or_default()
            .push(g);
    }
    let mut openings: Vec<_> = opening_groups
        .into_iter()
        .map(|(key, games)| {
            let o = games[0].replay.opening.clone().unwrap_or(OpeningMatch {
                name: "Не классифицировано".into(),
                family: "Не классифицировано".into(),
                ..Default::default()
            });
            let exits: Vec<_> = games.iter().filter_map(|g| quality[&g.id].exit_q).collect();
            OpeningRow {
                eco: o.eco,
                name: o.name.clone(),
                family: o.family,
                variation: o.variation,
                result: result_row(key, o.name, &games, &quality),
                exit_q: metrics::mean(&exits),
                exit_sample: exits.len(),
                first_ply: Some(o.ply),
                position_key: Some(o.position_key),
            }
        })
        .collect();
    openings.sort_by_key(|a| std::cmp::Reverse(a.result.games));
    let phases = [
        ("opening", "Дебют"),
        ("middlegame", "Миттельшпиль"),
        ("endgame", "Эндшпиль"),
    ]
    .into_iter()
    .map(|(key, label)| {
        time_row(
            key,
            label,
            &entries
                .iter()
                .filter(|(_, n)| n.phase == key)
                .copied()
                .collect::<Vec<_>>(),
            &quality,
        )
    })
    .collect();
    let time = [
        ("<10", "<10 с", 0., 10.),
        ("10-30", "10–30 с", 10., 30.),
        ("30-60", "30–60 с", 30., 60.),
        ("60-180", "60–180 с", 60., 180.),
        (">=180", "≥180 с", 180., f64::INFINITY),
    ]
    .into_iter()
    .map(|(key, label, lo, hi)| {
        time_row(
            key,
            label,
            &entries
                .iter()
                .filter(|(_, n)| n.remaining_before.is_some_and(|c| c >= lo && c < hi))
                .copied()
                .collect::<Vec<_>>(),
            &quality,
        )
    })
    .collect();
    let mut endings_groups: BTreeMap<String, Vec<&StoredGame>> = BTreeMap::new();
    for g in &selected {
        let endings: HashSet<_> = g
            .replay
            .nodes
            .iter()
            .filter(|n| n.phase == "endgame")
            .map(|n| n.material.ending.clone())
            .collect();
        for ending in endings {
            endings_groups.entry(ending).or_default().push(g);
        }
    }
    let endings = endings_groups
        .into_iter()
        .map(|(key, games)| {
            let mut row = result_row(key.clone(), key.clone(), &games, &quality);
            let selected_decisions: Vec<_> = games
                .iter()
                .flat_map(|g| {
                    quality[&g.id].decisions.iter().filter(|e| {
                        e.phase == "endgame"
                            && g.replay
                                .nodes
                                .get(e.ply.saturating_sub(1) as usize)
                                .is_some_and(|n| n.material.ending == key)
                    })
                })
                .collect();
            let loss: Vec<_> = selected_decisions.iter().filter_map(|e| e.loss_q).collect();
            let accuracy: Vec<_> = games
                .iter()
                .flat_map(|g| {
                    quality[&g.id]
                        .move_accuracies
                        .iter()
                        .filter(|(ply, _)| {
                            g.replay
                                .nodes
                                .get(*ply as usize)
                                .is_some_and(|n| n.phase == "endgame")
                                && g.replay
                                    .nodes
                                    .get(ply.saturating_sub(1) as usize)
                                    .is_some_and(|n| n.material.ending == key)
                        })
                        .map(|(_, a)| *a)
                })
                .collect();
            row.mean_loss = metrics::mean(&loss);
            row.analysed = loss.len();
            row.mean_accuracy = metrics::harmonic(&accuracy);
            row
        })
        .collect();
    let trend = build_trend(&selected, &quality, filter);
    let mut summaries: Vec<_> = selected
        .iter()
        .map(|g| {
            let (own, opp) = players(g);
            let q = &quality[&g.id];
            GameSummary {
                id: g.id.clone(),
                external_id: g.external_id.clone(),
                account_id: g.account_id.clone(),
                source: g.source.clone(),
                timestamp_ms: game_time(g),
                observed_interval_seconds: observed_interval(g).0,
                interval_quality: observed_interval(g).1.into(),
                white: g.white.name.clone(),
                black: g.black.name.clone(),
                own_color: g.own_color.clone(),
                opponent: opp.name.clone(),
                opponent_rating: opp.rating,
                own_rating: own.rating,
                rating_diff: own.rating_diff,
                result: g.result.clone(),
                outcome: metrics::outcome(g).into(),
                speed: g.speed.clone(),
                rated: g.rated,
                control: control(g),
                opening: g.replay.opening.clone(),
                plies: g.replay.nodes.len() - 1,
                own_decisions: g
                    .replay
                    .nodes
                    .iter()
                    .skip(1)
                    .filter(|n| {
                        n.actor.as_deref() == Some(&g.own_color)
                            && filter.phase.as_ref().is_none_or(|p| n.phase == *p)
                    })
                    .count(),
                eligibility: g.eligibility.clone(),
                status: g.status.clone(),
                accuracy: q.accuracy,
                acpl: metrics::mean(&q.cp_losses),
                mean_loss: metrics::mean(&q.loss_q),
                significant: q.loss_q.iter().filter(|l| **l >= 10.).count(),
                analysed: q.loss_q.len(),
                clock_sample: g
                    .replay
                    .nodes
                    .iter()
                    .filter(|n| {
                        n.actor.as_deref() == Some(&g.own_color) && n.spent_seconds.is_some()
                    })
                    .count(),
                profile_id: run_map.get(&g.id).map(|r| r.profile_id.clone()),
            }
        })
        .collect();
    summaries.sort_by(|a, b| {
        b.timestamp_ms
            .cmp(&a.timestamp_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut events: Vec<_> = quality
        .values()
        .flat_map(|m| m.events.iter().cloned())
        .collect();
    events.sort_by(|a, b| {
        b.loss_q
            .partial_cmp(&a.loss_q)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.game_id.cmp(&b.game_id))
            .then(a.ply.cmp(&b.ply))
    });
    let mut comparison = vec![];
    if let (Some(since), Some(until)) = (filter.comparison_since_ms, filter.comparison_until_ms) {
        let mut other_filter = filter.clone();
        other_filter.since_ms = Some(since);
        other_filter.until_ms = Some(until);
        other_filter.comparison_since_ms = None;
        other_filter.comparison_until_ms = None;
        other_filter.profile_id = selected_profile.clone();
        let previous = dedup_games(games, &other_filter);
        let previous_quality: HashMap<_, _> = previous
            .iter()
            .map(|g| {
                (
                    g.id.clone(),
                    metrics::game_metrics(g, run_map.get(&g.id).copied()),
                )
            })
            .collect();
        let mut current_strata: BTreeMap<String, Vec<&StoredGame>> = BTreeMap::new();
        let mut previous_strata: BTreeMap<String, Vec<&StoredGame>> = BTreeMap::new();
        for g in &selected {
            current_strata.entry(stratum(g)).or_default().push(g);
        }
        for g in &previous {
            previous_strata.entry(stratum(g)).or_default().push(g);
        }
        let common: Vec<_> = current_strata
            .iter()
            .filter_map(|(key, a)| previous_strata.get(key).map(|b| (a, b)))
            .collect();
        let common_current = common.iter().map(|(a, _)| a.len()).sum::<usize>();
        let common_previous = common.iter().map(|(_, b)| b.len()).sum::<usize>();
        comparison.push(value("P04_comparison","Общие условия двух периодов","%",(n+previous.len()>0).then(||100.*(common_current+common_previous)as f64/(n+previous.len())as f64),common_current+common_previous,n+previous.len(),"Покрытие общих source/perf/variant/exact control/color/rated/рейтинговых групп соперников; missing rating отдельная страта",ids.clone()));
        for id in ["G03", "Q02", "Q09", "T01", "E02", "E08", "E10"] {
            let mut sum = 0.;
            let mut total_weight = 0.;
            let mut current_sample = 0;
            let mut previous_sample = 0;
            for (a, b) in &common {
                let (va, na) = cohort_metric(id, a, &quality, filter.phase.as_deref());
                let (vb, nb) = cohort_metric(id, b, &previous_quality, filter.phase.as_deref());
                if let Some((va, vb)) = va.zip(vb) {
                    let weight = (a.len() + b.len()) as f64;
                    sum += weight * (va - vb);
                    total_weight += weight;
                    current_sample += na;
                    previous_sample += nb;
                }
            }
            if let Some(base) = mv.iter().find(|m| m.id == id) {
                let mut delta = value(
                    &format!("delta_{id}"),
                    &format!("Δ {}", base.name),
                    &base.unit,
                    (total_weight > 0.).then(|| sum / total_weight),
                    current_sample,
                    previous_sample,
                    "Разница по общим стратам с одинаковыми весами объединённой выборки; неподходящие страты исключены в обоих периодах. Описательное сравнение, без теста значимости и причинного вывода",
                    ids.clone(),
                );
                delta.method = "common_strata_fixed_pooled_weights_v1".into();
                let overlap = filter.since_ms.unwrap_or(i64::MIN) < until
                    && since < filter.until_ms.unwrap_or(i64::MAX);
                if delta.value.is_some() {
                    if overlap {
                        delta.status = "no_independent_comparison".into();
                    } else if let Some((low, high)) = bootstrap_stratified_difference(
                        id,
                        &common,
                        &quality,
                        &previous_quality,
                        filter.phase.as_deref(),
                    ) {
                        delta.low = Some(low);
                        delta.high = Some(high);
                        if (high - low).abs() < 1e-12 {
                            delta.status = "descriptive_degenerate_interval".into();
                        }
                    } else {
                        delta.status = "insufficient_interval_sample".into();
                    }
                }
                comparison.push(delta);
            }
        }
    }
    mv.extend(additional_p0_metrics(&selected, &entries, decisions));
    let time_spent = [
        ("spent_<1", "<1 с", 0., 1.),
        ("spent_1-3", "1–3 с", 1., 3.),
        ("spent_3-10", "3–10 с", 3., 10.),
        ("spent_10-30", "10–30 с", 10., 30.),
        ("spent_>=30", "≥30 с", 30., f64::INFINITY),
    ]
    .into_iter()
    .map(|(key, label, lo, hi)| {
        time_row(
            key,
            label,
            &entries
                .iter()
                .filter(|(_, n)| n.spent_seconds.is_some_and(|v| v >= lo && v < hi))
                .copied()
                .collect::<Vec<_>>(),
            &quality,
        )
    })
    .collect();
    let problems = opening_problems(&selected, &quality);
    let mut report = StatisticsReport {
        evidence_revision: String::new(),
        evidence_counts: Default::default(),
        opening_analysis: Default::default(),
        extended: Default::default(),
        analysis: Default::default(),
        filter: filter.clone(),
        generated_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64,
        total_games: n,
        excluded_games: games.len().saturating_sub(n),
        total_decisions: decisions,
        analysed_decisions: all_losses.len(),
        profile_ids,
        profile_labels,
        selected_profile_id: selected_profile,
        metrics: mv,
        results,
        openings,
        problems,
        opening_tree: tree(games, filter).unwrap_or_default(),
        phases,
        time,
        time_spent,
        endings,
        trend,
        games: summaries,
        events,
        comparison,
    };
    super::opening_metrics::append(&mut report, games, runs, &quality);
    super::engine_metrics::append(&mut report, &selected, &run_map, filter);
    super::extended::append_extended(&mut report, games, runs);
    report
}
pub(crate) fn stratum(g: &StoredGame) -> String {
    let rating = players(g)
        .1
        .rating
        .map(|r| (r / 200).to_string())
        .unwrap_or_else(|| "missing".into());
    format!(
        "{}:{}:{}:{}:{}:{:?}:{}",
        g.source,
        g.speed,
        g.variant,
        control(g),
        g.own_color,
        g.rated,
        rating
    )
}
fn cohort_metric(
    id: &str,
    games: &[&StoredGame],
    quality: &HashMap<String, GameMetrics>,
    phase: Option<&str>,
) -> (Option<f64>, usize) {
    let decisions: Vec<_> = games
        .iter()
        .filter_map(|g| quality.get(&g.id))
        .flat_map(|m| m.decisions.iter())
        .filter(|e| phase.is_none_or(|p| p == e.phase))
        .collect();
    let losses: Vec<_> = decisions.iter().filter_map(|e| e.loss_q).collect();
    match id {
        "G03" => (
            metrics::mean(
                &games
                    .iter()
                    .filter_map(|g| metrics::own_score(g).map(|x| 100. * x))
                    .collect::<Vec<_>>(),
            ),
            games.len(),
        ),
        "Q02" => (metrics::mean(&losses), losses.len()),
        "Q09" => (
            (!losses.is_empty()).then(|| {
                100. * losses.iter().filter(|v| **v >= 10.).count() as f64 / losses.len() as f64
            }),
            losses.len(),
        ),
        "T01" => {
            let spent: Vec<_> = games
                .iter()
                .flat_map(|g| {
                    g.replay
                        .nodes
                        .iter()
                        .filter(|n| {
                            n.actor.as_deref() == Some(&g.own_color)
                                && phase.is_none_or(|p| p == n.phase)
                        })
                        .filter_map(|n| n.spent_seconds)
                })
                .collect();
            (metrics::quantile(&spent, 0.5), spent.len())
        }
        "E02" => {
            let events: Vec<_> = games
                .iter()
                .filter_map(|g| quality.get(&g.id))
                .flat_map(|m| m.events.iter())
                .filter(|e| {
                    phase.is_none_or(|p| p == e.phase)
                        && ["opportunity_used", "opportunity_missed"].contains(&e.kind.as_str())
                })
                .collect();
            (
                (!events.is_empty()).then(|| {
                    100. * events
                        .iter()
                        .filter(|e| e.kind == "opportunity_used")
                        .count() as f64
                        / events.len() as f64
                }),
                events.len(),
            )
        }
        "E08" | "E10" => {
            let kind = if id == "E08" {
                "advantage_entry"
            } else {
                "defense_entry"
            };
            let entered: Vec<_> = games
                .iter()
                .filter(|g| {
                    quality.get(&g.id).is_some_and(|m| {
                        m.events
                            .iter()
                            .any(|e| e.kind == kind && phase.is_none_or(|p| p == e.phase))
                    })
                })
                .collect();
            let success = entered
                .iter()
                .filter(|g| {
                    if id == "E08" {
                        metrics::outcome(g) == "win"
                    } else {
                        metrics::outcome(g) != "loss"
                    }
                })
                .count();
            (
                (!entered.is_empty()).then(|| 100. * success as f64 / entered.len() as f64),
                entered.len(),
            )
        }
        _ => (None, 0),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn game(id: &str, moves: &str, result: &str) -> StoredGame {
        StoredGame {
            id: id.into(),
            source: "lichess".into(),
            account_id: "lichess:me".into(),
            own_color: "white".into(),
            result: result.into(),
            eligibility: "eligible".into(),
            replay: chess::replay_pgn(&format!("{moves} {result}")).unwrap(),
            ..Default::default()
        }
    }
    #[test]
    fn empty_is_missing_and_observed_zero_is_zero() {
        let r = report(&[], &[], &StatisticsFilter::default());
        assert_eq!(
            r.metrics.iter().find(|m| m.id == "G03").unwrap().value,
            None
        );
        let g = game("a", "1. e4 e5", "0-1");
        let r = report(&[g], &[], &StatisticsFilter::default());
        assert_eq!(
            r.metrics.iter().find(|m| m.id == "G03").unwrap().value,
            Some(0.)
        );
        assert_eq!(
            r.metrics.iter().find(|m| m.id == "Q02").unwrap().value,
            None
        );
    }
    #[test]
    fn constant_scores_keep_observation_but_decline_false_interval_precision() {
        let games = [game("a", "1. e4 e5", "1-0"), game("b", "1. d4 d5", "1-0")];
        let r = report(&games, &[], &StatisticsFilter::default());
        let score = r.metrics.iter().find(|m| m.id == "G03").unwrap();
        assert_eq!(score.value, Some(100.));
        assert_eq!(score.sample, 2);
        assert_eq!((score.low, score.high), (None, None));
        assert_eq!(score.status, "descriptive_degenerate_interval");
        let one = report(&games[..1], &[], &StatisticsFilter::default());
        assert_eq!(
            one.metrics.iter().find(|m| m.id == "G03").unwrap().status,
            "insufficient_interval_sample"
        );
    }
    #[test]
    fn path_identity_keeps_initial_rule_context_and_legal_display_history() {
        let normal = game("normal", "1. e4 e5", "1-0");
        let mut custom = normal.clone();
        custom.id = "custom".into();
        custom.replay = chess::replay_pgn(
            "[FEN \"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 23\"]\n\n23. e4 e5 1-0",
        )
        .unwrap();
        let line = vec!["e2e4".into()];
        let filter = StatisticsFilter {
            tree_mode: Some("line".into()),
            line: Some(line.clone()),
            ..Default::default()
        };
        let games = [normal.clone(), custom.clone()];
        let t = tree(&games, &filter).unwrap();
        assert_eq!(t.game_ids, vec!["normal"]);
        let custom_filter = StatisticsFilter {
            opening_initial_fen: Some(custom.replay.initial_fen.clone()),
            ..filter.clone()
        };
        let t = tree(&games, &custom_filter).unwrap();
        assert_eq!(t.game_ids, vec!["custom"]);
        assert!(t.fen.ends_with(" 0 23"));
        let same_position = tree(
            &games,
            &StatisticsFilter {
                position_key: Some(normal.replay.nodes[1].position_key.clone()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(same_position.games, 2);
        assert_eq!(same_position.distinct_paths, 2);
        assert_eq!(query_unknown_label_path(), 1);
    }
    fn query_unknown_label_path() -> usize {
        chess::opening_name_path("Uncatalogued Header: Artificial Branch, Claimed Variation").len()
    }
    #[test]
    fn tree_positions_merge_transpositions_paths_do_not() {
        let a = game("a", "1. Nf3 Nf6 2. g3 g6 3. Bg2", "1-0");
        let b = game("b", "1. g3 g6 2. Nf3 Nf6 3. Bg2", "0-1");
        let key = a.replay.nodes[4].position_key.clone();
        let f = StatisticsFilter {
            position_key: Some(key),
            ..Default::default()
        };
        let t = tree(&[a.clone(), b.clone()], &f).unwrap();
        assert_eq!(t.games, 2);
        assert_eq!(t.edges[0].visits, 2);
        let f = StatisticsFilter {
            line: Some(vec![
                "g1f3".into(),
                "g8f6".into(),
                "g2g3".into(),
                "g7g6".into(),
            ]),
            ..Default::default()
        };
        assert_eq!(tree(&[a, b], &f).unwrap().games, 1);
    }
    #[test]
    fn duplicate_games_do_not_inflate_results() {
        let g = game("a", "1. e4 e5", "1/2-1/2");
        let r = report(&[g.clone(), g], &[], &StatisticsFilter::default());
        assert_eq!(r.total_games, 1);
        assert_eq!(
            r.metrics.iter().find(|m| m.id == "G03").unwrap().value,
            Some(50.)
        );
    }
}

#[cfg(test)]
mod comparison_tests {
    use super::*;
    fn fixture(id: &str, t: i64, speed: &str, result: &str) -> StoredGame {
        StoredGame {
            id: id.into(),
            source: "lichess".into(),
            account_id: "me".into(),
            own_color: "white".into(),
            result: result.into(),
            speed: speed.into(),
            started_at_ms: Some(t),
            eligibility: "eligible".into(),
            replay: chess::replay_pgn(&format!("1. e4 e5 {result}")).unwrap(),
            ..Default::default()
        }
    }
    #[test]
    fn different_controls_are_not_compared_as_progress() {
        let games = [
            fixture("old", 1, "bullet", "0-1"),
            fixture("new", 11, "rapid", "1-0"),
        ];
        let f = StatisticsFilter {
            since_ms: Some(10),
            until_ms: Some(20),
            comparison_since_ms: Some(0),
            comparison_until_ms: Some(10),
            ..Default::default()
        };
        let r = report(&games, &[], &f);
        assert_eq!(
            r.comparison
                .iter()
                .find(|m| m.id == "delta_G03")
                .unwrap()
                .value,
            None
        );
        assert_eq!(
            r.comparison
                .iter()
                .find(|m| m.id == "P04_comparison")
                .unwrap()
                .value,
            Some(0.)
        );
    }
    #[test]
    fn shared_conditions_compare_with_the_same_weights() {
        let games = [
            fixture("old", 1, "rapid", "0-1"),
            fixture("new", 11, "rapid", "1-0"),
        ];
        let f = StatisticsFilter {
            since_ms: Some(10),
            until_ms: Some(20),
            comparison_since_ms: Some(0),
            comparison_until_ms: Some(10),
            ..Default::default()
        };
        let r = report(&games, &[], &f);
        assert_eq!(
            r.comparison
                .iter()
                .find(|m| m.id == "delta_G03")
                .unwrap()
                .value,
            Some(100.)
        );
        assert_eq!(
            r.comparison
                .iter()
                .find(|m| m.id == "P04_comparison")
                .unwrap()
                .value,
            Some(100.)
        );
    }
    #[test]
    fn incompatible_engine_profiles_never_mix() {
        let g = fixture("game", 1, "rapid", "1-0");
        let mk = |profile: &str, time: i64, loss: u32| {
            let e = |q| PositionEvaluation {
                complete: true,
                wdl: Some([q, 0, 1000 - q]),
                ..Default::default()
            };
            EngineRun {
                id: profile.into(),
                game_id: g.id.clone(),
                profile_id: profile.into(),
                completed_at_ms: time,
                confirmed_moves: vec![MoveComparison {
                    ply: 1,
                    best: e(500),
                    played: e(500 - loss),
                    complete: true,
                    method: "same_root_searchmoves".into(),
                }],
                ..Default::default()
            }
        };
        let runs = [mk("old", 1, 100), mk("new", 2, 200)];
        let r = report(
            std::slice::from_ref(&g),
            &runs,
            &StatisticsFilter::default(),
        );
        assert_eq!(r.selected_profile_id.as_deref(), Some("new"));
        assert!(
            (r.metrics
                .iter()
                .find(|m| m.id == "Q02")
                .unwrap()
                .value
                .unwrap()
                - 20.)
                .abs()
                < 1e-9
        );
        let r = report(
            &[g],
            &runs,
            &StatisticsFilter {
                profile_id: Some("old".into()),
                ..Default::default()
            },
        );
        assert!(
            (r.metrics
                .iter()
                .find(|m| m.id == "Q02")
                .unwrap()
                .value
                .unwrap()
                - 10.)
                .abs()
                < 1e-9
        );
    }
}

/// Fixed-stratum whole-game bootstrap. Every game's decisions and episode events
/// travel together; moves are never treated as independent observations.
pub(crate) fn bootstrap_stratified_difference(
    id: &str,
    common: &[(&Vec<&StoredGame>, &Vec<&StoredGame>)],
    current: &HashMap<String, GameMetrics>,
    previous: &HashMap<String, GameMetrics>,
    phase: Option<&str>,
) -> Option<(f64, f64)> {
    let valid: Vec<_> = common
        .iter()
        .filter(|(a, b)| {
            cohort_metric(id, a, current, phase).0.is_some()
                && cohort_metric(id, b, previous, phase).0.is_some()
        })
        .collect();
    if valid.is_empty()
        || valid.iter().map(|(a, _)| a.len()).sum::<usize>() < 2
        || valid.iter().map(|(_, b)| b.len()).sum::<usize>() < 2
    {
        return None;
    }
    let mut seed = 0x12d34791b7ac5319u64;
    let mut samples = Vec::with_capacity(400);
    let mut random_index = |n: usize| {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed % n as u64) as usize
    };
    for _ in 0..400 {
        let mut delta = 0.;
        let mut weights = 0.;
        let mut complete = true;
        for (a, b) in &valid {
            let aa: Vec<_> = (0..a.len()).map(|_| a[random_index(a.len())]).collect();
            let bb: Vec<_> = (0..b.len()).map(|_| b[random_index(b.len())]).collect();
            let av = cohort_metric(id, &aa, current, phase).0;
            let bv = cohort_metric(id, &bb, previous, phase).0;
            if let Some((av, bv)) = av.zip(bv) {
                let w = (a.len() + b.len()) as f64;
                delta += w * (av - bv);
                weights += w;
            } else {
                complete = false;
                break;
            }
        }
        if complete && weights > 0. {
            samples.push(delta / weights);
        }
    }
    if samples.len() < 360 {
        return None;
    }
    Some((
        metrics::quantile(&samples, 0.025)?,
        metrics::quantile(&samples, 0.975)?,
    ))
}

#[cfg(test)]
mod ending_scope_fixture {
    use super::*;
    #[test]
    fn endgame_quality_excludes_a_large_middlegame_error() {
        let replay = chess::replay_pgn(
            "[FEN \"r5k1/8/8/8/8/8/6N1/RNB2BKR w - - 0 1\"]\n\n1. Ra7 Rxa7 2. Bd2 Ra8 0-1",
        )
        .unwrap();
        assert_eq!(replay.middle_ply, Some(0));
        assert_eq!(replay.end_ply, Some(2));
        let g = StoredGame {
            id: "game".into(),
            source: "lichess".into(),
            own_color: "white".into(),
            result: "0-1".into(),
            eligibility: "eligible".into(),
            replay,
            ..Default::default()
        };
        let e = |q| PositionEvaluation {
            complete: true,
            wdl: Some([q, 0, 1000 - q]),
            ..Default::default()
        };
        let r = EngineRun {
            game_id: g.id.clone(),
            profile_id: "p".into(),
            confirmed_moves: vec![
                MoveComparison {
                    ply: 1,
                    best: e(500),
                    played: e(100),
                    complete: true,
                    method: "same_root_searchmoves".into(),
                },
                MoveComparison {
                    ply: 3,
                    best: e(500),
                    played: e(450),
                    complete: true,
                    method: "same_root_searchmoves".into(),
                },
            ],
            ..Default::default()
        };
        let report = report(&[g], &[r], &StatisticsFilter::default());
        assert!(
            (report
                .metrics
                .iter()
                .find(|m| m.id == "Q02")
                .unwrap()
                .value
                .unwrap()
                - 22.5)
                .abs()
                < 1e-9
        );
        let ending = &report.endings[0];
        assert_eq!(ending.analysed, 1);
        assert!((ending.mean_loss.unwrap() - 5.).abs() < 1e-9);
    }
    #[test]
    fn independent_difference_bootstraps_games_and_overlaps_are_explicit() {
        let make = |id: &str, t, result: &str| StoredGame {
            id: id.into(),
            source: "lichess".into(),
            speed: "rapid".into(),
            own_color: "white".into(),
            started_at_ms: Some(t),
            result: result.into(),
            eligibility: "eligible".into(),
            replay: chess::replay_pgn(&format!("1. e4 e5 {result}")).unwrap(),
            ..Default::default()
        };
        let games = [
            make("a", 1, "0-1"),
            make("b", 2, "1/2-1/2"),
            make("c", 11, "1-0"),
            make("d", 12, "1/2-1/2"),
        ];
        let mut f = StatisticsFilter {
            since_ms: Some(10),
            until_ms: Some(20),
            comparison_since_ms: Some(0),
            comparison_until_ms: Some(10),
            ..Default::default()
        };
        let r = report(&games, &[], &f);
        let d = r.comparison.iter().find(|m| m.id == "delta_G03").unwrap();
        assert!(d.low.is_some() && d.high.is_some());
        assert!(d.low.unwrap() <= d.value.unwrap() && d.value.unwrap() <= d.high.unwrap());
        f.comparison_until_ms = Some(15);
        let r = report(&games, &[], &f);
        assert_eq!(
            r.comparison
                .iter()
                .find(|m| m.id == "delta_G03")
                .unwrap()
                .status,
            "no_independent_comparison"
        );
    }
}

fn opening_problems(
    games: &[&StoredGame],
    quality: &HashMap<String, GameMetrics>,
) -> Vec<OpeningProblem> {
    let mut groups: BTreeMap<(String, String), Vec<(&StoredGame, &DecisionEvent)>> =
        BTreeMap::new();
    for g in games {
        for event in quality[&g.id]
            .decisions
            .iter()
            .filter(|e| e.phase == "opening" && e.loss_q.is_some_and(|l| l >= 10.))
        {
            let root = &g.replay.nodes[event.ply.saturating_sub(1) as usize];
            let played = &g.replay.nodes[event.ply as usize];
            if let Some(uci) = &played.uci {
                groups
                    .entry((root.position_key.clone(), uci.clone()))
                    .or_default()
                    .push((g, event));
            }
        }
    }
    let mut problems: Vec<_> = groups
        .into_iter()
        .map(|((position_key, played_uci), mut events)| {
            events.sort_by_key(|(g, e)| (game_time(g), g.id.clone(), e.ply));
            let (g, first) = events[0];
            let spent: Vec<_> = events.iter().filter_map(|(_, e)| e.spent_seconds).collect();
            let losses: Vec<_> = events.iter().filter_map(|(_, e)| e.loss_q).collect();
            let mut game_ids: Vec<_> = events
                .iter()
                .map(|(g, _)| g.id.clone())
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            game_ids.sort();
            OpeningProblem {
                median_loss: metrics::quantile(&losses, 0.5),
                cases: vec![],
                comparison: vec![],
                position_key,
                played_uci,
                san: first.san.clone(),
                occurrences: events.len(),
                games: game_ids.len(),
                mean_loss: metrics::mean(&losses).unwrap(),
                sum_loss: losses.iter().sum(),
                median_spent: metrics::quantile(&spent, 0.5),
                game_ids,
                first_game_id: g.id.clone(),
                first_ply: first.ply,
                opening: chess::opening_at(&g.replay, first.ply.saturating_sub(1)),
            }
        })
        .collect();
    problems.sort_by(|a, b| {
        b.sum_loss
            .total_cmp(&a.sum_loss)
            .then(b.occurrences.cmp(&a.occurrences))
            .then(a.position_key.cmp(&b.position_key))
    });
    problems
}

#[cfg(test)]
mod opening_problem_fixture {
    use super::*;
    #[test]
    fn transposed_opening_losses_share_a_group_and_clock_visits() {
        let game = |id: &str, moves: &str, spent: f64| {
            let mut r = chess::replay_pgn(&format!("{moves} 1-0")).unwrap();
            r.nodes[5].spent_seconds = Some(spent);
            StoredGame {
                id: id.into(),
                source: "lichess".into(),
                own_color: "white".into(),
                result: "1-0".into(),
                eligibility: "eligible".into(),
                replay: r,
                ..Default::default()
            }
        };
        let a = game("a", "1. Nf3 Nf6 2. g3 g6 3. Bg2", 2.);
        let b = game("b", "1. g3 g6 2. Nf3 Nf6 3. Bg2", 4.);
        let key = a.replay.nodes[4].position_key.clone();
        let eval = |q| PositionEvaluation {
            complete: true,
            wdl: Some([q, 0, 1000 - q]),
            ..Default::default()
        };
        let run = |g: &StoredGame, after| EngineRun {
            game_id: g.id.clone(),
            profile_id: "p".into(),
            confirmed_moves: vec![MoveComparison {
                ply: 5,
                best: eval(500),
                played: eval(after),
                complete: true,
                method: "same_root_searchmoves".into(),
            }],
            ..Default::default()
        };
        let runs = [run(&a, 300), run(&b, 200)];
        let games = [a, b];
        let r = report(&games, &runs, &StatisticsFilter::default());
        assert_eq!(r.problems.len(), 1);
        let p = &r.problems[0];
        assert_eq!((p.games, p.occurrences), (2, 2));
        assert!((p.sum_loss - 50.).abs() < 1e-9);
        assert!((p.mean_loss - 25.).abs() < 1e-9);
        assert_eq!(p.median_spent, Some(3.));
        assert_eq!(p.first_ply, 5);
        assert_eq!(p.played_uci, "f1g2");
        let t = tree(
            &games,
            &StatisticsFilter {
                position_key: Some(key),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(t.edges[0].spent_sample, 2);
        assert_eq!(t.edges[0].spent_median, Some(3.));
    }
}

fn profile_label(run: &EngineRun) -> String {
    let settings: serde_json::Value = serde_json::from_str(&run.settings_json).unwrap_or_default();
    let config = &settings[0];
    let (label, nodes, time) = match run.profile.as_str() {
        "quick" => ("Быстрый", 250_000, 1000),
        "deep" => ("Углублённый", 20_000_000, 30000),
        _ => ("Стандартный", 2_000_000, 5000),
    };
    let nodes = config["custom_nodes"].as_u64().unwrap_or(nodes);
    let time = config["custom_movetime_ms"].as_u64().unwrap_or(time);
    let threads = config["threads"].as_u64().unwrap_or(1);
    let hash = config["hash_mb"].as_u64().unwrap_or(128);
    format!(
        "{} · {label} · {nodes} узлов/{time} мс · {threads} пот. · {hash} MiB",
        run.engine_version
    )
}

/// Append independent pool observations relative to the immutable initial account
/// snapshot. A missing post-rating or an ambiguous timestamp is never replaced by
/// the pre-rating, zero, or the incidental order of games in the result vector.
pub fn append_rating_baselines(report: &mut StatisticsReport, baselines: &[(String, i64, i64)]) {
    report
        .metrics
        .retain(|m| m.id != "G15_delta" && !m.id.starts_with("G15_delta_"));
    if baselines.is_empty() {
        let mut metric = value(
            "G15_delta",
            "Изменение рейтинга к T0",
            "пунктов",
            None,
            0,
            0,
            "Начальный подтверждённый рейтинговый снимок T0 отсутствует",
            vec![],
        );
        metric.method = "lichess_standard_post_rating_vs_initial_snapshot_v1".into();
        metric.status = "no_data".into();
        report.metrics.push(metric);
        return;
    }
    let mut pools: BTreeMap<&str, Vec<(i64, i64)>> = BTreeMap::new();
    for (pool, rating, timestamp) in baselines {
        let entries = pools.entry(pool).or_default();
        if !entries.contains(&(*rating, *timestamp)) {
            entries.push((*rating, *timestamp));
        }
    }
    for (pool, snapshots) in pools {
        let games: Vec<_> = report
            .games
            .iter()
            .filter(|g| g.source == "lichess" && g.speed == pool && g.rated == Some(true))
            .collect();
        let latest_time = games.iter().filter_map(|g| g.timestamp_ms).max();
        let latest: Vec<_> = games
            .iter()
            .filter(|g| latest_time.is_some() && g.timestamp_ms == latest_time)
            .copied()
            .collect();
        let mut game_ids: Vec<_> = latest.iter().map(|g| g.id.clone()).collect();
        game_ids.sort();
        game_ids.dedup();
        let baseline = snapshots.first().copied();
        let mut status = "ready";
        let mut delta = None;
        let standard = report.filter.variant.as_deref() == Some("standard");
        if snapshots.len() != 1 {
            status = "ambiguous_rating_baseline";
        } else if !standard || latest.is_empty() {
            status = "no_data";
        } else {
            let posts: Option<Vec<i32>> = latest
                .iter()
                .map(|g| g.own_rating?.checked_add(g.rating_diff?))
                .collect();
            match posts {
                None => status = "no_data",
                Some(posts) => {
                    let observed: HashSet<_> = posts.into_iter().collect();
                    if observed.len() != 1 {
                        status = "ambiguous_rating_time";
                    } else if let Some((initial, _)) = baseline {
                        delta = observed
                            .into_iter()
                            .next()
                            .map(|post| post as f64 - initial as f64);
                    }
                }
            }
        }
        let definition = match baseline {
            Some((rating, timestamp)) => format!(
                "Lichess standard/{pool}: последнее наблюдаемое post-rating=pre-rating+ratingDiff минус начальный снимок {rating} при T0={timestamp} ms. Только рейтинговые партии и фильтр standard. Порядок по доступному timestamp партии, не точное время завершения и не текущий рейтинг аккаунта. Разные значения при одинаковом последнем timestamp делают разницу недоступной."
            ),
            None => "Нет начального рейтингового снимка".into(),
        };
        let mut metric = value(
            &format!("G15_delta_{pool}"),
            &format!("Изменение рейтинга к T0 · {pool}"),
            "пунктов",
            delta,
            latest.len(),
            latest.len(),
            &definition,
            game_ids,
        );
        metric.method = "lichess_standard_post_rating_vs_initial_snapshot_v1".into();
        metric.status = status.into();
        report.metrics.push(metric);
    }
}

#[cfg(test)]
mod rating_baseline_tests {
    use super::*;
    fn observation(id: &str, pool: &str, time: i64, rating: i32, diff: Option<i32>) -> GameSummary {
        GameSummary {
            id: id.into(),
            source: "lichess".into(),
            speed: pool.into(),
            timestamp_ms: Some(time),
            rated: Some(true),
            own_rating: Some(rating),
            rating_diff: diff,
            ..Default::default()
        }
    }
    fn report_with(games: Vec<GameSummary>) -> StatisticsReport {
        StatisticsReport {
            filter: StatisticsFilter {
                variant: Some("standard".into()),
                ..Default::default()
            },
            games,
            ..Default::default()
        }
    }
    #[test]
    fn delta_uses_latest_post_rating_and_initial_pool_snapshot() {
        let mut r = report_with(vec![
            observation("early", "rapid", 11, 1500, Some(5)),
            observation("latest", "rapid", 12, 1505, Some(7)),
        ]);
        append_rating_baselines(&mut r, &[("rapid".into(), 1500, 10)]);
        let m = r
            .metrics
            .iter()
            .find(|m| m.id == "G15_delta_rapid")
            .unwrap();
        assert_eq!(m.value, Some(12.));
        assert_eq!(m.game_ids, vec!["latest"]);
    }
    #[test]
    fn pools_never_share_ratings_and_casual_games_are_excluded() {
        let mut casual = observation("casual", "rapid", 100, 2000, Some(50));
        casual.rated = Some(false);
        let mut r = report_with(vec![
            observation("rapid", "rapid", 12, 1500, Some(10)),
            observation("blitz", "blitz", 13, 1700, Some(-5)),
            casual,
        ]);
        append_rating_baselines(
            &mut r,
            &[("rapid".into(), 1500, 10), ("blitz".into(), 1700, 10)],
        );
        assert_eq!(
            r.metrics
                .iter()
                .find(|m| m.id == "G15_delta_rapid")
                .unwrap()
                .value,
            Some(10.)
        );
        assert_eq!(
            r.metrics
                .iter()
                .find(|m| m.id == "G15_delta_blitz")
                .unwrap()
                .value,
            Some(-5.)
        );
    }
    #[test]
    fn conflicting_latest_timestamps_and_missing_post_rating_stay_missing() {
        let mut r = report_with(vec![
            observation("a", "rapid", 12, 1500, Some(5)),
            observation("b", "rapid", 12, 1500, Some(-5)),
        ]);
        let baseline = [("rapid".into(), 1500, 10)];
        append_rating_baselines(&mut r, &baseline);
        let m = r
            .metrics
            .iter()
            .find(|m| m.id == "G15_delta_rapid")
            .unwrap();
        assert_eq!(m.value, None);
        assert_eq!(m.status, "ambiguous_rating_time");
        r.games[1].rating_diff = None;
        append_rating_baselines(&mut r, &baseline);
        assert_eq!(
            r.metrics
                .iter()
                .find(|m| m.id == "G15_delta_rapid")
                .unwrap()
                .value,
            None
        );
    }
    #[test]
    fn equal_ties_are_one_unambiguous_observation_and_empty_has_no_delta() {
        let mut r = report_with(vec![
            observation("a", "rapid", 12, 1500, Some(5)),
            observation("b", "rapid", 12, 1500, Some(5)),
        ]);
        append_rating_baselines(&mut r, &[("rapid".into(), 1500, 10)]);
        assert_eq!(
            r.metrics
                .iter()
                .find(|m| m.id == "G15_delta_rapid")
                .unwrap()
                .value,
            Some(5.)
        );
        r.games.clear();
        append_rating_baselines(&mut r, &[("rapid".into(), 1500, 10)]);
        assert_eq!(
            r.metrics
                .iter()
                .find(|m| m.id == "G15_delta_rapid")
                .unwrap()
                .value,
            None
        );
    }
    #[test]
    fn standard_filter_is_required_for_a_pool_delta() {
        let mut r = report_with(vec![observation("a", "rapid", 12, 1500, Some(5))]);
        r.filter.variant = None;
        append_rating_baselines(&mut r, &[("rapid".into(), 1500, 10)]);
        assert_eq!(
            r.metrics
                .iter()
                .find(|m| m.id == "G15_delta_rapid")
                .unwrap()
                .value,
            None
        );
    }
}

fn observed_interval(g: &StoredGame) -> (Option<f64>, &'static str) {
    let Some(start) = g.started_at_ms else {
        return (None, "missing");
    };
    if ["exact", "source_exact", "confirmed_end"].contains(&g.end_time_quality.as_str())
        && let Some(end) = g.finished_at_ms.filter(|end| *end >= start)
    {
        return (Some((end - start) as f64 / 1000.), "exact_end");
    }
    if let Some(last) = g.last_move_at_ms.filter(|last| *last >= start) {
        return (Some((last - start) as f64 / 1000.), "last_move_span");
    }
    (None, "missing")
}
fn relative_remaining(g: &StoredGame, n: &ReplayNode) -> Option<f64> {
    let initial = g.initial_seconds?;
    if !initial.is_finite()
        || initial <= 0.
        || g.source_metadata["players"][&g.own_color]["berserk"]
            .as_bool()
            .unwrap_or(false)
    {
        return None;
    }
    n.remaining_before.map(|r| 100. * r / initial)
}
fn additional_p0_metrics(
    games: &[&StoredGame],
    entries: &[(&StoredGame, &ReplayNode)],
    decisions: usize,
) -> Vec<MetricValue> {
    let ids: Vec<_> = games.iter().map(|g| g.id.clone()).collect();
    let n = games.len();
    let mut output = vec![];
    let rating_pools: HashSet<_> = games
        .iter()
        .map(|g| (&g.source, &g.speed, &g.variant))
        .collect();
    let ratings: Vec<_> = games
        .iter()
        .filter_map(|g| players(g).1.rating.map(|r| r as f64))
        .collect();
    for (id, label, number) in [
        (
            "G06_mean",
            "Рейтинг соперников · среднее",
            metrics::mean(&ratings),
        ),
        (
            "G06_p25",
            "Рейтинг соперников · P25",
            metrics::quantile(&ratings, 0.25),
        ),
        (
            "G06_p75",
            "Рейтинг соперников · P75",
            metrics::quantile(&ratings, 0.75),
        ),
        (
            "G06_iqr",
            "Рейтинг соперников · IQR",
            metrics::quantile(&ratings, 0.75)
                .zip(metrics::quantile(&ratings, 0.25))
                .map(|(a, b)| a - b),
        ),
    ] {
        output.push(value(id,label,"пунктов",if rating_pools.len()<=1{number}else{None},ratings.len(),n,"Предыгровые рейтинги одной source/perf/variant шкалы; missing и provisional учитываются отдельно",ids.clone()));
    }
    let missing = n.saturating_sub(ratings.len());
    output.push(value(
        "G06_missing",
        "Без рейтинга соперника",
        "%",
        (n > 0).then(|| 100. * missing as f64 / n as f64),
        n,
        n,
        "Доля партий без предыгрового рейтинга соперника; missing не заменяется нулём",
        games
            .iter()
            .filter(|g| players(g).1.rating.is_none())
            .map(|g| g.id.clone())
            .collect(),
    ));
    let provisional = games
        .iter()
        .filter(|g| players(g).1.rating.is_some() && players(g).1.provisional)
        .count();
    output.push(value("G06_provisional","Предварительный рейтинг соперника","%",(!ratings.is_empty()).then(||100.*provisional as f64/ratings.len()as f64),ratings.len(),n,"Доля provisional среди известных предыгровых рейтингов; точность рейтинга не оценивается по одной партии",games.iter().filter(|g|players(g).1.provisional).map(|g|g.id.clone()).collect()));
    let lengths: Vec<_> = games
        .iter()
        .map(|g| g.replay.nodes.len().saturating_sub(1) as f64)
        .collect();
    for (id, label, p) in [
        ("G10_p25", "Длина партии · P25", 0.25),
        ("G10_p75", "Длина партии · P75", 0.75),
    ] {
        output.push(value(
            id,
            label,
            "полуходов",
            metrics::quantile(&lengths, p),
            n,
            n,
            "Квантиль числа легально воспроизведённых полуходов",
            ids.clone(),
        ));
    }
    output.push(value(
        "G10_iqr",
        "Длина партии · IQR",
        "полуходов",
        metrics::quantile(&lengths, 0.75)
            .zip(metrics::quantile(&lengths, 0.25))
            .map(|(a, b)| a - b),
        n,
        n,
        "P75−P25 длины легальной главной линии",
        ids.clone(),
    ));
    for (quality, id, label) in [
        (
            "last_move_span",
            "G10_last_move_span",
            "От начала до последнего хода · медиана",
        ),
        (
            "exact_end",
            "G10_exact_duration",
            "Точное время партии · медиана",
        ),
    ] {
        let values: Vec<_> = games
            .iter()
            .filter_map(|g| {
                let (v, q) = observed_interval(g);
                (q == quality).then_some(v).flatten()
            })
            .collect();
        output.push(value(id,label,"с",metrics::quantile(&values,0.5),values.len(),n,if quality=="last_move_span"{"lastMoveAt−createdAt; наблюдаемый интервал, не время завершения и не точная длительность партии"}else{"endedAt−createdAt только с явным exact end evidence; lastMoveAt не подставляется"},games.iter().filter(|g|observed_interval(g).1==quality).map(|g|g.id.clone()).collect()));
    }
    let spent: Vec<_> = entries
        .iter()
        .filter_map(|(_, n)| n.spent_seconds)
        .collect();
    for (id, label, number) in [
        ("T01_mean", "Время хода · среднее", metrics::mean(&spent)),
        (
            "T01_p25",
            "Время хода · P25",
            metrics::quantile(&spent, 0.25),
        ),
        (
            "T01_p75",
            "Время хода · P75",
            metrics::quantile(&spent, 0.75),
        ),
        (
            "T01_iqr",
            "Время хода · IQR",
            metrics::quantile(&spent, 0.75)
                .zip(metrics::quantile(&spent, 0.25))
                .map(|(a, b)| a - b),
        ),
        (
            "T01_p90",
            "Время хода · P90",
            metrics::quantile(&spent, 0.9),
        ),
        (
            "T01_max",
            "Время хода · максимум",
            spent.iter().copied().reduce(f64::max),
        ),
    ] {
        output.push(value(id,label,"с",number,spent.len(),decisions,"Известное списанное часами время; первые elapsed, gaps, invalid и неизвестный increment исключены",ids.clone()));
    }
    let clock_games: HashSet<_> = entries
        .iter()
        .filter(|(_, n)| n.remaining_before.is_some())
        .map(|(g, _)| g.id.clone())
        .collect();
    for threshold in [10, 30, 60] {
        let low: HashSet<_> = entries
            .iter()
            .filter(|(_, n)| n.remaining_before.is_some_and(|r| r < threshold as f64))
            .map(|(g, _)| g.id.clone())
            .collect();
        let count = low.len();
        output.push(value(&format!("T04_games_{threshold}"),&format!("Партии с остатком <{threshold} с"),"%",(!clock_games.is_empty()).then(||100.*count as f64/clock_games.len()as f64),clock_games.len(),n,&format!("{count}/{} уникальных партий с известными remaining_before; один game в знаменателе один раз",clock_games.len()),low.into_iter().collect()));
    }
    let relative_games: HashSet<_> = entries
        .iter()
        .filter(|(g, n)| relative_remaining(g, n).is_some())
        .map(|(g, _)| g.id.clone())
        .collect();
    for threshold in [5, 10] {
        let low: HashSet<_> = entries
            .iter()
            .filter(|(g, n)| relative_remaining(g, n).is_some_and(|r| r < threshold as f64))
            .map(|(g, _)| g.id.clone())
            .collect();
        let count = low.len();
        output.push(value(&format!("T04_relative_games_{threshold}"),&format!("Партии с остатком <{threshold}% начального"),"%",(!relative_games.is_empty()).then(||100.*count as f64/relative_games.len()as f64),relative_games.len(),n,&format!("{count}/{} уникальных игр с известным relative_remaining; неизвестный effective initial исключён",relative_games.len()),low.into_iter().collect()));
    }
    for (phase, label) in [
        ("opening", "Дебют"),
        ("middlegame", "Миттельшпиль"),
        ("endgame", "Эндшпиль"),
    ] {
        let reached: Vec<_> = games
            .iter()
            .filter(|g| g.replay.nodes.iter().any(|node| node.phase == phase))
            .copied()
            .collect();
        output.push(value(&format!("S01_{phase}"),&format!("Достигнута фаза · {label}"),"%",(n>0).then(||100.*reached.len()as f64/n as f64),n,n,"Уникальные игры с наблюдаемой фазой / легально воспроизведённые игры; versioned Divider и отдельный initial-FEN material classifier",reached.iter().map(|g|g.id.clone()).collect()));
        let shares: Vec<_> = games
            .iter()
            .filter_map(|g| {
                let all: Vec<_> = g
                    .replay
                    .nodes
                    .iter()
                    .filter(|node| node.actor.as_deref() == Some(&g.own_color))
                    .filter_map(|node| node.spent_seconds)
                    .collect();
                let total: f64 = all.iter().sum();
                let in_phase: Vec<_> = g
                    .replay
                    .nodes
                    .iter()
                    .filter(|node| {
                        node.actor.as_deref() == Some(&g.own_color) && node.phase == phase
                    })
                    .filter_map(|node| node.spent_seconds)
                    .collect();
                (total > 0. && !in_phase.is_empty())
                    .then(|| 100. * in_phase.iter().sum::<f64>() / total)
            })
            .collect();
        output.push(value(&format!("T02_{phase}_share"),&format!("Доля известного времени · {label}"),"%",metrics::mean(&shares),shares.len(),reached.len(),"Сначала sum(known spent в фазе)/sum(known own spent всей игры), затем среднее игровых долей; missing фазы и нулевой общий расход исключены",reached.iter().map(|g|g.id.clone()).collect()));
    }
    output
}

#[cfg(test)]
mod p0_completeness_fixtures {
    use super::*;
    fn game(id: &str) -> StoredGame {
        StoredGame {
            id: id.into(),
            source: "lichess".into(),
            own_color: "white".into(),
            speed: "rapid".into(),
            variant: "standard".into(),
            result: "1-0".into(),
            eligibility: "eligible".into(),
            initial_seconds: Some(100.),
            replay: chess::replay_pgn("1. e4 e5 2. Nf3 Nc6 1-0").unwrap(),
            ..Default::default()
        }
    }
    fn metric<'a>(r: &'a StatisticsReport, id: &str) -> &'a MetricValue {
        r.metrics.iter().find(|m| m.id == id).unwrap()
    }
    #[test]
    fn p0_quantiles_game_denominators_shares_and_spent_quality_are_distinct() {
        let mut a = game("a");
        let mut b = game("b");
        let c = game("c");
        a.black.rating = Some(1000);
        a.black.provisional = true;
        b.black.rating = Some(2000);
        a.started_at_ms = Some(1000);
        a.last_move_at_ms = Some(31000);
        b.started_at_ms = Some(1000);
        b.last_move_at_ms = Some(21000);
        b.finished_at_ms = Some(61000);
        b.end_time_quality = "exact".into();
        for (g, spent, remaining) in [(&mut a, [1., 3.], [8., 4.]), (&mut b, [9., 1.], [100., 9.])]
        {
            g.replay.nodes[1].phase = "opening".into();
            g.replay.nodes[3].phase = "endgame".into();
            for (j, ply) in [1, 3].into_iter().enumerate() {
                g.replay.nodes[ply].spent_seconds = Some(spent[j]);
                g.replay.nodes[ply].remaining_before = Some(remaining[j]);
            }
        }
        let eval = |q| PositionEvaluation {
            complete: true,
            wdl: Some([q, 0, 1000 - q]),
            ..Default::default()
        };
        let run = |g: &StoredGame, q1, q3| EngineRun {
            game_id: g.id.clone(),
            profile_id: "p".into(),
            confirmed_moves: [(1, q1), (3, q3)]
                .into_iter()
                .map(|(ply, q)| MoveComparison {
                    ply,
                    best: eval(500),
                    played: eval(q),
                    complete: true,
                    method: "same_root_searchmoves".into(),
                })
                .collect(),
            ..Default::default()
        };
        let runs = [run(&a, 400, 200), run(&b, 300, 500)];
        let r = report(&[a, b, c], &runs, &StatisticsFilter::default());
        assert_eq!(metric(&r, "G06_mean").value, Some(1500.));
        assert_eq!(metric(&r, "G06_iqr").value, Some(500.));
        assert_eq!(metric(&r, "G06_provisional").value, Some(50.));
        assert_eq!(metric(&r, "G10_last_move_span").value, Some(30.));
        assert_eq!(metric(&r, "G10_exact_duration").value, Some(60.));
        assert_eq!(
            r.games
                .iter()
                .find(|g| g.id == "a")
                .unwrap()
                .interval_quality,
            "last_move_span"
        );
        assert_eq!(metric(&r, "T01_mean").value, Some(3.5));
        assert_eq!(metric(&r, "T01_iqr").value, Some(3.5));
        assert_eq!(metric(&r, "T01_max").value, Some(9.));
        assert_eq!(metric(&r, "T04_10").value, Some(75.));
        assert_eq!(metric(&r, "T04_games_10").value, Some(100.));
        assert_eq!(metric(&r, "T04_games_10").sample, 2);
        assert_eq!(metric(&r, "T04_relative_games_5").value, Some(50.));
        assert_eq!(metric(&r, "T02_endgame_share").value, Some(42.5));
        assert!((metric(&r, "S01_endgame").value.unwrap() - 200. / 3.).abs() < 1e-9);
        let fast = r
            .time_spent
            .iter()
            .find(|row| row.key == "spent_1-3")
            .unwrap();
        let slower = r
            .time_spent
            .iter()
            .find(|row| row.key == "spent_3-10")
            .unwrap();
        assert_eq!(fast.spent_sample, 2);
        assert!((fast.mean_loss.unwrap() - 5.).abs() < 1e-9);
        assert!((slower.mean_loss.unwrap() - 25.).abs() < 1e-9);
    }
    #[test]
    fn tree_distinguishes_paths_and_unplayed_legal_moves() {
        let mut a = game("a");
        a.replay = chess::replay_pgn("1. Nf3 Nf6 2. g3 g6 3. Bg2 1-0").unwrap();
        let mut b = game("b");
        b.replay = chess::replay_pgn("1. g3 g6 2. Nf3 Nf6 3. Bg2 1-0").unwrap();
        let key = a.replay.nodes[4].position_key.clone();
        let initial = tree(&[a.clone(), b.clone()], &StatisticsFilter::default()).unwrap();
        assert_eq!(initial.distinct_paths, 1);
        assert_eq!(initial.edges.len() + initial.unplayed.len(), 20);
        assert!(initial.unplayed.iter().any(|m| m.uci == "e2e4"));
        assert!(!initial.unplayed.iter().any(|m| m.uci == "g1f3"));
        let t = tree(
            &[a, b],
            &StatisticsFilter {
                position_key: Some(key),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(t.distinct_paths, 2);
    }
    #[test]
    fn uncertain_checkbox_never_includes_excluded_or_quarantined_games() {
        let mut games = vec![];
        for status in [
            "eligible",
            "boundary_uncertain",
            "excluded",
            "unknown",
            "quarantined",
        ] {
            let mut g = game(status);
            g.eligibility = status.into();
            games.push(g);
        }
        assert_eq!(
            report(&games, &[], &StatisticsFilter::default()).total_games,
            1
        );
        assert_eq!(
            report(
                &games,
                &[],
                &StatisticsFilter {
                    include_uncertain: Some(true),
                    ..Default::default()
                }
            )
            .total_games,
            2
        );
    }
}

const MOSCOW_OFFSET_MS: i64 = 10_800_000;
const DAY_MS: i64 = 86_400_000;
// Gregorian civil-date conversion using 400-year eras; constant time even for
// large timestamps. Only day-of-month is required to find a month boundary.
fn day_of_month(day: i64) -> i64 {
    let shifted = day + 719_468;
    let era = shifted.div_euclid(146_097);
    let doe = shifted - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    doy - (153 * mp + 2) / 5 + 1
}
fn calendar_bucket(timestamp: i64, granularity: &str) -> i64 {
    let local_day = timestamp
        .saturating_add(MOSCOW_OFFSET_MS)
        .div_euclid(DAY_MS);
    let first_day = match granularity {
        "day" => local_day,
        "month" => local_day - day_of_month(local_day) + 1,
        _ => (local_day + 3).div_euclid(7) * 7 - 3,
    };
    first_day
        .saturating_mul(DAY_MS)
        .saturating_sub(MOSCOW_OFFSET_MS)
}
fn observed_trend_rating(games: &[&StoredGame]) -> Option<f64> {
    let last = games
        .iter()
        .filter(|g| {
            g.rated == Some(true)
                && players(g).0.rating.is_some()
                && players(g).0.rating_diff.is_some()
        })
        .filter_map(|g| game_time(g))
        .max()?;
    let values: HashSet<_> = games
        .iter()
        .filter(|g| g.rated == Some(true) && game_time(g) == Some(last))
        .filter_map(|g| {
            let p = players(g).0;
            p.rating?.checked_add(p.rating_diff?)
        })
        .collect();
    (values.len() == 1).then(|| *values.iter().next().unwrap() as f64)
}
fn trend_point(
    timestamp_ms: i64,
    stratum: String,
    games: &[&StoredGame],
    quality: &HashMap<String, GameMetrics>,
    phase: Option<&str>,
) -> TrendPoint {
    let r = result_row(String::new(), String::new(), games, quality);
    let spent: Vec<_> = games
        .iter()
        .flat_map(|g| {
            g.replay
                .nodes
                .iter()
                .filter(|n| {
                    n.actor.as_deref() == Some(&g.own_color) && phase.is_none_or(|p| p == n.phase)
                })
                .filter_map(|n| n.spent_seconds)
        })
        .collect();
    TrendPoint {
        timestamp_ms,
        stratum,
        games: games.len(),
        score: r.score,
        rating: observed_trend_rating(games),
        mean_loss: r.mean_loss,
        accuracy: r.mean_accuracy,
        median_spent: metrics::quantile(&spent, 0.5),
        analysed: r.analysed,
        game_ids: r.game_ids,
    }
}
fn build_trend(
    games: &[&StoredGame],
    quality: &HashMap<String, GameMetrics>,
    filter: &StatisticsFilter,
) -> Vec<TrendPoint> {
    let granularity = filter.trend_granularity.as_deref().unwrap_or("week");
    let key = |g: &StoredGame| format!("{}:{}:{}:{}", g.source, g.speed, g.variant, control(g));
    let mut result = vec![];
    if granularity == "rolling50" {
        let mut groups: BTreeMap<String, Vec<&StoredGame>> = BTreeMap::new();
        for g in games.iter().filter(|g| game_time(g).is_some()) {
            groups.entry(key(g)).or_default().push(g);
        }
        for (stratum, mut group) in groups {
            group.sort_by_key(|g| (game_time(g), g.id.clone()));
            for window in group.windows(50) {
                let timestamp = game_time(window[49]).unwrap();
                result.push(trend_point(
                    timestamp,
                    stratum.clone(),
                    window,
                    quality,
                    filter.phase.as_deref(),
                ));
            }
        }
    } else {
        let mut groups: BTreeMap<(i64, String), Vec<&StoredGame>> = BTreeMap::new();
        for g in games {
            if let Some(t) = game_time(g) {
                groups
                    .entry((calendar_bucket(t, granularity), key(g)))
                    .or_default()
                    .push(g);
            }
        }
        for ((timestamp, stratum), group) in groups {
            result.push(trend_point(
                timestamp,
                stratum,
                &group,
                quality,
                filter.phase.as_deref(),
            ));
        }
    }
    result.sort_by(|a, b| {
        a.timestamp_ms
            .cmp(&b.timestamp_ms)
            .then(a.stratum.cmp(&b.stratum))
    });
    result
}

/// Activity uses the days touched by [max(T0,since), min(now,until)) in Moscow.
/// Closed-app/source gaps do not prove inactivity; the metric counts only saved observations.
pub fn append_observation(report: &mut StatisticsReport, t0_ms: i64, now_ms: i64) {
    report.metrics.retain(|m| {
        ![
            "G11_observation_days",
            "G11_active_share",
            "G11_games_per_active_day",
        ]
        .contains(&m.id.as_str())
    });
    let start = t0_ms.max(report.filter.since_ms.unwrap_or(t0_ms));
    let end = now_ms.min(report.filter.until_ms.unwrap_or(now_ms));
    let valid = t0_ms <= now_ms && start < end;
    let observation_days = if valid {
        (end.saturating_sub(1)
            .saturating_add(MOSCOW_OFFSET_MS)
            .div_euclid(DAY_MS)
            - start.saturating_add(MOSCOW_OFFSET_MS).div_euclid(DAY_MS)
            + 1)
        .max(0) as usize
    } else {
        0
    };
    let games: Vec<_> = report
        .games
        .iter()
        .filter(|g| valid && g.timestamp_ms.is_some_and(|t| t >= start && t < end))
        .collect();
    let active: HashSet<_> = games
        .iter()
        .filter_map(|g| {
            g.timestamp_ms
                .map(|t| t.saturating_add(MOSCOW_OFFSET_MS).div_euclid(DAY_MS))
        })
        .collect();
    let ids: Vec<_> = games.iter().map(|g| g.id.clone()).collect();
    if let Some(existing) = report.metrics.iter_mut().find(|m| m.id == "G11") {
        existing.value = valid.then_some(active.len() as f64);
        existing.sample = games.len();
        existing.eligible = observation_days;
        existing.game_ids = ids.clone();
        existing.status = if valid { "ready" } else { "no_data" }.into();
        existing.definition="Календарные дни Europe/Moscow с сохранёнными партиями внутри T0 и фильтра; разрыв синхронизации не доказывает отсутствие игры".into();
    }
    let definition = format!(
        "Europe/Moscow, интервал [{start},{end}) ms, ограничен T0, now и фильтром дат. Активность по доступным timestamp сохранённых партий; источник и разрывы синхронизации могут давать неполное покрытие. Будущий T0 и пустой интервал не дают численного вывода."
    );
    for (id, label, unit, number, sample) in [
        (
            "G11_observation_days",
            "Дни наблюдения в периоде",
            "дней",
            valid.then_some(observation_days as f64),
            observation_days,
        ),
        (
            "G11_active_share",
            "Доля дней с партиями",
            "%",
            (valid && observation_days > 0)
                .then(|| 100. * active.len() as f64 / observation_days as f64),
            observation_days,
        ),
        (
            "G11_games_per_active_day",
            "Партий на активный день",
            "партий/день",
            (valid && !active.is_empty()).then(|| games.len() as f64 / active.len() as f64),
            active.len(),
        ),
    ] {
        let mut metric = value(
            id,
            label,
            unit,
            number,
            sample,
            observation_days,
            &definition,
            ids.clone(),
        );
        metric.method = "moscow_calendar_activity_observation_v1".into();
        if !valid {
            metric.status = "no_data".into();
        }
        report.metrics.push(metric);
    }
}

#[cfg(test)]
mod trend_and_observation_fixtures {
    use super::*;
    fn game(id: usize, time: i64, speed: &str) -> StoredGame {
        StoredGame {
            id: format!("g{id:03}"),
            source: "lichess".into(),
            speed: speed.into(),
            variant: "standard".into(),
            own_color: "white".into(),
            result: if id == 0 { "0-1" } else { "1-0" }.into(),
            rated: Some(true),
            initial_seconds: Some(60.),
            increment_seconds: Some(0.),
            eligibility: "eligible".into(),
            started_at_ms: Some(time),
            white: Participant {
                rating: Some(1000 + id as i32),
                rating_diff: Some(1),
                ..Default::default()
            },
            replay: chess::replay_pgn("1. e4 e5 *").unwrap(),
            ..Default::default()
        }
    }
    #[test]
    fn month_and_day_use_moscow_boundaries_and_week_starts_monday() {
        let games = [
            game(0, 1_769_893_199_000, "rapid"),
            game(1, 1_769_893_200_000, "rapid"),
        ];
        let r = report(
            &games,
            &[],
            &StatisticsFilter {
                trend_granularity: Some("month".into()),
                ..Default::default()
            },
        );
        assert_eq!(r.trend.len(), 2);
        assert_eq!(r.trend[0].timestamp_ms, 1_767_214_800_000);
        assert_eq!(r.trend[1].timestamp_ms, 1_769_893_200_000);
        assert_eq!(r.trend[0].games, 1);
        assert_eq!(r.trend[1].game_ids, vec!["g001"]);
        let r = report(
            &games,
            &[],
            &StatisticsFilter {
                trend_granularity: Some("day".into()),
                ..Default::default()
            },
        );
        assert_eq!(r.trend.len(), 2);
        assert_eq!(
            calendar_bucket(1_791_752_400_000, "week"),
            1_791_752_400_000
        );
    }
    #[test]
    fn rolling_windows_are_exactly_fifty_and_timestamp_last_observation() {
        let games: Vec<_> = (0..51)
            .map(|i| game(i, 10_000 + i as i64, "rapid"))
            .collect();
        let r = report(
            &games,
            &[],
            &StatisticsFilter {
                trend_granularity: Some("rolling50".into()),
                ..Default::default()
            },
        );
        assert_eq!(r.trend.len(), 2);
        assert!(
            r.trend
                .iter()
                .all(|p| p.games == 50 && p.game_ids.len() == 50)
        );
        assert_eq!(r.trend[0].timestamp_ms, 10_049);
        assert_eq!(r.trend[1].timestamp_ms, 10_050);
        assert_eq!(r.trend[0].score, Some(98.));
        assert_eq!(r.trend[1].score, Some(100.));
        assert_eq!(r.trend[1].rating, Some(1051.));
        assert!(r.trend[0].game_ids.contains(&"g000".into()));
        assert!(!r.trend[1].game_ids.contains(&"g000".into()));
    }
    #[test]
    fn rolling_windows_never_combine_strata_to_reach_fifty() {
        let mut games: Vec<_> = (0..98)
            .map(|i| {
                game(
                    i,
                    10_000 + i as i64,
                    if i % 2 == 0 { "rapid" } else { "blitz" },
                )
            })
            .collect();
        let f = StatisticsFilter {
            trend_granularity: Some("rolling50".into()),
            ..Default::default()
        };
        assert!(report(&games, &[], &f).trend.is_empty());
        games.push(game(98, 10_098, "rapid"));
        games.push(game(99, 10_099, "blitz"));
        let r = report(&games, &[], &f);
        assert_eq!(r.trend.len(), 2);
        for point in r.trend {
            let expected_even = point.stratum.contains(":rapid:");
            assert!(
                point
                    .game_ids
                    .iter()
                    .all(|id| id[1..].parse::<usize>().unwrap() % 2 == usize::from(!expected_even))
            );
        }
    }
    #[test]
    fn activity_respects_half_open_dates_moscow_and_future_t0() {
        let base = 1_769_893_200_000;
        let mut r = report(
            &[
                game(0, base - 1, "rapid"),
                game(1, base, "rapid"),
                game(2, base + 1, "rapid"),
                game(3, base + DAY_MS, "rapid"),
            ],
            &[],
            &StatisticsFilter {
                since_ms: Some(base),
                until_ms: Some(base + DAY_MS),
                ..Default::default()
            },
        );
        append_observation(&mut r, base - DAY_MS, base + 2 * DAY_MS);
        let get =
            |r: &StatisticsReport, id: &str| r.metrics.iter().find(|m| m.id == id).unwrap().value;
        assert_eq!(get(&r, "G11_observation_days"), Some(1.));
        assert_eq!(get(&r, "G11_active_share"), Some(100.));
        assert_eq!(get(&r, "G11_games_per_active_day"), Some(2.));
        append_observation(&mut r, base + 3 * DAY_MS, base + 2 * DAY_MS);
        assert_eq!(get(&r, "G11_active_share"), None);
    }
}

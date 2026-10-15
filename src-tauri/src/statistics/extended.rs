//! Additional catalogue A/B metrics. Source evidence stays attached to every descriptive result.
use super::{
    metrics::{self, GameMetrics},
    query,
    types::*,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

pub const VERSION: &str = "statistics-extended-v1";
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoveEvidence {
    pub game_id: String,
    pub ply: u32,
    pub run_id: Option<String>,
    pub fen: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionRow {
    pub id: String,
    pub gap_minutes: u32,
    pub first_start_ms: i64,
    pub last_start_ms: i64,
    pub ended_at_ms: Option<i64>,
    pub span_seconds: Option<f64>,
    pub observed_start_span_seconds: f64,
    pub overlapping: bool,
    pub order_ambiguous: bool,
    pub games: usize,
    pub wins: usize,
    pub draws: usize,
    pub losses: usize,
    pub score: Option<f64>,
    pub game_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StreakRow {
    pub id: String,
    pub kind: String,
    pub length: usize,
    pub first_ms: i64,
    pub last_ms: i64,
    pub stratum: String,
    pub game_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextRow {
    pub id: String,
    pub metric_id: String,
    pub label: String,
    pub stratum: String,
    pub unit: String,
    pub value: Option<f64>,
    pub low: Option<f64>,
    pub high: Option<f64>,
    pub sample: usize,
    pub eligible: usize,
    pub mean_loss: Option<f64>,
    pub significant: usize,
    pub status: String,
    pub game_ids: Vec<String>,
    pub evidence: Vec<MoveEvidence>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StructureRow {
    pub id: String,
    pub metric_id: String,
    pub label: String,
    pub games: usize,
    pub visits: usize,
    pub decisions: usize,
    pub share: Option<f64>,
    pub game_ids: Vec<String>,
    pub evidence: Vec<MoveEvidence>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProgressRow {
    pub id: String,
    pub metric_id: String,
    pub label: String,
    pub key: String,
    pub unit: String,
    pub current: Option<f64>,
    pub previous: Option<f64>,
    pub delta: Option<f64>,
    pub current_sample: usize,
    pub previous_sample: usize,
    pub low: Option<f64>,
    pub high: Option<f64>,
    pub status: String,
    pub game_ids: Vec<String>,
    pub evidence: Vec<MoveEvidence>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CoverageRow {
    pub reason: String,
    pub games: usize,
    pub decisions: usize,
    pub game_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExtendedReport {
    pub sessions: Vec<SessionRow>,
    pub streaks: Vec<StreakRow>,
    pub time_contexts: Vec<ContextRow>,
    pub structures: Vec<StructureRow>,
    pub progress: Vec<ProgressRow>,
    pub coverage: Vec<CoverageRow>,
}
fn ids<'a>(games: impl IntoIterator<Item = &'a StoredGame>) -> Vec<String> {
    games
        .into_iter()
        .map(|g| g.id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}
#[allow(clippy::too_many_arguments)]
fn metric(
    id: &str,
    name: &str,
    unit: &str,
    value: Option<f64>,
    sample: usize,
    eligible: usize,
    definition: &str,
    game_ids: Vec<String>,
) -> MetricValue {
    MetricValue {
        id: id.into(),
        name: name.into(),
        unit: unit.into(),
        value,
        sample,
        eligible,
        definition: definition.into(),
        method: VERSION.into(),
        game_ids,
        status: if value.is_some() {
            "ready"
        } else if eligible == 0 {
            "no_sample"
        } else {
            "missing_input"
        }
        .into(),
        ..Default::default()
    }
}
fn own(g: &StoredGame) -> (&Participant, &Participant) {
    if g.own_color == "white" {
        (&g.white, &g.black)
    } else {
        (&g.black, &g.white)
    }
}
fn rating_pool(g: &StoredGame) -> String {
    format!("{} · {} · {}", g.source, g.speed, g.variant)
}
fn event_evidence(g: &StoredGame, n: &ReplayNode, run: Option<&EngineRun>) -> MoveEvidence {
    MoveEvidence {
        game_id: g.id.clone(),
        ply: n.ply,
        run_id: run.map(|r| r.id.clone()),
        fen: n.fen.clone(),
    }
}
fn interval_status(value: Option<f64>, interval: Option<(f64, f64)>) -> String {
    if value.is_none() {
        "missing_input"
    } else if interval.is_none() {
        "insufficient_interval_sample"
    } else if interval.is_some_and(|(a, b)| (a - b).abs() < 1e-12) {
        "descriptive_degenerate_interval"
    } else {
        "ready"
    }
    .into()
}
fn set_interval(m: &mut MetricValue, ci: Option<(f64, f64)>) {
    m.low = ci.map(|c| c.0);
    m.high = ci.map(|c| c.1);
    if m.value.is_some() {
        m.status = interval_status(m.value, ci);
    }
}
fn random_index(seed: &mut u64, n: usize) -> usize {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    (*seed % n as u64) as usize
}
fn bootstrap_difference(a: &[f64], b: &[f64]) -> Option<(f64, f64)> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let mut seed = 0x895d_325a_1539_c71b;
    let mut samples = Vec::with_capacity(400);
    for _ in 0..400 {
        let x = (0..a.len())
            .map(|_| a[random_index(&mut seed, a.len())])
            .sum::<f64>()
            / a.len() as f64;
        let y = (0..b.len())
            .map(|_| b[random_index(&mut seed, b.len())])
            .sum::<f64>()
            / b.len() as f64;
        samples.push(x - y);
    }
    Some((
        metrics::quantile(&samples, 0.025)?,
        metrics::quantile(&samples, 0.975)?,
    ))
}
fn ratio_bootstrap(rows: &BTreeMap<String, (f64, usize)>) -> Option<(f64, f64)> {
    if rows.len() < 2 {
        return None;
    }
    let rows: Vec<_> = rows.values().copied().collect();
    let mut seed = 0x7d29_5bc1_9a67_1f55;
    let mut samples = vec![];
    for _ in 0..400 {
        let (mut sum, mut n) = (0., 0);
        for _ in 0..rows.len() {
            let (s, k) = rows[random_index(&mut seed, rows.len())];
            sum += s;
            n += k;
        }
        if n > 0 {
            samples.push(sum / n as f64);
        }
    }
    Some((
        metrics::quantile(&samples, 0.025)?,
        metrics::quantile(&samples, 0.975)?,
    ))
}
/// Conditional logistic Elo, not an official rating or a Glicko estimate.
pub fn elo_performance(opponents: &[f64], scores: &[f64]) -> Option<f64> {
    if opponents.len() != scores.len() || opponents.is_empty() {
        return None;
    }
    let target: f64 = scores.iter().sum();
    if target <= 0. || target >= scores.len() as f64 {
        return None;
    }
    let (mut lo, mut hi) = (-8000., 12000.);
    for _ in 0..64 {
        let mid = (lo + hi) / 2.;
        let expected: f64 = opponents.iter().map(|r| elo_expected(mid, *r)).sum();
        if expected < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some((lo + hi) / 2.)
}
pub fn elo_expected(own: f64, opponent: f64) -> f64 {
    1. / (1. + 10f64.powf(((opponent - own) / 400.).clamp(-100., 100.)))
}
fn performance_ci(rows: &[(&StoredGame, f64, f64)]) -> Option<(f64, f64)> {
    if rows.len() < 5 {
        return None;
    }
    let mut seed = 0x984b_3cc1_224d_0001;
    let mut samples = vec![];
    for _ in 0..300 {
        let selected: Vec<_> = (0..rows.len())
            .map(|_| rows[random_index(&mut seed, rows.len())])
            .collect();
        if let Some(value) = elo_performance(
            &selected.iter().map(|r| r.1).collect::<Vec<_>>(),
            &selected.iter().map(|r| r.2).collect::<Vec<_>>(),
        ) {
            samples.push(value);
        }
    }
    if samples.len() < 270 {
        return None;
    }
    Some((
        metrics::quantile(&samples, 0.025)?,
        metrics::quantile(&samples, 0.975)?,
    ))
}
fn general_metrics(report: &mut StatisticsReport, games: &[&StoredGame]) {
    let mut pools: BTreeMap<String, Vec<&StoredGame>> = BTreeMap::new();
    for g in games {
        pools.entry(rating_pool(g)).or_default().push(g);
    }
    if pools.len() > 1 {
        for m in report.metrics.iter_mut().filter(|m| {
            matches!(
                m.id.as_str(),
                "G06" | "G06_mean" | "G06_p25" | "G06_p75" | "G06_iqr"
            )
        }) {
            m.value = None;
            m.low = None;
            m.high = None;
            m.status = "mixed_rating_scales".into();
            m.definition.push_str("; несколько platform/perf/variant: общий рейтинг не вычисляется; ниже отдельные шкалы");
        }
    }
    for (pool, group) in pools {
        let rated: Vec<_> = group
            .iter()
            .copied()
            .filter(|g| g.rated == Some(true))
            .collect();
        let opponents: Vec<_> = group
            .iter()
            .filter_map(|g| own(g).1.rating.filter(|r| *r > 0).map(|r| r as f64))
            .collect();
        for (suffix, label, value) in [
            ("mean", "среднее", metrics::mean(&opponents)),
            ("median", "медиана", metrics::quantile(&opponents, 0.5)),
            ("p25", "P25", metrics::quantile(&opponents, 0.25)),
            ("p75", "P75", metrics::quantile(&opponents, 0.75)),
            (
                "iqr",
                "IQR",
                metrics::quantile(&opponents, 0.75)
                    .zip(metrics::quantile(&opponents, 0.25))
                    .map(|(p75, p25)| p75 - p25),
            ),
        ] {
            report.metrics.push(metric(&format!("G06_pool_{pool}_{suffix}"),&format!("Рейтинг соперников · {pool} · {label}"),"пунктов",value,opponents.len(),group.len(),"Предыгровые рейтинги в одной platform/perf/variant; missing не ноль, provisional сохраняется отдельным контекстом",ids(group.iter().copied())));
        }
        let provisional = group
            .iter()
            .filter(|g| own(g).1.rating.is_some_and(|r| r > 0) && own(g).1.provisional)
            .count();
        for (suffix, label, numerator, denominator) in [
            (
                "missing",
                "без рейтинга",
                group.len() - opponents.len(),
                group.len(),
            ),
            (
                "provisional",
                "предварительный",
                provisional,
                opponents.len(),
            ),
        ] {
            report.metrics.push(metric(
                &format!("G06_pool_{pool}_{suffix}"), &format!("Рейтинг соперников · {pool} · {label}"), "%",
                (denominator > 0).then(|| 100. * numerator as f64 / denominator as f64),
                denominator, group.len(),
                "Missing / все партии; provisional / партии с известным положительным предыгровым рейтингом. Отдельно по platform/perf/variant; отсутствие не ноль рейтинга.",
                ids(group.iter().copied()),
            ));
        }
        let eligible: Vec<_> = rated
            .iter()
            .filter_map(|g| {
                let p = own(g).1;
                if p.provisional {
                    return None;
                }
                Some((
                    *g,
                    p.rating.filter(|r| *r > 0)? as f64,
                    metrics::own_score(g)?,
                ))
            })
            .collect();
        let value = elo_performance(
            &eligible.iter().map(|r| r.1).collect::<Vec<_>>(),
            &eligible.iter().map(|r| r.2).collect::<Vec<_>>(),
        );
        let mut m = metric(
            &format!("G08_{pool}"),
            &format!("Условный Elo-перформанс · {pool}"),
            "пунктов Elo",
            value,
            eligible.len(),
            rated.len(),
            "Σ logistic((Rp−Ropp)/400)=Σscore; одна rating pool, rated, известные непредварительные opponent ratings. Не официальный рейтинг; все победы/поражения не дают конечного значения. 95% bootstrap целых игр, >=90% конечных реплик.",
            ids(eligible.iter().map(|r| r.0)),
        );
        set_interval(&mut m, performance_ci(&eligible));
        if value.is_none() && !eligible.is_empty() {
            m.status = "unbounded_performance".into();
        }
        report.metrics.push(m);
        let residuals: Vec<_> = rated
            .iter()
            .filter_map(|g| {
                let (a, b) = own(g);
                if a.provisional || b.provisional {
                    return None;
                }
                let (x, y) = (a.rating.filter(|r| *r > 0)?, b.rating.filter(|r| *r > 0)?);
                Some((
                    *g,
                    metrics::own_score(g)? - elo_expected(x as f64, y as f64),
                ))
            })
            .collect();
        let values: Vec<_> = residuals.iter().map(|r| r.1).collect();
        let mut m = metric(
            &format!("G16_{pool}"),
            &format!("Очки относительно Elo-базы · {pool}"),
            "очка / партию",
            metrics::mean(&values),
            values.len(),
            rated.len(),
            "mean(score−1/(1+10^((Ropp−Rself)/400))); исходные рейтинги до игры, provisional исключён. Описательная Elo-база для Glicko, не прогноз и не причинный эффект.",
            ids(residuals.iter().map(|r| r.0)),
        );
        set_interval(&mut m, metrics::bootstrap_mean(&values));
        report.metrics.push(m);
        let observations: Vec<_> = rated
            .iter()
            .flat_map(|g| {
                let p = own(g).0;
                let mut values = vec![];
                if let Some(r) = p.rating {
                    values.push(r as f64);
                    if let Some(d) = p.rating_diff {
                        values.push((r + d) as f64);
                    }
                }
                values
            })
            .collect();
        for (suffix, label, value) in [
            (
                "min",
                "минимум",
                observations.iter().copied().reduce(f64::min),
            ),
            (
                "max",
                "максимум",
                observations.iter().copied().reduce(f64::max),
            ),
        ] {
            report.metrics.push(metric(&format!("G15_{suffix}_{pool}"),&format!("Наблюдаемый рейтинг · {pool} · {label}"),"пунктов",value,observations.len(),rated.len()*2,"Доступные pre-rating и pre+ratingDiff в одной рейтинговой шкале. Это наблюдения выбранных партий, не восстановление истории до T0.",ids(rated.iter().copied())));
        }
    }
    let colors = |color: &str| {
        games
            .iter()
            .filter(|g| g.own_color == color)
            .filter_map(|g| metrics::own_score(g).map(|s| 100. * s))
            .collect::<Vec<_>>()
    };
    let white = colors("white");
    let black = colors("black");
    let mut m = metric(
        "G04_difference",
        "Разница очков: белые − чёрные",
        "п.п.",
        metrics::mean(&white)
            .zip(metrics::mean(&black))
            .map(|(a, b)| a - b),
        white.len(),
        black.len(),
        "Разность mean(score×100) по цветам при тех же фильтрах. N белых / N чёрных; состав дебютов и соперников может различаться.",
        ids(games.iter().copied()),
    );
    set_interval(&mut m, bootstrap_difference(&white, &black));
    report.metrics.push(m);
}
fn sessions_and_streaks(report: &mut StatisticsReport, games: &[&StoredGame]) {
    let mut by_source: BTreeMap<String, Vec<&StoredGame>> = BTreeMap::new();
    for g in games.iter().filter(|g| g.started_at_ms.is_some()) {
        by_source
            .entry(format!("{}|{}", g.account_id, g.source))
            .or_default()
            .push(g);
    }
    for group in by_source.values_mut() {
        group.sort_by_key(|g| (g.started_at_ms, g.id.clone()));
    }
    for gap in [15u32, 30, 60] {
        let mut count = 0;
        for (source, group) in &by_source {
            let mut groups: Vec<Vec<&StoredGame>> = vec![];
            for g in group {
                let start = g.started_at_ms.unwrap();
                let append = groups
                    .last()
                    .and_then(|r| r.last())
                    .is_some_and(|previous| {
                        start - previous.started_at_ms.unwrap() <= gap as i64 * 60_000
                    });
                if append {
                    groups.last_mut().unwrap().push(g);
                } else {
                    groups.push(vec![g]);
                }
            }
            for (index, row) in groups.into_iter().enumerate() {
                count += 1;
                let first = row[0].started_at_ms.unwrap();
                let last = row.last().unwrap().started_at_ms.unwrap();
                let exact_ends: Option<Vec<_>> = row
                    .iter()
                    .map(|g| g.finished_at_ms.filter(|t| *t >= g.started_at_ms.unwrap()))
                    .collect();
                let ended = exact_ends.and_then(|values| values.into_iter().max());
                let order_ambiguous = row
                    .windows(2)
                    .any(|w| w[0].started_at_ms == w[1].started_at_ms);
                let overlapping = row.windows(2).any(|w| {
                    w[0].finished_at_ms
                        .or(w[0].last_move_at_ms)
                        .is_some_and(|end| end > w[1].started_at_ms.unwrap())
                });
                let wins = row.iter().filter(|g| metrics::outcome(g) == "win").count();
                let draws = row.iter().filter(|g| metrics::outcome(g) == "draw").count();
                report.extended.sessions.push(SessionRow {
                    id: format!("{source}:{gap}:{index}:{first}"),
                    gap_minutes: gap,
                    first_start_ms: first,
                    last_start_ms: last,
                    ended_at_ms: ended,
                    span_seconds: ended.map(|end| (end - first) as f64 / 1000.),
                    observed_start_span_seconds: (last - first) as f64 / 1000.,
                    overlapping,
                    order_ambiguous,
                    games: row.len(),
                    wins,
                    draws,
                    losses: row.len() - wins - draws,
                    score: Some(100. * (wins as f64 + 0.5 * draws as f64) / row.len() as f64),
                    game_ids: row.iter().map(|g| g.id.clone()).collect(),
                });
                if gap == 30 && !order_ambiguous {
                    for (ordinal, g) in row.iter().enumerate() {
                        let key = format!("G14_session_order_{}", ordinal + 1);
                        let existing = report.results.iter_mut().find(|r| r.key == key);
                        let outcome = metrics::outcome(g);
                        if let Some(result) = existing {
                            result.games += 1;
                            result.wins += usize::from(outcome == "win");
                            result.draws += usize::from(outcome == "draw");
                            result.losses += usize::from(outcome == "loss");
                            result.game_ids.push(g.id.clone());
                            result.score = Some(
                                100. * (result.wins as f64 + 0.5 * result.draws as f64)
                                    / result.games as f64,
                            );
                        } else {
                            report.results.push(ResultRow {
                                key,
                                label: format!("Партия {} в наблюдаемой сессии", ordinal + 1),
                                games: 1,
                                wins: usize::from(outcome == "win"),
                                draws: usize::from(outcome == "draw"),
                                losses: usize::from(outcome == "loss"),
                                score: metrics::own_score(g).map(|s| s * 100.),
                                game_ids: vec![g.id.clone()],
                                ..Default::default()
                            });
                        }
                    }
                }
            }
        }
        report.metrics.push(metric(&format!("G13_{gap}"),&format!("Наблюдаемые сессии · gap {gap} мин"),"сессий",(!by_source.is_empty()).then_some(count as f64),games.iter().filter(|g|g.started_at_ms.is_some()).count(),games.len(),"Группы по промежутку между соседними starts <= gap. Не доказывает непрерывную игру; точный span только если все ends достоверны. 15/30/60 — sensitivity заданной политики, не диагноз.",ids(games.iter().copied())));
    }
    let mut rating_groups: BTreeMap<String, Vec<&StoredGame>> = BTreeMap::new();
    for g in games.iter().filter(|g| query::game_time(g).is_some()) {
        rating_groups
            .entry(format!("{}|{}", g.account_id, rating_pool(g)))
            .or_default()
            .push(g);
    }
    for (stratum, mut ordered) in rating_groups {
        ordered.sort_by_key(|g| (query::game_time(g), g.id.clone()));
        let mut sequence: Vec<&StoredGame> = vec![];
        let mut previous = None;
        let mut kind = "";
        let mut serial = 0;
        let flush = |sequence: &mut Vec<&StoredGame>,
                     kind: &str,
                     serial: &mut usize,
                     output: &mut Vec<StreakRow>| {
            if !sequence.is_empty() {
                *serial += 1;
                output.push(StreakRow {
                    id: format!("{stratum}:{kind}:{}", *serial),
                    kind: kind.into(),
                    length: sequence.len(),
                    first_ms: query::game_time(sequence[0]).unwrap(),
                    last_ms: query::game_time(sequence.last().unwrap()).unwrap(),
                    stratum: stratum.clone(),
                    game_ids: sequence.iter().map(|g| g.id.clone()).collect(),
                });
                sequence.clear();
            }
        };
        let mut i = 0;
        while i < ordered.len() {
            let g = ordered[i];
            let time = query::game_time(g).unwrap();
            let tied = (i + 1 < ordered.len() && query::game_time(ordered[i + 1]) == Some(time))
                || previous == Some(time);
            let outcome = metrics::outcome(g);
            if tied || outcome == "draw" {
                flush(
                    &mut sequence,
                    kind,
                    &mut serial,
                    &mut report.extended.streaks,
                );
                kind = "";
            } else {
                if kind != outcome {
                    flush(
                        &mut sequence,
                        kind,
                        &mut serial,
                        &mut report.extended.streaks,
                    );
                    kind = outcome;
                }
                sequence.push(g);
            }
            previous = Some(time);
            i += 1;
        }
        flush(
            &mut sequence,
            kind,
            &mut serial,
            &mut report.extended.streaks,
        );
    }
    for kind in ["win", "loss"] {
        let maximum = report
            .extended
            .streaks
            .iter()
            .filter(|r| r.kind == kind)
            .map(|r| r.length)
            .max();
        report.metrics.push(metric(&format!("G14_{kind}_max"),if kind=="win"{"Максимальная серия побед"}else{"Максимальная серия поражений"},"партий",maximum.map(|n|n as f64),games.iter().filter(|g|query::game_time(g).is_some()).count(),games.len(),"Последовательность внутри platform/perf/variant; ничья и неоднозначные одинаковые timestamps разрывают серию. Не диагноз тильта; известный порядок не доказывает причину.",report.extended.streaks.iter().filter(|r|r.kind==kind&&Some(r.length)==maximum).flat_map(|r|r.game_ids.clone()).collect()));
    }
}

#[derive(Clone)]
struct Decision {
    game_id: String,
    ply: u32,
    own_index: usize,
    time_ms: Option<i64>,
    last_move_ms: Option<i64>,
    position_key: String,
    phase: String,
    stratum: String,
    remaining: Option<f64>,
    relative: Option<f64>,
    spent: Option<f64>,
    loss: Option<f64>,
    before_q: Option<f64>,
    run_id: Option<String>,
    fen: String,
    uci: String,
    initial: Option<f64>,
}
impl Decision {
    fn evidence(&self) -> MoveEvidence {
        MoveEvidence {
            game_id: self.game_id.clone(),
            ply: self.ply,
            run_id: self.run_id.clone(),
            fen: self.fen.clone(),
        }
    }
}
fn q_band(q: Option<f64>) -> &'static str {
    match q {
        Some(v) if v < 0.1 => "q<.1",
        Some(v) if v < 0.4 => "q.1-.4",
        Some(v) if v <= 0.6 => "q.4-.6",
        Some(v) if v < 0.9 => "q.6-.9",
        Some(_) => "q>=.9",
        None => "q-missing",
    }
}
fn time_bin(t: f64) -> &'static str {
    if t < 1. {
        "<1 с"
    } else if t < 5. {
        "1–5 с"
    } else if t < 15. {
        "5–15 с"
    } else if t < 30. {
        "15–30 с"
    } else if t < 60. {
        "30–60 с"
    } else {
        "≥60 с"
    }
}
fn remaining_bin(t: f64) -> &'static str {
    if t < 10. {
        "<10 с"
    } else if t < 30. {
        "10–30 с"
    } else if t < 60. {
        "30–60 с"
    } else if t < 180. {
        "60–180 с"
    } else {
        "≥180 с"
    }
}
fn collect_decisions(
    games: &[&StoredGame],
    quality: &HashMap<String, GameMetrics>,
    runs: &HashMap<String, &EngineRun>,
    phase: Option<&str>,
) -> Vec<Decision> {
    let mut output = vec![];
    for g in games {
        for (ordinal, n) in g
            .replay
            .nodes
            .iter()
            .skip(1)
            .filter(|n| n.actor.as_deref() == Some(&g.own_color))
            .enumerate()
        {
            if phase.is_some_and(|p| p != n.phase) {
                continue;
            }
            let event = quality
                .get(&g.id)
                .and_then(|m| m.decisions.iter().find(|e| e.ply == n.ply));
            let berserk = g.source_metadata["players"][&g.own_color]["berserk"]
                .as_bool()
                .unwrap_or(false);
            let initial = g.initial_seconds.filter(|v| *v > 0. && !berserk);
            output.push(Decision {
                game_id: g.id.clone(),
                ply: n.ply,
                own_index: ordinal + 1,
                time_ms: g.started_at_ms,
                last_move_ms: g.last_move_at_ms,
                position_key: g.replay.nodes[n.ply.saturating_sub(1) as usize]
                    .position_key
                    .clone(),
                phase: n.phase.clone(),
                stratum: query::stratum(g),
                remaining: n.remaining_before,
                relative: n.remaining_before.zip(initial).map(|(a, b)| 100. * a / b),
                spent: n.spent_seconds,
                loss: event.and_then(|e| e.loss_q),
                before_q: event.and_then(|e| e.before_q),
                run_id: runs.get(&g.id).map(|r| r.id.clone()),
                fen: n.fen.clone(),
                uci: n.uci.clone().unwrap_or_default(),
                initial,
            });
        }
    }
    output
}
fn context(
    id: &str,
    metric_id: &str,
    label: &str,
    stratum: &str,
    rows: &[&Decision],
    eligible: usize,
) -> ContextRow {
    let analysed: Vec<_> = rows.iter().copied().filter(|d| d.loss.is_some()).collect();
    let sum = analysed.iter().map(|d| d.loss.unwrap()).sum::<f64>();
    let mut blocks: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    for d in &analysed {
        let e = blocks.entry(d.game_id.clone()).or_default();
        e.0 += d.loss.unwrap();
        e.1 += 1;
    }
    let ci = ratio_bootstrap(&blocks);
    let value = (!analysed.is_empty()).then(|| sum / analysed.len() as f64);
    ContextRow {
        id: id.into(),
        metric_id: metric_id.into(),
        label: label.into(),
        stratum: stratum.into(),
        unit: "п.п.".into(),
        value,
        low: ci.map(|c| c.0),
        high: ci.map(|c| c.1),
        sample: analysed.len(),
        eligible,
        mean_loss: value,
        significant: analysed.iter().filter(|d| d.loss.unwrap() >= 10.).count(),
        status: interval_status(value, ci),
        game_ids: rows
            .iter()
            .map(|d| d.game_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
        evidence: rows.iter().map(|d| d.evidence()).collect(),
    }
}
fn event_rate(mut row: ContextRow, rows: &[&Decision]) -> ContextRow {
    let eligible: Vec<_> = rows.iter().copied().filter(|d| d.loss.is_some()).collect();
    let mut blocks: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    for d in &eligible {
        let e = blocks.entry(d.game_id.clone()).or_default();
        e.0 += if d.loss.unwrap() >= 10. { 100. } else { 0. };
        e.1 += 1;
    }
    let ci = ratio_bootstrap(&blocks);
    row.value =
        (!eligible.is_empty()).then(|| 100. * row.significant as f64 / eligible.len() as f64);
    row.unit = "%".into();
    row.low = ci.map(|c| c.0);
    row.high = ci.map(|c| c.1);
    row.status = interval_status(row.value, ci);
    row
}
fn time_metrics(report: &mut StatisticsReport, decisions: &[Decision]) {
    let all_ids: Vec<_> = decisions
        .iter()
        .map(|d| d.game_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let mut numbered: BTreeMap<(String, usize), Vec<&Decision>> = BTreeMap::new();
    let mut conditions: BTreeMap<String, Vec<&Decision>> = BTreeMap::new();
    let mut time_groups: BTreeMap<(String, String, String), Vec<&Decision>> = BTreeMap::new();
    for d in decisions {
        numbered
            .entry((d.stratum.clone(), d.own_index))
            .or_default()
            .push(d);
        let stratum = format!("{} | {} | {}", d.stratum, d.phase, q_band(d.before_q));
        if let Some(t) = d.remaining {
            time_groups
                .entry(("T05".into(), stratum.clone(), remaining_bin(t).into()))
                .or_default()
                .push(d);
        }
        if let Some(t) = d.relative {
            let bin = if t < 5. {
                "<5%"
            } else if t < 10. {
                "5–10%"
            } else if t < 25. {
                "10–25%"
            } else if t < 50. {
                "25–50%"
            } else {
                "≥50%"
            };
            time_groups
                .entry(("T05_relative".into(), stratum.clone(), bin.into()))
                .or_default()
                .push(d);
        }
        if let Some(t) = d.spent {
            time_groups
                .entry(("T06".into(), stratum.clone(), time_bin(t).into()))
                .or_default()
                .push(d);
            conditions.entry(stratum).or_default().push(d);
        }
    }
    for ((stratum, number), rows) in numbered {
        let values: Vec<_> = rows.iter().filter_map(|d| d.spent).collect();
        let mut row = context(
            &format!("T01:{stratum}:{number}"),
            "T01_by_move",
            &format!("Собственное решение {number}"),
            &stratum,
            &rows,
            rows.len(),
        );
        row.value = metrics::quantile(&values, 0.5);
        row.unit = "с".into();
        row.sample = values.len();
        row.low = metrics::quantile(&values, 0.25);
        row.high = metrics::quantile(&values, 0.75);
        row.status = if row.value.is_some() {
            "descriptive_iqr"
        } else {
            "missing_input"
        }
        .into();
        report.extended.time_contexts.push(row);
    }
    for ((id, stratum, bin), rows) in time_groups {
        let mut row = context(
            &format!("{id}:{stratum}:{bin}"),
            &id,
            &format!(
                "{} · {bin}",
                if id.starts_with("T05") {
                    "Остаток до решения"
                } else {
                    "Расход часов"
                }
            ),
            &stratum,
            &rows,
            rows.len(),
        );
        row.label.push_str(" · средний Lq; 95% CI по целым играм");
        if id.starts_with("T05") {
            let mut median = row.clone();
            median.id.push_str(":median");
            median.metric_id.push_str("_median");
            median.label =
                format!("Остаток до решения · {bin} · медиана Lq; 95% CI по целым играм");
            median.value =
                metrics::quantile(&rows.iter().filter_map(|d| d.loss).collect::<Vec<_>>(), 0.5);
            let ci = median_loss_interval(&rows, None);
            median.low = ci.map(|v| v.0);
            median.high = ci.map(|v| v.1);
            median.status = interval_status(median.value, ci);
            report.extended.time_contexts.push(median);
        }
        report.extended.time_contexts.push(row);
    }
    let mut long = vec![];
    let mut short = vec![];
    let mut long_comparison = vec![];
    let mut small_strata = 0;
    for (stratum, rows) in conditions {
        let values: Vec<_> = rows.iter().filter_map(|d| d.spent).collect();
        if values.len() < 20 {
            small_strata += 1;
            let mut row = context(
                &format!("T07:{stratum}:insufficient"),
                "T07",
                "Дециль расхода · требуется ≥20 clock-наблюдений",
                &stratum,
                &[],
                rows.len(),
            );
            row.status = "insufficient_quantile_sample".into();
            report.extended.time_contexts.push(row);
            continue;
        }
        let p90 = metrics::quantile(&values, 0.9).unwrap();
        let p10 = metrics::quantile(&values, 0.1).unwrap();
        let long_rows: Vec<_> = rows
            .iter()
            .copied()
            .filter(|d| d.spent.unwrap() >= p90)
            .collect();
        let other: Vec<_> = rows
            .iter()
            .copied()
            .filter(|d| d.spent.unwrap() < p90)
            .collect();
        let short_rows: Vec<_> = rows
            .iter()
            .copied()
            .filter(|d| {
                d.spent.unwrap() > 0.
                    && d.spent.unwrap() <= p10
                    && d.remaining
                        .zip(d.initial)
                        .is_some_and(|(remaining, initial)| remaining >= 10f64.max(initial * 0.1))
            })
            .collect();
        let zero: Vec<_> = rows
            .iter()
            .copied()
            .filter(|d| d.spent == Some(0.))
            .collect();
        long.extend(long_rows.clone());
        short.extend(short_rows.clone());
        long_comparison.extend(other.clone());
        for (id, label, selection) in [
            ("T07", format!("Spent ≥ P90={p90:.2} с"), &long_rows),
            ("T07_reference", format!("Spent < P90={p90:.2} с"), &other),
            (
                "T08",
                format!("0 < spent ≤ P10={p10:.2} с; remaining ≥ max(10 с, 10% initial)"),
                &short_rows,
            ),
            (
                "T08_zero",
                "Нулевой/округлённый расход, premove не подтверждён".into(),
                &zero,
            ),
        ] {
            let row = event_rate(
                context(
                    &format!("{id}:{stratum}"),
                    id,
                    &label,
                    &stratum,
                    selection,
                    selection.len(),
                ),
                selection,
            );
            report.extended.time_contexts.push(row);
        }
    }
    for (id, name, rows) in [
        ("T07", "Потери ≥10 п.п. после долгого расхода", long),
        (
            "T08",
            "Потери ≥10 п.п. после короткого расхода при запасе",
            short,
        ),
    ] {
        let row = event_rate(
            context(id, id, name, "within_strata", &rows, rows.len()),
            &rows,
        );
        let mut value = metric(
            id,
            name,
            "%",
            row.value,
            row.sample,
            decisions.len(),
            &format!(
                "Один profile; P90/P10 внутри source/perf/variant/control/color/opponent-band/phase/q-before страт с ≥20 known spent; ties включены. T08: spent>0, remaining≥max(10с,10% initial). {small_strata} малых страт исключено. Знаменатель — пригодные оценки выбранных clock-решений; корреляция не причина."
            ),
            row.game_ids.clone(),
        );
        value.low = row.low;
        value.high = row.high;
        value.status = row.status;
        report.metrics.push(value);
    }
    let reference = event_rate(
        context(
            "T07_reference",
            "T07",
            "Прочие решения в тех же стратах",
            "within_strata",
            &long_comparison,
            long_comparison.len(),
        ),
        &long_comparison,
    );
    report.metrics.push(metric("T07_reference","Потери ≥10 п.п. при прочем расходе в тех же стратах","%",reference.value,reference.sample,decisions.len(),"Остальные known spent той же quantile-страты; описательный comparator, не причинный эффект размышления",reference.game_ids));
    report.metrics.push(metric("T05","Покрытие качества по остаткам","решений",Some(decisions.iter().filter(|d|d.remaining.is_some()&&d.loss.is_some()).count()as f64),decisions.iter().filter(|d|d.remaining.is_some()&&d.loss.is_some()).count(),decisions.len(),"Подробные абсолютные/relative bins с mean Lq и CI по играм представлены в таблице time_contexts; exact control/phase/q раздельны",all_ids.clone()));
    report.metrics.push(metric("T06","Покрытие качества по расходу","решений",Some(decisions.iter().filter(|d|d.spent.is_some()&&d.loss.is_some()).count()as f64),decisions.iter().filter(|d|d.spent.is_some()&&d.loss.is_some()).count(),decisions.len(),"Spent bins и same-strata quantiles; mean Lq, frequency ≥10, sample, game/ply links в time_contexts",all_ids));
    low_time_episodes(report, decisions);
    repeated_position_time(report, decisions);
    prior_windows(report, decisions);
}
fn low_time_episodes(report: &mut StatisticsReport, decisions: &[Decision]) {
    let mut games: BTreeMap<String, Vec<&Decision>> = BTreeMap::new();
    for d in decisions {
        games.entry(d.game_id.clone()).or_default().push(d);
    }
    for rows in games.values_mut() {
        rows.sort_by_key(|d| d.ply);
    }
    for (relative, threshold) in [
        (false, 10.),
        (false, 30.),
        (false, 60.),
        (true, 5.),
        (true, 10.),
    ] {
        let mut total = 0;
        let mut eligible = 0;
        let mut ids = BTreeSet::new();
        for (game_id, rows) in &games {
            let mut episodes: Vec<Vec<&Decision>> = vec![];
            let mut current = vec![];
            for d in rows {
                let remaining = if relative { d.relative } else { d.remaining };
                if remaining.is_some() {
                    eligible += 1;
                }
                if remaining.is_some_and(|t| t < threshold) {
                    if current
                        .last()
                        .is_some_and(|previous: &&Decision| previous.own_index + 1 != d.own_index)
                    {
                        episodes.push(std::mem::take(&mut current));
                    }
                    current.push(*d);
                } else if !current.is_empty() {
                    episodes.push(std::mem::take(&mut current));
                }
            }
            if !current.is_empty() {
                episodes.push(current);
            }
            for episode in episodes {
                total += 1;
                ids.insert(game_id.clone());
                let start = episode[0].ply;
                let end = episode.last().unwrap().ply;
                let mut row = context(
                    &format!("T04_episode:{relative}:{threshold}:{game_id}:{start}"),
                    "T04_episode",
                    &format!(
                        "Остаток <{threshold}{} · ply {start}–{end}",
                        if relative { "% initial" } else { " с" }
                    ),
                    &episode[0].stratum,
                    &episode,
                    episode.len(),
                );
                row.value = episode
                    .iter()
                    .filter_map(|d| if relative { d.relative } else { d.remaining })
                    .reduce(f64::min);
                row.unit = if relative { "% initial" } else { "с" }.into();
                row.low = None;
                row.high = None;
                row.sample = episode.len();
                row.status = "descriptive_episode".into();
                report.extended.time_contexts.push(row);
            }
        }
        report.metrics.push(metric(&format!("T04_episodes_{}_{threshold}",if relative{"relative"}else{"seconds"}),&format!("Эпизоды остатка <{threshold}{}",if relative{"% initial"}else{" с"}),"эпизодов",(eligible>0).then_some(total as f64),total,eligible,"Максимальные последовательности соседних личных решений с remaining_before ниже порога. Высокие/неизвестные часы и пропуск собственного решения разрывают эпизод; game/ply evidence и N отдельно. Это операционный clock-порог, не психологическое состояние.",ids.into_iter().collect()));
    }
}
fn repeated_position_time(report: &mut StatisticsReport, decisions: &[Decision]) {
    let mut groups: BTreeMap<(String, String), Vec<&Decision>> = BTreeMap::new();
    for d in decisions
        .iter()
        .filter(|d| d.time_ms.is_some() && d.spent.is_some())
    {
        groups
            .entry((d.stratum.clone(), d.position_key.clone()))
            .or_default()
            .push(d);
    }
    let mut changes = vec![];
    let mut evidence_games = BTreeSet::new();
    for ((stratum, key), mut rows) in groups {
        rows.sort_by_key(|d| (d.time_ms, d.game_id.clone(), d.ply));
        let first = rows[0];
        // Creation order alone cannot order visits inside simultaneous games.
        if rows.iter().any(|d| {
            d.game_id != first.game_id
                && d.time_ms.is_some_and(|start| {
                    start <= first.last_move_ms.unwrap_or(first.time_ms.unwrap())
                })
        }) {
            continue;
        }
        let later: Vec<_> = rows
            .iter()
            .skip(1)
            .copied()
            .filter(|d| {
                (d.game_id == first.game_id && d.ply > first.ply)
                    || (d.time_ms > first.time_ms
                        && first.last_move_ms.is_some_and(|end| Some(end) < d.time_ms))
            })
            .collect();
        if later.is_empty() {
            continue;
        }
        let values: Vec<_> = later.iter().filter_map(|d| d.spent).collect();
        let current = metrics::quantile(&values, 0.5);
        let delta = current.zip(first.spent).map(|(a, b)| a - b);
        if let Some(delta) = delta {
            changes.push(delta);
        }
        let mut row_evidence = vec![first.evidence()];
        row_evidence.extend(later.iter().map(|d| d.evidence()));
        let game_ids: Vec<_> = row_evidence
            .iter()
            .map(|e| e.game_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        evidence_games.extend(game_ids.clone());
        report.extended.progress.push(ProgressRow {
            id: format!("T09:{stratum}:{key}"),
            metric_id: "T09".into(),
            label: "Расход при последующих посещениях − первый наблюдаемый".into(),
            key: format!("{stratum} | {key}"),
            unit: "с".into(),
            current,
            previous: first.spent,
            delta,
            current_sample: later.len(),
            previous_sample: 1,
            status: "descriptive_first_observation".into(),
            game_ids,
            evidence: row_evidence,
            ..Default::default()
        });
    }
    report.metrics.push(metric("T09","Изменение расхода в повторных позициях","с",metrics::mean(&changes),changes.len(),decisions.iter().filter(|d|d.spent.is_some()).count(),"Сначала median subsequent spent − first observed spent внутри Kpos/exact control/color, затем среднее по позициям. Между играми используется только доказанный порядок lastMoveAt ранней < createdAt поздней; совпадения и overlap исключены. Впервые в выборке не означает незнакомо, позже не означает выучено.",evidence_games.into_iter().collect()));
}
type TimeWindow<'a> = (&'a Decision, f64);
type WindowsByCondition<'a> = BTreeMap<String, (Vec<TimeWindow<'a>>, Vec<TimeWindow<'a>>)>;
fn prior_windows(report: &mut StatisticsReport, decisions: &[Decision]) {
    let mut games: BTreeMap<String, Vec<&Decision>> = BTreeMap::new();
    for d in decisions {
        games.entry(d.game_id.clone()).or_default().push(d);
    }
    for rows in games.values_mut() {
        rows.sort_by_key(|d| d.ply);
    }
    for k in [1usize, 3, 5] {
        let mut groups: WindowsByCondition = BTreeMap::new();
        let mut eligible = 0;
        for rows in games.values() {
            for i in 0..rows.len() {
                let d = rows[i];
                if d.loss.is_none() {
                    continue;
                }
                if d.loss.unwrap() >= 10. {
                    eligible += 1;
                }
                if i < k {
                    continue;
                }
                let before = &rows[i - k..i];
                if !before
                    .windows(2)
                    .all(|pair| pair[1].own_index == pair[0].own_index + 1)
                    || before.last().unwrap().own_index + 1 != d.own_index
                {
                    continue;
                }
                let spent: Option<Vec<_>> = before.iter().map(|row| row.spent).collect();
                let Some(spent) = spent else {
                    continue;
                };
                let key = format!(
                    "{} | {} | {} | {}",
                    d.stratum,
                    d.phase,
                    q_band(d.before_q),
                    d.remaining
                        .map(remaining_bin)
                        .unwrap_or("remaining-missing")
                );
                let e = groups.entry(key).or_default();
                if d.loss.unwrap() >= 10. {
                    e.0.push((d, spent.iter().sum()));
                } else {
                    e.1.push((d, spent.iter().sum()));
                }
            }
        }
        let mut differences = vec![];
        let mut all_ids = BTreeSet::new();
        let mut event_values = vec![];
        let mut comparator_values = vec![];
        for (stratum, (events, controls)) in groups {
            let mut used = HashSet::new();
            for (event, sum) in events {
                let matched = controls
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !used.contains(i))
                    .min_by_key(|(_, (d, _))| {
                        (
                            usize::from(d.game_id != event.game_id),
                            d.ply.abs_diff(event.ply),
                            d.game_id.clone(),
                        )
                    });
                let Some((index, (control, control_sum))) = matched else {
                    continue;
                };
                used.insert(index);
                event_values.push(sum);
                comparator_values.push(*control_sum);
                differences.push(sum - control_sum);
                all_ids.insert(event.game_id.clone());
                all_ids.insert(control.game_id.clone());
                report.extended.progress.push(ProgressRow {
                    id: format!("T10:{k}:{}:{}", event.game_id, event.ply),
                    metric_id: "T10".into(),
                    label: format!("Сумма списанного времени в {k} предыдущих личных решениях"),
                    key: stratum.clone(),
                    unit: "с".into(),
                    current: Some(sum),
                    previous: Some(*control_sum),
                    delta: Some(sum - control_sum),
                    current_sample: k,
                    previous_sample: k,
                    status: "descriptive_matched_windows".into(),
                    game_ids: vec![event.game_id.clone(), control.game_id.clone()],
                    evidence: vec![event.evidence(), control.evidence()],
                    ..Default::default()
                });
            }
        }
        report.metrics.push(metric(&format!("T10_k{k}"),&format!("Расход перед потерей ≥10 п.п. · окно {k}"),"с",metrics::quantile(&event_values,0.5),event_values.len(),eligible,"Медиана суммы spent в предыдущих k личных решениях; только полные окна. Один matched non-event на событие без повторного использования, одинаковые control/phase/q-before/remaining bins. Окна могут зависеть; причинных выводов и CI независимых ходов нет.",all_ids.iter().cloned().collect()));
        report.metrics.push(metric(&format!("T10_delta_k{k}"),&format!("Δ расхода перед потерей относительно matched окна · {k}"),"с",metrics::quantile(&differences,0.5),differences.len(),eligible,"Медиана парных разностей сумм spent; comparator без существенной потери в тех же наблюдаемых условиях, без причинной интерпретации.",all_ids.into_iter().collect()));
    }
}

#[derive(Default)]
struct ShapeAccumulator {
    label: String,
    visits: usize,
    game_ids: BTreeSet<String>,
    evidence: Vec<MoveEvidence>,
}
fn record_shape(
    groups: &mut BTreeMap<(String, String), ShapeAccumulator>,
    metric_id: &str,
    key: String,
    label: String,
    evidence: MoveEvidence,
) {
    let row = groups.entry((metric_id.into(), key)).or_default();
    row.label = label;
    row.visits += 1;
    row.game_ids.insert(evidence.game_id.clone());
    row.evidence.push(evidence);
}
#[derive(Default)]
struct Geometry {
    pawns: [Vec<(u32, u32)>; 2],
    pawn_bits: [u64; 2],
}
fn geometry(fen: &str) -> Geometry {
    let mut board = Geometry::default();
    let mut rank = 7u32;
    let mut file = 0;
    for ch in fen.split_whitespace().next().unwrap_or("").chars() {
        match ch {
            '/' => {
                rank = rank.saturating_sub(1);
                file = 0;
            }
            '1'..='8' => file += ch.to_digit(10).unwrap(),
            _ => {
                if ch == 'P' || ch == 'p' {
                    let side = usize::from(ch == 'p');
                    board.pawns[side].push((file, rank));
                    board.pawn_bits[side] |= 1u64 << (file + rank * 8);
                }
                file += 1;
            }
        }
    }
    board
}
fn pawn_counts(board: &Geometry, side: usize) -> (usize, usize, usize, usize) {
    let own = &board.pawns[side];
    let enemy = &board.pawns[1 - side];
    let isolated = own
        .iter()
        .filter(|(file, _)| !own.iter().any(|(other, _)| file.abs_diff(*other) == 1))
        .count();
    let mut files = [0usize; 8];
    for (file, _) in own {
        files[*file as usize] += 1;
    }
    let doubled_files = files.iter().filter(|n| **n >= 2).count();
    let doubled_pawns = files.iter().filter(|n| **n >= 2).sum();
    let passed = own
        .iter()
        .filter(|(file, rank)| {
            !enemy.iter().any(|(other, enemy_rank)| {
                file.abs_diff(*other) <= 1
                    && if side == 0 {
                        enemy_rank > rank
                    } else {
                        enemy_rank < rank
                    }
            })
        })
        .count();
    (isolated, doubled_files, doubled_pawns, passed)
}
fn pawn_signature(board: &Geometry) -> String {
    format!("{:016x}/{:016x}", board.pawn_bits[0], board.pawn_bits[1])
}
fn structures(report: &mut StatisticsReport, games: &[&StoredGame], decisions: &[Decision]) {
    let by_id: HashMap<_, _> = games.iter().map(|g| (g.id.as_str(), *g)).collect();
    let mut groups = BTreeMap::new();
    for d in decisions {
        let g = by_id[d.game_id.as_str()];
        let root = &g.replay.nodes[d.ply.saturating_sub(1) as usize];
        let evidence = event_evidence(g, root, None);
        let side = usize::from(g.own_color == "black");
        let board = geometry(&root.fen);
        let signed = root.material.balance * if side == 0 { 1 } else { -1 };
        let signature = format!("{:?}/{:?}", root.material.white, root.material.black);
        record_shape(
            &mut groups,
            "S09",
            format!("{}:{signature}", g.own_color),
            format!(
                "P/N/B/R/Q {signature} · {} свой баланс {signed:+}",
                g.own_color
            ),
            evidence.clone(),
        );
        for feature_side in 0..2 {
            let color = if feature_side == 0 { "white" } else { "black" };
            let (isolated, doubled_files, doubled_pawns, passed) =
                pawn_counts(&board, feature_side);
            for (key, label, count) in [
                ("isolated", "Изолированные пешки", isolated),
                (
                    "doubled_files",
                    "Файлы со сдвоенными пешками",
                    doubled_files,
                ),
                ("doubled_pawns", "Пешки на сдвоенных файлах", doubled_pawns),
                ("passed", "Геометрически проходные пешки", passed),
            ] {
                record_shape(
                    &mut groups,
                    "S10",
                    format!("{color}:{key}:{count}"),
                    format!("{label} · {count} · {color}"),
                    evidence.clone(),
                );
            }
            let mut half = 0;
            for file in 0..8 {
                let own = board.pawns[feature_side].iter().any(|(f, _)| *f == file);
                let enemy = board.pawns[1 - feature_side]
                    .iter()
                    .any(|(f, _)| *f == file);
                if !own && enemy {
                    half += 1;
                    record_shape(
                        &mut groups,
                        "S11",
                        format!("half:{color}:{file}"),
                        format!(
                            "Полуоткрытый для {color} файл {}",
                            (b'a' + file as u8) as char
                        ),
                        evidence.clone(),
                    );
                }
            }
            record_shape(
                &mut groups,
                "S11",
                format!("half_count:{color}:{half}"),
                format!("Полуоткрытых файлов для {color} · {half}"),
                evidence.clone(),
            );
        }
        let mut open = 0;
        for file in 0..8 {
            if !board.pawns.iter().flatten().any(|(f, _)| *f == file) {
                open += 1;
                record_shape(
                    &mut groups,
                    "S11",
                    format!("open:{file}"),
                    format!("Открытый файл {}", (b'a' + file as u8) as char),
                    evidence.clone(),
                );
            }
        }
        record_shape(
            &mut groups,
            "S11",
            format!("open_count:{open}"),
            format!("Открытых файлов · {open}"),
            evidence.clone(),
        );
        let pawn_key = pawn_signature(&board);
        record_shape(
            &mut groups,
            "S12",
            format!("signature:{pawn_key}"),
            format!("Точная пешечная форма · {pawn_key}"),
            evidence,
        );
    }
    let mut queenless = 0;
    let mut castled = 0;
    let mut transitions = 0;
    let mut castle_moves = 0;
    let mut queen_captures = 0;
    for g in games {
        if let Some(first) = g
            .replay
            .nodes
            .iter()
            .find(|n| n.material.white[4] + n.material.black[4] == 0)
        {
            queenless += 1;
            record_shape(
                &mut groups,
                "S13",
                if first.ply == 0 { "initial" } else { "reached" }.into(),
                if first.ply == 0 {
                    "Без ферзей в исходном FEN"
                } else {
                    "Достигнуто состояние без ферзей"
                }
                .into(),
                event_evidence(g, first, None),
            );
        }
        let mut castles = [None, None];
        for pair in g.replay.nodes.windows(2) {
            let (before, after) = (&pair[0], &pair[1]);
            if report
                .filter
                .phase
                .as_ref()
                .is_none_or(|phase| *phase == after.phase)
            {
                let a = pawn_signature(&geometry(&before.fen));
                let b = pawn_signature(&geometry(&after.fen));
                if a != b {
                    transitions += 1;
                    record_shape(
                        &mut groups,
                        "S12",
                        format!("transition:{a}>{b}"),
                        format!("Пешечный переход · {a} → {b}"),
                        event_evidence(g, after, None),
                    );
                }
                for side in 0..2 {
                    let previous = if side == 0 {
                        before.material.white[4]
                    } else {
                        before.material.black[4]
                    };
                    let current = if side == 0 {
                        after.material.white[4]
                    } else {
                        after.material.black[4]
                    };
                    if current < previous {
                        queen_captures += 1;
                        record_shape(
                            &mut groups,
                            "S13",
                            format!("queen_capture:{side}"),
                            format!(
                                "Фактическое взятие ферзя {}",
                                if side == 0 {
                                    "белых"
                                } else {
                                    "чёрных"
                                }
                            ),
                            event_evidence(g, after, None),
                        );
                    }
                }
            }
            if let Some(san) = after.san.as_deref() {
                let wing = if san.starts_with("O-O-O") {
                    Some("queenside")
                } else if san.starts_with("O-O") {
                    Some("kingside")
                } else {
                    None
                };
                if let Some(wing) = wing {
                    castle_moves += 1;
                    let side = usize::from(after.actor.as_deref() == Some("black"));
                    castles[side] = Some(wing);
                    record_shape(
                        &mut groups,
                        "S14",
                        format!("castle:{side}:{wing}"),
                        format!(
                            "Наблюдаемая рокировка {} · {wing}",
                            if side == 0 {
                                "белых"
                            } else {
                                "чёрных"
                            }
                        ),
                        event_evidence(g, after, None),
                    );
                }
            }
        }
        let full_history =
            g.variant == "standard" && g.replay.initial_fen == super::chess::INITIAL_FEN;
        let white = castles[0].unwrap_or(if full_history {
            "none"
        } else {
            "not_observed_from_fen"
        });
        let black = castles[1].unwrap_or(if full_history {
            "none"
        } else {
            "not_observed_from_fen"
        });
        if castles.iter().all(Option::is_some) {
            castled += 1;
        }
        record_shape(
            &mut groups,
            "S14",
            format!("combination:{white}:{black}"),
            format!("Рокировки · белые {white} / чёрные {black}"),
            event_evidence(g, &g.replay.nodes[0], None),
        );
        if let Some((a, b)) = castles[0].zip(castles[1]) {
            record_shape(
                &mut groups,
                "S14",
                format!("wings:{}", a != b),
                if a != b {
                    "Подтверждённые разносторонние рокировки"
                } else {
                    "Подтверждённые односторонние рокировки"
                }
                .into(),
                event_evidence(g, &g.replay.nodes[0], None),
            );
        }
    }
    for ((metric_id, id), row) in groups {
        let denominator = if metric_id == "S14" && id.starts_with("wings:") {
            castled
        } else if metric_id == "S14" && id.starts_with("combination:") {
            games.len()
        } else if metric_id == "S14" && id.starts_with("castle:") {
            castle_moves
        } else if metric_id == "S13" && id.starts_with("queen_capture:") {
            queen_captures
        } else if metric_id == "S13" {
            games.len()
        } else if metric_id == "S12" && id.starts_with("transition:") {
            transitions
        } else {
            decisions.len()
        };
        report.extended.structures.push(StructureRow {
            id,
            metric_id,
            label: row.label,
            games: row.game_ids.len(),
            visits: row.visits,
            decisions: denominator,
            share: (denominator > 0).then(|| 100. * row.visits as f64 / denominator as f64),
            game_ids: row.game_ids.into_iter().collect(),
            evidence: row.evidence,
        });
    }
    for (id, name, definition) in [
        (
            "S09",
            "Материальные составы",
            "Частоты точных (P,N,B,R,Q) обеих сторон в корнях собственных решений; signed weights P1/N3/B3/R5/Q9 не являются оценкой компенсации",
        ),
        (
            "S10",
            "Геометрические пешечные признаки",
            "Свои isolated/doubled/passed: по pawn bitboards и направлению. Изолированная — нет своей пешки на соседних файлах; passed — нет enemy впереди на своём/соседних файлах. Пешки и doubled-файлы раздельны; не диагноз слабости",
        ),
        (
            "S11",
            "Открытые и полуоткрытые файлы",
            "Open: нет пешек обеих сторон. Half-open own: нет своих, есть enemy; это не оценка контроля файла",
        ),
        (
            "S12",
            "Точные пешечные формы и переходы",
            "Два точных pawn bitboard, N уникальных игр/число посещений отдельно. Каждый реальный переход после изменения pawn signature, без семантических ярлыков",
        ),
    ] {
        let count = report
            .extended
            .structures
            .iter()
            .filter(|row| row.metric_id == id)
            .count();
        report.metrics.push(metric(
            id,
            name,
            "групп",
            (!games.is_empty()).then_some(count as f64),
            decisions.len(),
            decisions.len(),
            definition,
            ids(games.iter().copied()),
        ));
    }
    report.metrics.push(metric("S13","Партии с позицией без ферзей","%",(!games.is_empty()).then(||100.*queenless as f64/games.len()as f64),games.len(),games.len(),"First ply без ферзей обеих сторон; initial FEN отдельно. Факт queenless не доказывает добровольный размен или границу эндшпиля; queen captures отдельно.",report.extended.structures.iter().filter(|r|r.metric_id=="S13"&&!r.id.starts_with("queen_capture")).flat_map(|r|r.game_ids.clone()).collect()));
    report.metrics.push(metric("S14","Партии с обеими подтверждёнными рокировками","%",(!games.is_empty()).then(||100.*castled as f64/games.len()as f64),games.len(),games.len(),"Только legal SAN O-O/O-O-O в истории; отсутствие рокировки до initial FEN не угадывается по клеткам. Сочетания и opposite-wing отражаются отдельно.",ids(games.iter().copied())));
}
fn build_quality(
    games: &[&StoredGame],
    runs: &HashMap<String, &EngineRun>,
) -> HashMap<String, GameMetrics> {
    games
        .iter()
        .map(|g| {
            (
                g.id.clone(),
                metrics::game_metrics(g, runs.get(&g.id).copied()),
            )
        })
        .collect()
}
/// Percentile interval of the pooled decision median, resampling whole games with all decisions.
fn median_loss_interval(a: &[&Decision], b: Option<&[&Decision]>) -> Option<(f64, f64)> {
    let blocks = |rows: &[&Decision]| {
        let mut groups: BTreeMap<String, Vec<f64>> = BTreeMap::new();
        for d in rows {
            if let Some(loss) = d.loss {
                groups.entry(d.game_id.clone()).or_default().push(loss);
            }
        }
        groups.into_values().collect::<Vec<_>>()
    };
    let a = blocks(a);
    let b = b.map(blocks);
    if a.len() < 2 || b.as_ref().is_some_and(|b| b.len() < 2) {
        return None;
    }
    let mut seed = 0x7129_5cc1_9a67_1221;
    let mut samples = Vec::with_capacity(400);
    let sample = |groups: &[Vec<f64>], seed: &mut u64| {
        let mut values = vec![];
        for _ in 0..groups.len() {
            values.extend_from_slice(&groups[random_index(seed, groups.len())]);
        }
        metrics::quantile(&values, 0.5)
    };
    for _ in 0..400 {
        let av = sample(&a, &mut seed)?;
        let value = match &b {
            Some(b) => av - sample(b, &mut seed)?,
            None => av,
        };
        samples.push(value);
    }
    metrics::quantile(&samples, 0.025)
        .zip(metrics::quantile(&samples, 0.975))
        .filter(|(low, high)| high - low > 1e-12)
}

fn period_progress(
    report: &mut StatisticsReport,
    all_games: &[StoredGame],
    current: &[Decision],
    run_map: &HashMap<String, &EngineRun>,
    quality: &HashMap<String, GameMetrics>,
) {
    let (Some(since), Some(until)) = (
        report.filter.comparison_since_ms,
        report.filter.comparison_until_ms,
    ) else {
        return;
    };
    let mut previous_filter = report.filter.clone();
    previous_filter.since_ms = Some(since);
    previous_filter.until_ms = Some(until);
    previous_filter.comparison_since_ms = None;
    previous_filter.comparison_until_ms = None;
    let mut seen = HashSet::new();
    let previous_games: Vec<_> = all_games
        .iter()
        .filter(|g| query::eligible(g, &previous_filter) && seen.insert(g.id.clone()))
        .collect();
    let previous_quality = build_quality(&previous_games, run_map);
    let previous = collect_decisions(
        &previous_games,
        &previous_quality,
        run_map,
        report.filter.phase.as_deref(),
    );
    let overlaps = report.filter.since_ms.unwrap_or(i64::MIN) < until
        && since < report.filter.until_ms.unwrap_or(i64::MAX);
    let mut pair_groups: BTreeMap<String, (Vec<&Decision>, Vec<&Decision>)> = BTreeMap::new();
    for (period, rows) in [(0, current), (1, previous.as_slice())] {
        for d in rows.iter().filter(|d| d.loss.is_some()) {
            let key = format!("{}|{}|{}|{}", d.stratum, d.phase, d.position_key, d.uci);
            let e = pair_groups.entry(key).or_default();
            if period == 0 {
                e.0.push(d);
            } else {
                e.1.push(d);
            }
        }
    }
    let mut deltas = vec![];
    let mut median_deltas = vec![];
    let mut error_deltas = vec![];
    let mut paired_ids = BTreeSet::new();
    for (key, (a, b)) in pair_groups {
        if a.is_empty() || b.is_empty() {
            continue;
        }
        let values_a = a.iter().filter_map(|d| d.loss).collect::<Vec<_>>();
        let values_b = b.iter().filter_map(|d| d.loss).collect::<Vec<_>>();
        let by_game = |rows: &[&Decision], errors: bool| {
            let mut groups: BTreeMap<String, (f64, usize)> = BTreeMap::new();
            for d in rows {
                let e = groups.entry(d.game_id.clone()).or_default();
                e.0 += if errors {
                    if d.loss.unwrap() >= 10. { 100. } else { 0. }
                } else {
                    d.loss.unwrap()
                };
                e.1 += 1;
            }
            groups
                .into_iter()
                .map(|(id, (sum, count))| (id, sum, count))
                .collect::<Vec<_>>()
        };
        let game_ids: Vec<_> = a
            .iter()
            .chain(&b)
            .map(|d| d.game_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        paired_ids.extend(game_ids.clone());
        for (id, label, av, bv) in [
            (
                "P02",
                "Средний Lq в одинаковой позиции при том же played move",
                metrics::mean(&values_a),
                metrics::mean(&values_b),
            ),
            (
                "P02_median",
                "Медиана Lq в одинаковой позиции при том же played move",
                metrics::quantile(&values_a, 0.5),
                metrics::quantile(&values_b, 0.5),
            ),
            (
                "P02_errors",
                "Частота Lq≥10 в одинаковой позиции при том же played move",
                Some(
                    100. * values_a.iter().filter(|v| **v >= 10.).count() as f64
                        / values_a.len() as f64,
                ),
                Some(
                    100. * values_b.iter().filter(|v| **v >= 10.).count() as f64
                        / values_b.len() as f64,
                ),
            ),
        ] {
            let delta = av.zip(bv).map(|(x, y)| x - y);
            if let Some(value) = delta {
                match id {
                    "P02_median" => median_deltas.push(value),
                    "P02_errors" => error_deltas.push(value),
                    _ => deltas.push(value),
                }
            }
            let ci = if overlaps {
                None
            } else if id == "P02_median" {
                median_loss_interval(&a, Some(&b))
            } else {
                ratio_difference_ci(
                    &by_game(&a, id == "P02_errors"),
                    &by_game(&b, id == "P02_errors"),
                )
                .filter(|(low, high)| high - low > 1e-12)
            };
            report.extended.progress.push(ProgressRow {
                id: format!("{id}:{key}"),
                metric_id: id.into(),
                label: label.into(),
                key: key.clone(),
                unit: "п.п.".into(),
                current: av,
                previous: bv,
                delta,
                current_sample: a.len(),
                previous_sample: b.len(),
                low: ci.map(|v| v.0),
                high: ci.map(|v| v.1),
                status: if overlaps {
                    "no_independent_comparison".into()
                } else {
                    interval_status(delta, ci)
                },
                game_ids: game_ids.clone(),
                evidence: a.iter().chain(&b).map(|d| d.evidence()).collect(),
            });
        }
    }
    for (id, name, changes) in [
        (
            "P02",
            "Δ средней потери в повторяющихся позициях/ходах",
            deltas,
        ),
        (
            "P02_median",
            "Δ медианы потери в повторяющихся позициях/ходах",
            median_deltas,
        ),
        (
            "P02_errors",
            "Δ частоты Lq≥10 в повторяющихся позициях/ходах",
            error_deltas,
        ),
    ] {
        report.metrics.push(metric(id,name,"п.п.",metrics::mean(&changes),changes.len(),current.iter().filter(|d|d.loss.is_some()).count(),"Сначала разность указанной характеристики в каждой общей exact Kpos+played move+source/perf/variant/exact control/color/opponent-band/phase при одном engine profile, затем среднее позиционных разностей. Медиана и Q09/100 имеют отдельные строки. Rule/repetition contexts могут различаться; это не доказательство изменения человеческой сложности.",paired_ids.iter().cloned().collect()));
    }
    let ids_by_report: HashSet<_> = report.games.iter().map(|g| g.id.as_str()).collect();
    let selected_games: Vec<_> = all_games
        .iter()
        .filter(|g| {
            ids_by_report.contains(g.id.as_str())
                && report
                    .filter
                    .account_id
                    .as_ref()
                    .is_none_or(|id| id == &g.account_id)
        })
        .collect();
    for (id, label) in [
        ("E02", "Использование возможностей"),
        ("E08", "Реализация преимущества"),
        ("E10", "Спасение трудных партий"),
        ("E11", "Защитные потери"),
    ] {
        let a = conditional_groups(&selected_games, quality, id, report.filter.phase.as_deref());
        let b = conditional_groups(
            &previous_games,
            &previous_quality,
            id,
            report.filter.phase.as_deref(),
        );
        for (key, rows) in a {
            let Some(other) = b.get(&key) else {
                continue;
            };
            let ratio = |values: &[(String, f64, usize)]| {
                let n = values.iter().map(|r| r.2).sum::<usize>();
                (n > 0).then(|| values.iter().map(|r| r.1).sum::<f64>() / n as f64)
            };
            let current = ratio(&rows);
            let previous = ratio(other);
            let delta = current.zip(previous).map(|(x, y)| x - y);
            let ci = if overlaps {
                None
            } else {
                ratio_difference_ci(&rows, other)
            };
            report.extended.progress.push(ProgressRow {
                id: format!("P03:{id}:{key}"),
                metric_id: "P03".into(),
                label: label.into(),
                key,
                unit: "п.п.".into(),
                current,
                previous,
                delta,
                current_sample: rows.iter().map(|r| r.2).sum(),
                previous_sample: other.iter().map(|r| r.2).sum(),
                low: ci.map(|v| v.0),
                high: ci.map(|v| v.1),
                status: if overlaps {
                    "no_independent_comparison".into()
                } else {
                    interval_status(delta, ci)
                },
                game_ids: rows
                    .iter()
                    .chain(other)
                    .map(|r| r.0.clone())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                evidence: vec![],
            });
        }
    }
}
fn conditional_groups(
    games: &[&StoredGame],
    quality: &HashMap<String, GameMetrics>,
    id: &str,
    phase: Option<&str>,
) -> BTreeMap<String, Vec<(String, f64, usize)>> {
    let mut accumulated: BTreeMap<(String, String), (f64, usize)> = BTreeMap::new();
    for g in games {
        let m = &quality[&g.id];
        let events: Vec<_> = m
            .events
            .iter()
            .filter(|e| phase.is_none_or(|p| p == e.phase))
            .collect();
        let condition = |event: &DecisionEvent| {
            format!(
                "{} | entry {} | remaining {}",
                query::stratum(g),
                q_band(event.before_q),
                event.clock_seconds.map(remaining_bin).unwrap_or("missing")
            )
        };
        let mut add = |key: String, sum: f64, count: usize| {
            if count > 0 {
                let e = accumulated.entry((key, g.id.clone())).or_default();
                e.0 += sum;
                e.1 += count;
            }
        };
        match id {
            "E02" => {
                for event in events
                    .iter()
                    .filter(|e| e.kind == "opportunity_used" || e.kind == "opportunity_missed")
                {
                    add(
                        condition(event),
                        if event.kind == "opportunity_used" {
                            100.
                        } else {
                            0.
                        },
                        1,
                    );
                }
            }
            "E08" | "E10" => {
                let kind = if id == "E08" {
                    "advantage_entry"
                } else {
                    "defense_entry"
                };
                if let Some(entry) = events
                    .iter()
                    .filter(|e| e.kind == kind)
                    .min_by_key(|e| e.ply)
                {
                    let success = if id == "E08" {
                        metrics::outcome(g) == "win"
                    } else {
                        ["win", "draw"].contains(&metrics::outcome(g))
                    };
                    add(condition(entry), if success { 100. } else { 0. }, 1);
                }
            }
            _ => {
                for event in m
                    .defensive_decisions
                    .iter()
                    .filter(|e| phase.is_none_or(|p| p == e.phase))
                {
                    if let Some(loss) = event.loss_q {
                        add(condition(event), loss, 1);
                    }
                }
            }
        }
    }
    let mut groups: BTreeMap<String, Vec<(String, f64, usize)>> = BTreeMap::new();
    for ((key, game_id), (sum, count)) in accumulated {
        groups.entry(key).or_default().push((game_id, sum, count));
    }
    groups
}
fn ratio_difference_ci(
    a: &[(String, f64, usize)],
    b: &[(String, f64, usize)],
) -> Option<(f64, f64)> {
    if a.len() < 2 || b.len() < 2 {
        return None;
    }
    let mut seed = 0x239a_0bd1_413d_772b;
    let mut samples = vec![];
    for _ in 0..400 {
        let (mut sum_a, mut sum_b, mut n_a, mut n_b) = (0., 0., 0, 0);
        for _ in 0..a.len() {
            let r = &a[random_index(&mut seed, a.len())];
            sum_a += r.1;
            n_a += r.2;
        }
        for _ in 0..b.len() {
            let r = &b[random_index(&mut seed, b.len())];
            sum_b += r.1;
            n_b += r.2;
        }
        if n_a > 0 && n_b > 0 {
            samples.push(sum_a / n_a as f64 - sum_b / n_b as f64);
        }
    }
    Some((
        metrics::quantile(&samples, 0.025)?,
        metrics::quantile(&samples, 0.975)?,
    ))
}
fn coverage(
    report: &mut StatisticsReport,
    games: &[&StoredGame],
    decisions: &[Decision],
    runs: &HashMap<String, &EngineRun>,
) {
    let definitions = [
        (
            "missing_profile",
            games
                .iter()
                .filter(|g| !runs.contains_key(&g.id))
                .map(|g| {
                    (
                        g.id.clone(),
                        g.replay
                            .nodes
                            .iter()
                            .filter(|n| {
                                n.actor.as_deref() == Some(&g.own_color)
                                    && report
                                        .filter
                                        .phase
                                        .as_ref()
                                        .is_none_or(|phase| phase == &n.phase)
                            })
                            .count(),
                    )
                })
                .collect::<Vec<_>>(),
        ),
        (
            "missing_spent",
            decisions
                .iter()
                .filter(|d| d.spent.is_none())
                .map(|d| (d.game_id.clone(), 1))
                .collect(),
        ),
        (
            "missing_remaining",
            decisions
                .iter()
                .filter(|d| d.remaining.is_none())
                .map(|d| (d.game_id.clone(), 1))
                .collect(),
        ),
        (
            "missing_comparable_q",
            decisions
                .iter()
                .filter(|d| d.loss.is_none())
                .map(|d| (d.game_id.clone(), 1))
                .collect(),
        ),
    ];
    for (reason, rows) in definitions {
        let game_ids: Vec<_> = rows
            .iter()
            .map(|r| r.0.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        report.extended.coverage.push(CoverageRow {
            reason: reason.into(),
            games: game_ids.len(),
            decisions: rows.iter().map(|r| r.1).sum(),
            game_ids,
        });
    }
    let by_game: HashMap<_, _> = games.iter().map(|g| (g.id.as_str(), *g)).collect();
    let mut reasons: BTreeMap<String, Vec<&Decision>> = BTreeMap::new();
    for d in decisions {
        if d.spent.is_none() {
            let g = by_game[d.game_id.as_str()];
            if let Some(node) = g.replay.nodes.get(d.ply as usize) {
                reasons
                    .entry(format!("clock_{}", node.clock_quality))
                    .or_default()
                    .push(d);
            }
        }
        if let Some(run) = runs.get(&d.game_id) {
            match run.confirmed_moves.iter().find(|c| c.ply == d.ply) {
                None => reasons
                    .entry("missing_same_root".into())
                    .or_default()
                    .push(d),
                Some(c) if c.method != "same_root_searchmoves" || !c.complete => reasons
                    .entry("incomplete_same_root".into())
                    .or_default()
                    .push(d),
                Some(c) => {
                    if c.best.bound.is_some()
                        || c.played.bound.is_some()
                        || !c.best.complete
                        || !c.played.complete
                    {
                        reasons
                            .entry("bound_or_incomplete_score".into())
                            .or_default()
                            .push(d);
                    } else if metrics::eval_q(&c.best, &by_game[d.game_id.as_str()].own_color)
                        .is_none()
                        || metrics::eval_q(&c.played, &by_game[d.game_id.as_str()].own_color)
                            .is_none()
                    {
                        reasons
                            .entry("missing_wdl_terminal_score".into())
                            .or_default()
                            .push(d);
                    } else if d.loss.is_none() {
                        reasons
                            .entry("unstable_same_root_score".into())
                            .or_default()
                            .push(d);
                    }
                }
            }
        }
    }
    for (reason, rows) in reasons {
        let ids: Vec<_> = rows
            .iter()
            .map(|d| d.game_id.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        report.extended.coverage.push(CoverageRow {
            reason,
            games: ids.len(),
            decisions: rows.len(),
            game_ids: ids,
        });
    }
    report.metrics.push(metric("P04_clock","Покрытие восстановленным расходом часов","%",(!decisions.is_empty()).then(||100.*decisions.iter().filter(|d|d.spent.is_some()).count()as f64/decisions.len()as f64),decisions.iter().filter(|d|d.spent.is_some()).count(),decisions.len(),"Известный spent / выбранные личные решения; пропущенные clocks, первый ход и неполные пары не являются нулями. Причины отсутствия исходных игр поступают отдельно из ingest-state.",ids(games.iter().copied())));
}
pub fn append_extended(
    report: &mut StatisticsReport,
    all_games: &[StoredGame],
    runs: &[EngineRun],
) {
    report.extended = ExtendedReport::default();
    let wanted: HashSet<_> = report.games.iter().map(|g| g.id.as_str()).collect();
    let mut seen = HashSet::new();
    let selected: Vec<_> = all_games
        .iter()
        .filter(|g| {
            wanted.contains(g.id.as_str())
                && report
                    .filter
                    .account_id
                    .as_ref()
                    .is_none_or(|id| id == &g.account_id)
                && seen.insert(g.id.clone())
        })
        .collect();
    let mut run_map: HashMap<String, &EngineRun> = HashMap::new();
    for run in runs
        .iter()
        .filter(|r| Some(&r.profile_id) == report.selected_profile_id.as_ref())
    {
        let existing = run_map.entry(run.game_id.clone()).or_insert(run);
        if run.completed_at_ms > existing.completed_at_ms {
            *existing = run;
        }
    }
    let quality = build_quality(&selected, &run_map);
    let decisions = collect_decisions(
        &selected,
        &quality,
        &run_map,
        report.filter.phase.as_deref(),
    );
    general_metrics(report, &selected);
    sessions_and_streaks(report, &selected);
    time_metrics(report, &decisions);
    structures(report, &selected, &decisions);
    period_progress(report, all_games, &decisions, &run_map, &quality);
    coverage(report, &selected, &decisions, &run_map);
}

#[cfg(test)]
mod tests {
    use super::super::chess;
    use super::*;
    fn fixture(id: &str, time: i64, result: &str) -> StoredGame {
        StoredGame {
            id: id.into(),
            external_id: id.into(),
            account_id: "account".into(),
            source: "lichess".into(),
            started_at_ms: Some(time),
            last_move_at_ms: Some(time + 1),
            white: Participant {
                name: "Fixture".into(),
                rating: Some(1500),
                ..Default::default()
            },
            black: Participant {
                name: "Opponent".into(),
                rating: Some(1500),
                ..Default::default()
            },
            own_color: "white".into(),
            result: result.into(),
            status: if result == "1/2-1/2" {
                "draw"
            } else {
                "resign"
            }
            .into(),
            speed: "blitz".into(),
            rated: Some(true),
            variant: "standard".into(),
            initial_seconds: Some(100.),
            increment_seconds: Some(0.),
            raw_pgn: format!("1. e4 e5 2. Nf3 Nc6 {result}"),
            eligibility: "eligible".into(),
            replay: chess::replay_pgn(&format!("1. e4 e5 2. Nf3 Nc6 {result}")).unwrap(),
            ..Default::default()
        }
    }
    fn quality(g: &StoredGame, loss: f64, profile: &str) -> EngineRun {
        let evaluate = |q: f64| PositionEvaluation {
            complete: true,
            wdl: Some([
                (q * 1000.).round() as u32,
                0,
                ((1. - q) * 1000.).round() as u32,
            ]),
            ..Default::default()
        };
        EngineRun {
            id: format!("run:{}:{profile}", g.id),
            game_id: g.id.clone(),
            profile_id: profile.into(),
            profile: "test".into(),
            completed_at_ms: 1,
            confirmed_moves: g
                .replay
                .nodes
                .iter()
                .filter(|n| n.actor.as_deref() == Some("white"))
                .map(|n| MoveComparison {
                    ply: n.ply,
                    best: evaluate(0.5),
                    played: evaluate(0.5 - loss / 100.),
                    complete: true,
                    method: "same_root_searchmoves".into(),
                })
                .collect(),
            ..Default::default()
        }
    }
    fn find<'a>(report: &'a StatisticsReport, id: &str) -> &'a MetricValue {
        report.metrics.iter().find(|m| m.id == id).unwrap()
    }
    #[test]
    fn conditional_elo_solves_logistic_and_extreme_results_stay_missing() {
        assert!((elo_performance(&[1500., 1500.], &[1., 0.]).unwrap() - 1500.).abs() < 1e-8);
        assert!(
            (elo_performance(&[1500., 1500., 1500., 1500.], &[1., 1., 1., 0.]).unwrap()
                - (1500. + 400. * 3f64.log10()))
            .abs()
                < 1e-8
        );
        assert!(elo_performance(&[1500., 1500.], &[1., 1.]).is_none());
        assert!(elo_performance(&[], &[]).is_none());
        assert!((elo_expected(1600., 1500.) + elo_expected(1500., 1600.) - 1.).abs() < 1e-12);
    }
    #[test]
    fn rating_pools_are_separate_and_expected_residual_is_observed_score_minus_baseline() {
        let mut games = vec![
            fixture("w", 1, "1-0"),
            fixture("l", 2, "0-1"),
            fixture("r", 3, "1/2-1/2"),
        ];
        games[2].speed = "rapid".into();
        let report = query::report(&games, &[], &StatisticsFilter::default());
        assert_eq!(find(&report, "G06").value, None);
        assert_eq!(find(&report, "G06").status, "mixed_rating_scales");
        assert!(
            (find(&report, "G08_lichess · blitz · standard")
                .value
                .unwrap()
                - 1500.)
                .abs()
                < 1e-7
        );
        assert_eq!(
            find(&report, "G16_lichess · blitz · standard").value,
            Some(0.)
        );
        assert_eq!(
            find(&report, "G15_min_lichess · rapid · standard").value,
            Some(1500.)
        );
    }
    #[test]
    fn missing_and_provisional_ratings_do_not_become_zero() {
        let mut g = fixture("a", 1, "1-0");
        g.black.rating = None;
        let report = query::report(std::slice::from_ref(&g), &[], &StatisticsFilter::default());
        assert!(
            find(&report, "G08_lichess · blitz · standard")
                .value
                .is_none()
        );
        g.black.rating = Some(1600);
        g.black.provisional = true;
        let report = query::report(&[g], &[], &StatisticsFilter::default());
        assert_eq!(find(&report, "G16_lichess · blitz · standard").sample, 0);
    }
    #[test]
    fn sessions_have_sensitivity_but_no_invented_duration() {
        let games = vec![
            fixture("a", 1000, "1-0"),
            fixture("b", 1000 + 20 * 60_000, "0-1"),
            fixture("c", 1000 + 70 * 60_000, "1/2-1/2"),
        ];
        let report = query::report(&games, &[], &StatisticsFilter::default());
        assert_eq!(find(&report, "G13_15").value, Some(3.));
        assert_eq!(find(&report, "G13_30").value, Some(2.));
        assert_eq!(find(&report, "G13_60").value, Some(1.));
        assert!(
            report
                .extended
                .sessions
                .iter()
                .all(|s| s.span_seconds.is_none())
        );
        assert!(
            report
                .results
                .iter()
                .any(|row| row.key == "G14_session_order_2" && row.games == 1)
        );
    }
    #[test]
    fn equal_timestamps_and_draw_break_streaks() {
        let games = vec![
            fixture("a", 1, "1-0"),
            fixture("b", 2, "1-0"),
            fixture("c", 3, "1/2-1/2"),
            fixture("d", 4, "1-0"),
            fixture("e", 4, "1-0"),
            fixture("f", 5, "1-0"),
        ];
        let report = query::report(&games, &[], &StatisticsFilter::default());
        assert_eq!(find(&report, "G14_win_max").value, Some(2.));
        assert!(
            !report
                .extended
                .streaks
                .iter()
                .any(|r| r.game_ids.contains(&"d".to_owned())
                    || r.game_ids.contains(&"e".to_owned()))
        );
    }
    fn time_fixture(id: &str, time: i64) -> (StoredGame, EngineRun) {
        let mut g = fixture(id, time, "1-0");
        let mut pgn = String::new();
        for _ in 0..10 {
            pgn.push_str("Nf3 Nf6 Ng1 Ng8 ");
        }
        pgn.push_str("1-0");
        g.raw_pgn = pgn.clone();
        g.replay = chess::replay_pgn(&pgn).unwrap();
        let mut index = 0;
        for n in &mut g.replay.nodes {
            n.phase = "middlegame".into();
            if n.actor.as_deref() == Some("white") {
                index += 1;
                n.spent_seconds = Some(index as f64);
                n.remaining_before = Some(100.);
            }
        }
        let mut run = quality(&g, 0., "p");
        for (i, c) in run.confirmed_moves.iter_mut().enumerate() {
            if [0, 1, 18, 19].contains(&i) {
                c.played.wdl = Some([300, 0, 700]);
            }
        }
        (g, run)
    }
    #[test]
    fn time_deciles_use_strata_and_loss_denominators() {
        let (g, run) = time_fixture("time", 1);
        let report = query::report(&[g], &[run], &StatisticsFilter::default());
        assert_eq!(find(&report, "T07").value, Some(100.));
        assert_eq!(find(&report, "T07").sample, 2);
        assert_eq!(find(&report, "T08").value, Some(100.));
        assert_eq!(find(&report, "T08").sample, 2);
        assert!((find(&report, "T07_reference").value.unwrap() - 100. * 2. / 18.).abs() < 1e-8);
        assert!(
            report
                .extended
                .time_contexts
                .iter()
                .any(|r| r.metric_id == "T05_relative" && r.sample > 0)
        );
        assert!(
            report
                .extended
                .progress
                .iter()
                .any(|r| r.metric_id == "T10" && r.evidence.len() == 2)
        );
        assert!(
            report
                .extended
                .progress
                .iter()
                .any(|r| r.metric_id == "T09")
        );
        assert!(
            report
                .extended
                .time_contexts
                .iter()
                .any(|r| r.metric_id == "T01_by_move")
        );
    }
    #[test]
    fn small_clock_strata_do_not_manufacture_quantiles_or_analysis() {
        let mut g = fixture("a", 1, "1-0");
        g.replay.nodes[1].spent_seconds = Some(0.);
        g.replay.nodes[1].remaining_before = Some(100.);
        let report = query::report(&[g], &[], &StatisticsFilter::default());
        assert!(find(&report, "T07").value.is_none());
        assert!(
            report
                .extended
                .time_contexts
                .iter()
                .any(|r| r.status == "insufficient_quantile_sample")
        );
        assert_eq!(find(&report, "P04_clock").value, Some(50.));
    }
    #[test]
    fn pawn_geometry_counts_files_and_pawns_separately_and_direction_matters() {
        let board = geometry("4k3/5p2/8/4P3/1p6/2P5/P1P3P1/4K3 w - - 0 1");
        assert_eq!(pawn_counts(&board, 0), (5, 1, 2, 0));
        let board = geometry("4k3/8/8/4P3/3p4/8/8/4K3 w - - 0 1");
        assert_eq!(pawn_counts(&board, 0).3, 1);
        assert_eq!(pawn_counts(&board, 1).3, 1);
    }
    #[test]
    fn structural_evidence_uses_real_castles_and_does_not_guess_from_king_squares() {
        let mut g = fixture("castle", 1, "1/2-1/2");
        g.raw_pgn = "1. Nf3 Nf6 2. g3 g6 3. Bg2 Bg7 4. O-O O-O 1/2-1/2".into();
        g.replay = chess::replay_pgn(&g.raw_pgn).unwrap();
        let report = query::report(&[g], &[], &StatisticsFilter::default());
        assert_eq!(find(&report, "S14").value, Some(100.));
        assert!(
            report
                .extended
                .structures
                .iter()
                .any(|r| r.id == "combination:kingside:kingside")
        );
        let mut g = fixture("fen", 2, "1-0");
        g.variant = "fromPosition".into();
        g.raw_pgn = "[FEN \"5rk1/7p/8/8/8/8/P7/5RK1 w - - 0 1\"]\n\n1. a3 h6 1-0".into();
        g.replay = chess::replay_pgn(&g.raw_pgn).unwrap();
        let report = query::report(&[g], &[], &StatisticsFilter::default());
        assert_eq!(find(&report, "S14").value, Some(0.));
        assert_eq!(find(&report, "S13").value, Some(100.));
        assert!(
            report
                .extended
                .structures
                .iter()
                .any(|r| r.id.contains("not_observed_from_fen"))
        );
        assert!(
            report
                .extended
                .structures
                .iter()
                .filter(|r| r.metric_id == "S12" && r.id.starts_with("transition:"))
                .all(|r| r.share.unwrap() <= 100.)
        );
    }
    #[test]
    fn paired_position_progress_uses_one_profile_and_preserves_evidence() {
        let games = vec![
            fixture("old1", 100, "1-0"),
            fixture("old2", 101, "1-0"),
            fixture("new1", 300, "1-0"),
            fixture("new2", 301, "1-0"),
        ];
        let mut runs: Vec<_> = games
            .iter()
            .map(|g| quality(g, if g.id.starts_with("old") { 20. } else { 0. }, "p"))
            .collect();
        let mut wrong = quality(&games[2], 40., "other");
        wrong.completed_at_ms = 0;
        runs.push(wrong);
        let f = StatisticsFilter {
            since_ms: Some(200),
            until_ms: Some(400),
            comparison_since_ms: Some(0),
            comparison_until_ms: Some(200),
            profile_id: Some("p".into()),
            ..Default::default()
        };
        let report = query::report(&games, &runs, &f);
        let rows: Vec<_> = report
            .extended
            .progress
            .iter()
            .filter(|r| r.metric_id == "P02")
            .collect();
        assert!(!rows.is_empty());
        assert!(rows.iter().all(|r| (r.delta.unwrap() + 20.).abs() < 1e-8));
        assert!(rows.iter().all(|r| {
            r.evidence
                .iter()
                .all(|e| e.run_id.as_ref().unwrap().ends_with(":p"))
        }));
        assert!(rows.iter().all(|r| r.low.is_none()));
        let medians: Vec<_> = report
            .extended
            .progress
            .iter()
            .filter(|r| r.metric_id == "P02_median")
            .collect();
        let errors: Vec<_> = report
            .extended
            .progress
            .iter()
            .filter(|r| r.metric_id == "P02_errors")
            .collect();
        assert_eq!(medians.len(), rows.len());
        assert_eq!(errors.len(), rows.len());
        assert!(
            medians
                .iter()
                .all(|r| (r.delta.unwrap() + 20.).abs() < 1e-8)
        );
        assert!(
            errors
                .iter()
                .all(|r| (r.delta.unwrap() + 100.).abs() < 1e-8)
        );
        assert!(medians.iter().chain(&errors).all(|r| r.current_sample == 2
            && r.previous_sample == 2
            && r.low.is_none()
            && r.evidence.len() == 4));
    }
    #[test]
    fn remaining_time_median_is_separate_from_mean_and_error_frequency() {
        let (g, run) = time_fixture("median", 1);
        let report = query::report(&[g], &[run], &StatisticsFilter::default());
        for (id, median_id) in [
            ("T05", "T05_median"),
            ("T05_relative", "T05_relative_median"),
        ] {
            let mean = report
                .extended
                .time_contexts
                .iter()
                .find(|r| r.metric_id == id)
                .unwrap();
            let median = report
                .extended
                .time_contexts
                .iter()
                .find(|r| r.metric_id == median_id)
                .unwrap();
            assert!((mean.value.unwrap() - 4.).abs() < 1e-8);
            assert_eq!(median.value, Some(0.));
            assert_eq!(median.sample, 20);
            assert_eq!(median.significant, 4);
            assert_eq!(median.evidence.len(), 20);
            assert_eq!(median.low, None);
        }
    }

    #[test]
    fn phase_filter_does_not_create_complete_prior_windows_across_missing_decisions() {
        let (g, run) = time_fixture("phase", 1);
        let mut report = query::report(
            std::slice::from_ref(&g),
            std::slice::from_ref(&run),
            &StatisticsFilter {
                phase: Some("opening".into()),
                ..Default::default()
            },
        );
        assert_eq!(find(&report, "T07").value, None);
        assert!(
            report
                .extended
                .progress
                .iter()
                .all(|r| r.metric_id != "T10")
        );
        append_extended(&mut report, &[g], &[run]);
        assert!(report.extended.time_contexts.is_empty());
    }
}

#[cfg(test)]
mod additional_regressions {
    use super::*;
    fn game(id: &str) -> StoredGame {
        StoredGame {
            id: id.into(),
            source: "lichess".into(),
            speed: "blitz".into(),
            variant: "standard".into(),
            own_color: "white".into(),
            rated: Some(true),
            initial_seconds: Some(100.),
            increment_seconds: Some(0.),
            result: "1-0".into(),
            eligibility: "eligible".into(),
            started_at_ms: Some(100),
            last_move_at_ms: Some(101),
            white: Participant {
                rating: Some(1500),
                ..Default::default()
            },
            black: Participant {
                rating: Some(1500),
                ..Default::default()
            },
            replay: super::super::chess::replay_pgn(
                "1. e4 e5 2. Nf3 Nc6 3. Bb5 a6 4. Ba4 Nf6 5. O-O Be7 6. Re1 b5 7. Bb3 d6 1-0",
            )
            .unwrap(),
            ..Default::default()
        }
    }
    #[test]
    fn opportunity_progress_weights_events_and_bootstraps_whole_games() {
        let a = game("a");
        let b = game("b");
        let mut q = HashMap::new();
        let event = |kind: &str| DecisionEvent {
            kind: kind.into(),
            phase: "opening".into(),
            ..Default::default()
        };
        let mut m = GameMetrics::default();
        m.events.push(event("opportunity_used"));
        for _ in 0..9 {
            m.events.push(event("opportunity_missed"));
        }
        q.insert(a.id.clone(), m);
        let mut m = GameMetrics::default();
        m.events.push(event("opportunity_used"));
        q.insert(b.id.clone(), m);
        let grouped = conditional_groups(&[&a, &b], &q, "E02", None);
        let rows = grouped.values().next().unwrap();
        assert_eq!(rows.iter().map(|r| r.2).sum::<usize>(), 11);
        assert!((rows.iter().map(|r| r.1).sum::<f64>() / 11. - 200. / 11.).abs() < 1e-8);
        assert!(conditional_groups(&[&a, &b], &q, "E02", Some("endgame")).is_empty());
        let b_rows = vec![("old1".into(), 0., 1), ("old2".into(), 0., 1)];
        let ci = ratio_difference_ci(rows, &b_rows).unwrap();
        assert!(ci.0 <= 200. / 11. && ci.1 >= 200. / 11.);
    }
    #[test]
    fn low_time_episodes_break_on_missing_high_and_nonconsecutive_decisions() {
        let mut g = game("clock");
        let values = [
            Some(9.),
            Some(8.),
            None,
            Some(7.),
            Some(40.),
            Some(9.),
            Some(8.),
        ];
        for (n, value) in g
            .replay
            .nodes
            .iter_mut()
            .filter(|n| n.actor.as_deref() == Some("white"))
            .zip(values)
        {
            n.remaining_before = value;
        }
        let report = query::report(&[g], &[], &StatisticsFilter::default());
        let count = report
            .metrics
            .iter()
            .find(|m| m.id == "T04_episodes_seconds_10")
            .unwrap();
        assert_eq!(count.value, Some(3.));
        assert_eq!(count.eligible, 6);
        let mut lengths: Vec<_> = report
            .extended
            .time_contexts
            .iter()
            .filter(|r| r.id.starts_with("T04_episode:false:10:"))
            .map(|r| r.evidence.len())
            .collect();
        lengths.sort();
        assert_eq!(lengths, vec![1, 2, 2]);
    }
    #[test]
    fn repeated_position_first_visit_is_not_chosen_by_id_in_overlapping_games() {
        let mut a = game("a");
        let mut b = game("b");
        a.last_move_at_ms = Some(1000);
        b.last_move_at_ms = Some(1001);
        for g in [&mut a, &mut b] {
            for n in &mut g.replay.nodes {
                n.spent_seconds = Some(2.);
            }
        }
        let report = query::report(&[a, b], &[], &StatisticsFilter::default());
        assert!(
            report
                .extended
                .progress
                .iter()
                .all(|r| r.metric_id != "T09")
        );
    }
    #[test]
    fn missing_profiles_and_decisions_respect_phase_filter() {
        let mut g = game("phase");
        for n in &mut g.replay.nodes {
            n.phase = if n.ply < 5 { "opening" } else { "middlegame" }.into();
        }
        let report = query::report(
            &[g],
            &[],
            &StatisticsFilter {
                phase: Some("opening".into()),
                ..Default::default()
            },
        );
        let row = report
            .extended
            .coverage
            .iter()
            .find(|r| r.reason == "missing_profile")
            .unwrap();
        assert_eq!(row.decisions, 2);
    }
}

#[cfg(test)]
mod rating_coverage_regression {
    use super::*;
    #[test]
    fn mixed_pools_keep_honest_rating_coverage_and_pool_iqr() {
        let game = |id: &str, speed: &str, rating: Option<i32>, provisional: bool| StoredGame {
            id: id.into(),
            source: "lichess".into(),
            speed: speed.into(),
            variant: "standard".into(),
            own_color: "white".into(),
            result: "1/2-1/2".into(),
            rated: Some(true),
            white: Participant {
                rating: Some(1500),
                ..Default::default()
            },
            black: Participant {
                rating,
                provisional,
                ..Default::default()
            },
            ..Default::default()
        };
        let games = [
            game("a", "blitz", Some(1000), false),
            game("b", "blitz", Some(2000), true),
            game("c", "rapid", None, false),
        ];
        let mut report = StatisticsReport {
            metrics: vec![
                MetricValue {
                    id: "G06".into(),
                    value: Some(1500.),
                    ..Default::default()
                },
                MetricValue {
                    id: "G06_missing".into(),
                    value: Some(100. / 3.),
                    ..Default::default()
                },
                MetricValue {
                    id: "G06_provisional".into(),
                    value: Some(50.),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };
        general_metrics(&mut report, &games.iter().collect::<Vec<_>>());
        let value = |id: &str| report.metrics.iter().find(|m| m.id == id).unwrap().value;
        assert_eq!(value("G06"), None);
        assert!((value("G06_missing").unwrap() - 100. / 3.).abs() < 1e-8);
        assert_eq!(value("G06_provisional"), Some(50.));
        assert_eq!(value("G06_pool_lichess · blitz · standard_iqr"), Some(500.));
        assert_eq!(
            value("G06_pool_lichess · blitz · standard_provisional"),
            Some(50.)
        );
        assert_eq!(
            value("G06_pool_lichess · rapid · standard_missing"),
            Some(100.)
        );
        assert_eq!(
            value("G06_pool_lichess · rapid · standard_provisional"),
            None
        );
    }
}

#[cfg(test)]
mod independent_catalogue_goldens {
    use super::*;
    use crate::statistics::chess;

    #[test]
    fn color_score_difference_uses_personal_pov_and_distinct_color_denominators() {
        let game = |id: &str, color: &str, result: &str| StoredGame {
            id: id.into(),
            own_color: color.into(),
            result: result.into(),
            ..Default::default()
        };
        let games = [
            game("w1", "white", "1-0"),
            game("w2", "white", "1/2-1/2"),
            game("b1", "black", "0-1"),
            game("b2", "black", "1-0"),
            game("b3", "black", "1/2-1/2"),
        ];
        let mut report = StatisticsReport::default();
        general_metrics(&mut report, &games.iter().collect::<Vec<_>>());
        let row = report
            .metrics
            .iter()
            .find(|m| m.id == "G04_difference")
            .unwrap();
        // White 1.5/2=75%; Black 1.5/3=50%; never score Black from White's result.
        assert_eq!(row.value, Some(25.));
        assert_eq!((row.sample, row.eligible), (2, 3));
        assert_eq!(row.game_ids.len(), 5);
        let mut one_color = StatisticsReport::default();
        general_metrics(&mut one_color, &[&games[0], &games[1]]);
        assert_eq!(
            one_color
                .metrics
                .iter()
                .find(|m| m.id == "G04_difference")
                .unwrap()
                .value,
            None
        );
    }
    fn structural_report(fen: &str, moves: &str) -> StatisticsReport {
        let pgn = format!("[FEN \"{fen}\"]\n\n{moves} 1/2-1/2");
        let replay = chess::replay_pgn(&pgn).unwrap();
        let games = ["white", "black"].map(|color| StoredGame {
            id: color.into(),
            own_color: color.into(),
            replay: replay.clone(),
            result: "1/2-1/2".into(),
            raw_pgn: pgn.clone(),
            ..Default::default()
        });
        let games = games.iter().collect::<Vec<_>>();
        let decisions = collect_decisions(&games, &HashMap::new(), &HashMap::new(), None);
        assert_eq!(decisions.len(), 2);
        let mut report = StatisticsReport::default();
        structures(&mut report, &games, &decisions);
        report
    }
    #[test]
    fn material_signature_preserves_opposite_personal_signs_and_visit_denominators() {
        let report = structural_report("2b4k/pp6/8/8/8/8/P3N3/R6K w - - 0 1", "1. Rf1 a6");
        let rows = report
            .extended
            .structures
            .iter()
            .filter(|r| r.metric_id == "S09")
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 2);
        // White P+N+R=9, Black 2P+B=5. Same board must stay +4/-4 by account POV.
        for (color, sign) in [("white", "+4"), ("black", "-4")] {
            let row = rows.iter().find(|r| r.id.starts_with(color)).unwrap();
            assert!(row.id.contains("[1, 1, 0, 1, 0]/[2, 0, 1, 0, 0]"));
            assert!(row.label.contains(&format!("баланс {sign}")));
            assert_eq!((row.games, row.visits, row.decisions), (1, 1, 2));
            assert_eq!(row.share, Some(50.));
            assert_eq!(row.evidence.len(), 1);
            assert_eq!(row.evidence[0].game_id, color);
        }
    }
    #[test]
    fn open_and_half_open_files_use_both_actual_colors_and_root_visits() {
        let report = structural_report("7k/1pp2p2/8/8/8/8/P1P5/7K w - - 0 1", "1. Kg1 Kg8");
        let rows = report
            .extended
            .structures
            .iter()
            .filter(|r| r.metric_id == "S11")
            .collect::<Vec<_>>();
        // Pawn files: White a,c; Black b,c,f. Open d,e,g,h; half-open White b,f / Black a.
        for id in [
            "open_count:4",
            "half_count:white:2",
            "half_count:black:1",
            "half:white:1",
            "half:white:5",
            "half:black:0",
        ] {
            let row = rows.iter().find(|r| r.id == id).unwrap();
            assert_eq!((row.games, row.visits, row.decisions), (2, 2, 2));
            assert_eq!(row.share, Some(100.));
        }
        let open = rows
            .iter()
            .filter(|r| r.id.starts_with("open:"))
            .map(|r| r.id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            open,
            BTreeSet::from(["open:3", "open:4", "open:6", "open:7"])
        );
        assert!(
            !rows
                .iter()
                .any(|r| r.id == "half:white:2" || r.id == "half:black:2")
        );
    }
}

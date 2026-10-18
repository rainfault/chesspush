//! Bounded IPC views of full, computed statistics. Aggregates never depend on this projection.
use super::{
    opening_metrics::OpeningQuality,
    types::{DecisionEvent, StatisticsReport},
};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const PREVIEW_LIMIT: usize = 20;
pub const PAGE_LIMIT: usize = 100;

/// Daily workspace projection of the same calculated snapshot. Full analytics
/// stay in the report cache and are requested explicitly by detailed screens.
/// Keep complete game summaries: their outcomes are the overview denominator.
pub fn practice(report: StatisticsReport) -> StatisticsReport {
    StatisticsReport {
        evidence_revision: report.evidence_revision,
        filter: report.filter,
        generated_at_ms: report.generated_at_ms,
        total_games: report.total_games,
        excluded_games: report.excluded_games,
        total_decisions: report.total_decisions,
        analysed_decisions: report.analysed_decisions,
        profile_ids: report.profile_ids,
        profile_labels: report.profile_labels,
        selected_profile_id: report.selected_profile_id,
        games: report.games,
        events: report.events,
        ..Default::default()
    }
}

fn count_and_cap<T>(
    values: &mut Vec<T>,
    counts: &mut BTreeMap<String, usize>,
    kind: &str,
    key: &str,
) {
    counts
        .entry(format!("{kind}:{key}"))
        .and_modify(|count| *count = (*count).max(values.len()))
        .or_insert(values.len());
    values.truncate(PREVIEW_LIMIT);
}
fn event_in_subset(event: &DecisionEvent, key: &str) -> bool {
    match key {
        "all" => true,
        "time" => event.clock_seconds.is_some() || event.spent_seconds.is_some(),
        "endings" => event.phase == "endgame" || event.kind != "decision",
        _ => false,
    }
}
fn quality_keys(base: &str, previous: bool) -> String {
    format!("{base}::{}quality", if previous { "previous_" } else { "" })
}
fn compact_quality(
    quality: &mut OpeningQuality,
    counts: &mut BTreeMap<String, usize>,
    kind: &str,
    base: &str,
    previous: bool,
) {
    let key = quality_keys(base, previous);
    count_and_cap(&mut quality.cases, counts, kind, &key);
    for row in &mut quality.time_games {
        count_and_cap(
            &mut row.cases,
            counts,
            kind,
            &format!("{key}_time:{}", row.game_id),
        );
    }
}

/// Keep a small preview of each evidence list while preserving complete counts and aggregates.
/// `query::report` remains unprojected; detail pages must be requested from that full report.
pub fn compact(report: &mut StatisticsReport) {
    let counts = &mut report.evidence_counts;
    for node in &mut report.opening_analysis.hierarchy {
        count_and_cap(&mut node.cases, counts, "hierarchy", &node.id);
        compact_quality(&mut node.quality, counts, "hierarchy", &node.id, false);
        compact_quality(
            &mut node.previous_quality,
            counts,
            "hierarchy",
            &node.id,
            true,
        );
    }
    for edge in &mut report.opening_tree.edges {
        count_and_cap(&mut edge.cases, counts, "edge", &edge.uci);
        compact_quality(&mut edge.quality, counts, "edge", &edge.uci, false);
        count_and_cap(
            &mut edge.temporal.previous_cases,
            counts,
            "edge",
            &format!("{}::previous", edge.uci),
        );
    }
    for context in &mut report.opening_analysis.node.contexts {
        count_and_cap(&mut context.cases, counts, "node", &context.context_key);
    }
    for problem in &mut report.problems {
        count_and_cap(
            &mut problem.cases,
            counts,
            "problem",
            &format!("{}|{}", problem.position_key, problem.played_uci),
        );
    }
    for boundary in &mut report.opening_analysis.boundaries {
        count_and_cap(&mut boundary.cases, counts, "boundary", &boundary.kind);
    }
    count_and_cap(
        &mut report.opening_analysis.first_losses,
        counts,
        "first_losses",
        "all",
    );
    for response in &mut report.opening_analysis.opponent_responses {
        // Each response window has at most its configured, finite decision window.
        count_and_cap(
            &mut response.cases,
            counts,
            "response",
            &format!("{}|{}", response.position_key, response.played_uci),
        );
    }
    let mut by_metric = BTreeMap::<&str, usize>::new();
    for case in &report.analysis.cases {
        *by_metric.entry(&case.metric_id).or_default() += 1;
    }
    for (metric, count) in by_metric {
        counts
            .entry(format!("engine:{metric}"))
            .and_modify(|old| *old = (*old).max(count))
            .or_insert(count);
    }
    count_and_cap(&mut report.analysis.cases, counts, "engine", "all");
    let mut by_run = BTreeMap::<&str, usize>::new();
    for point in &report.analysis.trajectory {
        *by_run.entry(&point.run_id).or_default() += 1;
    }
    for (run, count) in by_run {
        counts
            .entry(format!("trajectory:{run}"))
            .and_modify(|old| *old = (*old).max(count))
            .or_insert(count);
    }
    count_and_cap(&mut report.analysis.trajectory, counts, "trajectory", "all");
    count_and_cap(&mut report.analysis.episodes, counts, "episodes", "all");
    for key in ["time", "endings"] {
        let count = report
            .events
            .iter()
            .filter(|e| event_in_subset(e, key))
            .count();
        counts
            .entry(format!("events:{key}"))
            .and_modify(|old| *old = (*old).max(count))
            .or_insert(count);
    }
    count_and_cap(&mut report.events, counts, "events", "all");
    for row in &mut report.extended.time_contexts {
        count_and_cap(&mut row.evidence, counts, "context", &row.id);
    }
    for row in &mut report.extended.structures {
        count_and_cap(&mut row.evidence, counts, "structure", &row.id);
    }
    for row in &mut report.extended.progress {
        count_and_cap(&mut row.evidence, counts, "progress", &row.id);
    }
    for gap in [15, 30, 60] {
        let count = report
            .extended
            .sessions
            .iter()
            .filter(|row| row.gap_minutes == gap)
            .count();
        counts
            .entry(format!("sessions:{gap}"))
            .and_modify(|old| *old = (*old).max(count))
            .or_insert(count);
    }
    count_and_cap(&mut report.extended.sessions, counts, "sessions", "all");
    count_and_cap(&mut report.extended.streaks, counts, "streaks", "all");
}

fn slice_page<T: Serialize>(values: &[T], offset: usize, limit: usize) -> Result<Value, String> {
    let offset = offset.min(values.len());
    let limit = limit.clamp(1, PAGE_LIMIT);
    let end = offset.saturating_add(limit).min(values.len());
    Ok(json!({
        "items": serde_json::to_value(&values[offset..end]).map_err(|e| e.to_string())?,
        "total": values.len(),
        "offset": offset,
        "limit": limit,
    }))
}
fn quality_page(
    quality: &OpeningQuality,
    base: &str,
    previous: bool,
    key: &str,
    offset: usize,
    limit: usize,
) -> Option<Result<Value, String>> {
    let quality_key = quality_keys(base, previous);
    if key == quality_key {
        return Some(slice_page(&quality.cases, offset, limit));
    }
    let prefix = format!("{quality_key}_time:");
    key.strip_prefix(&prefix).map(|game_id| {
        quality
            .time_games
            .iter()
            .find(|row| row.game_id == game_id)
            .ok_or_else(|| "Не найдена партия в дебютной временной выборке".to_string())
            .and_then(|row| slice_page(&row.cases, offset, limit))
    })
}
fn missing() -> Result<Value, String> {
    Err("Не найдена выборка подтверждающих партий или ходов".into())
}

/// Return original, typed cases as JSON. Call with a full report, never its compact IPC preview.
/// Extra hierarchy/edge quality keys are `<base>::quality`, `<base>::previous_quality`,
/// and `<base>::[previous_]quality_time:<game_id>`; edge history is `<uci>::previous`.
pub fn page(
    report: &StatisticsReport,
    kind: &str,
    key: &str,
    offset: usize,
    limit: usize,
) -> Result<Value, String> {
    // A preview cannot provide a trustworthy complete page. The desktop command rebuilds full data.
    if !report.evidence_counts.is_empty() {
        return Err("Для страницы подтверждений требуется полный отчёт".into());
    }
    match kind {
        "hierarchy" => {
            for node in &report.opening_analysis.hierarchy {
                if node.id == key {
                    return slice_page(&node.cases, offset, limit);
                }
                if let Some(page) = quality_page(&node.quality, &node.id, false, key, offset, limit)
                {
                    return page;
                }
                if let Some(page) =
                    quality_page(&node.previous_quality, &node.id, true, key, offset, limit)
                {
                    return page;
                }
            }
            missing()
        }
        "edge" => {
            for edge in &report.opening_tree.edges {
                if edge.uci == key {
                    return slice_page(&edge.cases, offset, limit);
                }
                if key == format!("{}::previous", edge.uci) {
                    return slice_page(&edge.temporal.previous_cases, offset, limit);
                }
                if let Some(page) =
                    quality_page(&edge.quality, &edge.uci, false, key, offset, limit)
                {
                    return page;
                }
            }
            missing()
        }
        "node" => report
            .opening_analysis
            .node
            .contexts
            .iter()
            .find(|row| row.context_key == key)
            .map(|row| slice_page(&row.cases, offset, limit))
            .unwrap_or_else(missing),
        "problem" => report
            .problems
            .iter()
            .find(|row| format!("{}|{}", row.position_key, row.played_uci) == key)
            .map(|row| slice_page(&row.cases, offset, limit))
            .unwrap_or_else(missing),
        "boundary" => report
            .opening_analysis
            .boundaries
            .iter()
            .find(|row| row.kind == key)
            .map(|row| slice_page(&row.cases, offset, limit))
            .unwrap_or_else(missing),
        "first_losses" if key == "all" => {
            slice_page(&report.opening_analysis.first_losses, offset, limit)
        }
        "response" => report
            .opening_analysis
            .opponent_responses
            .iter()
            .find(|row| format!("{}|{}", row.position_key, row.played_uci) == key)
            .map(|row| slice_page(&row.cases, offset, limit))
            .unwrap_or_else(missing),
        "engine" if key == "all" => slice_page(&report.analysis.cases, offset, limit),
        "engine" => {
            let cases: Vec<_> = report
                .analysis
                .cases
                .iter()
                .filter(|row| row.metric_id == key)
                .collect();
            if cases.is_empty() && !report.metrics.iter().any(|row| row.id == key) {
                missing()
            } else {
                slice_page(&cases, offset, limit)
            }
        }
        "trajectory" if key == "all" => slice_page(&report.analysis.trajectory, offset, limit),
        "trajectory" => {
            let points: Vec<_> = report
                .analysis
                .trajectory
                .iter()
                .filter(|row| row.run_id == key)
                .collect();
            if points.is_empty() {
                missing()
            } else {
                slice_page(&points, offset, limit)
            }
        }
        "context" => report
            .extended
            .time_contexts
            .iter()
            .find(|row| row.id == key)
            .map(|row| slice_page(&row.evidence, offset, limit))
            .unwrap_or_else(missing),
        "structure" => report
            .extended
            .structures
            .iter()
            .find(|row| row.id == key)
            .map(|row| slice_page(&row.evidence, offset, limit))
            .unwrap_or_else(missing),
        "progress" => report
            .extended
            .progress
            .iter()
            .find(|row| row.id == key)
            .map(|row| slice_page(&row.evidence, offset, limit))
            .unwrap_or_else(missing),
        "sessions" if key == "all" => slice_page(&report.extended.sessions, offset, limit),
        "sessions" if ["15", "30", "60"].contains(&key) => {
            let gap = key.parse::<u32>().expect("validated session gap");
            let sessions: Vec<_> = report
                .extended
                .sessions
                .iter()
                .filter(|row| row.gap_minutes == gap)
                .collect();
            slice_page(&sessions, offset, limit)
        }
        "streaks" if key == "all" => slice_page(&report.extended.streaks, offset, limit),
        "episodes" if key == "all" => slice_page(&report.analysis.episodes, offset, limit),
        "events" if ["all", "time", "endings"].contains(&key) => {
            let events: Vec<_> = report
                .events
                .iter()
                .filter(|e| event_in_subset(e, key))
                .collect();
            slice_page(&events, offset, limit)
        }
        _ => missing(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statistics::{engine_metrics::*, extended::*, opening_metrics::*, types::*};

    const N: usize = 37;
    fn cases() -> Vec<OpeningCase> {
        (0..N)
            .map(|i| OpeningCase {
                game_id: format!("game-{i}"),
                ply: i as u32 + 1,
                fen: format!("fen-{i}"),
                run_id: Some(format!("run-{}", i % 2)),
                ..Default::default()
            })
            .collect()
    }
    fn quality() -> OpeningQuality {
        OpeningQuality {
            decisions: 1234,
            mean_loss: Some(12.5),
            time_games: (0..N)
                .map(|i| OpeningTimeGame {
                    game_id: format!("game-{i}"),
                    decisions: N,
                    known_seconds: Some(100.),
                    cases: cases(),
                    ..Default::default()
                })
                .collect(),
            cases: cases(),
            ..Default::default()
        }
    }
    fn fixture() -> StatisticsReport {
        let evidence: Vec<_> = (0..N)
            .map(|i| MoveEvidence {
                game_id: format!("game-{i}"),
                ply: i as u32 + 1,
                run_id: Some("run-0".into()),
                fen: format!("fen-{i}"),
            })
            .collect();
        StatisticsReport {
            evidence_revision: "snapshot-123".into(),
            total_games: 999,
            metrics: vec![MetricValue {
                id: "Q00".into(),
                value: Some(12.5),
                sample: 1234,
                eligible: 4321,
                game_ids: (0..N).map(|i| format!("game-{i}")).collect(),
                ..Default::default()
            }],
            games: (0..N)
                .map(|i| GameSummary {
                    id: format!("game-{i}"),
                    ..Default::default()
                })
                .collect(),
            opening_tree: OpeningTree {
                edges: vec![OpeningEdge {
                    uci: "e2e4".into(),
                    visits: 1234,
                    cases: cases(),
                    quality: quality(),
                    temporal: OpeningTemporal {
                        previous_cases: cases(),
                        ..Default::default()
                    },
                    ..Default::default()
                }],
                ..Default::default()
            },
            opening_analysis: OpeningAnalysis {
                hierarchy: vec![OpeningHierarchyNode {
                    id: "family".into(),
                    cases: cases(),
                    quality: quality(),
                    previous_quality: quality(),
                    result: ResultRow {
                        game_ids: (0..N).map(|i| format!("game-{i}")).collect(),
                        ..Default::default()
                    },
                    ..Default::default()
                }],
                node: OpeningNodeAnalysis {
                    contexts: vec![OpeningPositionContext {
                        context_key: "context-key".into(),
                        cases: cases(),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                boundaries: vec![OpeningBoundarySummary {
                    kind: "phase".into(),
                    eligible: 1234,
                    cases: cases()
                        .into_iter()
                        .map(|c| OpeningBoundary {
                            game_id: c.game_id.clone(),
                            case: Some(c),
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                }],
                first_losses: cases()
                    .into_iter()
                    .map(|c| OpeningFirstLoss {
                        game_id: c.game_id.clone(),
                        case: Some(c),
                        ..Default::default()
                    })
                    .collect(),
                opponent_responses: vec![OpeningOpponentResponse {
                    position_key: "root".into(),
                    played_uci: "e7e5".into(),
                    visits: 1234,
                    cases: cases()
                        .into_iter()
                        .map(|c| OpeningResponseWindow {
                            response: c.clone(),
                            decisions: vec![c.clone(), c.clone(), c],
                            ..Default::default()
                        })
                        .collect(),
                    ..Default::default()
                }],
                ..Default::default()
            },
            problems: vec![OpeningProblem {
                position_key: "root".into(),
                played_uci: "e2e4".into(),
                cases: cases(),
                occurrences: 1234,
                ..Default::default()
            }],
            analysis: EngineDetails {
                cases: (0..N)
                    .map(|i| AnalysisCase {
                        metric_id: format!("Q0{}", i % 2),
                        game_id: format!("game-{i}"),
                        ply: i as u32,
                        ..Default::default()
                    })
                    .collect(),
                trajectory: (0..N)
                    .map(|i| TrajectoryPoint {
                        game_id: format!("game-{i}"),
                        run_id: format!("run-{}", i % 2),
                        ply: i as u32,
                        ..Default::default()
                    })
                    .collect(),
                episodes: (0..N)
                    .map(|i| Episode {
                        game_id: format!("game-{i}"),
                        start_ply: i as u32,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
            events: (0..N)
                .map(|i| DecisionEvent {
                    game_id: format!("game-{i}"),
                    ply: i as u32,
                    ..Default::default()
                })
                .collect(),
            extended: ExtendedReport {
                time_contexts: vec![ContextRow {
                    id: "time".into(),
                    sample: 1234,
                    evidence: evidence.clone(),
                    ..Default::default()
                }],
                structures: vec![StructureRow {
                    id: "shape".into(),
                    visits: 1234,
                    evidence: evidence.clone(),
                    ..Default::default()
                }],
                progress: vec![ProgressRow {
                    id: "period".into(),
                    current_sample: 1234,
                    evidence,
                    ..Default::default()
                }],
                sessions: (0..N)
                    .map(|i| SessionRow {
                        id: format!("session-{i}"),
                        games: 1234,
                        game_ids: (0..N).map(|j| format!("game-{j}")).collect(),
                        ..Default::default()
                    })
                    .collect(),
                streaks: (0..N)
                    .map(|i| StreakRow {
                        id: format!("streak-{i}"),
                        length: 1234,
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            },
            ..Default::default()
        }
    }
    fn all_pages(report: &StatisticsReport, kind: &str, key: &str) -> Vec<Value> {
        let mut result = vec![];
        loop {
            let p = page(report, kind, key, result.len(), 7).unwrap();
            result.extend(p["items"].as_array().unwrap().iter().cloned());
            if result.len() == p["total"].as_u64().unwrap() as usize {
                break;
            }
        }
        result
    }
    #[test]
    fn event_subsets_keep_zero_clock_values_counts_and_pages_in_the_same_domain() {
        let mut report = StatisticsReport::default();
        for block in 0..5 {
            for (index, (phase, kind, clock, spent)) in [
                ("opening", "decision", None, None),
                ("opening", "decision", Some(0.), None),
                ("middlegame", "decision", None, Some(0.)),
                ("endgame", "decision", None, None),
                ("opening", "opportunity", None, None),
                ("endgame", "decision", Some(12.), Some(5.)),
                ("middlegame", "significant", None, None),
            ]
            .into_iter()
            .enumerate()
            {
                report.events.push(DecisionEvent {
                    game_id: format!("game-{block}-{index}"),
                    ply: index as u32 + 1,
                    run_id: format!("run-{block}"),
                    phase: phase.into(),
                    kind: kind.into(),
                    clock_seconds: clock,
                    spent_seconds: spent,
                    ..Default::default()
                });
            }
        }
        for (key, indices) in [("time", vec![1, 2, 5]), ("endings", vec![3, 4, 5, 6])] {
            let actual = all_pages(&report, "events", key);
            let expected: Vec<_> = (0..5)
                .flat_map(|block| {
                    indices
                        .iter()
                        .map(move |index| format!("game-{block}-{index}"))
                })
                .collect();
            assert_eq!(
                actual
                    .iter()
                    .map(|v| v["game_id"].as_str().unwrap().to_owned())
                    .collect::<Vec<_>>(),
                expected
            );
            assert!(
                actual
                    .iter()
                    .all(|v| v["run_id"].as_str().unwrap().starts_with("run-"))
            );
        }
        compact(&mut report);
        assert_eq!(report.events.len(), PREVIEW_LIMIT);
        assert_eq!(report.evidence_counts["events:all"], 35);
        assert_eq!(report.evidence_counts["events:time"], 15);
        assert_eq!(report.evidence_counts["events:endings"], 20);
        compact(&mut report);
        assert_eq!(report.evidence_counts["events:time"], 15);
        assert_eq!(report.evidence_counts["events:endings"], 20);
        assert!(page(&StatisticsReport::default(), "events", "unrecognized", 0, 7).is_err());
    }
    #[test]
    fn session_gap_pages_do_not_depend_on_the_global_preview_prefix() {
        let mut report = StatisticsReport::default();
        for gap in [15, 30, 60] {
            for index in 0..37 {
                report.extended.sessions.push(SessionRow {
                    id: format!("session-{gap}-{index}"),
                    gap_minutes: gap,
                    games: index + 1,
                    game_ids: vec![format!("game-{gap}-{index}")],
                    ..Default::default()
                });
            }
        }
        for gap in [15, 30, 60] {
            let items = all_pages(&report, "sessions", &gap.to_string());
            assert_eq!(items.len(), 37);
            for (i, item) in items.iter().enumerate() {
                assert_eq!(item["id"], format!("session-{gap}-{i}"));
                assert_eq!(item["games"], i + 1);
                assert_eq!(item["game_ids"][0], format!("game-{gap}-{i}"));
            }
        }
        compact(&mut report);
        assert_eq!(report.extended.sessions.len(), PREVIEW_LIMIT);
        assert!(
            report
                .extended
                .sessions
                .iter()
                .all(|row| row.gap_minutes == 15)
        );
        for gap in [15, 30, 60] {
            assert_eq!(report.evidence_counts[&format!("sessions:{gap}")], 37);
        }
        assert_eq!(report.evidence_counts["sessions:all"], 111);
        compact(&mut report);
        assert_eq!(report.evidence_counts["sessions:30"], 37);
        assert!(page(&StatisticsReport::default(), "sessions", "45", 0, 10).is_err());
    }
    #[test]
    fn practice_keeps_snapshot_outcomes_profiles_and_event_evidence() {
        let full = fixture();
        let before = serde_json::to_value(&full).unwrap();
        let mut daily = practice(full.clone());
        for key in [
            "evidence_revision",
            "filter",
            "total_games",
            "excluded_games",
            "total_decisions",
            "analysed_decisions",
            "games",
            "events",
            "profile_ids",
            "profile_labels",
            "selected_profile_id",
        ] {
            assert_eq!(
                serde_json::to_value(&daily).unwrap()[key],
                before[key],
                "{key}"
            );
        }
        assert!(daily.opening_analysis.hierarchy.is_empty());
        assert!(daily.analysis.cases.is_empty());
        assert!(daily.opening_tree.edges.is_empty());
        compact(&mut daily);
        assert_eq!(daily.events.len(), PREVIEW_LIMIT);
        assert_eq!(daily.evidence_counts["events:all"], N);
        assert_eq!(daily.games.len(), full.games.len());
        assert_eq!(serde_json::to_value(&full).unwrap(), before);
        // Paging still resolves against the unprojected cached snapshot.
        assert_eq!(
            page(&full, "events", "all", 20, PAGE_LIMIT).unwrap()["total"],
            N
        );
        assert_eq!(
            page(&full, "hierarchy", "family", 0, PAGE_LIMIT).unwrap()["total"],
            N
        );
    }

    #[test]
    fn all_evidence_kinds_page_complete_original_cases() {
        let report = fixture();
        for (kind, key) in [
            ("hierarchy", "family"),
            ("hierarchy", "family::quality"),
            ("hierarchy", "family::previous_quality"),
            ("hierarchy", "family::quality_time:game-0"),
            ("hierarchy", "family::previous_quality_time:game-0"),
            ("edge", "e2e4"),
            ("edge", "e2e4::quality"),
            ("edge", "e2e4::quality_time:game-0"),
            ("edge", "e2e4::previous"),
            ("node", "context-key"),
            ("problem", "root|e2e4"),
            ("boundary", "phase"),
            ("first_losses", "all"),
            ("response", "root|e7e5"),
            ("engine", "all"),
            ("trajectory", "all"),
            ("context", "time"),
            ("structure", "shape"),
            ("progress", "period"),
            ("sessions", "all"),
            ("streaks", "all"),
            ("episodes", "all"),
            ("events", "all"),
        ] {
            let values = all_pages(&report, kind, key);
            assert_eq!(values.len(), N, "{kind}:{key}");
            let original = page(&report, kind, key, 0, PAGE_LIMIT).unwrap();
            assert_eq!(
                values,
                *original["items"].as_array().unwrap(),
                "{kind}:{key}"
            );
            let first = &values[0];
            let last = &values[N - 1];
            let field = if kind == "response" { "response" } else { "" };
            if field.is_empty() && first.get("game_id").is_some() {
                assert_eq!(first["game_id"], "game-0");
                assert_eq!(last["game_id"], format!("game-{}", N - 1));
            }
            if kind == "response" {
                assert_eq!(first["response"]["game_id"], "game-0");
                assert_eq!(first["decisions"].as_array().unwrap().len(), 3);
            }
        }
        let metric = all_pages(&report, "engine", "Q01");
        assert_eq!(metric.len(), N / 2);
        assert!(metric.iter().all(|v| v["metric_id"] == "Q01"));
        let run = all_pages(&report, "trajectory", "run-0");
        assert_eq!(run.len(), N.div_ceil(2));
        assert!(run.iter().all(|v| v["run_id"] == "run-0"));
        assert_eq!(report.evidence_counts.len(), 0);
    }
    #[test]
    fn compact_preserves_counts_aggregates_and_all_game_ids() {
        let mut report = fixture();
        compact(&mut report);
        assert_eq!(report.total_games, 999);
        assert_eq!(report.evidence_revision, "snapshot-123");
        assert_eq!(report.metrics[0].value, Some(12.5));
        assert_eq!(report.metrics[0].sample, 1234);
        assert_eq!(report.metrics[0].eligible, 4321);
        assert_eq!(report.metrics[0].game_ids.len(), N);
        assert_eq!(report.games.len(), N);
        assert_eq!(report.opening_tree.edges.len(), 1);
        assert_eq!(report.opening_tree.edges[0].visits, 1234);
        let hierarchy = &report.opening_analysis.hierarchy[0];
        assert_eq!(hierarchy.result.game_ids.len(), N);
        assert_eq!(hierarchy.quality.decisions, 1234);
        assert_eq!(hierarchy.quality.mean_loss, Some(12.5));
        assert_eq!(hierarchy.quality.time_games.len(), N);
        assert_eq!(hierarchy.quality.time_games[0].decisions, N);
        assert_eq!(hierarchy.quality.time_games[0].cases.len(), PREVIEW_LIMIT);
        assert_eq!(
            report.opening_analysis.opponent_responses[0].cases[0]
                .decisions
                .len(),
            3
        );
        assert_eq!(report.extended.time_contexts[0].sample, 1234);
        assert_eq!(report.extended.structures[0].visits, 1234);
        assert_eq!(report.extended.progress[0].current_sample, 1234);
        assert_eq!(report.extended.sessions[0].game_ids.len(), N);
        for (key, count) in &report.evidence_counts {
            assert!(key.contains(':'));
            assert_eq!(
                *count,
                // The generic fixture has no clocks and gap_minutes=0; these
                // filtered subsets are empty while events:all/endings stay N.
                if matches!(
                    key.as_str(),
                    "events:time" | "sessions:15" | "sessions:30" | "sessions:60"
                ) {
                    0
                } else if key == "engine:Q01" {
                    N / 2
                } else if key == "engine:Q00" || key == "trajectory:run-0" {
                    N.div_ceil(2)
                } else if key == "trajectory:run-1" {
                    N / 2
                } else {
                    N
                },
                "{key}"
            );
        }
        assert_eq!(report.events.len(), PREVIEW_LIMIT);
        assert_eq!(report.analysis.cases.len(), PREVIEW_LIMIT);
        assert_eq!(report.analysis.trajectory.len(), PREVIEW_LIMIT);
        assert_eq!(report.analysis.episodes.len(), PREVIEW_LIMIT);
        assert_eq!(report.extended.sessions.len(), PREVIEW_LIMIT);
        assert_eq!(report.extended.streaks.len(), PREVIEW_LIMIT);
        let first = serde_json::to_value(&report).unwrap();
        compact(&mut report);
        assert_eq!(serde_json::to_value(&report).unwrap(), first);
        assert!(page(&report, "events", "all", 0, 20).is_err());
    }
    #[test]
    fn pages_clamp_limits_offsets_and_reject_unknown_keys() {
        let report = fixture();
        let zero = page(&report, "events", "all", 0, 0).unwrap();
        assert_eq!(zero["limit"], 1);
        assert_eq!(zero["items"].as_array().unwrap().len(), 1);
        let huge = page(&report, "events", "all", usize::MAX, usize::MAX).unwrap();
        assert_eq!(huge["limit"], PAGE_LIMIT);
        assert_eq!(huge["offset"], N);
        assert!(huge["items"].as_array().unwrap().is_empty());
        for (kind, key) in [
            ("unknown", "all"),
            ("events", "unknown"),
            ("hierarchy", "missing"),
            ("hierarchy", "family::quality_time:missing"),
            ("engine", "unknown"),
            ("trajectory", "unknown"),
            ("structure", "unknown"),
        ] {
            assert!(page(&report, kind, key, 0, 20).is_err(), "{kind}:{key}");
        }
        let empty = StatisticsReport::default();
        assert_eq!(page(&empty, "engine", "all", 0, 20).unwrap()["total"], 0);
    }
}

#[cfg(test)]
mod tail_count_regression {
    use super::*;
    use crate::statistics::engine_metrics::AnalysisCase;
    #[test]
    fn engine_metric_counts_include_cases_beyond_preview() {
        let mut report = StatisticsReport::default();
        report.analysis.cases = (0..25)
            .map(|i| AnalysisCase {
                metric_id: if i < 20 { "Q01" } else { "Q99" }.into(),
                ..Default::default()
            })
            .collect();
        compact(&mut report);
        assert_eq!(report.evidence_counts["engine:all"], 25);
        assert_eq!(report.evidence_counts["engine:Q99"], 5);
        assert!(
            !report
                .analysis
                .cases
                .iter()
                .any(|case| case.metric_id == "Q99")
        );
    }
}

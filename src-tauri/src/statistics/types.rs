use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Participant {
    pub id: Option<String>,
    pub name: String,
    pub rating: Option<i32>,
    pub rating_diff: Option<i32>,
    pub provisional: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MaterialSnapshot {
    /// P, N, B, R, Q for each side. Kings excluded.
    pub white: [u8; 5],
    pub black: [u8; 5],
    pub balance: i32,
    pub ending: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ReplayNode {
    pub ply: u32,
    pub absolute_ply: u32,
    pub fen: String,
    pub position_key: String,
    pub uci: Option<String>,
    pub san: Option<String>,
    pub actor: Option<String>,
    pub phase: String,
    pub clock_seconds: Option<f64>,
    pub remaining_before: Option<f64>,
    pub spent_seconds: Option<f64>,
    pub clock_quality: String,
    pub comment: String,
    pub material: MaterialSnapshot,
    pub legal_moves: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningMatch {
    pub eco: String,
    pub name: String,
    pub family: String,
    pub variation: String,
    pub ply: u32,
    pub position_key: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Replay {
    #[serde(default)]
    pub source_result: Option<String>,
    pub initial_fen: String,
    pub nodes: Vec<ReplayNode>,
    pub middle_ply: Option<u32>,
    pub end_ply: Option<u32>,
    pub phase_version: String,
    pub replay_version: String,
    pub opening: Option<OpeningMatch>,
    pub catalogue_version: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredGame {
    pub id: String,
    pub external_id: String,
    pub account_id: String,
    pub source: String,
    pub started_at_ms: Option<i64>,
    pub last_move_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub end_time_quality: String,
    pub white: Participant,
    pub black: Participant,
    pub own_color: String,
    pub result: String,
    pub status: String,
    pub speed: String,
    pub rated: Option<bool>,
    pub variant: String,
    pub initial_seconds: Option<f64>,
    pub increment_seconds: Option<f64>,
    pub raw_pgn: String,
    pub source_metadata: Value,
    pub eligibility: String,
    pub replay: Replay,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PositionEvaluation {
    pub ply: u32,
    pub cp: Option<i32>,
    pub mate: Option<i32>,
    pub wdl: Option<[u32; 3]>,
    pub best_move: Option<String>,
    pub pv: Vec<String>,
    pub depth: u32,
    pub nodes: u64,
    pub bound: Option<String>,
    pub seldepth: Option<u32>,
    pub score_kind: String,
    pub root_pov: String,
    pub terminal_outcome: Option<String>,
    pub complete: bool,
    #[serde(default)]
    pub time_ms: u64,
    #[serde(default)]
    pub stop_reason: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoveComparison {
    pub ply: u32,
    pub best: PositionEvaluation,
    pub played: PositionEvaluation,
    pub complete: bool,
    pub method: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EngineRun {
    pub id: String,
    pub game_id: String,
    pub profile: String,
    pub profile_id: String,
    pub engine_version: String,
    pub engine_binary_hash: String,
    pub revision: String,
    pub settings_json: String,
    pub completed_at_ms: i64,
    pub evaluations: Vec<PositionEvaluation>,
    pub confirmed_moves: Vec<MoveComparison>,
    #[serde(default)]
    pub tablebases: Vec<super::engine_metrics::TablebaseObservation>,
    #[serde(default)]
    pub mate_rechecks: Vec<MoveComparison>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatisticsFilter {
    pub account_id: Option<String>,
    pub since_ms: Option<i64>,
    pub until_ms: Option<i64>,
    pub speed: Option<String>,
    pub color: Option<String>,
    pub opening: Option<String>,
    pub control: Option<String>,
    pub variant: Option<String>,
    pub phase: Option<String>,
    pub opponent_rating_min: Option<i32>,
    pub opponent_rating_max: Option<i32>,
    pub source: Option<String>,
    pub rated: Option<bool>,
    pub profile_id: Option<String>,
    pub include_uncertain: Option<bool>,
    pub game_ids: Option<Vec<String>>,
    pub position_key: Option<String>,
    pub line: Option<Vec<String>>,
    pub tree_mode: Option<String>,
    pub trend_granularity: Option<String>,
    pub opening_actor: Option<String>,
    pub opening_initial_fen: Option<String>,
    pub opening_label_id: Option<String>,
    pub comparison_since_ms: Option<i64>,
    pub comparison_until_ms: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetricValue {
    pub id: String,
    pub name: String,
    pub unit: String,
    pub value: Option<f64>,
    pub sample: usize,
    pub eligible: usize,
    pub definition: String,
    pub method: String,
    pub low: Option<f64>,
    pub high: Option<f64>,
    pub game_ids: Vec<String>,
    pub status: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ResultRow {
    pub key: String,
    pub label: String,
    pub games: usize,
    pub wins: usize,
    pub draws: usize,
    pub losses: usize,
    pub score: Option<f64>,
    pub game_ids: Vec<String>,
    pub mean_loss: Option<f64>,
    pub mean_accuracy: Option<f64>,
    pub analysed: usize,
    pub opponent_rating_median: Option<f64>,
    pub opponent_rating_p25: Option<f64>,
    pub opponent_rating_p75: Option<f64>,
    pub rating_sample: usize,
    pub provisional_opponents: usize,
    pub score_low: Option<f64>,
    pub score_high: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningRow {
    pub eco: String,
    pub name: String,
    pub family: String,
    pub variation: String,
    pub result: ResultRow,
    pub exit_q: Option<f64>,
    pub exit_sample: usize,
    pub first_ply: Option<u32>,
    pub position_key: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningEdge {
    pub actor: String,
    pub cases: Vec<super::opening_metrics::OpeningCase>,
    pub quality: super::opening_metrics::OpeningQuality,
    pub temporal: super::opening_metrics::OpeningTemporal,
    pub spent_median: Option<f64>,
    pub spent_sample: usize,
    pub uci: String,
    pub san: String,
    pub fen: String,
    pub position_key: String,
    pub result: ResultRow,
    pub frequency: f64,
    pub visits: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningTree {
    pub initial_fen: String,
    pub distinct_paths: usize,
    pub unplayed: Vec<LegalContinuation>,
    pub mode: String,
    pub fen: String,
    pub position_key: String,
    pub line: Vec<String>,
    pub games: usize,
    pub visits: usize,
    pub edges: Vec<OpeningEdge>,
    pub game_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DecisionEvent {
    pub game_id: String,
    pub ply: u32,
    pub kind: String,
    pub actor: String,
    pub phase: String,
    pub san: String,
    pub loss_q: Option<f64>,
    pub loss_pl: Option<f64>,
    pub cp_loss: Option<f64>,
    pub before_q: Option<f64>,
    pub after_q: Option<f64>,
    pub clock_seconds: Option<f64>,
    pub spent_seconds: Option<f64>,
    pub run_id: String,
    pub profile_id: String,
    pub method: String,
    pub best_move: Option<String>,
    pub fen: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TimeRow {
    pub key: String,
    pub label: String,
    pub decisions: usize,
    pub games: usize,
    pub clock_sample: usize,
    pub spent_sample: usize,
    pub median_spent: Option<f64>,
    pub p25_spent: Option<f64>,
    pub p75_spent: Option<f64>,
    pub p90_spent: Option<f64>,
    pub total_spent: Option<f64>,
    pub mean_loss: Option<f64>,
    pub significant: usize,
    pub analysed: usize,
    pub game_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrendPoint {
    pub timestamp_ms: i64,
    pub games: usize,
    pub score: Option<f64>,
    pub rating: Option<f64>,
    pub mean_loss: Option<f64>,
    pub accuracy: Option<f64>,
    pub median_spent: Option<f64>,
    pub analysed: usize,
    pub game_ids: Vec<String>,
    pub stratum: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GameSummary {
    pub id: String,
    pub external_id: String,
    pub account_id: String,
    pub source: String,
    pub timestamp_ms: Option<i64>,
    pub observed_interval_seconds: Option<f64>,
    pub interval_quality: String,
    pub white: String,
    pub black: String,
    pub own_color: String,
    pub opponent: String,
    pub opponent_rating: Option<i32>,
    pub own_rating: Option<i32>,
    pub rating_diff: Option<i32>,
    pub result: String,
    pub outcome: String,
    pub speed: String,
    pub rated: Option<bool>,
    pub control: String,
    pub opening: Option<OpeningMatch>,
    pub plies: usize,
    pub own_decisions: usize,
    pub eligibility: String,
    pub status: String,
    pub accuracy: Option<f64>,
    pub acpl: Option<f64>,
    pub mean_loss: Option<f64>,
    pub significant: usize,
    pub analysed: usize,
    pub clock_sample: usize,
    pub profile_id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StatisticsReport {
    #[serde(default)]
    pub evidence_revision: String,
    #[serde(default)]
    pub evidence_counts: std::collections::BTreeMap<String, usize>,
    #[serde(default)]
    pub opening_analysis: super::opening_metrics::OpeningAnalysis,
    #[serde(default)]
    pub extended: super::extended::ExtendedReport,
    #[serde(default)]
    pub analysis: super::engine_metrics::EngineDetails,
    pub filter: StatisticsFilter,
    pub generated_at_ms: i64,
    pub total_games: usize,
    pub excluded_games: usize,
    pub total_decisions: usize,
    pub analysed_decisions: usize,
    pub profile_ids: Vec<String>,
    pub profile_labels: std::collections::BTreeMap<String, String>,
    pub selected_profile_id: Option<String>,
    pub metrics: Vec<MetricValue>,
    pub results: Vec<ResultRow>,
    pub openings: Vec<OpeningRow>,
    pub problems: Vec<OpeningProblem>,
    pub opening_tree: OpeningTree,
    pub phases: Vec<TimeRow>,
    pub time: Vec<TimeRow>,
    pub time_spent: Vec<TimeRow>,
    pub endings: Vec<ResultRow>,
    pub trend: Vec<TrendPoint>,
    pub games: Vec<GameSummary>,
    pub events: Vec<DecisionEvent>,
    pub comparison: Vec<MetricValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OpeningProblem {
    pub median_loss: Option<f64>,
    pub cases: Vec<super::opening_metrics::OpeningCase>,
    pub comparison: Vec<MetricValue>,
    pub position_key: String,
    pub played_uci: String,
    pub san: String,
    pub occurrences: usize,
    pub games: usize,
    pub mean_loss: f64,
    pub sum_loss: f64,
    pub median_spent: Option<f64>,
    pub game_ids: Vec<String>,
    pub first_game_id: String,
    pub first_ply: u32,
    pub opening: Option<OpeningMatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LegalContinuation {
    pub uci: String,
    pub san: String,
}

import type {
  OpeningAnalysis,
  OpeningCase,
  OpeningQuality,
  OpeningTemporal,
  ExtendedReport,
  EngineDetails,
} from "./analyticsTypes";
import { invoke } from "@tauri-apps/api/core";
export interface Participant {
  id: string | null;
  name: string;
  rating: number | null;
  rating_diff: number | null;
  provisional: boolean;
}
export interface Material {
  white: number[];
  black: number[];
  balance: number;
  ending: string;
}
export interface ReplayNode {
  ply: number;
  absolute_ply: number;
  fen: string;
  position_key: string;
  uci: string | null;
  san: string | null;
  actor: string | null;
  phase: string;
  clock_seconds: number | null;
  remaining_before: number | null;
  spent_seconds: number | null;
  clock_quality: string;
  comment: string;
  material: Material;
  legal_moves: number;
}
export interface OpeningMatch {
  eco: string;
  name: string;
  family: string;
  variation: string;
  ply: number;
  position_key: string;
}
export interface StoredGame {
  id: string;
  external_id: string;
  account_id: string;
  source: string;
  started_at_ms: number | null;
  last_move_at_ms: number | null;
  finished_at_ms: number | null;
  end_time_quality: string;
  white: Participant;
  black: Participant;
  own_color: string;
  result: string;
  status: string;
  speed: string;
  rated: boolean | null;
  variant: string;
  initial_seconds: number | null;
  increment_seconds: number | null;
  raw_pgn: string;
  eligibility: string;
  replay: {
    initial_fen: string;
    nodes: ReplayNode[];
    middle_ply: number | null;
    end_ply: number | null;
    phase_version: string;
    opening: OpeningMatch | null;
  };
}
export interface Evaluation {
  ply: number;
  cp: number | null;
  mate: number | null;
  wdl: [number, number, number] | null;
  best_move: string | null;
  pv: string[];
  depth: number;
  nodes: number;
  bound: string | null;
  root_pov: string;
  complete: boolean;
  terminal_outcome: string | null;
}
export interface EngineRun {
  id: string;
  game_id: string;
  profile: string;
  profile_id: string;
  engine_version: string;
  engine_binary_hash: string;
  revision: string;
  settings_json: string;
  completed_at_ms: number;
  evaluations: Evaluation[];
  confirmed_moves: {
    ply: number;
    best: Evaluation;
    played: Evaluation;
    complete: boolean;
    method: string;
  }[];
}
export interface Filter {
  tree_mode?: string | null;
  opening_initial_fen?: string | null;
  opening_actor?: string | null;
  opening_label_id?: string | null;
  trend_granularity?: string | null;
  account_id?: string | null;
  since_ms?: number | null;
  until_ms?: number | null;
  speed?: string | null;
  control?: string | null;
  color?: string | null;
  opening?: string | null;
  opponent_rating_min?: number | null;
  opponent_rating_max?: number | null;
  source?: string | null;
  variant?: string | null;
  rated?: boolean | null;
  profile_id?: string | null;
  include_uncertain?: boolean | null;
  game_ids?: string[] | null;
  position_key?: string | null;
  line?: string[] | null;
  phase?: string | null;
  comparison_since_ms?: number | null;
  comparison_until_ms?: number | null;
}
export interface Metric {
  id: string;
  name: string;
  unit: string;
  value: number | null;
  sample: number;
  eligible: number;
  definition: string;
  method: string;
  low: number | null;
  high: number | null;
  game_ids: string[];
  status: string;
}
export interface ResultRow {
  opponent_rating_median?: number | null;
  rating_sample?: number;
  score_low?: number | null;
  score_high?: number | null;
  key: string;
  label: string;
  games: number;
  wins: number;
  draws: number;
  losses: number;
  score: number | null;
  game_ids: string[];
  mean_loss: number | null;
  mean_accuracy: number | null;
  analysed: number;
}
export interface OpeningRow {
  eco: string;
  name: string;
  family: string;
  variation: string;
  result: ResultRow;
  exit_q: number | null;
  exit_sample: number;
  first_ply: number | null;
  position_key: string | null;
}
export interface OpeningEdge {
  actor: string;
  cases: OpeningCase[];
  quality: OpeningQuality;
  temporal: OpeningTemporal;
  spent_median: number | null;
  spent_sample: number;
  uci: string;
  san: string;
  fen: string;
  position_key: string;
  result: ResultRow;
  frequency: number;
  visits: number;
}
export interface OpeningTree {
  initial_fen: string;
  mode: string;
  fen: string;
  position_key: string;
  line: string[];
  games: number;
  visits: number;
  distinct_paths: number;
  unplayed: { uci: string; san: string }[];
  edges: OpeningEdge[];
  game_ids: string[];
}
export interface DecisionEvent {
  game_id: string;
  ply: number;
  kind: string;
  actor: string;
  phase: string;
  san: string;
  loss_q: number | null;
  loss_pl: number | null;
  cp_loss: number | null;
  before_q: number | null;
  after_q: number | null;
  clock_seconds: number | null;
  spent_seconds: number | null;
  run_id: string;
  profile_id: string;
  method: string;
  best_move: string | null;
  fen: string;
}
export interface TimeRow {
  key: string;
  label: string;
  decisions: number;
  games: number;
  clock_sample: number;
  spent_sample: number;
  median_spent: number | null;
  p25_spent: number | null;
  p75_spent: number | null;
  p90_spent: number | null;
  total_spent: number | null;
  mean_loss: number | null;
  significant: number;
  analysed: number;
  game_ids: string[];
}
export interface TrendPoint {
  timestamp_ms: number;
  games: number;
  score: number | null;
  rating: number | null;
  mean_loss: number | null;
  accuracy: number | null;
  median_spent: number | null;
  analysed: number;
  game_ids: string[];
  stratum: string;
}
export interface GameSummary {
  id: string;
  external_id: string;
  account_id: string;
  source: string;
  timestamp_ms: number | null;
  observed_interval_seconds: number | null;
  interval_quality: string;
  white: string;
  black: string;
  own_color: string;
  opponent: string;
  opponent_rating: number | null;
  own_rating: number | null;
  rating_diff: number | null;
  result: string;
  outcome: string;
  speed: string;
  rated: boolean | null;
  control: string;
  opening: OpeningMatch | null;
  plies: number;
  own_decisions: number;
  eligibility: string;
  status: string;
  accuracy: number | null;
  acpl: number | null;
  mean_loss: number | null;
  significant: number;
  analysed: number;
  clock_sample: number;
  profile_id: string | null;
}
export interface Report {
  evidence_counts: Record<string, number>;
  evidence_revision: string;
  opening_analysis: OpeningAnalysis;
  extended: ExtendedReport;
  analysis: EngineDetails;
  filter: Filter;
  generated_at_ms: number;
  total_games: number;
  excluded_games: number;
  total_decisions: number;
  analysed_decisions: number;
  profile_ids: string[];
  profile_labels: Record<string, string>;
  problems: OpeningProblem[];
  selected_profile_id: string | null;
  metrics: Metric[];
  results: ResultRow[];
  openings: OpeningRow[];
  opening_tree: OpeningTree;
  phases: TimeRow[];
  time: TimeRow[];
  time_spent: TimeRow[];
  endings: ResultRow[];
  trend: TrendPoint[];
  games: GameSummary[];
  events: DecisionEvent[];
  comparison: Metric[];
}
export interface OpeningProblem {
  median_loss: number | null;
  cases: OpeningCase[];
  comparison: Metric[];
  position_key: string;
  played_uci: string;
  san: string;
  occurrences: number;
  games: number;
  mean_loss: number;
  sum_loss: number;
  median_spent: number | null;
  game_ids: string[];
  first_game_id: string;
  first_ply: number;
  opening: OpeningMatch | null;
}
export interface QuarantineRecord {
  account_id: string;
  external_id: string;
  received_at_ms: number;
  reason: string;
  raw_json: string;
}
export interface Observation {
  id: string;
  external_id: string;
  username: string;
  t0_ms: number;
  enabled: boolean;
  generation: number;
}
export interface EngineConfig {
  path: string;
  threads: number;
  hash_mb: number;
  profile: string;
  custom_nodes: number | null;
  custom_movetime_ms: number | null;
  syzygy_path?: string | null;
}
export interface Job {
  id: number;
  game_id: string;
  state: string;
  progress: number;
  total: number;
  last_error: string | null;
  profile_id: string;
  attempts: number;
}
export interface EngineStatus {
  paused: boolean;
  configured: boolean;
  jobs: Job[];
  identity: { name: string; profile_id: string } | null;
}
export interface SyncStatus {
  status: string;
  last_success_ms: number | null;
  last_reconciled_ms: number | null;
  error_message: string | null;
  error_code: string | null;
  retry_not_before_ms: number | null;
  pending_games: number;
  pending_ranges: number;
  quarantined_games: number;
  boundary_uncertain: boolean;
  baseline_complete: boolean;
}
export interface StatisticsState {
  accounts: Observation[];
  active_account_id: string | null;
  sync: SyncStatus | null;
  engine: EngineStatus;
  quarantine: QuarantineRecord[];
}
export interface GameDetail {
  analyses: Record<string, EngineDetails>;
  game: StoredGame;
  runs: EngineRun[];
  events: DecisionEvent[];
}
export const getReport = (filter: Filter) =>
  invoke<Report>("statistics_report", { filter });
export const getState = (accountId?: string | null) =>
  invoke<StatisticsState>("statistics_state", { accountId: accountId ?? null });
export const retryQuarantined = (accountId: string, id: string) =>
  invoke<void>("statistics_quarantine_retry", { accountId, id });
export const getGame = (id: string, accountId?: string | null) =>
  invoke<GameDetail>("statistics_game", { id, accountId: accountId ?? null });
export const requestSync = () => invoke<void>("statistics_sync");
export const enqueue = (gameIds: string[]) =>
  invoke<number>("statistics_engine_enqueue", { gameIds });
export const pauseAnalysis = (paused: boolean) =>
  invoke<void>("statistics_engine_pause", { paused });
export const cancelAnalysis = (jobId: number) =>
  invoke<void>("statistics_engine_cancel", { jobId });

export interface EvidenceSelector {
  kind: string;
  key: string;
}
export interface EvidencePage<T> {
  items: T[];
  total: number;
  offset: number;
  limit: number;
}
export const getEvidence = <T>(
  report: Report,
  selector: EvidenceSelector,
  offset: number,
) =>
  invoke<EvidencePage<T>>("statistics_evidence", {
    filter: report.filter,
    revision: report.evidence_revision,
    ...selector,
    offset,
    limit: 100,
  });

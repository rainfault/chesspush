// Serialization contracts for the native Statistics analytical views.
import type { Metric, Evaluation, ResultRow, OpeningMatch } from "./api";

export interface MoveEvidence {
  game_id: string;
  ply: number;
  run_id: string | null;
  fen: string;
}

export interface SessionRow {
  id: string;
  gap_minutes: number;
  first_start_ms: number;
  last_start_ms: number;
  ended_at_ms: number | null;
  span_seconds: number | null;
  observed_start_span_seconds: number;
  overlapping: boolean;
  order_ambiguous: boolean;
  games: number;
  wins: number;
  draws: number;
  losses: number;
  score: number | null;
  game_ids: string[];
}

export interface StreakRow {
  id: string;
  kind: string;
  length: number;
  first_ms: number;
  last_ms: number;
  stratum: string;
  game_ids: string[];
}

export interface ContextRow {
  id: string;
  metric_id: string;
  label: string;
  stratum: string;
  unit: string;
  value: number | null;
  low: number | null;
  high: number | null;
  sample: number;
  eligible: number;
  mean_loss: number | null;
  significant: number;
  status: string;
  game_ids: string[];
  evidence: MoveEvidence[];
}

export interface StructureRow {
  id: string;
  metric_id: string;
  label: string;
  games: number;
  visits: number;
  decisions: number;
  share: number | null;
  game_ids: string[];
  evidence: MoveEvidence[];
}

export interface ProgressRow {
  id: string;
  metric_id: string;
  label: string;
  key: string;
  unit: string;
  current: number | null;
  previous: number | null;
  delta: number | null;
  current_sample: number;
  previous_sample: number;
  low: number | null;
  high: number | null;
  status: string;
  game_ids: string[];
  evidence: MoveEvidence[];
}

export interface CoverageRow {
  reason: string;
  games: number;
  decisions: number;
  game_ids: string[];
}

export interface ExtendedReport {
  sessions: SessionRow[];
  streaks: StreakRow[];
  time_contexts: ContextRow[];
  structures: StructureRow[];
  progress: ProgressRow[];
  coverage: CoverageRow[];
}

export interface OpeningCase {
  game_id: string;
  ply: number;
  run_id: string | null;
  profile_id: string | null;
  fen: string;
  position_key: string;
  san: string | null;
  uci: string | null;
  actor: string;
  own_color: string;
  timestamp_ms: number | null;
}

export interface OpeningTimeGame {
  game_id: string;
  known_seconds: number | null;
  known_decisions: number;
  decisions: number;
  coverage: number | null;
  cases: OpeningCase[];
}

export interface OpeningQuality {
  decisions: number;
  wdl_sample: number;
  cp_sample: number;
  accuracy_sample: number;
  mean_loss: number | null;
  median_loss: number | null;
  p90_loss: number | null;
  acpl: number | null;
  accuracy: number | null;
  significant: number;
  significant_per_100: number | null;
  spent_sample: number;
  median_spent: number | null;
  total_spent: number | null;
  mean_spent_per_game: number | null;
  spent_game_sample: number;
  clock_coverage: number | null;
  time_games: OpeningTimeGame[];
  cases: OpeningCase[];
}

export interface OpeningTemporal {
  current_visits: number;
  previous_visits: number;
  current_total: number;
  previous_total: number;
  current_share: number | null;
  previous_share: number | null;
  share_delta: number | null;
  first_seen_ms: number | null;
  first_seen: OpeningCase | null;
  previous_cases: OpeningCase[];
  new_paths: number;
  repeated_paths: number;
  unknown_paths: number;
  new_path_percent: number | null;
  repeated_path_percent: number | null;
  comparison: Metric[];
}

export interface OpeningHierarchyNode {
  id: string;
  parent_id: string | null;
  level: string;
  label: string;
  path: string[];
  color: string;
  frequency: number | null;
  result: ResultRow;
  previous_result: ResultRow;
  quality: OpeningQuality;
  previous_quality: OpeningQuality;
  rating_difference_median: number | null;
  rating_difference_sample: number;
  comparison: Metric[];
  cases: OpeningCase[];
}

export interface OpeningPositionContext {
  context_key: string;
  cases: OpeningCase[];
  evaluation: Evaluation | null;
  tablebase: TablebaseObservation | null;
  q: number | null;
  perspective: string;
  status: string;
}

export interface OpeningNodeAnalysis {
  contexts: OpeningPositionContext[];
  unique_contexts: number;
  usable_contexts: number;
  q_median: number | null;
  q_p25: number | null;
  q_p75: number | null;
  perspective: string;
  definition: string;
}

export interface OpeningBoundary {
  case: OpeningCase | null;
  game_id: string;
  kind: string;
  ply: number | null;
  cp: number | null;
  mate: number | null;
  wdl: [number, number, number] | null;
  q: number | null;
  pov: string;
  terminal_outcome: string | null;
  status: string;
  method: string;
}

export interface OpeningBoundarySummary {
  kind: string;
  eligible: number;
  sample: number;
  q_median: number | null;
  q_mean: number | null;
  q_p25: number | null;
  q_p75: number | null;
  advantage_percent: number | null;
  hard_percent: number | null;
  cases: OpeningBoundary[];
}

export interface OpeningFirstLoss {
  game_id: string;
  case: OpeningCase | null;
  loss_q: number | null;
  status: string;
  decisions: number;
  analysed: number;
  result: string;
  exit_q: number | null;
  opening: OpeningMatch | null;
}

export interface OpeningResponseWindow {
  response: OpeningCase;
  decisions: OpeningCase[];
  mean_loss: number | null;
  significant: number;
  status: string;
}

export interface OpeningOpponentResponse {
  position_key: string;
  played_uci: string;
  san: string;
  visits: number;
  games: number;
  frequency: number | null;
  window_size: number;
  eligible_windows: number;
  excluded_windows: number;
  mean_loss: number | null;
  significant_per_100: number | null;
  cases: OpeningResponseWindow[];
}

export interface OpeningBlocker {
  id: string;
  status: string;
  dependency: string;
}

export interface OpeningAnalysis {
  method: string;
  hierarchy: OpeningHierarchyNode[];
  node: OpeningNodeAnalysis;
  boundaries: OpeningBoundarySummary[];
  first_losses: OpeningFirstLoss[];
  opponent_responses: OpeningOpponentResponse[];
  comparison: Metric[];
  blockers: OpeningBlocker[];
}

export interface AnalysisCase {
  metric_id: string;
  position_ply: number;
  game_id: string;
  ply: number;
  run_id: string;
  label: string;
  value: number | null;
  unit: string;
  phase: string;
  details: string;
}

export interface Episode {
  game_id: string;
  run_id: string;
  kind: string;
  start_ply: number;
  end_ply: number;
  confirmation_ply: number;
  exit_ply: number | null;
  terminal_ply: number | null;
  status: string;
  own_decisions: number;
  first_loss_ply: number | null;
  total_loss: number;
  phase: string;
}

export interface LossBin {
  lo: number;
  hi: number | null;
  decisions: number;
  game_ids: string[];
}

export interface TrajectoryPoint {
  game_id: string;
  run_id: string;
  ply: number;
  cp: number | null;
  mate: number | null;
  wdl: [number, number, number] | null;
  q: number | null;
  pov: string;
  status: string;
  accuracy: number | null;
  advice: string | null;
}

export interface EngineDetails {
  version: string;
  cases: AnalysisCase[];
  episodes: Episode[];
  histogram: LossBin[];
  tablebase_status: string;
  trajectory: TrajectoryPoint[];
}

export interface TablebaseObservation {
  ply: number;
  wdl: number | null;
  dtz: number | null;
  rounded: boolean;
  status: string;
  manifest_id: string;
  rule50: boolean;
  context_hash: string;
}

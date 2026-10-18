import { invoke } from "@tauri-apps/api/core";
import type { Filter, ReplayNode } from "../statistics/api";
export interface OccurrenceRef {
  game_id: string;
  revision: number;
  ply: number;
}
export interface PreparedLink {
  document_id: string;
  chapter_id: string;
  node_id: string;
  document_revision: number;
  side: string;
}
export interface GraphNode {
  id: string;
  fen: string;
  games: number;
  visits: number;
  wins: number;
  draws: number;
  losses: number;
  score: number | null;
  first_ms: number | null;
  last_ms: number | null;
  hidden_continuations: number;
  example: OccurrenceRef | null;
  engine_cp?: number | null;
  engine_mate?: number | null;
  engine_profile?: string | null;
  engine_depth?: number | null;
  engine_coverage?: number;
  engine_reference?: OccurrenceRef | null;
  engine_run_id?: string | null;
}
export interface GraphEdge {
  branch_games?: number;
  id: string;
  from: string;
  to: string;
  uci: string;
  san: string;
  games: number;
  visits: number;
  wins: number;
  draws: number;
  losses: number;
  score: number | null;
  frequency: number;
  prepared: PreparedLink[];
  coverage: string;
  example: OccurrenceRef | null;
  first_ms: number | null;
  last_ms: number | null;
  spent_mean: number | null;
  spent_sample: number;
}
export interface GraphPreview {
  uci: string;
  replies: GraphEdge[];
  hidden: number;
}
export interface GraphReviewReference {
  game_id: string;
  ply: number;
  run_id: string;
  position_key?: string | null;
  played_uci?: string | null;
  revision?: number | null;
}
export interface Projection {
  root_key: string;
  selected_key: string;
  selected_fen: string;
  path_nodes: ReplayNode[];
  initial_fen: string;
  line: string[];
  nodes: GraphNode[];
  edges: GraphEdge[];
  continuations: GraphEdge[];
  previews: GraphPreview[];
  alternative_paths: number;
  branch_games: number;
  all_paths: boolean;
  repertoire_ends_here: boolean;
  review_refs: GraphReviewReference[];
  review_analysed_games: number;
  selected: GraphNode;
  hidden_nodes: number;
  indexed_games: number;
  eligible_games: number;
  total_positions: number;
  total_visits: number;
  index_revision: string;
}
export interface GraphRequest {
  filter: Filter;
  position_key?: string | null;
  initial_fen?: string;
  line?: string[];
  depth?: number;
  top_k?: number;
  preview_limit?: number;
  preview_top_k?: number;
  max_nodes?: number;
  repertoire_ids?: string[];
  all_paths?: boolean;
  review_refs?: GraphReviewReference[];
}
export interface OccurrenceContext {
  reference: OccurrenceRef;
  initial_fen: string;
  fen: string;
  position_key: string;
  line: string[];
  raw_pgn: string;
  current_revision: number;
  white: string;
  black: string;
}
export interface EvidenceRow {
  reference: OccurrenceRef;
  account_id: string;
  own_color: string;
  white: string;
  black: string;
  result: string;
  outcome: string;
  timestamp_ms: number | null;
  fen: string;
  san: string | null;
  uci: string | null;
}
export interface EvidencePage {
  rows: EvidenceRow[];
  total: number;
  offset: number;
  has_more: boolean;
}
export const graphQuery = (request: GraphRequest) =>
  invoke<Projection>("game_graph_query", { request });
export const graphEvidence = (
  filter: Filter,
  position_key: string,
  uci: string | null,
  offset = 0,
  scope?: { initial_fen: string; line: string[]; all_paths: boolean },
) =>
  invoke<EvidencePage>("game_graph_evidence", {
    request: { filter, position_key, uci, offset, limit: 50, ...scope },
  });
export const resolveOccurrence = (reference: OccurrenceRef) =>
  invoke<OccurrenceContext>("game_graph_resolve_occurrence", { reference });
export const loadGraphState = (accountId?: string | null) =>
  invoke<unknown>("game_graph_load_state", { accountId: accountId ?? null });
export const saveGraphState = (
  accountId: string | null | undefined,
  state: unknown,
) =>
  invoke<void>("game_graph_save_state", {
    accountId: accountId ?? null,
    state,
  });

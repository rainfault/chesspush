import { invoke } from "@tauri-apps/api/core";
import type { KnowledgeDocument } from "../knowledge/types";
import type { Evaluation, Observation, StoredGame } from "../statistics/api";

export interface ReviewTarget {
  gameId: string;
  accountId?: string;
  revision?: number;
  ply?: number;
  requestId?: number;
}
export interface ReviewFilter {
  account_id?: string | null;
  query?: string;
  color?: string | null;
  speed?: string | null;
  tag?: string | null;
  reviewed?: boolean | null;
  since_ms?: number | null;
  until_ms?: number | null;
  offset?: number;
  limit?: number;
}
export interface ReviewRow {
  game_id: string;
  account_id: string;
  source_revision: number;
  white: string;
  black: string;
  white_rating: number | null;
  black_rating: number | null;
  result: string;
  own_color: string;
  started_at_ms: number | null;
  speed: string;
  opening: string | null;
  eco: string | null;
  tags: string[];
  reviewed: boolean;
  document_id: string | null;
  analysis_status: string;
  analysis_error: string | null;
  analysis_progress: { progress: number; total: number } | null;
}
export interface ReviewPage {
  accounts: Observation[];
  selected_account_id: string | null;
  total: number;
  tags: string[];
  rows: ReviewRow[];
}
export interface OpenReview {
  game: StoredGame;
  document: KnowledgeDocument;
  chapter_id: string;
  node_id: string;
  reviewed: boolean;
  analysis_status: string;
  source_nodes: { ply: number; chapter_id: string; node_id: string }[];
}
export interface ReviewEvaluation {
  evaluation: Evaluation;
  engine: { name: string; profile_id: string; binary_hash: string };
}
export const listReviews = (request: ReviewFilter) =>
  invoke<ReviewPage>("game_review_list", { request });
export const openReview = (target: ReviewTarget) =>
  invoke<OpenReview>("game_review_open", {
    gameId: target.gameId,
    accountId: target.accountId ?? null,
    sourceRevision: target.revision ?? null,
  });
export const markReview = (
  gameId: string,
  accountId: string,
  sourceRevision: number,
  reviewed: boolean,
) =>
  invoke<void>("game_review_mark", {
    gameId,
    accountId,
    sourceRevision,
    reviewed,
  });
export const saveReviewKnowledge = (
  documentId: string,
  expectedRevision: number,
  chapterId: string,
  nodeId: string,
  title: string,
  scope: "position" | "branch" | "game",
) =>
  invoke<KnowledgeDocument>("game_review_save_knowledge", {
    request: {
      document_id: documentId,
      expected_revision: expectedRevision,
      chapter_id: chapterId,
      node_id: nodeId,
      title,
      scope,
    },
  });
export const evaluateReview = (
  documentId: string,
  revision: number,
  chapterId: string,
  nodeId: string,
  requestId: string,
) =>
  invoke<ReviewEvaluation>("game_review_evaluate", {
    documentId,
    revision,
    chapterId,
    nodeId,
    requestId,
  });
export const cancelReviewEvaluation = (requestId: string) =>
  invoke<void>("game_review_cancel_evaluation", { requestId });

export function analysisLabel(status: string): string {
  return (
    (
      {
        not_configured: "Без движка",
        not_queued: "Нет анализа",
        queued: "В очереди",
        running: "Анализируется",
        paused: "Пауза",
        retryable_failed: "Ошибка анализа",
        permanent_failed: "Ошибка анализа",
        cancelled: "Отменён",
        succeeded: "Готов",
      } as Record<string, string>
    )[status] ?? status
  );
}

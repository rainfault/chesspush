import { invoke } from "@tauri-apps/api/core";

export type TrainingMode = "srs" | "full_drill";
export interface TrainingScope {
  side: "white" | "black";
  chapter_id?: string;
  node_id?: string;
  source_document_id?: string;
}
export interface TrainingSummary {
  id: string;
  completed: number;
  total: number;
  mistakes: number;
  finished: boolean;
}
export interface TrainingOverview {
  total: number;
  due: number;
  next_due_at_ms?: number | null;
  active_srs?: TrainingSummary | null;
  active_full_drill?: TrainingSummary | null;
}
export interface TrainingMove {
  uci: string;
  san: string;
}
export interface TrainingPrompt {
  decision_id: string;
  chapter_id: string;
  node_id: string;
  initial_fen: string;
  fen: string;
  position_key: string;
  context: (TrainingMove & { fen: string })[];
  legal_moves: TrainingMove[];
  accepted_moves: TrainingMove[];
  comments: string[];
  attempted_uci?: string | null;
}
export interface TrainingSession extends TrainingSummary {
  mode: TrainingMode;
  scope: TrainingScope;
  feedback: "awaiting" | "correct" | "retry" | "assisted" | "finished";
  prompt?: TrainingPrompt | null;
}

export const trainingOverview = (scope: TrainingScope) =>
  invoke<TrainingOverview>("training_overview", { scope });
export const startTraining = (
  scope: TrainingScope,
  mode: TrainingMode,
  restart = false,
) =>
  invoke<TrainingSession>("training_start", {
    request: { scope, mode, restart },
  });
export const answerTraining = (sessionId: string, uci: string) =>
  invoke<TrainingSession>("training_answer", { sessionId, uci });
export const hintTraining = (sessionId: string) =>
  invoke<TrainingSession>("training_hint", { sessionId });
export const nextTraining = (sessionId: string) =>
  invoke<TrainingSession>("training_next", { sessionId });

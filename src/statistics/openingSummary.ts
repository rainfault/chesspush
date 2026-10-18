import { invoke } from "@tauri-apps/api/core";
import type { Observation } from "./api";

export interface OpeningCoverage {
  covered_decisions: number;
  total_decisions: number;
  percent: number | null;
  games: number;
  depth_moves: number;
  version: string;
  source: string;
  repertoire_documents: number;
}
export interface OpeningSummaryRow {
  color: "white" | "black";
  preview_fen: string | null;
  preview_last_move: [string, string] | null;
  preview_pgn: string | null;
  preview_source: string | null;
  coverage: OpeningCoverage;
  eco: string | null;
  name: string | null;
  games: number;
  wins: number;
  draws: number;
  losses: number;
  score: number;
  game_ids: string[];
}
export interface OpeningSummary {
  accounts: Observation[];
  active_account_id: string | null;
  selected_account_id: string | null;
  color: "white" | "black";
  coverage_depth_moves: number;
  coverage_version: string;
  total_games: number;
  wins: number;
  draws: number;
  losses: number;
  score: number | null;
  rows: OpeningSummaryRow[];
}
export const getOpeningSummary = (accountId: string | null, color: string) =>
  invoke<OpeningSummary>("statistics_openings", { accountId, color });

export type OpeningSort =
  "eco" | "name" | "games" | "wins" | "draws" | "losses" | "score" | "coverage";
export function sortOpenings(
  rows: OpeningSummaryRow[],
  key: OpeningSort,
  direction: "asc" | "desc",
) {
  return [...rows].sort((a, b) => {
    const left = key === "coverage" ? a.coverage.percent : a[key],
      right = key === "coverage" ? b.coverage.percent : b[key];
    // Unknown classification remains explicit and always follows known values.
    if (left == null && right != null) return 1;
    if (right == null && left != null) return -1;
    const compared =
      typeof left === "number" && typeof right === "number"
        ? left - right
        : String(left ?? "").localeCompare(String(right ?? ""), "ru", {
            numeric: true,
          });
    return (
      (direction === "desc" ? -compared : compared) ||
      (a.name ?? "").localeCompare(b.name ?? "", "ru") ||
      (a.eco ?? "").localeCompare(b.eco ?? "")
    );
  });
}

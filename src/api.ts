import { invoke } from "@tauri-apps/api/core";
export interface Filters {
  color: string;
  result_mode: string;
  min_white_elo: number;
  max_white_elo: number;
  min_black_elo: number;
  max_black_elo: number;
  min_both_elo: number;
  max_rating_diff: number;
  eco: string;
  opening_contains: string;
  move_prefix: string;
  perf_type: string;
  min_moves: number;
  exclude_bullet: boolean;
  include_timeout: boolean;
  username: string;
}
export const defaultFilters: Filters = {
  color: "any",
  result_mode: "all",
  min_white_elo: 2300,
  max_white_elo: 4000,
  min_black_elo: 2300,
  max_black_elo: 4000,
  min_both_elo: 0,
  max_rating_diff: 4000,
  eco: "",
  opening_contains: "",
  move_prefix: "",
  perf_type: "any",
  min_moves: 25,
  exclude_bullet: true,
  include_timeout: false,
  username: "",
};
export interface Progress {
  scanned: number;
  matched: number;
  cancelled: boolean;
}
export interface AppError {
  code: string;
  message: string;
}
export interface ParseResult {
  progress: Progress;
  output: string | null;
  error: AppError | null;
  warning: string | null;
  elapsed_ms: number;
}
export interface LibraryFile {
  id: number;
  path: string;
  kind: string;
  games: number;
}
export const files = () => invoke<LibraryFile[]>("library_files");
export function errorMessage(error: unknown): string {
  if (typeof error === "object" && error && "message" in error)
    return String(error.message);
  return String(error);
}

import { invoke } from "@tauri-apps/api/core";
export interface RecentGame {
  id: string;
  account_id: string;
  timestamp_ms: number | null;
  opponent: string;
  opponent_rating: number | null;
  color: string;
  result: string;
  opening: string | null;
  analysed: boolean;
}
export const recentGames = () => invoke<RecentGame[]>("statistics_recent");

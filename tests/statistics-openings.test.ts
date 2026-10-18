import { describe, expect, it } from "vitest";
import {
  sortOpenings,
  type OpeningSummaryRow,
} from "../src/statistics/openingSummary";
const row = (
  eco: string | null,
  name: string | null,
  games: number,
  score: number,
): OpeningSummaryRow => ({
  color: "white",
  preview_fen: null,
  preview_pgn: null,
  preview_last_move: null,
  preview_source: null,
  coverage: {
    covered_decisions: 0,
    total_decisions: 0,
    percent: null,
    games,
    depth_moves: 15,
    version: "observed-own-decisions-v1",
    source: "active-attached-repertoire",
    repertoire_documents: 0,
  },
  eco,
  name,
  games,
  score,
  wins: 0,
  draws: 0,
  losses: 0,
  game_ids: [],
});
describe("opening table sorting", () => {
  const rows = [
    row("C20", "King's Pawn Game", 12, 25),
    row(null, null, 3, 0),
    row("B20", "Sicilian Defense", 7, 100),
    row("B21", "Sicilian Defense: Smith-Morra Gambit", 7, 50),
  ];
  it("sorts numeric game counts and scores rather than formatted strings", () => {
    expect(
      sortOpenings(rows, "games", "desc").map((item) => item.games),
    ).toEqual([12, 7, 7, 3]);
    expect(
      sortOpenings(rows, "score", "asc").map((item) => item.score),
    ).toEqual([0, 25, 50, 100]);
    expect(rows[0].eco).toBe("C20");
  });
  it("sorts classifications in both directions while keeping unknown classification explicit", () => {
    expect(sortOpenings(rows, "eco", "asc").map((item) => item.eco)).toEqual([
      "B20",
      "B21",
      "C20",
      null,
    ]);
    expect(sortOpenings(rows, "eco", "desc").map((item) => item.eco)).toEqual([
      "C20",
      "B21",
      "B20",
      null,
    ]);
    expect(sortOpenings(rows, "name", "asc").at(-1)?.name).toBeNull();
  });
});

describe("repertoire coverage sorting", () => {
  it("sorts numeric coverage and leaves unavailable results last in both directions", () => {
    const rows = [
      row("C20", "First", 2, 0),
      row("B20", "Second", 5, 0),
      row("A00", "Unknown", 1, 0),
    ];
    rows[0].coverage.percent = 12.5;
    rows[1].coverage.percent = 100;
    expect(
      sortOpenings(rows, "coverage", "desc").map((r) => r.coverage.percent),
    ).toEqual([100, 12.5, null]);
    expect(
      sortOpenings(rows, "coverage", "asc").map((r) => r.coverage.percent),
    ).toEqual([12.5, 100, null]);
  });
});

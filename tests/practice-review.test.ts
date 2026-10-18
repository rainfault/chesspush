import { describe, expect, it } from "vitest";
import { reviewCases } from "../src/statistics/review";
import type { DecisionEvent, GameSummary } from "../src/statistics/api";

const game = (id: string, own_color = "white") =>
  ({ id, own_color }) as GameSummary;
const event = (patch: Partial<DecisionEvent> = {}) =>
  ({
    game_id: "a",
    ply: 5,
    kind: "significant",
    actor: "white",
    phase: "opening",
    san: "Qh5",
    loss_q: 18,
    loss_pl: null,
    cp_loss: null,
    before_q: 0.7,
    after_q: 0.52,
    clock_seconds: null,
    spent_seconds: null,
    run_id: "run-a",
    profile_id: "profile",
    method: "same_root_searchmoves",
    best_move: "g1f3",
    fen: "position with full rule context",
    ...patch,
  }) as DecisionEvent;

describe("material for review", () => {
  it("keeps source game/ply/run and deduplicates repeated event classifications", () => {
    const rows = reviewCases(
      [event(), event({ kind: "advantage_lost" })],
      [game("a")],
    );
    expect(rows).toHaveLength(1);
    expect(rows[0].count).toBe(1);
    expect(rows[0].event).toMatchObject({
      game_id: "a",
      ply: 5,
      run_id: "run-a",
    });
  });
  it("does not describe absent, unstable or opposing evidence as a personal error", () => {
    const rows = reviewCases(
      [
        event({ loss_q: null }),
        event({ kind: "unstable" }),
        event({ actor: "black" }),
        event({ kind: "decision", loss_q: 0 }),
        event({ kind: "opportunity_used", loss_q: 0.1 }),
        event({ kind: "advantage_entry", loss_q: 0.1 }),
      ],
      [game("a")],
    );
    expect(rows).toEqual([]);
  });
  it("ranks repeated confirmed positions before a single larger loss without mixing profiles", () => {
    const rows = reviewCases(
      [
        event(),
        event({ game_id: "b", run_id: "run-b", loss_q: 21 }),
        event({ game_id: "c", fen: "another position", loss_q: 45 }),
        event({ game_id: "d", profile_id: "other", loss_q: 33 }),
      ],
      [game("a"), game("b"), game("c"), game("d")],
    );
    expect(rows.map((row) => row.count)).toEqual([2, 1, 1]);
    expect(rows[0].event.game_id).toBe("b");
    expect(rows[0].event.ply - 1).toBe(4);
    expect(rows[1].event.loss_q).toBe(45);
  });
  it("uses the chosen personal side and requires a game in the current cohort", () => {
    expect(
      reviewCases(
        [event({ actor: "black" }), event({ game_id: "outside" })],
        [game("a", "black")],
      ),
    ).toHaveLength(1);
  });
});

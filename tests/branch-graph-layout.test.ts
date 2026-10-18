import { describe, expect, it } from "vitest";
import {
  layoutBranches,
  GRAPH_NODE_HEIGHT,
  GRAPH_REPLY_HEIGHT,
} from "../src/gameGraph/branchLayout";
import type { GraphEdge } from "../src/gameGraph/api";
const edge = (uci: string, to = uci): GraphEdge => ({
  id: uci,
  from: "root",
  to,
  uci,
  san: uci,
  games: 1,
  visits: 1,
  wins: 1,
  draws: 0,
  losses: 0,
  score: 100,
  frequency: 50,
  prepared: [],
  coverage: "observed",
  example: null,
  first_ms: null,
  last_ms: null,
  spent_mean: null,
  spent_sample: 0,
});
describe("local branch graph layout", () => {
  it("keeps transposed occurrences separate and connects replies to their exact parent path", () => {
    const a = edge("g1f3", "same-position"),
      b = edge("d2d4", "same-position"),
      reply = edge("d7d5");
    const result = layoutBranches(
      [a, b],
      [
        { uci: a.uci, replies: [reply], hidden: 0 },
        { uci: b.uci, replies: [], hidden: 2 },
      ],
    );
    expect(result.rows).toHaveLength(2);
    expect(result.rows[0].edge.to).toBe(result.rows[1].edge.to);
    expect(result.rows[0].replies[0].edge.uci).toBe(reply.uci);
    expect(result.rows[1].replies).toHaveLength(0);
    expect(result.rows[0].center).toBeLessThan(result.rows[1].y);
  });
  it("allocates disjoint bounds to two next-ply replies and the next branch", () => {
    const a = edge("e2e4"),
      b = edge("d2d4");
    const result = layoutBranches(
      [a, b],
      [{ uci: a.uci, replies: [edge("c7c5"), edge("e7e5")], hidden: 3 }],
    );
    const [first, second] = result.rows[0].replies;
    expect(first.y + GRAPH_REPLY_HEIGHT).toBeLessThan(second.y);
    expect(second.y + GRAPH_REPLY_HEIGHT).toBeLessThan(result.rows[1].y);
    expect(result.rows[0].hidden).toBe(3);
    expect(result.rootY + GRAPH_NODE_HEIGHT).toBeLessThanOrEqual(result.height);
  });
});

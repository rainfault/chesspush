import { describe, expect, it } from "vitest";
import { pushRoute, restoreHistory } from "../src/gameGraph/navigation";

const root = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
describe("graph occurrence provenance", () => {
  it("retains a newly selected game occurrence even when its complete chess path matches", () => {
    const first = {
      initialFen: root,
      line: ["e2e4"],
      positionKey: "e4",
      reference: { game_id: "lichess:first", revision: 1, ply: 1 },
    };
    const history = pushRoute(restoreHistory(null, root), first);
    const next = pushRoute(history, {
      ...first,
      reference: { game_id: "lichess:second", revision: 1, ply: 1 },
    });
    expect(next.entries[next.cursor].reference?.game_id).toBe("lichess:second");
  });

  it("does not collapse different factual revisions onto an earlier occurrence", () => {
    const first = {
      initialFen: root,
      line: ["e2e4"],
      positionKey: "e4",
      reference: { game_id: "lichess:first", revision: 1, ply: 1 },
    };
    const history = pushRoute(restoreHistory(null, root), first);
    const next = pushRoute(history, {
      ...first,
      reference: { ...first.reference, revision: 2 },
    });
    expect(next.entries[next.cursor].reference?.revision).toBe(2);
  });

  it("always restores an integer cursor that resolves to an existing route", () => {
    const saved = {
      entries: [
        { initialFen: root, line: [] },
        { initialFen: root, line: ["e2e4"] },
        { initialFen: root, line: ["e2e4", "e7e5"] },
      ],
      cursor: 1.5,
    };
    const history = restoreHistory(saved, root);
    expect(Number.isInteger(history.cursor)).toBe(true);
    expect(history.entries[history.cursor]).toBeDefined();
  });
});

import { describe, it, expect } from "vitest";
import {
  pushRoute,
  restoreHistory,
  chosenContinuation,
} from "../src/gameGraph/navigation";
import { moveDestinations, uciInput } from "../src/chess/boardInput";
import { BoardSession } from "../src/chess/BoardSession";
const root = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
describe("graph navigation domain adapters", () => {
  it("keeps history for repeated positions and truncates forward only after selecting a new path", () => {
    let h = restoreHistory(null, root);
    h = pushRoute(h, {
      initialFen: root,
      line: ["g1f3", "g8f6", "f3g1", "f6g8"],
      positionKey: "root",
    });
    expect(h.entries).toHaveLength(2);
    h = pushRoute({ ...h, cursor: 0 }, { initialFen: root, line: ["d2d4"] });
    expect(h.entries).toHaveLength(2);
    expect(h.entries[1].line).toEqual(["d2d4"]);
  });
  it("restores the previously chosen next half-move at a fork after prefix back", () => {
    let h = restoreHistory(null, root);
    h = pushRoute(h, { initialFen: root, line: ["e2e4", "c7c5", "g1f3"] });
    h = pushRoute(h, { initialFen: root, line: ["e2e4"] });
    expect(chosenContinuation(h)?.line).toEqual(["e2e4", "c7c5"]);
    h = pushRoute(h, { initialFen: root, line: ["d2d4"] });
    expect(chosenContinuation(h)).toBeUndefined();
  });
  it("caps and validates persisted route input", () => {
    expect(
      restoreHistory(
        { entries: [{ initialFen: root, line: ["bad"] }], cursor: 100 },
        root,
      ).entries[0].line,
    ).toEqual([]);
    let h = restoreHistory(null, root);
    for (let i = 0; i < 220; i++)
      h = pushRoute(h, {
        initialFen: root,
        line: ["e2e4"],
        positionKey: String(i),
      });
    expect(h.entries).toHaveLength(200);
    expect(h.cursor).toBe(199);
  });
  it("merges destinations for promotions and normalizes castling presentation", () => {
    expect(moveDestinations(["a7a8q", "a7a8n"]).get("a7")).toEqual(["a8"]);
    const s = new BoardSession();
    s.loadFen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
    expect(uciInput(s, "e1", "h1")).toBe("e1g1");
    expect(uciInput(s, "e1", "a1")).toBe("e1c1");
    expect(uciInput(s, "a7", "a8", "knight")).toBe("a7a8n");
  });
});

import { describe, it, expect } from "vitest";
import { BoardSession } from "../src/chess/BoardSession";
import { INITIAL_FEN } from "chessops/fen";
describe("Reference board behavior", () => {
  it("legal input, mainline branching, stale callbacks and navigation", () => {
    const s = new BoardSession();
    expect(s.fen).toBe(INITIAL_FEN);
    expect(s.play("e2", "e5")).toBe(false);
    expect(s.play("e2", "e4")).toBe(true);
    const revision = s.revision;
    expect(s.play("e7", "e5")).toBe(true);
    expect(s.play("g1", "f3", undefined, revision)).toBe(false);
    expect(s.pgn).toBe("1. e4 e5");
    s.seek(1);
    expect(s.play("c7", "c5")).toBe(true);
    expect(s.total).toBe(2);
    expect(s.pgn).toBe("1. e4 c5");
    s.seek(0);
    expect(s.fen).toBe(INITIAL_FEN);
  });
  it("castling by king destination and by rook square", () => {
    for (const destination of ["g1", "h1"] as const) {
      const s = new BoardSession();
      s.loadFen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
      expect(s.destinations.get("e1")).toContain(destination);
      expect(s.play("e1", destination)).toBe(true);
      expect(s.fen.split(" ")[0]).toBe("r3k2r/8/8/8/8/8/8/R4RK1");
      expect(s.lastMove).toEqual(["e1", "g1"]);
    }
  });
  it("en passant and knight promotion", () => {
    const s = new BoardSession();
    s.loadPgn("1. e4 a6 2. e5 d5");
    expect(s.play("e5", "d6")).toBe(true);
    expect(s.fen.split(" ")[0]).toBe(
      "rnbqkbnr/1pp1pppp/p2P4/8/8/8/PPPP1PPP/RNBQKBNR",
    );
    s.loadFen("7k/P7/8/8/8/8/8/7K w - - 0 1");
    expect(s.play("a7", "a8")).toBe(false);
    expect(s.promotionRequired("a7", "a8")).toBe(true);
    expect(s.play("a7", "a8", "knight")).toBe(true);
    expect(s.fen.startsWith("N6k/")).toBe(true);
  });
  it("annotations are attached to the current ply", () => {
    const s = new BoardSession();
    s.setShapes([
      { orig: "e2", dest: "e4", brush: "green" },
      { orig: "d4", brush: "red" },
    ]);
    s.play("e2", "e4");
    expect(s.shapes).toHaveLength(0);
    s.seek(0);
    expect(s.shapes).toHaveLength(2);
    s.setShapes([]);
    expect(s.shapes).toHaveLength(0);
    s.flip();
    expect(s.orientation).toBe("black");
  });
  it("imports legal PGN and FEN atomically, ignores variations like the reference", () => {
    const s = new BoardSession();
    s.loadPgn("1.e4 {note} e5 (1...c5) 2.Nf3 $1 Nc6 *");
    expect(s.pgn).toBe("1. e4 e5 2. Nf3 Nc6");
    const fen = s.fen;
    expect(() => s.loadPgn("1. e4 INVALID")).toThrow();
    expect(s.fen).toBe(fen);
    expect(() => s.loadFen("8/8/8/8/8/8/8/8 w - - 0 1")).toThrow();
    expect(s.fen).toBe(fen);
    expect(() => s.loadPgn('[Variant "Chess960"]\n\n1. e4')).toThrow();
    s.reset();
    expect(s.standardRoot).toBe(true);
  });
});

import { describe, expect, it } from "vitest";
import {
  documentPosition,
  indexChapter,
  moveLabel,
  notationEntries,
} from "../src/knowledge/navigation";
import type {
  KnowledgeChapter,
  KnowledgeDocument,
  VariationNode,
} from "../src/knowledge/types";

function node(
  id: string,
  uci: string | null,
  children: VariationNode[] = [],
  positionKey = id,
): VariationNode {
  return {
    id,
    uci,
    children,
    position_key: positionKey,
    fen: "8/8/8/8/8/8/8/8 b - - 0 1",
    ply: 0,
    san: uci ? "e4" : null,
    comments: [],
    nags: [],
    shapes: [],
  };
}
function chapter(root: VariationNode): KnowledgeChapter {
  return {
    id: "chapter-1",
    title: "Branches",
    initial_fen: "root-fen",
    headers: {},
    root,
    links: [],
  };
}

describe("shared Knowledge document navigation", () => {
  it("retains distinct nodes and exact paths through transpositions and repeated positions", () => {
    const first = node(
      "one",
      "g1f3",
      [node("repeat", "g8f6", [], "root-position")],
      "same-position",
    );
    const alternative = node("two", "b1c3", [
      node("three", "b8c6", [], "same-position"),
    ]);
    const value = chapter(
      node("root", null, [first, alternative], "root-position"),
    );
    const index = indexChapter(value);
    expect(index.size).toBe(5);
    expect(index.get("repeat")?.pathIds).toEqual(["root", "one"]);
    expect(index.get("repeat")?.pathUci).toEqual(["g1f3", "g8f6"]);
    expect(index.get("three")?.pathUci).toEqual(["b1c3", "b8c6"]);
    expect(index.get("one")?.node.position_key).toBe(
      index.get("three")?.node.position_key,
    );
    expect(index.get("root")?.node.position_key).toBe(
      index.get("repeat")?.node.position_key,
    );
  });

  it("creates revision-qualified links without replacing the initial FEN by a position key", () => {
    const value = chapter(node("root", null, [node("move", "e2e4")]));
    const document: KnowledgeDocument = {
      id: "doc",
      title: "Study",
      kind: "repertoire",
      side: "white",
      revision: 9,
      created_at_ms: 0,
      updated_at_ms: 1,
      metadata: {},
      chapters: [value],
    };
    expect(
      documentPosition(document, value, indexChapter(value).get("move")!),
    ).toEqual({
      document_id: "doc",
      revision: 9,
      chapter_id: "chapter-1",
      node_id: "move",
      position_key: "move",
      fen: "8/8/8/8/8/8/8/8 b - - 0 1",
      initial_fen: "root-fen",
      path_uci: ["e2e4"],
    });
  });

  it("uses stable IDs after mainline promotion and preserves sibling annotations", () => {
    const a = node("a", "e2e4"),
      b = node("b", "d2d4");
    b.comments = ["Prepared alternative"];
    b.nags = [1];
    const value = chapter(node("root", null, [a, b]));
    expect(
      notationEntries(value).map((entry) => [entry.node.id, entry.depth]),
    ).toEqual([
      ["a", 0],
      ["b", 1],
    ]);
    value.root.children.reverse();
    const promoted = indexChapter(value);
    expect(promoted.get("b")?.node.comments).toEqual(["Prepared alternative"]);
    expect(promoted.get("b")?.node.nags).toEqual([1]);
    expect(
      notationEntries(value).map((entry) => [entry.node.id, entry.depth]),
    ).toEqual([
      ["b", 0],
      ["a", 1],
    ]);
    expect(promoted.get("b")?.parentId).toBe("root");
  });

  it("indexes long lines without recursive traversal or materializing every ancestor path", () => {
    const root = node("root", null);
    let tail = root;
    for (let i = 0; i < 8000; i++) {
      const child = node(`move-${i}`, i % 2 ? "g8f6" : "g1f3");
      tail.children = [child];
      tail = child;
    }
    const value = chapter(root),
      index = indexChapter(value);
    expect(index.size).toBe(8001);
    expect(index.get("move-7999")?.pathUci).toHaveLength(8000);
    expect(notationEntries(value)).toHaveLength(8000);
  });

  it("refuses duplicate IDs instead of silently relinking a node", () => {
    expect(() =>
      indexChapter(
        chapter(
          node("root", null, [node("same", "e2e4"), node("same", "d2d4")]),
        ),
      ),
    ).toThrow(/идентификатор/);
  });

  it("labels moves from the full FEN turn and fullmove counter", () => {
    const first = node("move", "e2e4");
    expect(moveLabel(first)).toBe("1. e4");
    first.fen = "8/8/8/8/8/8/8/8 w - - 0 28";
    first.san = "Nf6";
    expect(moveLabel(first)).toBe("27… Nf6");
  });
});

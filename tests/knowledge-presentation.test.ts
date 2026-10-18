import { describe, expect, it } from "vitest";
import {
  nagLabel,
  notationProjection,
  parseNags,
} from "../src/knowledge/presentation";
import type { KnowledgeChapter, VariationNode } from "../src/knowledge/types";

function node(id: string, children: VariationNode[] = []): VariationNode {
  return {
    id,
    children,
    fen: "",
    position_key: id,
    ply: 0,
    uci: id,
    san: id,
    nags: [],
    comments: [],
    shapes: [],
  };
}
function chapter(root: VariationNode): KnowledgeChapter {
  return {
    id: "chapter",
    title: "Title",
    initial_fen: "",
    root,
    headers: {},
    links: [],
  };
}
function moves(value: ReturnType<typeof notationProjection>) {
  return value.tokens
    .filter((token) => token.type === "move")
    .map((token) => token.node.id);
}

describe("repertoire presentation over the shared tree", () => {
  it("keeps mainline sequential and expands alternatives only at their parent", () => {
    const value = chapter(
      node("root", [
        node("e4", [node("e5"), node("c5", [node("Nf3")])]),
        node("d4", [node("d5")]),
      ]),
    );
    const original = JSON.stringify(value);
    expect(moves(notationProjection(value, new Set()))).toEqual(["e4", "e5"]);
    expect(moves(notationProjection(value, new Set(["e4"])))).toEqual([
      "e4",
      "e5",
      "c5",
      "Nf3",
    ]);
    expect(moves(notationProjection(value, new Set(["root"])))).toEqual([
      "e4",
      "d4",
      "d5",
      "e5",
    ]);
    expect(JSON.stringify(value)).toBe(original);
  });

  it("leaves every hidden branch addressable by stable node ID", () => {
    const value = chapter(
      node("root", [node("a", [node("c")]), node("b", [node("d"), node("e")])]),
    );
    const collapsed = notationProjection(value, new Set());
    expect(
      collapsed.tokens.find(
        (token) => token.type === "branches" && token.parentId === "root",
      ),
    ).toMatchObject({ nodes: [{ id: "b" }] });
    expect(moves(notationProjection(value, new Set(["root", "b"])))).toEqual([
      "a",
      "b",
      "d",
      "e",
      "c",
    ]);
  });

  it("bounds a long line without recursive rendering or discarding the remainder", () => {
    const root = node("root");
    let tail = root;
    for (let i = 0; i < 8000; i++) {
      const child = node(`m${i}`);
      tail.children = [child];
      tail = child;
    }
    const limited = notationProjection(chapter(root), new Set(), 300);
    expect(moves(limited)).toHaveLength(300);
    expect(limited.remaining).toBe(true);
    expect(
      moves(notationProjection(chapter(root), new Set(), 9000)),
    ).toHaveLength(8000);
  });

  it("shows familiar symbols while retaining unknown supported NAGs exactly", () => {
    expect(nagLabel(1)).toBe("!");
    expect(nagLabel(14)).toBe("⩲");
    expect(nagLabel(220)).toBe("$220");
    expect(parseNags("$1 $220, 14 $220")).toEqual([1, 220, 14]);
    expect(() => parseNags("$256")).toThrow(/255/);
    expect(() => parseNags("!")).toThrow(/номера/);
  });
});

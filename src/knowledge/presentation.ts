import type { KnowledgeChapter, VariationNode } from "./types";

// Display labels only. The authoritative NAG numbers stay in the shared tree.
export const COMMON_NAGS = [
  [1, "!", "Хороший ход"],
  [2, "?", "Ошибка"],
  [3, "!!", "Отличный ход"],
  [4, "??", "Грубая ошибка"],
  [5, "!?", "Интересный ход"],
  [6, "?!", "Сомнительный ход"],
  [10, "=", "Равенство"],
  [13, "∞", "Неясная позиция"],
  [14, "⩲", "Небольшое преимущество белых"],
  [15, "⩱", "Небольшое преимущество чёрных"],
  [16, "±", "Преимущество белых"],
  [17, "∓", "Преимущество чёрных"],
  [18, "+−", "Решающее преимущество белых"],
  [19, "−+", "Решающее преимущество чёрных"],
] as const;

export function nagLabel(nag: number) {
  return COMMON_NAGS.find(([number]) => number === nag)?.[1] ?? `$${nag}`;
}

export function parseNags(text: string): number[] {
  const tokens = text.trim() ? text.trim().split(/[\s,]+/) : [];
  if (tokens.some((token) => !/^\$?\d+$/.test(token)))
    throw new Error("Укажите номера аннотаций, например $1 $14");
  const values = tokens.map((token) => Number(token.replace(/^\$/, "")));
  if (values.some((value) => value > 255))
    throw new Error("Допустимы номера аннотаций от 0 до 255");
  return [...new Set(values)];
}

export type NotationToken =
  | { type: "move"; node: VariationNode; depth: number }
  | {
      type: "branches";
      parentId: string;
      nodes: VariationNode[];
      depth: number;
    }
  | { type: "open" | "close"; id: string; depth: number };

// A bounded projection, never a second PGN tree. Mainline runs inline; local
// alternative groups are materialised only when expanded. Iterative for long PGN.
export function notationProjection(
  chapter: KnowledgeChapter,
  expanded: ReadonlySet<string>,
  limit = 300,
): { tokens: NotationToken[]; remaining: boolean } {
  type Task =
    { type: "children"; node: VariationNode; depth: number } | NotationToken;
  const stack: Task[] = [{ type: "children", node: chapter.root, depth: 0 }];
  const tokens: NotationToken[] = [];
  let moves = 0;
  while (stack.length && moves < limit) {
    const task = stack.pop()!;
    if (task.type !== "children") {
      tokens.push(task);
      if (task.type === "move") moves++;
      continue;
    }
    const [main, ...alternatives] = task.node.children;
    if (!main) continue;
    stack.push({ type: "children", node: main, depth: task.depth });
    if (alternatives.length) {
      if (expanded.has(task.node.id)) {
        for (let i = alternatives.length - 1; i >= 0; i--) {
          const alternative = alternatives[i];
          const depth = task.depth + 1;
          stack.push({ type: "close", id: alternative.id, depth });
          stack.push({ type: "children", node: alternative, depth });
          stack.push({ type: "move", node: alternative, depth });
          stack.push({ type: "open", id: alternative.id, depth });
        }
      }
      stack.push({
        type: "branches",
        parentId: task.node.id,
        nodes: alternatives,
        depth: task.depth,
      });
    }
    stack.push({ type: "move", node: main, depth: task.depth });
  }
  return { tokens, remaining: stack.length > 0 };
}

export function sideLabel(side: string) {
  return side === "white"
    ? "Белыми"
    : side === "black"
      ? "Чёрными"
      : "Обе стороны";
}

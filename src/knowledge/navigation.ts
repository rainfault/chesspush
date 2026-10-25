import type {
  DocumentPosition,
  KnowledgeChapter,
  KnowledgeDocument,
  RepertoireChapterRef,
  VariationNode,
} from "./types";

export function repertoireChapters(
  document: KnowledgeDocument,
): RepertoireChapterRef[] {
  const chapters = document.metadata.source_chapters;
  return Array.isArray(chapters) ? chapters : [];
}

export function repertoireChapterKey(
  chapter: Pick<RepertoireChapterRef, "document_id" | "chapter_id">,
) {
  return JSON.stringify([chapter.document_id, chapter.chapter_id]);
}

export interface IndexedVariation {
  node: VariationNode;
  parentId: string | null;
  pathIds: string[];
  pathUci: string[];
}

// Indexes the authoritative tree without parsing PGN or replaying chess rules.
export function indexChapter(chapter: KnowledgeChapter) {
  const index = new Map<string, IndexedVariation>();
  const stack: { node: VariationNode; parentId: string | null }[] = [
    { node: chapter.root, parentId: null },
  ];
  while (stack.length) {
    const item = stack.pop()!;
    if (index.has(item.node.id))
      throw new Error("Повторяющийся идентификатор узла документа");
    index.set(item.node.id, {
      ...item,
      get pathIds() {
        const ids: string[] = [];
        let parentId = item.parentId;
        while (parentId !== null) {
          ids.push(parentId);
          parentId = index.get(parentId)!.parentId;
        }
        return ids.reverse();
      },
      get pathUci() {
        const moves: string[] = [];
        let cursor:
          { node: VariationNode; parentId: string | null } | undefined = item;
        while (cursor) {
          if (cursor.node.uci) moves.push(cursor.node.uci);
          cursor =
            cursor.parentId === null ? undefined : index.get(cursor.parentId);
        }
        return moves.reverse();
      },
    });
    for (let i = item.node.children.length - 1; i >= 0; i--) {
      const child = item.node.children[i];
      stack.push({
        node: child,
        parentId: item.node.id,
      });
    }
  }
  return index;
}

export function documentPosition(
  document: KnowledgeDocument,
  chapter: KnowledgeChapter,
  indexed: IndexedVariation,
): DocumentPosition {
  return {
    document_id: document.id,
    revision: document.revision,
    chapter_id: chapter.id,
    node_id: indexed.node.id,
    position_key: indexed.node.position_key,
    fen: indexed.node.fen,
    initial_fen: chapter.initial_fen,
    path_uci: [...indexed.pathUci],
  };
}

export interface NotationEntry {
  node: VariationNode;
  depth: number;
  branchStart: boolean;
  branchEnd: boolean;
}

// A bounded, iterable view over the same tree; no second editable representation.
export function notationEntries(chapter: KnowledgeChapter): NotationEntry[] {
  const entries: NotationEntry[] = [];
  const stack: { node: VariationNode; depth: number; branchStart: boolean }[] =
    [];
  for (let i = chapter.root.children.length - 1; i >= 0; i--)
    stack.push({
      node: chapter.root.children[i],
      depth: i === 0 ? 0 : 1,
      branchStart: i > 0,
    });
  while (stack.length) {
    const item = stack.pop()!;
    entries.push({ ...item, branchEnd: !item.node.children.length });
    for (let i = item.node.children.length - 1; i >= 0; i--)
      stack.push({
        node: item.node.children[i],
        depth: item.depth + (i ? 1 : 0),
        branchStart: i > 0,
      });
  }
  return entries;
}

export function moveLabel(node: Pick<VariationNode, "san" | "fen">) {
  if (!node.san) return "Начало";
  const fields = node.fen.split(/\s+/);
  const fullmove = Number(fields[5] ?? 1);
  return fields[1] === "b"
    ? `${fullmove}. ${node.san}`
    : `${Math.max(1, fullmove - 1)}… ${node.san}`;
}

export type DocumentSide = "white" | "black" | "both";
export type DocumentKind = "repertoire" | "study";

export interface KnowledgeShape {
  brush: "green" | "red" | "blue" | "yellow";
  orig: string;
  dest?: string;
}

export interface KnowledgeLink {
  target:
    | { type: "position"; position_key: string }
    | { type: "occurrence"; game_id: string; revision: number; ply: number }
    | {
        type: "document_node";
        document_id: string;
        revision: number;
        chapter_id: string;
        node_id: string;
      };
  provenance: string;
  path_uci: string[];
  initial_fen?: string | null;
}

export interface VariationNode {
  id: string;
  fen: string;
  position_key: string;
  ply: number;
  uci: string | null;
  san: string | null;
  comments: string[];
  starting_comments?: string[];
  nags: number[];
  shapes: KnowledgeShape[];
  children: VariationNode[];
}

export interface KnowledgeChapter {
  id: string;
  title: string;
  initial_fen: string;
  headers: Record<string, string>;
  root: VariationNode;
  source_pgn?: string | null;
  links: KnowledgeLink[];
}

export interface KnowledgeDocument {
  id: string;
  kind: DocumentKind;
  title: string;
  side: DocumentSide;
  revision: number;
  created_at_ms: number;
  updated_at_ms: number;
  metadata: Record<string, unknown>;
  chapters: KnowledgeChapter[];
}

export interface RepertoireChapterRef {
  document_id: string;
  chapter_id: string;
  title: string;
  document_title: string;
  filename: string | null;
  root_node_id: string;
  initial_fen: string;
}

export interface DocumentSummary {
  id: string;
  kind: DocumentKind;
  title: string;
  side: DocumentSide;
  revision: number;
  updated_at_ms: number;
  chapters: number;
  attachment?: boolean;
  attachment_active?: boolean;
  filename?: string;
  openings?: { eco: string; name: string }[];
}

export interface DocumentNavigation {
  document_id: string;
  revision: number;
  chapter_id: string;
  node_id: string;
  path_node_ids: string[];
  path_uci: string[];
  initial_fen: string;
  fen: string;
  position_key: string;
  turn: "white" | "black";
  legal_moves: { uci: string; san: string }[];
}

export type DocumentOperation =
  | { type: "add_move"; chapter_id: string; parent_id: string; uci: string }
  | { type: "remove_variation"; chapter_id: string; node_id: string }
  | { type: "promote_variation"; chapter_id: string; node_id: string }
  | {
      type: "annotate";
      chapter_id: string;
      node_id: string;
      comments: string[];
      nags: number[];
      shapes: KnowledgeShape[];
    }
  | { type: "rename"; title: string; side: DocumentSide }
  | {
      type: "add_chapter";
      title: string;
      initial_fen?: string;
      links?: KnowledgeLink[];
    }
  | { type: "rename_chapter"; chapter_id: string; title: string }
  | { type: "remove_chapter"; chapter_id: string };

export interface DocumentPosition {
  document_id: string;
  revision: number;
  chapter_id: string;
  node_id: string;
  position_key: string;
  fen: string;
  initial_fen: string;
  path_uci: string[];
}

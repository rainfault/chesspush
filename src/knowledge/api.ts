import { invoke } from "@tauri-apps/api/core";
import type {
  DocumentKind,
  DocumentNavigation,
  DocumentOperation,
  DocumentSide,
  DocumentSummary,
  KnowledgeDocument,
  KnowledgeLink,
} from "./types";

export const listDocuments = () => invoke<DocumentSummary[]>("knowledge_list");

export const loadDocument = (documentId: string, revision?: number) =>
  invoke<KnowledgeDocument>("knowledge_load", {
    documentId,
    revision: revision ?? null,
  });

export const createDocument = (request: {
  title: string;
  kind?: DocumentKind;
  side: DocumentSide;
  initial_fen?: string;
  links?: KnowledgeLink[];
}) => invoke<KnowledgeDocument>("knowledge_create", { request });

export const importDocument = (request: {
  title: string;
  side: DocumentSide;
  pgn: string;
  document_id?: string;
  expected_revision?: number;
}) => invoke<KnowledgeDocument>("knowledge_import", { request });

export const applyDocument = (
  documentId: string,
  expectedRevision: number,
  operation: DocumentOperation,
) =>
  invoke<KnowledgeDocument>("knowledge_apply", {
    documentId,
    expectedRevision,
    operation,
  });

export const exportDocument = (documentId: string, revision?: number) =>
  invoke<string>("knowledge_export", {
    documentId,
    revision: revision ?? null,
  });

export const navigateDocument = (
  documentId: string,
  chapterId: string,
  nodeId: string,
  revision?: number,
) =>
  invoke<DocumentNavigation>("knowledge_navigate", {
    documentId,
    chapterId,
    nodeId,
    revision: revision ?? null,
  });

export const attachRepertoireFile = (path: string, side: "white" | "black") =>
  invoke<KnowledgeDocument>("knowledge_attach_file", { path, side });

export const setRepertoireAttachment = (
  documentId: string,
  expectedRevision: number,
  active: boolean,
) =>
  invoke<KnowledgeDocument>("knowledge_set_attachment", {
    documentId,
    expectedRevision,
    active,
  });

export const loadCombinedRepertoire = (side: "white" | "black") =>
  invoke<KnowledgeDocument>("knowledge_repertoire", { side });

export const exportCombinedRepertoire = (
  side: "white" | "black",
  path: string,
) => invoke<void>("knowledge_repertoire_export", { side, path });

export const deleteRepertoire = (
  documentId: string,
  expectedRevision: number,
) =>
  invoke<void>("knowledge_delete_repertoire", { documentId, expectedRevision });

import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { isTauri } from "@tauri-apps/api/core";
import { save as saveFile } from "@tauri-apps/plugin-dialog";
import type { DrawShape } from "@lichess-org/chessground/draw";
import { BoardSession } from "../chess/BoardSession";
import { ChessBoard } from "../chess/ChessBoard";
import { errorMessage } from "../api";
import { uciInput } from "../chess/boardInput";
import type { Role, SquareName } from "chessops/types";
import {
  applyDocument,
  createDocument,
  exportDocument,
  exportKnowledgeFile,
  importDocument,
  loadDocument,
  navigateDocument,
} from "./api";
import { documentPosition, indexChapter, moveLabel } from "./navigation";
import type {
  DocumentNavigation,
  DocumentKind,
  DocumentOperation,
  DocumentPosition,
  DocumentSide,
  KnowledgeDocument,
  KnowledgeLink,
  KnowledgeShape,
} from "./types";
import {
  COMMON_NAGS,
  nagLabel,
  notationProjection,
  parseNags,
  type NotationToken,
} from "./presentation";
import { KnowledgeTagsInput } from "./KnowledgeTagsInput";
import "./knowledge.css";

export interface KnowledgeEditorActions {
  document: KnowledgeDocument;
  navigation: DocumentNavigation | null;
  busy: boolean;
  dirty: boolean;
  playMove: (uci: string) => void;
  commit: (
    action: (
      document: KnowledgeDocument,
      position: DocumentNavigation,
    ) => Promise<KnowledgeDocument | void> | KnowledgeDocument | void,
  ) => void;
}

interface Props {
  documentKind?: DocumentKind;
  initialTitle?: string;
  initialSide?: DocumentSide;
  active?: boolean;
  documentId?: string;
  initialFen?: string;
  initialLinks?: KnowledgeLink[];
  initialChapterId?: string;
  initialNodeId?: string;
  onClose: () => void;
  onSaved?: (document: KnowledgeDocument) => void;
  onDocumentChange?: (document: KnowledgeDocument) => void;
  onPositionChange?: (position: DocumentNavigation | null) => void;
  toolbar?: ReactNode | ((actions: KnowledgeEditorActions) => ReactNode);
  sidePanel?: ReactNode | ((actions: KnowledgeEditorActions) => ReactNode);
  onNavigatePosition?: (position: DocumentPosition) => void;
  onContextChange?: (context: {
    documentId: string;
    chapterId: string;
    nodeId: string;
    title: string;
  }) => void;
  closeLabel?: string;
  onReturnToBranch?: () => void;
  initialPgnMode?: "import";
}

export function KnowledgeEditor({
  documentKind = "repertoire",
  initialTitle,
  initialSide,
  active = true,
  documentId,
  initialFen,
  initialLinks,
  initialChapterId,
  initialNodeId,
  onClose,
  onSaved,
  onDocumentChange,
  onPositionChange,
  toolbar,
  sidePanel,
  onNavigatePosition,
  onContextChange,
  closeLabel = "К репертуарам",
  onReturnToBranch,
  initialPgnMode,
}: Props) {
  const [document, setDocument] = useState<KnowledgeDocument | null>(null);
  const [chapterId, setChapterId] = useState("");
  const [nodeId, setNodeId] = useState("");
  const [title, setTitle] = useState(
    initialTitle ??
      (documentKind === "analysis"
        ? "Разбор партии"
        : documentKind === "study"
          ? "Новое знание"
          : "Новый репертуар"),
  );
  const [side, setSide] = useState<DocumentSide>(
    initialSide ?? (documentKind !== "repertoire" ? "both" : "white"),
  );
  const [rootFen, setRootFen] = useState(initialFen ?? "");
  const [comments, setComments] = useState("");
  const [nags, setNags] = useState("");
  const [shapes, setShapes] = useState<KnowledgeShape[]>([]);
  const [annotationDirty, setAnnotationDirty] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [navigation, setNavigation] = useState<DocumentNavigation | null>(null);
  const [board] = useState(() => new BoardSession());
  const [boardRevision, setBoardRevision] = useState(0);
  const [pgnMode, setPgnMode] = useState<"import" | "export" | null>(
    initialPgnMode ?? null,
  );
  const [pgn, setPgn] = useState("");
  const [newChapter, setNewChapter] = useState(false);
  const [newChapterTitle, setNewChapterTitle] = useState("");
  const [newChapterFen, setNewChapterFen] = useState("");
  const [chapterTitle, setChapterTitle] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<"node" | "chapter" | null>(
    null,
  );
  const [visibleMoves, setVisibleMoves] = useState(300);
  const [expandedBranches, setExpandedBranches] = useState<Set<string>>(
    new Set(),
  );
  const [restoredDraft, setRestoredDraft] = useState(false);
  const [orphanedText, setOrphanedText] = useState("");
  const [saveFailed, setSaveFailed] = useState(false);
  const [tags, setTags] = useState<string[]>([]);
  const [headers, setHeaders] = useState<Record<string, string>>({});
  const [newHeaderName, setNewHeaderName] = useState("");
  const [annotationTab, setAnnotationTab] = useState<
    "comments" | "annotations" | "pgn"
  >("comments");
  const mounted = useRef(true);
  const annotationTabsId = useId();
  const draftVersion = useRef(0);
  const fileReadVersion = useRef(0);
  const pendingAddress = useRef<{
    documentId: string;
    chapterId: string;
    nodeId: string;
  } | null>(null);
  const changeDraft = () => {
    draftVersion.current++;
  };
  const documentRef = useRef(document);
  const workspaceRef = useRef<HTMLDivElement>(null);
  const locked = useRef(false);
  const chapter = document?.chapters.find((item) => item.id === chapterId);
  const analysis = (document?.kind ?? documentKind) === "analysis";
  const study = (document?.kind ?? documentKind) !== "repertoire";
  const subject = analysis ? "разбора" : study ? "знания" : "репертуара";
  const callbacks = useRef({ onDocumentChange, onPositionChange });
  callbacks.current = { onDocumentChange, onPositionChange };
  const documentTags = (document?.metadata.tags ?? []) as unknown;
  const savedTags = Array.isArray(documentTags)
    ? documentTags.filter((tag): tag is string => typeof tag === "string")
    : [];
  const index = useMemo(() => chapter && indexChapter(chapter), [chapter]);
  const selected = index?.get(nodeId);
  const currentNavigation =
    navigation &&
    navigation.document_id === document?.id &&
    navigation.revision === document.revision &&
    navigation.chapter_id === chapterId &&
    navigation.node_id === nodeId
      ? navigation
      : null;
  const address = useRef({
    documentId: document?.id,
    revision: document?.revision,
    chapterId,
    nodeId,
  });
  address.current = {
    documentId: document?.id,
    revision: document?.revision,
    chapterId,
    nodeId,
  };
  const selectedPath = useMemo(
    () => new Set(selected?.pathIds ?? []),
    [selected],
  );
  const notation = useMemo(
    () =>
      chapter
        ? notationProjection(chapter, expandedBranches, visibleMoves)
        : { tokens: [], remaining: false },
    [chapter, expandedBranches, visibleMoves],
  );
  const notationGroups = useMemo(() => {
    const groups: { mainline: boolean; tokens: NotationToken[] }[] = [];
    for (const token of notation.tokens) {
      const previous = groups.at(-1);
      const mainline = token.type === "move" && token.depth === 0;
      const whiteMove =
        token.type === "move" && token.node.fen.split(/\s+/)[1] === "b";
      if (
        previous &&
        previous.mainline === mainline &&
        (!mainline || (!whiteMove && previous.tokens.length === 1))
      )
        previous.tokens.push(token);
      else groups.push({ mainline, tokens: [token] });
    }
    return groups;
  }, [notation]);
  const dirty =
    annotationDirty ||
    (!!document &&
      (title !== document.title ||
        side !== document.side ||
        chapterTitle !== chapter?.title ||
        (study &&
          (JSON.stringify(tags) !== JSON.stringify(savedTags) ||
            JSON.stringify(headers) !==
              JSON.stringify(chapter?.headers ?? {})))));

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  useEffect(() => {
    if (document) callbacks.current.onDocumentChange?.(document);
  }, [document]);
  useEffect(() => {
    callbacks.current.onPositionChange?.(active ? currentNavigation : null);
  }, [currentNavigation, active]);

  function saved(value: KnowledgeDocument) {
    if (mounted.current) onSaved?.(value);
  }

  function install(
    value: KnowledgeDocument,
    nextChapterId = chapterId,
    nextNodeId = nodeId,
    preserveDraft = false,
  ) {
    if (!mounted.current) return;
    const nextChapter =
      value.chapters.find((item) => item.id === nextChapterId) ??
      value.chapters[0];
    const nextIndex = nextChapter && indexChapter(nextChapter);
    const nextNode = nextIndex?.get(nextNodeId)?.node ?? nextChapter?.root;
    documentRef.current = value;
    setDocument(value);
    if (preserveDraft) return;
    try {
      localStorage.removeItem(`chesspush.knowledge.draft.${value.id}`);
    } catch {
      /* Storage unavailable: in-memory draft remains. */
    }
    setRestoredDraft(false);
    setSaveFailed(false);
    setTitle(value.title);
    setSide(value.side);
    setChapterId(nextChapter?.id ?? "");
    setChapterTitle(nextChapter?.title ?? "");
    setNodeId(nextNode?.id ?? "");
    setComments(nextNode?.comments.join("\n\n") ?? "");
    setNags(nextNode?.nags.map((nag) => `$${nag}`).join(" ") ?? "");
    setShapes(nextNode?.shapes ?? []);
    const values = value.metadata.tags;
    setTags(
      Array.isArray(values)
        ? values.filter((tag): tag is string => typeof tag === "string")
        : [],
    );
    setHeaders({ ...nextChapter?.headers });
    setAnnotationDirty(false);
    setDeleteTarget(null);
  }

  useEffect(() => {
    if (!documentId) return;
    pendingAddress.current = null;
    let disposed = false;
    setBusy(true);
    void loadDocument(documentId)
      .then((value) => {
        if (!disposed) {
          let draft: {
            chapterId: string;
            nodeId: string;
            title: string;
            side: DocumentSide;
            chapterTitle: string;
            comments: string;
            nags: string;
            shapes: KnowledgeShape[];
            tags?: string[];
            headers?: Record<string, string>;
          } | null = null;
          try {
            draft = JSON.parse(
              localStorage.getItem(`chesspush.knowledge.draft.${value.id}`) ??
                "null",
            );
          } catch {
            /* Invalid recovery draft never changes the authoritative document. */
          }
          const draftChapter = value.chapters.find(
            (item) => item.id === draft?.chapterId,
          );
          const recover =
            draft &&
            draftChapter &&
            indexChapter(draftChapter).has(draft.nodeId);
          if (draft && !recover) {
            const text =
              typeof draft.comments === "string"
                ? draft.comments
                : JSON.stringify(draft);
            try {
              localStorage.setItem(
                `chesspush.knowledge.orphan.${value.id}`,
                text,
              );
            } catch {
              /* Keep visible recovery. */
            }
            setOrphanedText(text);
          } else {
            try {
              setOrphanedText(
                localStorage.getItem(
                  `chesspush.knowledge.orphan.${value.id}`,
                ) ?? "",
              );
            } catch {
              /* Optional recovery. */
            }
          }
          install(
            value,
            recover ? draft!.chapterId : initialChapterId,
            recover ? draft!.nodeId : initialNodeId,
          );
          if (recover && draft) {
            setTitle(draft.title);
            setSide(draft.side);
            setChapterTitle(draft.chapterTitle);
            setComments(draft.comments);
            setNags(draft.nags);
            setShapes(draft.shapes);
            setTags(
              draft.tags ??
                (Array.isArray(value.metadata.tags)
                  ? value.metadata.tags.filter(
                      (tag): tag is string => typeof tag === "string",
                    )
                  : []),
            );
            setHeaders(draft.headers ?? { ...draftChapter!.headers });
            setAnnotationDirty(true);
            setRestoredDraft(true);
            changeDraft();
            if (initialChapterId || initialNodeId) {
              const requestedChapter =
                value.chapters.find((item) => item.id === initialChapterId) ??
                value.chapters[0];
              const requestedNode = requestedChapter
                ? (indexChapter(requestedChapter).get(initialNodeId ?? "")
                    ?.node ?? requestedChapter.root)
                : null;
              if (
                requestedChapter &&
                requestedNode &&
                (requestedChapter.id !== draft.chapterId ||
                  requestedNode.id !== draft.nodeId)
              )
                pendingAddress.current = {
                  documentId: value.id,
                  chapterId: requestedChapter.id,
                  nodeId: requestedNode.id,
                };
            }
          }
        }
      })
      .catch((reason) => {
        if (!disposed) setError(errorMessage(reason));
      })
      .finally(() => {
        if (!disposed) setBusy(false);
      });
    return () => {
      disposed = true;
    };
  }, [documentId, initialChapterId, initialNodeId]);

  useEffect(() => {
    const requested = pendingAddress.current;
    if (
      !requested ||
      busy ||
      !selected ||
      document?.id !== requested.documentId
    )
      return;
    pendingAddress.current = null;
    select(requested.chapterId, requested.nodeId);
  }, [busy, document?.id, chapterId, nodeId]);

  useEffect(() => {
    if (!document || !chapter || !selected) return;
    let disposed = false;
    setNavigation(null);
    board.loadFen(selected.node.fen);
    setBoardRevision(board.revision);
    void navigateDocument(
      document.id,
      chapter.id,
      selected.node.id,
      document.revision,
    )
      .then((value) => {
        if (!disposed) setNavigation(value);
      })
      .catch((reason) => {
        if (!disposed) setError(errorMessage(reason));
      });
    return () => {
      disposed = true;
    };
  }, [document?.id, document?.revision, chapter?.id, selected?.node.id]);

  useEffect(() => {
    if (
      document &&
      document.side !== "both" &&
      board.orientation !== document.side
    ) {
      board.flip();
      setBoardRevision(board.revision);
    }
  }, [document?.id, document?.side]);

  useEffect(() => {
    const node = workspaceRef.current?.querySelector<HTMLElement>(
      `[data-knowledge-node="${nodeId}"]`,
    );
    if (node?.getClientRects().length)
      node.scrollIntoView({ block: "nearest", behavior: "instant" });
  }, [nodeId, visibleMoves, expandedBranches]);

  useEffect(() => {
    if (!document || !chapter || !selected) return;
    // Expand only the local alternatives needed to expose a linked/selected node.
    setVisibleMoves((previous) => Math.max(previous, selected.node.ply + 30));
    setExpandedBranches((previous) => {
      const next = new Set(previous);
      const path = [...selected.pathIds, selected.node.id];
      for (let i = 1; i < path.length; i++) {
        const parent = index?.get(path[i - 1])?.node;
        if (parent && parent.children[0]?.id !== path[i]) next.add(parent.id);
      }
      return next.size === previous.size ? previous : next;
    });
    onContextChange?.({
      documentId: document.id,
      chapterId: chapter.id,
      nodeId,
      title: document.title,
    });
  }, [document?.id, chapterId, nodeId]);

  useEffect(() => {
    if (!document || !dirty) return;
    try {
      localStorage.setItem(
        `chesspush.knowledge.draft.${document.id}`,
        JSON.stringify({
          chapterId,
          nodeId,
          title,
          side,
          chapterTitle,
          comments,
          nags,
          shapes,
          tags,
          headers,
        }),
      );
    } catch {
      /* The visible draft still survives errors and node navigation. */
    }
  }, [
    document?.id,
    dirty,
    chapterId,
    nodeId,
    title,
    side,
    chapterTitle,
    comments,
    nags,
    shapes,
    tags,
    headers,
  ]);

  useEffect(() => {
    const guard = (event: BeforeUnloadEvent) => {
      if (dirty) {
        event.preventDefault();
        event.returnValue = "";
      }
    };
    window.addEventListener("beforeunload", guard);
    return () => window.removeEventListener("beforeunload", guard);
  }, [dirty]);

  async function reconcile() {
    const current = documentRef.current;
    if (!current) return;
    await run(async () => {
      const latest = await loadDocument(current.id);
      const latestChapter = latest.chapters.find(
        (item) => item.id === chapterId,
      );
      if (!latestChapter || !indexChapter(latestChapter).has(nodeId))
        throw new Error(
          "Узел удалён в другой версии. Ваш текст остаётся в редакторе; скопируйте его перед открытием другой главы.",
        );
      documentRef.current = latest;
      setDocument(latest);
      setError(
        "Загружена текущая версия. Ваши изменения сохранены в редакторе. Проверьте их и сохраните повторно.",
      );
      setSaveFailed(false);
    });
  }

  useEffect(() => {
    fileReadVersion.current++;
    if (!pgnMode) return;
    const previous = window.document.activeElement as HTMLElement | null;
    const dialog = window.document.querySelector<HTMLElement>(".knowledge-pgn");
    const focusable = () => [
      ...(dialog?.querySelectorAll<HTMLElement>(
        'button:not(:disabled),textarea:not(:disabled),input:not(:disabled),[tabindex="0"]',
      ) ?? []),
    ];
    dialog?.querySelector<HTMLElement>("textarea")?.focus();
    const keyboard = (event: KeyboardEvent) => {
      if (!dialog?.getClientRects().length) return;
      if (event.key === "Escape") {
        event.preventDefault();
        setPgnMode(null);
      }
      if (event.key !== "Tab") return;
      const items = focusable();
      if (!items.length) return;
      const current = window.document.activeElement;
      if (
        event.shiftKey &&
        (current === items[0] || !dialog?.contains(current))
      ) {
        event.preventDefault();
        items.at(-1)?.focus();
      } else if (
        !event.shiftKey &&
        (current === items.at(-1) || !dialog?.contains(current))
      ) {
        event.preventDefault();
        items[0].focus();
      }
    };
    window.addEventListener("keydown", keyboard);
    return () => {
      fileReadVersion.current++;
      window.removeEventListener("keydown", keyboard);
      previous?.focus();
    };
  }, [pgnMode]);

  async function persist(value: KnowledgeDocument) {
    let current = value;
    const numbers = annotationDirty ? parseNags(nags) : [];
    const apply = async (operation: DocumentOperation) => {
      current = await applyDocument(current.id, current.revision, operation);
      documentRef.current = current;
    };
    if (title !== current.title || side !== current.side)
      await apply({ type: "rename", title, side });
    const currentTags = current.metadata.tags;
    if (
      study &&
      JSON.stringify(tags) !==
        JSON.stringify(Array.isArray(currentTags) ? currentTags : [])
    )
      await apply({ type: "set_tags", tags });
    const currentChapter = current.chapters.find(
      (item) => item.id === chapterId,
    );
    if (currentChapter && chapterTitle !== currentChapter.title)
      await apply({
        type: "rename_chapter",
        chapter_id: chapterId,
        title: chapterTitle,
      });
    if (
      study &&
      currentChapter &&
      JSON.stringify(headers) !== JSON.stringify(currentChapter.headers)
    )
      await apply({
        type: "set_chapter_headers",
        chapter_id: chapterId,
        headers,
      });
    if (annotationDirty && currentChapter && selected) {
      await apply({
        type: "annotate",
        chapter_id: chapterId,
        node_id: nodeId,
        comments: comments.trim()
          ? comments
              .split(/\n\s*\n/)
              .map((text) => text.trim())
              .filter(Boolean)
          : [],
        nags: [...new Set(numbers)],
        shapes,
      });
    }
    return current;
  }

  async function run(action: () => Promise<void>) {
    if (locked.current || !mounted.current) return;
    locked.current = true;
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (reason) {
      if (!mounted.current) return;
      setError(errorMessage(reason));
      setSaveFailed(true);
      const latest = documentRef.current;
      if (latest) {
        setDocument(latest);
        const currentChapter = latest.chapters.find(
          (item) => item.id === chapterId,
        );
        const currentNode =
          currentChapter && indexChapter(currentChapter).get(nodeId)?.node;
        if (currentNode) {
          board.loadFen(currentNode.fen);
          setBoardRevision(board.revision);
        }
      }
    } finally {
      locked.current = false;
      if (mounted.current) setBusy(false);
    }
  }

  async function save() {
    const current = documentRef.current;
    if (!current) return;
    await run(async () => {
      const version = draftVersion.current;
      const value = await persist(current);
      install(value, chapterId, nodeId, version !== draftVersion.current);
      saved(value);
    });
  }

  function select(nextChapterId: string, nextNodeId: string) {
    const current = documentRef.current;
    if (!current) return;
    void run(async () => {
      const version = draftVersion.current;
      const value = await persist(current);
      if (version !== draftVersion.current) {
        install(value, chapterId, nodeId, true);
        return;
      }
      install(value, nextChapterId, nextNodeId);
      if (value.revision !== current.revision) saved(value);
    });
  }

  function mutate(
    operation: DocumentOperation,
    selectAfter?: (value: KnowledgeDocument) => [string, string],
  ) {
    const current = documentRef.current;
    if (!current) return;
    void run(async () => {
      const version = draftVersion.current;
      const persisted = await persist(current);
      const value = await applyDocument(
        persisted.id,
        persisted.revision,
        operation,
      );
      const target = selectAfter?.(value);
      install(
        value,
        target?.[0],
        target?.[1],
        version !== draftVersion.current,
      );
      saved(value);
    });
  }

  function addMove(orig: SquareName, dest: SquareName, promotion?: Role) {
    if (!chapter || !selected || !currentNavigation || busy) return;
    const uci = uciInput(board, orig, dest, promotion);
    if (!currentNavigation.legal_moves.some((move) => move.uci === uci)) {
      setError("Ход отсутствует в проверенном списке допустимых продолжений");
      setBoardRevision((value) => value + 1);
      return;
    }
    const existing = selected.node.children.find((child) => child.uci === uci);
    // Optimistic board presentation only: the persisted tree is changed by Rust.
    if (!board.play(orig, dest, promotion)) {
      setError("Не удалось отобразить проверенный ход");
      setBoardRevision((value) => value + 1);
      return;
    }
    setBoardRevision(board.revision);
    if (existing) {
      select(chapter.id, existing.id);
      return;
    }
    const parentId = selected.node.id;
    mutate(
      { type: "add_move", chapter_id: chapter.id, parent_id: parentId, uci },
      (value) => {
        const nextChapter = value.chapters.find(
          (item) => item.id === chapter.id,
        )!;
        const parent = indexChapter(nextChapter).get(parentId)!.node;
        return [
          chapter.id,
          parent.children.find((child) => child.uci === uci)!.id,
        ];
      },
    );
  }

  function playMove(uci: string) {
    // An engine suggestion can outlive its rendered position. Never apply that
    // action to a newer node merely because the UCI move is also legal there.
    const current = address.current;
    if (
      !active ||
      current.documentId !== document?.id ||
      current.revision !== document?.revision ||
      current.chapterId !== chapterId ||
      current.nodeId !== nodeId ||
      !/^[a-h][1-8][a-h][1-8][qrbn]?$/.test(uci)
    )
      return;
    const promotions: Record<string, Role> = {
      q: "queen",
      r: "rook",
      b: "bishop",
      n: "knight",
    };
    addMove(
      uci.slice(0, 2) as SquareName,
      uci.slice(2, 4) as SquareName,
      promotions[uci[4]],
    );
  }

  function close(after = onClose) {
    if (busy) return;
    if (!document) {
      after();
      return;
    }
    void run(async () => {
      const version = draftVersion.current;
      const value = await persist(documentRef.current!);
      install(value, chapterId, nodeId, version !== draftVersion.current);
      saved(value);
      if (mounted.current && version === draftVersion.current) after();
    });
  }

  function commit(action: Parameters<KnowledgeEditorActions["commit"]>[0]) {
    if (!documentRef.current || !chapter || !selected) return;
    void run(async () => {
      const version = draftVersion.current;
      const value = await persist(documentRef.current!);
      install(value, chapterId, nodeId, version !== draftVersion.current);
      saved(value);
      if (version !== draftVersion.current) return;
      const position = await navigateDocument(
        value.id,
        chapterId,
        nodeId,
        value.revision,
      );
      const changed = await action(value, position);
      if (changed) {
        install(changed, chapterId, nodeId, version !== draftVersion.current);
        saved(changed);
      }
    });
  }

  function endOfVariation() {
    if (!selected || !chapter) return;
    let tail = selected.node;
    while (tail.children[0]) tail = tail.children[0];
    select(chapter.id, tail.id);
  }

  useEffect(() => {
    const keyboard = (event: KeyboardEvent) => {
      if (!active || !workspaceRef.current?.getClientRects().length || pgnMode)
        return;
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
        event.preventDefault();
        void save();
        return;
      }
      const target = event.target instanceof HTMLElement ? event.target : null;
      if (
        target?.closest("input,textarea,select") ||
        busy ||
        !chapter ||
        !selected
      )
        return;
      if (event.key === "ArrowLeft" && selected.parentId) {
        event.preventDefault();
        select(chapter.id, selected.parentId);
      } else if (event.key === "ArrowRight" && selected.node.children[0]) {
        event.preventDefault();
        select(chapter.id, selected.node.children[0].id);
      } else if (event.key === "Home") {
        event.preventDefault();
        select(chapter.id, chapter.root.id);
      } else if (study && event.key === "End") {
        event.preventDefault();
        endOfVariation();
      }
    };
    window.addEventListener("keydown", keyboard);
    return () => window.removeEventListener("keydown", keyboard);
  });

  function renderNotationToken(
    token: NotationToken,
    tokenIndex: number,
    mainline = false,
  ) {
    if (!chapter) return null;

    if (token.type === "move") {
      const { node, depth } = token;
      return (
        <button
          key={node.id}
          aria-label={`${moveLabel(node)}${node.nags.length ? ` ${node.nags.map(nagLabel).join(" ")}` : ""}`}
          aria-current={node.id === nodeId ? "step" : undefined}
          data-knowledge-node={node.id}
          className={`knowledge-move ${mainline ? (node.fen.split(/\s+/)[1] === "b" ? "knowledge-move-white" : "knowledge-move-black") : ""} ${node.id === nodeId ? "active" : ""} ${selectedPath.has(node.id) ? "in-path" : ""} ${depth ? "variation" : ""}`}
          disabled={busy}
          onClick={() => select(chapter.id, node.id)}
        >
          <span>{mainline ? node.san : moveLabel(node)}</span>
          <span className="knowledge-node-marks">
            {node.nags.map(nagLabel).join(" ")}
            {node.comments.length ? " ·" : ""}
            {node.shapes.length ? " ◇" : ""}
          </span>
        </button>
      );
    }
    if (token.type === "branches")
      return (
        <button
          key={`branches-${token.parentId}`}
          className="knowledge-branch-toggle"
          aria-expanded={expandedBranches.has(token.parentId)}
          aria-label={`Варианты после ${index?.get(token.parentId)?.node.san ?? "начала"}: ${token.nodes.map((node) => node.san).join(", ")}`}
          onClick={() =>
            setExpandedBranches((previous) => {
              const next = new Set(previous);
              if (next.has(token.parentId)) next.delete(token.parentId);
              else next.add(token.parentId);
              return next;
            })
          }
        >
          {expandedBranches.has(token.parentId) ? "−" : "+"}{" "}
          {token.nodes.length === 1
            ? token.nodes[0].san
            : `${token.nodes.length} варианта`}
        </button>
      );
    return (
      <span
        key={`${token.type}-${token.id}-${tokenIndex}`}
        className={`knowledge-branch-${token.type}`}
        aria-hidden="true"
      >
        {token.type === "open" ? "(" : ")"}
      </span>
    );
  }

  return (
    <div
      className={`knowledge-workspace${study ? " knowledge-study" : ""}${analysis ? " knowledge-analysis" : ""}`}
      ref={workspaceRef}
    >
      <section
        className="knowledge-editor"
        aria-label={
          analysis
            ? "Редактор разбора партии"
            : study
              ? "Редактор знания"
              : "Редактор репертуара"
        }
      >
        <header className="knowledge-header">
          <button onClick={() => close()} disabled={busy}>
            {closeLabel}
          </button>
          <h1>
            {document
              ? document.title
              : study
                ? "Новое знание"
                : "Новый репертуар"}
          </h1>
          {onReturnToBranch && (
            <button disabled={busy} onClick={() => close(onReturnToBranch)}>
              К ветви
            </button>
          )}
          <span
            role="status"
            className={saveFailed ? "knowledge-save-error" : ""}
          >
            {busy
              ? document
                ? "Сохраняется…"
                : "Загрузка…"
              : saveFailed
                ? "Не удалось сохранить"
                : document
                  ? dirty
                    ? "Есть изменения"
                    : "Сохранено"
                  : ""}
          </span>
          {document && (
            <button
              className={dirty ? "primary" : ""}
              disabled={busy || !dirty}
              onClick={() => void save()}
            >
              Сохранить
            </button>
          )}
          {document && toolbar && (
            <div className="knowledge-editor-toolbar">
              {typeof toolbar === "function"
                ? toolbar({
                    document,
                    navigation: currentNavigation,
                    busy,
                    dirty,
                    commit,
                    playMove,
                  })
                : toolbar}
            </div>
          )}
          {document && (
            <details className="knowledge-document-menu">
              <summary>Документ</summary>
              <div className="knowledge-menu-panel">
                <label>
                  Название
                  <input
                    aria-label={`Название ${subject}`}
                    value={title}
                    onChange={(event) => {
                      changeDraft();
                      setTitle(event.target.value);
                    }}
                  />
                </label>
                <label>
                  Сторона
                  <select
                    aria-label={`Сторона ${subject}`}
                    value={side}
                    onChange={(event) => {
                      changeDraft();
                      setSide(event.target.value as DocumentSide);
                    }}
                  >
                    <option value="white">Белые</option>
                    <option value="black">Чёрные</option>
                    <option value="both">Обе стороны</option>
                  </select>
                </label>
                <button
                  disabled={busy}
                  onClick={() => {
                    setPgn("");
                    setPgnMode("import");
                    workspaceRef.current
                      ?.querySelector(".knowledge-document-menu")
                      ?.removeAttribute("open");
                  }}
                >
                  Импорт PGN
                </button>
                <button
                  disabled={busy}
                  onClick={() =>
                    void run(async () => {
                      const version = draftVersion.current;
                      const value = await persist(documentRef.current!);
                      install(
                        value,
                        chapterId,
                        nodeId,
                        version !== draftVersion.current,
                      );
                      setPgn(await exportDocument(value.id, value.revision));
                      setPgnMode("export");
                      workspaceRef.current
                        ?.querySelector(".knowledge-document-menu")
                        ?.removeAttribute("open");
                      saved(value);
                    })
                  }
                >
                  Экспорт PGN
                </button>
              </div>
            </details>
          )}
        </header>
        {orphanedText && (
          <details className="knowledge-recovery" open>
            <summary>
              Сохранён черновик узла, удалённого в другой версии
            </summary>
            <p>
              Текст не прикреплён к другой позиции. Скопируйте его в нужное
              место.
            </p>
            <textarea
              aria-label="Черновик удалённого узла"
              value={orphanedText}
              readOnly
            />
            <button
              onClick={() =>
                void navigator.clipboard
                  .writeText(orphanedText)
                  .catch((reason) => setError(errorMessage(reason)))
              }
            >
              Копировать черновик
            </button>
          </details>
        )}
        {restoredDraft && (
          <div className="knowledge-recovery" role="status">
            Восстановлены несохранённые изменения. Проверьте их перед
            сохранением.
          </div>
        )}
        {error && !pgnMode && (
          <div className="error knowledge-error" role="alert">
            <span>{error}</span>
            {document ? (
              <>
                <button disabled={busy} onClick={() => void save()}>
                  Повторить сохранение
                </button>
                <button disabled={busy} onClick={() => void reconcile()}>
                  Сверить с сохранённой версией
                </button>
              </>
            ) : (
              <button onClick={() => setError("")}>Закрыть сообщение</button>
            )}
          </div>
        )}
        {!document && (
          <div className="knowledge-create-form">
            <h2>{study ? "Создать знание" : "Создать репертуар"}</h2>
            <label>
              Название
              <input
                aria-label={`Название ${subject}`}
                value={title}
                disabled={busy}
                onChange={(event) => setTitle(event.target.value)}
                autoFocus
              />
            </label>
            <label>
              Сторона
              <select
                aria-label={`Сторона ${subject}`}
                value={side}
                disabled={busy}
                onChange={(event) =>
                  setSide(event.target.value as DocumentSide)
                }
              >
                <option value="white">Белые</option>
                <option value="black">Чёрные</option>
                <option value="both">Обе стороны</option>
              </select>
            </label>
            <details>
              <summary>Начальная позиция</summary>
              <label>
                FEN
                <input
                  aria-label={`Начальная FEN ${subject}`}
                  value={rootFen}
                  disabled={busy}
                  placeholder="Стандартная позиция"
                  onChange={(event) => setRootFen(event.target.value)}
                />
              </label>
            </details>
            {study && (
              <KnowledgeTagsInput
                label={analysis ? "Тег партии" : undefined}
                value={tags}
                disabled={busy}
                onChange={setTags}
              />
            )}
            <div className="knowledge-create-actions">
              <button
                className="primary"
                disabled={busy || !title.trim()}
                onClick={() =>
                  void run(async () => {
                    let value = await createDocument({
                      title,
                      kind: documentKind,
                      side,
                      initial_fen: rootFen.trim() || undefined,
                      links: initialLinks,
                    });
                    if (study && tags.length)
                      value = await applyDocument(value.id, value.revision, {
                        type: "set_tags",
                        tags,
                      });
                    install(value);
                    saved(value);
                  })
                }
              >
                Создать
              </button>
              <button
                disabled={busy}
                onClick={() => {
                  setPgn("");
                  setPgnMode("import");
                }}
              >
                Импортировать PGN
              </button>
            </div>
          </div>
        )}
        {document && chapter && selected && (
          <div
            className={study ? "knowledge-study-grid" : "knowledge-editor-body"}
          >
            <div className="knowledge-chapters">
              <label>
                Глава
                <select
                  aria-label={`Глава ${subject}`}
                  value={chapterId}
                  disabled={busy}
                  onChange={(event) => {
                    const next = document.chapters.find(
                      (item) => item.id === event.target.value,
                    )!;
                    select(next.id, next.root.id);
                    setVisibleMoves(300);
                  }}
                >
                  {document.chapters.map((item) => (
                    <option key={item.id} value={item.id}>
                      {item.title}
                    </option>
                  ))}
                </select>
              </label>
              {study && (
                <>
                  <h2 className="knowledge-chapter-heading">
                    Главы <span>{document.chapters.length}</span>
                  </h2>
                  <nav
                    className="knowledge-chapter-rail"
                    aria-label={analysis ? "Главы разбора" : "Главы знания"}
                  >
                    {document.chapters.map((item, chapterIndex) => (
                      <button
                        key={item.id}
                        disabled={busy}
                        aria-current={
                          item.id === chapterId ? "page" : undefined
                        }
                        onClick={() => {
                          select(item.id, item.root.id);
                          setVisibleMoves(300);
                        }}
                      >
                        <span>{chapterIndex + 1}</span>
                        <span>{item.title}</span>
                      </button>
                    ))}
                  </nav>
                </>
              )}
              <button
                disabled={busy}
                onClick={() => setNewChapter(!newChapter)}
              >
                Добавить главу
              </button>
              <details className="knowledge-chapter-menu">
                <summary>Настройки главы</summary>
                <div className="knowledge-menu-panel">
                  <label>
                    Название главы
                    <input
                      aria-label="Название главы"
                      value={chapterTitle}
                      onChange={(event) => {
                        changeDraft();
                        setChapterTitle(event.target.value);
                      }}
                    />
                  </label>
                  {study && (
                    <div className="knowledge-chapter-reorder">
                      <button
                        disabled={
                          busy || document.chapters[0]?.id === chapterId
                        }
                        onClick={() =>
                          mutate({
                            type: "move_chapter",
                            chapter_id: chapterId,
                            index:
                              document.chapters.findIndex(
                                (item) => item.id === chapterId,
                              ) - 1,
                          })
                        }
                      >
                        Выше
                      </button>
                      <button
                        disabled={
                          busy || document.chapters.at(-1)?.id === chapterId
                        }
                        onClick={() =>
                          mutate({
                            type: "move_chapter",
                            chapter_id: chapterId,
                            index:
                              document.chapters.findIndex(
                                (item) => item.id === chapterId,
                              ) + 1,
                          })
                        }
                      >
                        Ниже
                      </button>
                    </div>
                  )}
                  <button
                    disabled={busy || document.chapters.length < 2}
                    onClick={() => setDeleteTarget("chapter")}
                  >
                    Удалить главу
                  </button>
                </div>
              </details>
              {study && (
                <KnowledgeTagsInput
                  label={analysis ? "Тег партии" : undefined}
                  value={tags}
                  disabled={busy}
                  onChange={(value) => {
                    changeDraft();
                    setTags(value);
                  }}
                />
              )}
            </div>
            {newChapter && (
              <form
                className="knowledge-new-chapter"
                onSubmit={(event) => {
                  event.preventDefault();
                  const previousIds = new Set(
                    document.chapters.map((item) => item.id),
                  );
                  mutate(
                    {
                      type: "add_chapter",
                      title:
                        newChapterTitle ||
                        `Глава ${document.chapters.length + 1}`,
                      initial_fen: newChapterFen.trim() || undefined,
                    },
                    (value) => {
                      const added = value.chapters.find(
                        (item) => !previousIds.has(item.id),
                      )!;
                      setNewChapter(false);
                      setNewChapterTitle("");
                      setNewChapterFen("");
                      return [added.id, added.root.id];
                    },
                  );
                }}
              >
                <label>
                  Название
                  <input
                    aria-label="Название новой главы"
                    value={newChapterTitle}
                    disabled={busy}
                    onChange={(event) => setNewChapterTitle(event.target.value)}
                  />
                </label>
                <label className="knowledge-fen">
                  FEN
                  <input
                    aria-label="FEN новой главы"
                    value={newChapterFen}
                    disabled={busy}
                    placeholder="Стандартная позиция"
                    onChange={(event) => setNewChapterFen(event.target.value)}
                  />
                </label>
                <button disabled={busy} type="submit">
                  Создать главу
                </button>
                <button type="button" onClick={() => setNewChapter(false)}>
                  Отмена
                </button>
              </form>
            )}
            <div className="knowledge-layout">
              <div
                className="knowledge-board-panel"
                aria-busy={busy || !currentNavigation}
              >
                <ChessBoard
                  session={board}
                  revision={boardRevision}
                  onChange={() => setBoardRevision(board.revision)}
                  readOnly={!active || busy || !currentNavigation}
                  onMove={addMove}
                  allowedMoves={
                    currentNavigation?.legal_moves.map((move) => move.uci) ?? []
                  }
                  shapes={shapes as DrawShape[]}
                  onShapesChange={(value) => {
                    setShapes(
                      value
                        .filter((shape) =>
                          ["green", "red", "blue", "yellow"].includes(
                            shape.brush ?? "",
                          ),
                        )
                        .map((shape) => ({
                          brush: shape.brush as KnowledgeShape["brush"],
                          orig: shape.orig,
                          ...(shape.dest ? { dest: shape.dest } : {}),
                        })),
                    );
                    setAnnotationDirty(true);
                    changeDraft();
                  }}
                />
                <div className="knowledge-board-controls">
                  <button
                    title="Начало главы"
                    aria-label="Начало главы"
                    disabled={busy || !selected.parentId}
                    onClick={() => select(chapter.id, chapter.root.id)}
                  >
                    │‹
                  </button>
                  <button
                    title="Предыдущий ход"
                    aria-label={
                      study
                        ? "Предыдущий ход знания"
                        : "Предыдущий ход репертуара"
                    }
                    disabled={busy || !selected.parentId}
                    onClick={() =>
                      selected.parentId && select(chapter.id, selected.parentId)
                    }
                  >
                    ‹
                  </button>
                  <button
                    title="Следующий ход"
                    aria-label={
                      study
                        ? "Следующий ход знания"
                        : "Следующий ход репертуара"
                    }
                    disabled={busy || !selected.node.children.length}
                    onClick={() =>
                      select(chapter.id, selected.node.children[0].id)
                    }
                  >
                    ›
                  </button>
                  <button
                    title="Перевернуть доску"
                    aria-label={
                      study
                        ? "Перевернуть доску знания"
                        : "Перевернуть доску репертуара"
                    }
                    onClick={() => {
                      board.flip();
                      setBoardRevision(board.revision);
                    }}
                  >
                    ↻
                  </button>
                  {study && (
                    <button
                      title="Конец варианта"
                      aria-label="Конец варианта"
                      disabled={busy || !selected.node.children.length}
                      onClick={endOfVariation}
                    >
                      ›│
                    </button>
                  )}
                  {onNavigatePosition && (
                    <button
                      disabled={busy}
                      onClick={() =>
                        void run(async () => {
                          const version = draftVersion.current;
                          const value = await persist(documentRef.current!);
                          install(
                            value,
                            chapterId,
                            nodeId,
                            version !== draftVersion.current,
                          );
                          saved(value);
                          if (version !== draftVersion.current) return;
                          const currentChapter = value.chapters.find(
                            (item) => item.id === chapter.id,
                          )!;
                          const currentNode =
                            indexChapter(currentChapter).get(nodeId)!;
                          onNavigatePosition(
                            documentPosition(
                              value,
                              currentChapter,
                              currentNode,
                            ),
                          );
                        })
                      }
                    >
                      {study ? "Открыть на графе" : "Открыть на карте"}
                    </button>
                  )}
                </div>
                <details className="knowledge-technical">
                  <summary>Технические сведения</summary>
                  <label>
                    FEN
                    <input
                      className="knowledge-position-fen"
                      aria-label={
                        study
                          ? "FEN выбранной позиции знания"
                          : "FEN выбранной позиции репертуара"
                      }
                      readOnly
                      value={selected.node.fen}
                      onFocus={(event) => event.target.select()}
                    />
                  </label>
                  <p>Версия документа: {document.revision}</p>
                </details>
              </div>
              <div className="knowledge-content">
                <div
                  className={`knowledge-moves-panel${sidePanel ? " has-side-panel" : ""}`}
                >
                  {sidePanel && (
                    <div className="knowledge-side-panel">
                      {typeof sidePanel === "function"
                        ? sidePanel({
                            document,
                            navigation: currentNavigation,
                            busy,
                            dirty,
                            commit,
                            playMove,
                          })
                        : sidePanel}
                    </div>
                  )}
                  <div
                    className="knowledge-notation"
                    aria-label={`Варианты ${subject}`}
                  >
                    <button
                      className={
                        selected.node.id === chapter.root.id
                          ? "active knowledge-root"
                          : "knowledge-root"
                      }
                      aria-current={
                        selected.node.id === chapter.root.id
                          ? "step"
                          : undefined
                      }
                      disabled={busy}
                      onClick={() => select(chapter.id, chapter.root.id)}
                    >
                      Начало
                    </button>
                    <div className="knowledge-moves">
                      {study
                        ? notationGroups.map((group, groupIndex) => (
                            <div
                              key={groupIndex}
                              className={
                                group.mainline
                                  ? "knowledge-mainline-row"
                                  : "knowledge-variation-line"
                              }
                            >
                              {group.mainline &&
                                group.tokens[0].type === "move" && (
                                  <span
                                    className="knowledge-move-number"
                                    aria-hidden="true"
                                  >
                                    {
                                      moveLabel(group.tokens[0].node).split(
                                        /[.…]/,
                                      )[0]
                                    }
                                  </span>
                                )}
                              {group.tokens.map((token, tokenIndex) =>
                                renderNotationToken(
                                  token,
                                  tokenIndex,
                                  group.mainline,
                                ),
                              )}
                            </div>
                          ))
                        : notation.tokens.map((token, tokenIndex) =>
                            renderNotationToken(token, tokenIndex),
                          )}
                      {notation.remaining && (
                        <button
                          onClick={() =>
                            setVisibleMoves((value) => value + 300)
                          }
                        >
                          Показать следующие ходы
                        </button>
                      )}
                    </div>
                  </div>
                  <div className="knowledge-variation-actions">
                    <button
                      disabled={busy || !selected.parentId}
                      onClick={() =>
                        mutate({
                          type: "promote_variation",
                          chapter_id: chapter.id,
                          node_id: nodeId,
                        })
                      }
                    >
                      Главный вариант
                    </button>
                    <button
                      disabled={busy || !selected.parentId}
                      onClick={() => setDeleteTarget("node")}
                    >
                      Удалить ветвь
                    </button>
                  </div>
                  {deleteTarget && (
                    <div className="knowledge-delete">
                      <button
                        className="danger"
                        disabled={busy}
                        onClick={() => {
                          if (deleteTarget === "node")
                            mutate(
                              {
                                type: "remove_variation",
                                chapter_id: chapter.id,
                                node_id: nodeId,
                              },
                              () => [chapter.id, selected.parentId!],
                            );
                          else
                            mutate(
                              {
                                type: "remove_chapter",
                                chapter_id: chapter.id,
                              },
                              (value) => [
                                value.chapters[0].id,
                                value.chapters[0].root.id,
                              ],
                            );
                        }}
                      >
                        {deleteTarget === "node"
                          ? "Удалить ветвь и продолжения"
                          : "Удалить главу с вариантами"}
                      </button>
                      <button
                        disabled={busy}
                        onClick={() => setDeleteTarget(null)}
                      >
                        Отмена
                      </button>
                    </div>
                  )}
                </div>
                <div className="knowledge-annotations">
                  {study && (
                    <nav
                      className="knowledge-study-tabs"
                      role="tablist"
                      aria-label="Материал знания"
                      onKeyDown={(event) => {
                        if (
                          !["ArrowLeft", "ArrowRight", "Home", "End"].includes(
                            event.key,
                          )
                        )
                          return;
                        event.preventDefault();
                        event.stopPropagation();
                        const tabs = [
                          "comments",
                          "annotations",
                          "pgn",
                        ] as const;
                        const current = tabs.indexOf(annotationTab);
                        const index =
                          event.key === "Home"
                            ? 0
                            : event.key === "End"
                              ? 2
                              : (current +
                                  (event.key === "ArrowRight" ? 1 : 2)) %
                                3;
                        setAnnotationTab(tabs[index]);
                        event.currentTarget
                          .querySelectorAll<HTMLButtonElement>('[role="tab"]')
                          [index]?.focus();
                      }}
                    >
                      <button
                        type="button"
                        role="tab"
                        id={`${annotationTabsId}-comments`}
                        aria-controls={`${annotationTabsId}-comments-panel`}
                        tabIndex={annotationTab === "comments" ? 0 : -1}
                        aria-selected={annotationTab === "comments"}
                        onClick={() => setAnnotationTab("comments")}
                      >
                        Комментарий
                      </button>
                      <button
                        type="button"
                        role="tab"
                        id={`${annotationTabsId}-annotations`}
                        aria-controls={`${annotationTabsId}-annotations-panel`}
                        tabIndex={annotationTab === "annotations" ? 0 : -1}
                        aria-selected={annotationTab === "annotations"}
                        onClick={() => setAnnotationTab("annotations")}
                      >
                        Аннотации
                      </button>
                      <button
                        type="button"
                        role="tab"
                        id={`${annotationTabsId}-pgn`}
                        aria-controls={`${annotationTabsId}-pgn-panel`}
                        tabIndex={annotationTab === "pgn" ? 0 : -1}
                        aria-selected={annotationTab === "pgn"}
                        onClick={() => setAnnotationTab("pgn")}
                      >
                        PGN / FEN
                      </button>
                    </nav>
                  )}
                  <div
                    hidden={study && annotationTab !== "comments"}
                    className="knowledge-comment-editor"
                    role={study ? "tabpanel" : undefined}
                    id={
                      study ? `${annotationTabsId}-comments-panel` : undefined
                    }
                    aria-labelledby={
                      study ? `${annotationTabsId}-comments` : undefined
                    }
                  >
                    <label>
                      Комментарий
                      <textarea
                        aria-label="Комментарий к позиции"
                        value={comments}
                        onChange={(event) => {
                          setComments(event.target.value);
                          setAnnotationDirty(true);
                          changeDraft();
                        }}
                      />
                    </label>
                  </div>
                  <div
                    hidden={study && annotationTab !== "annotations"}
                    className="knowledge-annotation-editor"
                    role={study ? "tabpanel" : undefined}
                    id={
                      study
                        ? `${annotationTabsId}-annotations-panel`
                        : undefined
                    }
                    aria-labelledby={
                      study ? `${annotationTabsId}-annotations` : undefined
                    }
                  >
                    <details
                      className="knowledge-annotation-menu"
                      open={study || undefined}
                    >
                      <summary>
                        Оценка хода
                        {nags
                          ? ` · ${nags
                              .split(/\s+/)
                              .filter(Boolean)
                              .map((value) =>
                                nagLabel(Number(value.replace("$", ""))),
                              )
                              .join(" ")}`
                          : ""}
                      </summary>
                      <div
                        className="knowledge-nag-options"
                        role="group"
                        aria-label="Шахматные аннотации"
                      >
                        {COMMON_NAGS.map(([number, symbol, label]) => {
                          const active = nags
                            .split(/[\s,]+/)
                            .some(
                              (value) =>
                                value === `$${number}` ||
                                value === String(number),
                            );
                          return (
                            <button
                              key={number}
                              title={label}
                              aria-label={`${symbol} — ${label}`}
                              aria-pressed={active}
                              onClick={() => {
                                try {
                                  const values = parseNags(nags);
                                  setNags(
                                    (active
                                      ? values.filter(
                                          (value) => value !== number,
                                        )
                                      : [...values, number]
                                    )
                                      .map((value) => `$${value}`)
                                      .join(" "),
                                  );
                                  setAnnotationDirty(true);
                                  changeDraft();
                                } catch (reason) {
                                  setError(errorMessage(reason));
                                }
                              }}
                            >
                              {symbol}
                            </button>
                          );
                        })}
                      </div>
                      <details>
                        <summary>Номера аннотаций</summary>
                        <label>
                          NAG
                          <input
                            aria-label="NAG выбранного хода"
                            value={nags}
                            onChange={(event) => {
                              setNags(event.target.value);
                              setAnnotationDirty(true);
                              changeDraft();
                            }}
                          />
                        </label>
                      </details>
                    </details>
                    {shapes.length > 0 && (
                      <div className="knowledge-annotation-actions">
                        <button
                          disabled={busy}
                          onClick={() => {
                            setShapes([]);
                            setAnnotationDirty(true);
                            changeDraft();
                          }}
                        >
                          Очистить стрелки
                        </button>
                      </div>
                    )}
                  </div>
                  {study && (
                    <div
                      hidden={annotationTab !== "pgn"}
                      className="knowledge-headers-editor"
                      role="tabpanel"
                      id={`${annotationTabsId}-pgn-panel`}
                      aria-labelledby={`${annotationTabsId}-pgn`}
                    >
                      <label>
                        FEN
                        <input
                          aria-label={analysis ? "FEN разбора" : "FEN знания"}
                          readOnly
                          value={selected.node.fen}
                          onFocus={(event) => event.target.select()}
                        />
                      </label>
                      <div
                        className="knowledge-header-fields"
                        aria-label="PGN теги главы"
                      >
                        {Object.entries(headers).map(([name, value]) => (
                          <label key={name}>
                            <span>{name}</span>
                            <input
                              aria-label={`PGN ${name}`}
                              value={value}
                              readOnly={["FEN", "SetUp", "Variant"].includes(
                                name,
                              )}
                              onChange={(event) => {
                                changeDraft();
                                setHeaders((previous) => ({
                                  ...previous,
                                  [name]: event.target.value,
                                }));
                              }}
                            />
                            {!["FEN", "SetUp", "Variant"].includes(name) && (
                              <button
                                type="button"
                                aria-label={`Удалить PGN тег ${name}`}
                                disabled={busy}
                                onClick={() => {
                                  changeDraft();
                                  setHeaders((previous) =>
                                    Object.fromEntries(
                                      Object.entries(previous).filter(
                                        ([key]) => key !== name,
                                      ),
                                    ),
                                  );
                                }}
                              >
                                ×
                              </button>
                            )}
                          </label>
                        ))}
                      </div>
                      <form
                        className="knowledge-add-header"
                        onSubmit={(event) => {
                          event.preventDefault();
                          const name = newHeaderName.trim();
                          if (!name || Object.hasOwn(headers, name)) return;
                          if (["FEN", "SetUp", "Variant"].includes(name)) {
                            setError(
                              "FEN, SetUp и Variant задаются начальной позицией главы",
                            );
                            return;
                          }
                          changeDraft();
                          setHeaders((previous) => ({
                            ...previous,
                            [name]: "",
                          }));
                          setNewHeaderName("");
                        }}
                      >
                        <input
                          aria-label="Название PGN тега"
                          value={newHeaderName}
                          disabled={busy}
                          onChange={(event) =>
                            setNewHeaderName(event.target.value)
                          }
                        />
                        <button
                          type="submit"
                          disabled={busy || !newHeaderName.trim()}
                        >
                          Добавить PGN тег
                        </button>
                      </form>
                      <div className="knowledge-pgn-tools">
                        <button
                          disabled={busy}
                          onClick={() => {
                            setPgn("");
                            setPgnMode("import");
                          }}
                        >
                          Импорт PGN
                        </button>
                        <button
                          disabled={busy}
                          onClick={() =>
                            void run(async () => {
                              const value = await persist(documentRef.current!);
                              install(value, chapterId, nodeId);
                              setPgn(
                                await exportDocument(value.id, value.revision),
                              );
                              setPgnMode("export");
                              saved(value);
                            })
                          }
                        >
                          Экспорт PGN
                        </button>
                      </div>
                    </div>
                  )}
                </div>
              </div>
            </div>
          </div>
        )}
        {pgnMode && (
          <div className="knowledge-task-backdrop">
            <section
              role="dialog"
              aria-modal="true"
              className="knowledge-pgn"
              aria-label={pgnMode === "import" ? "Импорт PGN" : "Экспорт PGN"}
            >
              <header>
                <h3>{pgnMode === "import" ? "Импорт PGN" : "Экспорт PGN"}</h3>
                <button
                  aria-label="Закрыть PGN"
                  onClick={() => setPgnMode(null)}
                >
                  ×
                </button>
              </header>
              {error && (
                <div className="error knowledge-error" role="alert">
                  <span>{error}</span>
                  <button onClick={() => setError("")}>
                    Закрыть сообщение
                  </button>
                </div>
              )}
              {!document && (
                <div className="knowledge-import-metadata">
                  <label>
                    Название
                    <input
                      aria-label={
                        study
                          ? "Название импортируемого знания"
                          : "Название импортируемого репертуара"
                      }
                      value={title}
                      disabled={busy}
                      onChange={(event) => setTitle(event.target.value)}
                    />
                  </label>
                  <label>
                    Сторона
                    <select
                      aria-label={
                        study
                          ? "Сторона импортируемого знания"
                          : "Сторона импортируемого репертуара"
                      }
                      value={side}
                      disabled={busy}
                      onChange={(event) =>
                        setSide(event.target.value as DocumentSide)
                      }
                    >
                      <option value="white">Белые</option>
                      <option value="black">Чёрные</option>
                      <option value="both">Обе стороны</option>
                    </select>
                  </label>
                </div>
              )}
              <textarea
                aria-label={
                  pgnMode === "import"
                    ? "PGN для импорта"
                    : "Экспортированный PGN"
                }
                value={pgn}
                readOnly={pgnMode === "export"}
                disabled={busy}
                onChange={(event) => {
                  fileReadVersion.current++;
                  setPgn(event.target.value);
                }}
              />
              <div className="knowledge-pgn-actions">
                {pgnMode === "import" ? (
                  <>
                    <label className="knowledge-file">
                      Открыть файл
                      <input
                        type="file"
                        aria-label="Открыть PGN-файл"
                        accept=".pgn,text/plain,application/x-chess-pgn"
                        disabled={busy}
                        onChange={(event) => {
                          const file = event.target.files?.[0];
                          const version = ++fileReadVersion.current;
                          if (file && file.size > 10 * 1024 * 1024) {
                            setError("PGN-файл превышает 10 МБ");
                            return;
                          }
                          if (file)
                            void file
                              .text()
                              .then((text) => {
                                if (
                                  mounted.current &&
                                  version === fileReadVersion.current
                                )
                                  setPgn(text);
                              })
                              .catch((reason) => {
                                if (
                                  mounted.current &&
                                  version === fileReadVersion.current
                                )
                                  setError(errorMessage(reason));
                              });
                        }}
                      />
                    </label>
                    <button
                      className="primary"
                      disabled={busy || !pgn.trim()}
                      onClick={() =>
                        void run(async () => {
                          const persisted = document
                            ? await persist(document)
                            : null;
                          const previous = new Set(
                            persisted?.chapters.map((item) => item.id) ?? [],
                          );
                          let value = await importDocument({
                            title,
                            kind: persisted?.kind ?? documentKind,
                            side,
                            pgn,
                            document_id: persisted?.id,
                            expected_revision: persisted?.revision,
                          });
                          if (!persisted && study && tags.length) {
                            const importedTags = Array.isArray(
                              value.metadata.tags,
                            )
                              ? value.metadata.tags.filter(
                                  (tag): tag is string =>
                                    typeof tag === "string",
                                )
                              : [];
                            value = await applyDocument(
                              value.id,
                              value.revision,
                              {
                                type: "set_tags",
                                tags: [...importedTags, ...tags],
                              },
                            );
                          }
                          const first =
                            value.chapters.find(
                              (item) => !previous.has(item.id),
                            ) ?? value.chapters[0];
                          install(value, first.id, first.root.id);
                          setPgnMode(null);
                          saved(value);
                        })
                      }
                    >
                      {document
                        ? "Добавить главы из PGN"
                        : study
                          ? "Импортировать знание"
                          : "Импортировать репертуар"}
                    </button>
                  </>
                ) : (
                  <>
                    <button
                      onClick={() =>
                        void navigator.clipboard
                          .writeText(pgn)
                          .catch((reason) => setError(errorMessage(reason)))
                      }
                    >
                      Копировать PGN
                    </button>
                    <button
                      disabled={busy}
                      onClick={() => {
                        if (isTauri() && document) {
                          void run(async () => {
                            const path = await saveFile({
                              title: "Сохранить PGN",
                              defaultPath: `${title.replace(/[<>:"/\\|?*]+/g, "_") || "knowledge"}.pgn`,
                              filters: [{ name: "PGN", extensions: ["pgn"] }],
                            });
                            if (path)
                              await exportKnowledgeFile(
                                document.id,
                                path,
                                document.revision,
                              );
                          });
                          return;
                        }
                        const blob = new Blob([pgn], {
                          type: "application/x-chess-pgn;charset=utf-8",
                        });
                        const url = URL.createObjectURL(blob);
                        const link = window.document.createElement("a");
                        link.href = url;
                        link.download = `${title.replace(/[<>:"/\\|?*]+/g, "_") || "repertoire"}.pgn`;
                        link.click();
                        setTimeout(() => URL.revokeObjectURL(url), 1000);
                      }}
                    >
                      Скачать PGN
                    </button>
                  </>
                )}
              </div>
            </section>
          </div>
        )}
      </section>
    </div>
  );
}

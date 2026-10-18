import { useEffect, useMemo, useRef, useState } from "react";
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
  importDocument,
  loadDocument,
  navigateDocument,
} from "./api";
import { documentPosition, indexChapter, moveLabel } from "./navigation";
import type {
  DocumentNavigation,
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
} from "./presentation";
import "./knowledge.css";

interface Props {
  documentId?: string;
  initialFen?: string;
  initialLinks?: KnowledgeLink[];
  initialChapterId?: string;
  initialNodeId?: string;
  onClose: () => void;
  onSaved?: (document: KnowledgeDocument) => void;
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
  documentId,
  initialFen,
  initialLinks,
  initialChapterId,
  initialNodeId,
  onClose,
  onSaved,
  onNavigatePosition,
  onContextChange,
  closeLabel = "К репертуарам",
  onReturnToBranch,
  initialPgnMode,
}: Props) {
  const [document, setDocument] = useState<KnowledgeDocument | null>(null);
  const [chapterId, setChapterId] = useState("");
  const [nodeId, setNodeId] = useState("");
  const [title, setTitle] = useState("Новый репертуар");
  const [side, setSide] = useState<DocumentSide>("white");
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
  const draftVersion = useRef(0);
  const changeDraft = () => {
    draftVersion.current++;
  };
  const documentRef = useRef(document);
  const workspaceRef = useRef<HTMLDivElement>(null);
  const locked = useRef(false);
  const chapter = document?.chapters.find((item) => item.id === chapterId);
  const index = useMemo(() => chapter && indexChapter(chapter), [chapter]);
  const selected = index?.get(nodeId);
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
  const dirty =
    annotationDirty ||
    (!!document &&
      (title !== document.title ||
        side !== document.side ||
        chapterTitle !== chapter?.title));

  function install(
    value: KnowledgeDocument,
    nextChapterId = chapterId,
    nextNodeId = nodeId,
    preserveDraft = false,
  ) {
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
    setAnnotationDirty(false);
    setDeleteTarget(null);
  }

  useEffect(() => {
    if (!documentId) return;
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
            setAnnotationDirty(true);
            setRestoredDraft(true);
            changeDraft();
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
    const currentChapter = current.chapters.find(
      (item) => item.id === chapterId,
    );
    if (currentChapter && chapterTitle !== currentChapter.title)
      await apply({
        type: "rename_chapter",
        chapter_id: chapterId,
        title: chapterTitle,
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
    if (locked.current) return;
    locked.current = true;
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (reason) {
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
      setBusy(false);
    }
  }

  async function save() {
    const current = documentRef.current;
    if (!current) return;
    await run(async () => {
      const version = draftVersion.current;
      const value = await persist(current);
      install(value, chapterId, nodeId, version !== draftVersion.current);
      onSaved?.(value);
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
      if (value.revision !== current.revision) onSaved?.(value);
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
      onSaved?.(value);
    });
  }

  function addMove(orig: SquareName, dest: SquareName, promotion?: Role) {
    if (!chapter || !selected || !navigation || busy) return;
    const uci = uciInput(board, orig, dest, promotion);
    if (!navigation.legal_moves.some((move) => move.uci === uci)) {
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
      onSaved?.(value);
      if (version === draftVersion.current) after();
    });
  }

  useEffect(() => {
    const keyboard = (event: KeyboardEvent) => {
      if (!workspaceRef.current?.getClientRects().length || pgnMode) return;
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
      }
    };
    window.addEventListener("keydown", keyboard);
    return () => window.removeEventListener("keydown", keyboard);
  });

  return (
    <div className="knowledge-workspace" ref={workspaceRef}>
      <section className="knowledge-editor" aria-label="Редактор репертуара">
        <header className="knowledge-header">
          <button onClick={() => close()} disabled={busy}>
            {closeLabel}
          </button>
          <h1>{document ? document.title : "Новый репертуар"}</h1>
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
          {document && (
            <details className="knowledge-document-menu">
              <summary>Документ</summary>
              <div className="knowledge-menu-panel">
                <label>
                  Название
                  <input
                    aria-label="Название репертуара"
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
                    aria-label="Сторона репертуара"
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
                      onSaved?.(value);
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
            <h2>Создать репертуар</h2>
            <label>
              Название
              <input
                aria-label="Название репертуара"
                value={title}
                disabled={busy}
                onChange={(event) => setTitle(event.target.value)}
                autoFocus
              />
            </label>
            <label>
              Сторона
              <select
                aria-label="Сторона репертуара"
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
                  aria-label="Начальная FEN репертуара"
                  value={rootFen}
                  disabled={busy}
                  placeholder="Стандартная позиция"
                  onChange={(event) => setRootFen(event.target.value)}
                />
              </label>
            </details>
            <div className="knowledge-create-actions">
              <button
                className="primary"
                disabled={busy || !title.trim()}
                onClick={() =>
                  void run(async () => {
                    const value = await createDocument({
                      title,
                      kind: "repertoire",
                      side,
                      initial_fen: rootFen.trim() || undefined,
                      links: initialLinks,
                    });
                    install(value);
                    onSaved?.(value);
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
          <>
            <div className="knowledge-chapters">
              <label>
                Глава
                <select
                  aria-label="Глава репертуара"
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
                  <button
                    disabled={busy || document.chapters.length < 2}
                    onClick={() => setDeleteTarget("chapter")}
                  >
                    Удалить главу
                  </button>
                </div>
              </details>
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
                aria-busy={busy || !navigation}
              >
                <ChessBoard
                  session={board}
                  revision={boardRevision}
                  onChange={() => setBoardRevision(board.revision)}
                  readOnly={busy || !navigation}
                  onMove={addMove}
                  allowedMoves={
                    navigation?.legal_moves.map((move) => move.uci) ?? []
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
                    aria-label="Предыдущий ход репертуара"
                    disabled={busy || !selected.parentId}
                    onClick={() =>
                      selected.parentId && select(chapter.id, selected.parentId)
                    }
                  >
                    ‹
                  </button>
                  <button
                    title="Следующий ход"
                    aria-label="Следующий ход репертуара"
                    disabled={busy || !selected.node.children.length}
                    onClick={() =>
                      select(chapter.id, selected.node.children[0].id)
                    }
                  >
                    ›
                  </button>
                  <button
                    title="Перевернуть доску"
                    aria-label="Перевернуть доску репертуара"
                    onClick={() => {
                      board.flip();
                      setBoardRevision(board.revision);
                    }}
                  >
                    ↻
                  </button>
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
                          onSaved?.(value);
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
                      Открыть на карте
                    </button>
                  )}
                </div>
                <details className="knowledge-technical">
                  <summary>Технические сведения</summary>
                  <label>
                    FEN
                    <input
                      className="knowledge-position-fen"
                      aria-label="FEN выбранной позиции репертуара"
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
                  className="knowledge-notation"
                  aria-label="Варианты репертуара"
                >
                  <button
                    className={
                      selected.node.id === chapter.root.id
                        ? "active knowledge-root"
                        : "knowledge-root"
                    }
                    aria-current={
                      selected.node.id === chapter.root.id ? "step" : undefined
                    }
                    disabled={busy}
                    onClick={() => select(chapter.id, chapter.root.id)}
                  >
                    Начало
                  </button>
                  <div className="knowledge-moves">
                    {notation.tokens.map((token, tokenIndex) => {
                      if (token.type === "move") {
                        const { node, depth } = token;
                        return (
                          <button
                            key={node.id}
                            aria-label={`${moveLabel(node)}${node.nags.length ? ` ${node.nags.map(nagLabel).join(" ")}` : ""}`}
                            aria-current={
                              node.id === nodeId ? "step" : undefined
                            }
                            data-knowledge-node={node.id}
                            className={`knowledge-move ${node.id === nodeId ? "active" : ""} ${selectedPath.has(node.id) ? "in-path" : ""} ${depth ? "variation" : ""}`}
                            disabled={busy}
                            onClick={() => select(chapter.id, node.id)}
                          >
                            <span>{moveLabel(node)}</span>
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
                                if (next.has(token.parentId))
                                  next.delete(token.parentId);
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
                    })}
                    {notation.remaining && (
                      <button
                        onClick={() => setVisibleMoves((value) => value + 300)}
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
                            { type: "remove_chapter", chapter_id: chapter.id },
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
                <details className="knowledge-annotation-menu">
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
                            value === `$${number}` || value === String(number),
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
                                  ? values.filter((value) => value !== number)
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
            </div>
          </>
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
                      aria-label="Название импортируемого репертуара"
                      value={title}
                      disabled={busy}
                      onChange={(event) => setTitle(event.target.value)}
                    />
                  </label>
                  <label>
                    Сторона
                    <select
                      aria-label="Сторона импортируемого репертуара"
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
                onChange={(event) => setPgn(event.target.value)}
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
                          if (file)
                            void file
                              .text()
                              .then(setPgn)
                              .catch((reason) =>
                                setError(errorMessage(reason)),
                              );
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
                          const value = await importDocument({
                            title,
                            side,
                            pgn,
                            document_id: persisted?.id,
                            expected_revision: persisted?.revision,
                          });
                          const first =
                            value.chapters.find(
                              (item) => !previous.has(item.id),
                            ) ?? value.chapters[0];
                          install(value, first.id, first.root.id);
                          setPgnMode(null);
                          onSaved?.(value);
                        })
                      }
                    >
                      {document
                        ? "Добавить главы из PGN"
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
                      onClick={() => {
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

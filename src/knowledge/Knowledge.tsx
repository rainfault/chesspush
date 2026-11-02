import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { errorMessage } from "../api";
import { KnowledgeEditor } from "./KnowledgeEditor";
import {
  KnowledgeRepertoireActions,
  type SourceGameTarget,
} from "./KnowledgeRepertoireActions";
import type { RepertoireTarget } from "./Repertoire";
import {
  deleteKnowledge,
  exportKnowledgeFile,
  importKnowledgeFile,
  listDocuments,
  loadDocument,
  navigateDocument,
} from "./api";
import type { DocumentPosition, DocumentSummary, KnowledgeLink } from "./types";
import "./library.css";

export interface KnowledgeTarget {
  documentId?: string;
  chapterId?: string;
  nodeId?: string;
  initialFen?: string;
  initialLinks?: KnowledgeLink[];
  mode?: "create" | "import" | "list";
  requestId?: number;
}

const classification = (item: DocumentSummary) =>
  item.classification === "position" ? "Позиция" : "Теоретическая заметка";
const updated = (timestamp: number) =>
  new Date(timestamp).toLocaleDateString("ru-RU", {
    day: "numeric",
    month: "short",
    year: "numeric",
  });

export function Knowledge({
  active,
  initialTarget,
  onOpenGraph,
  onOpenRepertoire,
  onOpenSourceGame,
}: {
  active: boolean;
  initialTarget?: KnowledgeTarget;
  onOpenGraph: (position: DocumentPosition) => void;
  onOpenRepertoire?: (target: RepertoireTarget) => void;
  onOpenSourceGame?: (target: SourceGameTarget) => void;
}) {
  const [documents, setDocuments] = useState<DocumentSummary[]>([]);
  const [target, setTarget] = useState<KnowledgeTarget | null>(
    initialTarget?.mode === "list" ? null : (initialTarget ?? null),
  );
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [query, setQuery] = useState("");
  const [kind, setKind] = useState("all");
  const [tag, setTag] = useState("");
  const [visible, setVisible] = useState(100);
  const [deleteTarget, setDeleteTarget] = useState<DocumentSummary | null>(
    null,
  );
  const generation = useRef(0);
  const mounted = useRef(true);
  const operation = useRef(false);
  const currentView = useRef({ active, target });
  currentView.current = { active, target };

  async function refresh() {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    const current = ++generation.current;
    try {
      const values = await listDocuments();
      if (mounted.current && current === generation.current)
        setDocuments(
          values
            .filter((item) => item.kind === "study")
            .sort(
              (a, b) =>
                b.updated_at_ms - a.updated_at_ms || a.id.localeCompare(b.id),
            ),
        );
    } catch (reason) {
      if (mounted.current && current === generation.current)
        setError(errorMessage(reason));
    } finally {
      if (mounted.current && current === generation.current) setLoading(false);
    }
  }
  useEffect(() => {
    mounted.current = true;
    void refresh();
    let disposed = false;
    let stop: (() => void) | undefined;
    if (isTauri())
      void listen("knowledge-updated", () => void refresh())
        .then((value) => {
          if (disposed) value();
          else stop = value;
        })
        .catch((reason) => {
          if (!disposed) setError(errorMessage(reason));
        });
    return () => {
      mounted.current = false;
      disposed = true;
      generation.current++;
      stop?.();
    };
  }, []);
  useEffect(() => {
    if (active) void refresh();
  }, [active]);
  useEffect(() => {
    if (!initialTarget) return;
    setTarget(initialTarget.mode === "list" ? null : initialTarget);
  }, [initialTarget?.requestId]);

  async function run(action: () => Promise<void>) {
    if (operation.current) return;
    operation.current = true;
    setBusy(true);
    setError("");
    try {
      await action();
    } catch (reason) {
      if (mounted.current) setError(errorMessage(reason));
    } finally {
      operation.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  async function importFiles() {
    await run(async () => {
      const paths = await open({
        title: "Добавить знания из PGN",
        multiple: true,
        filters: [{ name: "PGN", extensions: ["pgn"] }],
      });
      if (!paths) return;
      const failures: string[] = [];
      const created: DocumentSummary[] = [];
      for (const path of Array.isArray(paths) ? paths : [paths]) {
        try {
          const value = await importKnowledgeFile(path);
          created.push({ ...value, chapters: value.chapters.length });
        } catch (reason) {
          failures.push(errorMessage(reason));
        }
      }
      if (!mounted.current) return;
      await refresh();
      if (created.length === 1 && !failures.length)
        setTarget({ documentId: created[0].id, requestId: Date.now() });
      if (failures.length) setError(failures.join("\n"));
    });
  }
  async function exportFile(item: DocumentSummary) {
    await run(async () => {
      const path = await save({
        title: "Сохранить знание",
        defaultPath: `${item.title.replace(/[<>:"/\\|?*]+/g, "_") || "knowledge"}.pgn`,
        filters: [{ name: "PGN", extensions: ["pgn"] }],
      });
      if (path) await exportKnowledgeFile(item.id, path, item.revision);
    });
  }
  async function openGraph(item: DocumentSummary) {
    await run(async () => {
      const value = await loadDocument(item.id);
      const chapter = value.chapters[0];
      if (!chapter) throw new Error("В знании нет глав");
      const position = await navigateDocument(
        value.id,
        chapter.id,
        chapter.root.id,
        value.revision,
      );
      if (
        mounted.current &&
        currentView.current.active &&
        !currentView.current.target
      )
        onOpenGraph({
          document_id: position.document_id,
          revision: position.revision,
          chapter_id: position.chapter_id,
          node_id: position.node_id,
          position_key: position.position_key,
          fen: position.fen,
          initial_fen: position.initial_fen,
          path_uci: position.path_uci,
        });
    });
  }
  async function remove(item: DocumentSummary) {
    await run(async () => {
      await deleteKnowledge(item.id, item.revision);
      try {
        localStorage.removeItem(`chesspush.knowledge.draft.${item.id}`);
      } catch {
        /* SQLite owns the document; browser drafts are optional. */
      }
      if (!mounted.current) return;
      setDeleteTarget(null);
      await refresh();
    });
  }

  const tags = [...new Set(documents.flatMap((item) => item.tags ?? []))].sort(
    (a, b) => a.localeCompare(b, "ru"),
  );
  const effectiveTag = tags.includes(tag) ? tag : "";
  const term = query.trim().toLocaleLowerCase();
  const matching = documents.filter(
    (item) =>
      (kind === "all" || item.classification === kind) &&
      (!effectiveTag || item.tags?.includes(effectiveTag)) &&
      (!term ||
        [item.title, ...(item.tags ?? [])].some((value) =>
          value.toLocaleLowerCase().includes(term),
        )),
  );

  if (target)
    return (
      <main
        className="knowledge-library-editor"
        aria-label="Рабочее пространство знания"
      >
        <KnowledgeEditor
          key={
            target.documentId
              ? `${target.documentId}:${target.requestId ?? ""}`
              : `new:${target.requestId ?? ""}`
          }
          active={active}
          documentKind="study"
          initialSide="both"
          initialTitle={target.initialFen ? "Новая позиция" : "Новое знание"}
          documentId={target.documentId}
          initialFen={target.initialFen}
          initialLinks={target.initialLinks}
          initialChapterId={target.chapterId}
          initialNodeId={target.nodeId}
          initialPgnMode={target.mode === "import" ? "import" : undefined}
          closeLabel="К знаниям"
          onClose={() => {
            setTarget(null);
            void refresh();
          }}
          onNavigatePosition={onOpenGraph}
          toolbar={(actions) => (
            <KnowledgeRepertoireActions
              actions={actions}
              onOpenRepertoire={onOpenRepertoire}
              onOpenSourceGame={onOpenSourceGame}
            />
          )}
          onSaved={(value) => {
            void refresh();
            if (!target.documentId)
              setTarget((previous) =>
                previous ? { ...previous, documentId: value.id } : previous,
              );
          }}
        />
      </main>
    );

  return (
    <main className="knowledge-library" aria-label="Знания">
      <header className="knowledge-library-heading">
        <h1>
          Знания <span>{documents.length}</span>
        </h1>
        <button
          disabled={busy || !isTauri()}
          onClick={() => void importFiles()}
        >
          Добавить PGN
        </button>
        <button
          className="primary"
          disabled={busy || !isTauri()}
          onClick={() => setTarget({ mode: "create", requestId: Date.now() })}
        >
          Создать знание
        </button>
      </header>
      <div className="knowledge-library-filters">
        <input
          type="search"
          aria-label="Поиск знаний"
          placeholder="Название или тег"
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setVisible(100);
          }}
        />
        <select
          aria-label="Тип знания"
          value={kind}
          onChange={(event) => {
            setKind(event.target.value);
            setVisible(100);
          }}
        >
          <option value="all">Все типы</option>
          <option value="position">Позиции</option>
          <option value="theory">Теоретические заметки</option>
        </select>
        <select
          aria-label="Фильтр по тегу"
          value={effectiveTag}
          onChange={(event) => {
            setTag(event.target.value);
            setVisible(100);
          }}
        >
          <option value="">Все теги</option>
          {tags.map((value) => (
            <option key={value} value={value}>
              {value}
            </option>
          ))}
        </select>
      </div>
      {error && (
        <div className="error knowledge-library-error" role="alert">
          <span>{error}</span>
          <button
            onClick={() => {
              setError("");
              void refresh();
            }}
          >
            Повторить
          </button>
        </div>
      )}
      {busy && <p role="status">Обработка PGN…</p>}
      {loading ? (
        <p role="status">Загрузка знаний…</p>
      ) : !matching.length ? (
        <p className="knowledge-library-empty" role="status">
          {documents.length ? "Ничего не найдено" : "Знаний нет"}
        </p>
      ) : (
        <div className="knowledge-library-rows" aria-label="Список знаний">
          {matching.slice(0, visible).map((item) => (
            <article className="knowledge-library-row" key={item.id}>
              <button
                className="knowledge-library-open"
                onClick={() =>
                  setTarget({ documentId: item.id, requestId: Date.now() })
                }
              >
                <svg
                  viewBox="0 0 24 24"
                  width="25"
                  height="25"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  aria-hidden="true"
                >
                  <path d="M12 5C8 3 4 3 2 4v15c3-1 6-1 10 1 4-2 7-2 10-1V4c-2-1-6-1-10 1Zm0 0v15" />
                </svg>
                <span>
                  <strong>{item.title}</strong>
                  <span className="knowledge-library-kind">
                    {classification(item)}
                  </span>
                </span>
              </button>
              <div className="knowledge-library-tags">
                {item.tags?.map((value) => (
                  <button
                    key={value}
                    onClick={() => {
                      setTag(value);
                      setVisible(100);
                    }}
                  >
                    {value}
                  </button>
                ))}
              </div>
              <span className="knowledge-library-chapters">
                Глав: {item.chapters}
              </span>
              <time dateTime={new Date(item.updated_at_ms).toISOString()}>
                {updated(item.updated_at_ms)}
              </time>
              <div className="knowledge-library-actions">
                <button
                  disabled={busy}
                  aria-label={`Показать в графе: ${item.title}`}
                  title="Показать в графе"
                  onClick={() => void openGraph(item)}
                >
                  <svg
                    viewBox="0 0 24 24"
                    width="20"
                    height="20"
                    aria-hidden="true"
                  >
                    <path d="M7 12h4m4-6h-4v12h4M3 9h4v6H3zm12-6h6v6h-6zm0 12h6v6h-6z" />
                  </svg>
                </button>
                <button
                  disabled={busy}
                  aria-label={`Экспорт PGN: ${item.title}`}
                  title="Экспорт PGN"
                  onClick={() => void exportFile(item)}
                >
                  <svg
                    viewBox="0 0 24 24"
                    width="20"
                    height="20"
                    aria-hidden="true"
                  >
                    <path d="M12 3v12m-4-4 4 4 4-4M4 15v6h16v-6" />
                  </svg>
                </button>
                <button
                  disabled={busy}
                  aria-label={`Удалить знание: ${item.title}`}
                  title="Удалить знание"
                  onClick={() => setDeleteTarget(item)}
                >
                  <svg
                    viewBox="0 0 24 24"
                    width="20"
                    height="20"
                    aria-hidden="true"
                  >
                    <path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14M10 10v7M14 10v7" />
                  </svg>
                </button>
              </div>
            </article>
          ))}
          {matching.length > visible && (
            <button
              className="knowledge-library-more"
              onClick={() => setVisible((value) => value + 100)}
            >
              Показать ещё
            </button>
          )}
        </div>
      )}
      {deleteTarget && (
        <DeleteKnowledgeDialog
          item={deleteTarget}
          busy={busy}
          onCancel={() => setDeleteTarget(null)}
          onDelete={() => void remove(deleteTarget)}
        />
      )}
    </main>
  );
}

function DeleteKnowledgeDialog({
  item,
  busy,
  onCancel,
  onDelete,
}: {
  item: DocumentSummary;
  busy: boolean;
  onCancel: () => void;
  onDelete: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => {
      dialog.current?.close();
      previous?.focus();
    };
  }, []);
  return (
    <dialog
      ref={dialog}
      className="knowledge-delete-dialog"
      aria-labelledby="knowledge-delete-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onCancel();
      }}
    >
      <h2 id="knowledge-delete-title">Удалить знание?</h2>
      <p>{item.title}</p>
      {item.attachment_active && (
        <p>Знание также будет убрано из репертуара.</p>
      )}
      <div>
        <button disabled={busy} autoFocus onClick={onCancel}>
          Отмена
        </button>
        <button className="danger" disabled={busy} onClick={onDelete}>
          Удалить
        </button>
      </div>
    </dialog>
  );
}

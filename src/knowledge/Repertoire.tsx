import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open, save } from "@tauri-apps/plugin-dialog";
import { errorMessage } from "../api";
import {
  attachRepertoireFile,
  deleteRepertoire,
  exportCombinedRepertoire,
  listDocuments,
  loadCombinedRepertoire,
  setRepertoireAttachment,
} from "./api";
import { CombinedRepertoire } from "./CombinedRepertoire";
import { KnowledgeEditor } from "./KnowledgeEditor";
import { sideLabel } from "./presentation";
import type {
  DocumentPosition,
  DocumentSummary,
  KnowledgeDocument,
  KnowledgeLink,
} from "./types";
import "./repertoire.css";

export interface RepertoireTarget {
  documentId?: string;
  chapterId?: string;
  nodeId?: string;
  initialFen?: string;
  initialLinks?: KnowledgeLink[];
  mode?: "create" | "import" | "list";
  requestId?: string | number;
}
export interface RepertoireContext {
  documentId: string;
  chapterId: string;
  nodeId: string;
  title: string;
}
interface Props {
  active?: boolean;
  initialTarget?: RepertoireTarget;
  onOpenGraph?: (position: DocumentPosition) => void;
  onContextChange?: (context: RepertoireContext) => void;
  onReturnToBranch?: () => void;
}
function initialSide(): "white" | "black" {
  try {
    return localStorage.getItem("chesspush.repertoire.side") === "black"
      ? "black"
      : "white";
  } catch {
    return "white";
  }
}
export function Repertoire({
  active = true,
  initialTarget,
  onOpenGraph,
  onContextChange,
  onReturnToBranch,
}: Props) {
  const [documents, setDocuments] = useState<DocumentSummary[]>([]);
  const [target, setTarget] = useState<RepertoireTarget | null>(
    initialTarget?.mode === "list" ? null : (initialTarget ?? null),
  );
  const [combined, setCombined] = useState<KnowledgeDocument | null>(null);
  const [side, setSide] = useState<"white" | "black">(initialSide);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState("");
  const [deleteTarget, setDeleteTarget] = useState<DocumentSummary | null>(
    null,
  );
  const [error, setError] = useState("");
  const generation = useRef(0);
  const unionGeneration = useRef(0);
  const combinedRef = useRef(combined);
  combinedRef.current = combined;
  const lastContext = useRef<RepertoireContext | null>(null);
  const contextCallback = useRef(onContextChange);
  contextCallback.current = onContextChange;

  async function refresh() {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    const request = ++generation.current;
    try {
      const values = await listDocuments();
      if (request === generation.current) {
        setDocuments(
          values
            .filter((item) => item.kind === "repertoire")
            .sort(
              (a, b) =>
                b.updated_at_ms - a.updated_at_ms || a.id.localeCompare(b.id),
            ),
        );
      }
    } catch (reason) {
      if (request === generation.current) setError(errorMessage(reason));
    } finally {
      if (request === generation.current) setLoading(false);
    }
  }
  useEffect(() => {
    void refresh();
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    if (isTauri())
      void listen("knowledge-updated", () => {
        void refresh();
        // Source edits update the same projection without navigating away from
        // the selected occurrence. Unrelated document imports keep it open too.
        const current = combinedRef.current;
        if (current) {
          const request = ++unionGeneration.current;
          void loadCombinedRepertoire(
            current.side === "black" ? "black" : "white",
          )
            .then((value) => {
              if (request === unionGeneration.current)
                setCombined((previous) =>
                  previous?.id === value.id
                    ? value.chapters.length
                      ? value
                      : null
                    : previous,
                );
            })
            .catch((reason) => setError(errorMessage(reason)));
        }
      }).then((value) => {
        if (disposed) value();
        else unsubscribe = value;
      });
    return () => {
      disposed = true;
      generation.current++;
      unionGeneration.current++;
      unsubscribe?.();
    };
  }, []);
  useEffect(() => {
    if (initialTarget) {
      lastContext.current = null;
      setCombined(null);
      setTarget(initialTarget.mode === "list" ? null : initialTarget);
    }
  }, [
    initialTarget?.documentId,
    initialTarget?.chapterId,
    initialTarget?.nodeId,
    initialTarget?.mode,
    initialTarget?.requestId,
  ]);
  useEffect(() => {
    if (active && lastContext.current)
      contextCallback.current?.(lastContext.current);
  }, [active]);

  function remember(context: RepertoireContext) {
    lastContext.current = context;
    try {
      localStorage.setItem(
        "chesspush.repertoire.context",
        JSON.stringify(context),
      );
    } catch {
      /* Optional UI address. */
    }
    if (active) contextCallback.current?.(context);
  }
  async function attach() {
    setError("");
    try {
      const paths = await open({
        title:
          side === "white"
            ? "PGN репертуара за белых"
            : "PGN репертуара за чёрных",
        multiple: true,
        filters: [{ name: "PGN", extensions: ["pgn"] }],
      });
      if (!paths) return;
      const selected = Array.isArray(paths) ? paths : [paths];
      const failures: string[] = [];
      for (let i = 0; i < selected.length; i++) {
        setBusy(`Прикрепление ${i + 1} из ${selected.length}…`);
        try {
          await attachRepertoireFile(selected[i], side);
        } catch (reason) {
          failures.push(
            `${selected[i].split(/[\\/]/).at(-1)}: ${errorMessage(reason)}`,
          );
        }
      }
      await refresh();
      if (failures.length) setError(failures.join("\n"));
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy("");
    }
  }
  async function changeAttachment(document: DocumentSummary, enabled: boolean) {
    setError("");
    setBusy("Сохранение…");
    try {
      await setRepertoireAttachment(document.id, document.revision, enabled);
      await refresh();
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy("");
    }
  }
  async function remove(document: DocumentSummary) {
    setError("");
    setBusy("Удаление репертуара…");
    try {
      await deleteRepertoire(document.id, document.revision);
      // Browser drafts are not primary data and must not resurrect a deleted document.
      try {
        localStorage.removeItem(`chesspush.knowledge.draft.${document.id}`);
        const context = JSON.parse(
          localStorage.getItem("chesspush.repertoire.context") ?? "null",
        ) as RepertoireContext | null;
        if (context?.documentId === document.id)
          localStorage.removeItem("chesspush.repertoire.context");
      } catch {
        /* Optional UI state. */
      }
      if (lastContext.current?.documentId === document.id)
        lastContext.current = null;
      setDeleteTarget(null);
      await refresh();
    } catch (reason) {
      setDeleteTarget(null);
      setError(errorMessage(reason));
      await refresh();
    } finally {
      setBusy("");
    }
  }
  function deleteButton(document: DocumentSummary) {
    const name = document.filename ?? document.title;
    return (
      <button
        className="repertoire-delete"
        disabled={!!busy}
        aria-label={`Удалить ${name}`}
        title={`Удалить ${name}`}
        onClick={() => setDeleteTarget(document)}
      >
        <svg viewBox="0 0 24 24" width="19" height="19" aria-hidden="true">
          <path d="M4 7h16M9 7V4h6v3M6 7l1 14h10l1-14M10 10v7M14 10v7" />
        </svg>
      </button>
    );
  }
  async function showCombined() {
    setError("");
    setBusy("Открытие репертуара…");
    const request = ++unionGeneration.current;
    try {
      const value = await loadCombinedRepertoire(side);
      if (request === unionGeneration.current && value.chapters.length)
        setCombined(value);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy("");
    }
  }
  async function exportCombined() {
    setError("");
    try {
      const path = await save({
        title: "Сохранить общий репертуар",
        defaultPath: `repertoire-${side}.pgn`,
        filters: [{ name: "PGN", extensions: ["pgn"] }],
      });
      if (!path) return;
      setBusy("Сохранение PGN…");
      await exportCombinedRepertoire(side, path);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy("");
    }
  }
  const attached = documents.filter(
    (item) =>
      item.attachment &&
      item.attachment_active &&
      (item.side === side || item.side === "both"),
  );
  const detached = documents.filter(
    (item) =>
      item.attachment &&
      !item.attachment_active &&
      (item.side === side || item.side === "both"),
  );
  const legacy = documents.filter((item) => !item.attachment);

  if (target)
    return (
      <div className="repertoire-workspace">
        <KnowledgeEditor
          key={
            target.documentId
              ? `${target.documentId}:${target.requestId ?? ""}`
              : `new-${target.requestId ?? target.mode ?? "create"}`
          }
          documentId={target.documentId}
          initialChapterId={target.chapterId}
          initialNodeId={target.nodeId}
          initialFen={target.initialFen}
          initialLinks={target.initialLinks}
          initialPgnMode={target.mode === "import" ? "import" : undefined}
          onClose={() => {
            lastContext.current = null;
            setTarget(null);
            void refresh();
          }}
          closeLabel="К файлам репертуара"
          onReturnToBranch={onReturnToBranch}
          onContextChange={remember}
          onNavigatePosition={onOpenGraph}
          onSaved={(document) => {
            void refresh();
            if (!target.documentId) setTarget({ documentId: document.id });
          }}
        />
      </div>
    );
  if (combined)
    return (
      <>
        {error && (
          <div className="error repertoire-list-error" role="alert">
            {error}
          </div>
        )}
        {busy && <span role="status">{busy}</span>}
        <CombinedRepertoire
          document={combined}
          onClose={() => setCombined(null)}
          onExport={() => void exportCombined()}
          onOpenSource={(source) => {
            setCombined(null);
            setTarget({
              documentId: source.document_id,
              chapterId: source.chapter_id,
              nodeId: source.node_id,
            });
          }}
        />
      </>
    );

  return (
    <main className="repertoire-list" aria-label="Репертуар">
      {deleteTarget && (
        <DeleteRepertoireDialog
          document={deleteTarget}
          busy={!!busy}
          onCancel={() => setDeleteTarget(null)}
          onDelete={() => void remove(deleteTarget)}
        />
      )}
      <header>
        <h1>Репертуар</h1>
        <div>
          <div className="repertoire-sides" aria-label="Сторона репертуара">
            {(["white", "black"] as const).map((value) => (
              <button
                key={value}
                aria-pressed={side === value}
                onClick={() => {
                  setSide(value);
                  try {
                    localStorage.setItem("chesspush.repertoire.side", value);
                  } catch {
                    /* Optional UI preference. */
                  }
                }}
              >
                {value === "white" ? "За белых" : "За чёрных"}
              </button>
            ))}
          </div>
          <button
            className="primary"
            disabled={!!busy || !isTauri()}
            onClick={() => void attach()}
          >
            Прикрепить PGN
          </button>
        </div>
      </header>
      {error && (
        <div className="error repertoire-list-error" role="alert">
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
      {busy && <p role="status">{busy}</p>}
      {loading ? (
        <p role="status">Загрузка репертуара…</p>
      ) : (
        <>
          {attached.length > 0 && (
            <div className="repertoire-combined-actions">
              <button
                className="primary"
                disabled={!!busy}
                onClick={() => void showCombined()}
              >
                Открыть общий репертуар
              </button>
              <button disabled={!!busy} onClick={() => void exportCombined()}>
                Экспорт PGN
              </button>
              <span>{attached.length} PGN</span>
            </div>
          )}
          <div className="repertoire-tiles" aria-label="Прикреплённые PGN">
            {attached.map((document) => (
              <article className="repertoire-tile" key={document.id}>
                <button
                  className="repertoire-tile-open"
                  onClick={() => setTarget({ documentId: document.id })}
                >
                  <span className="repertoire-tile-eco">
                    {document.openings?.length
                      ? [...new Set(document.openings.map((item) => item.eco))]
                          .slice(0, 4)
                          .join(" · ")
                      : "Без ECO"}
                  </span>
                  <strong>
                    {document.openings?.[0]?.name ?? "Без классификации"}
                  </strong>
                  {(document.openings?.length ?? 0) > 1 && (
                    <span
                      className="repertoire-extra-openings"
                      title={document.openings
                        ?.map((item) => `${item.eco} ${item.name}`)
                        .join("\n")}
                    >
                      Дебютов: {document.openings!.length}
                    </span>
                  )}
                  <span className="repertoire-tile-filename">
                    {document.filename ?? document.title}
                  </span>
                </button>
                <button
                  className="repertoire-detach"
                  disabled={!!busy}
                  aria-label={`Отсоединить ${document.filename ?? document.title}`}
                  onClick={() => void changeAttachment(document, false)}
                >
                  ×
                </button>
                {deleteButton(document)}
              </article>
            ))}
          </div>
          {!attached.length && !error && (
            <div className="repertoire-empty">
              <button
                className="primary"
                disabled={!!busy || !isTauri()}
                onClick={() => void attach()}
              >
                Прикрепить PGN за {side === "white" ? "белых" : "чёрных"}
              </button>
            </div>
          )}
          {detached.length > 0 && (
            <details className="repertoire-existing">
              <summary>Отсоединённые файлы ({detached.length})</summary>
              <ul>
                {detached.map((document) => (
                  <li key={document.id}>
                    <button
                      onClick={() => setTarget({ documentId: document.id })}
                    >
                      {document.filename ?? document.title}
                    </button>
                    <button
                      disabled={!!busy}
                      onClick={() => void changeAttachment(document, true)}
                    >
                      Прикрепить снова
                    </button>
                    {deleteButton(document)}
                  </li>
                ))}
              </ul>
            </details>
          )}
          {legacy.length > 0 && (
            <details className="repertoire-existing">
              <summary>Ранее созданные документы ({legacy.length})</summary>
              <ul>
                {legacy.map((document) => (
                  <li key={document.id}>
                    <button
                      onClick={() => setTarget({ documentId: document.id })}
                    >
                      {document.title}
                    </button>
                    <span>{sideLabel(document.side)}</span>
                    <button
                      disabled={!!busy}
                      onClick={() => void changeAttachment(document, true)}
                    >
                      Включить в репертуар
                    </button>
                    {deleteButton(document)}
                  </li>
                ))}
              </ul>
            </details>
          )}
        </>
      )}
    </main>
  );
}

function DeleteRepertoireDialog({
  document,
  busy,
  onCancel,
  onDelete,
}: {
  document: DocumentSummary;
  busy: boolean;
  onCancel: () => void;
  onDelete: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const element = dialog.current;
    element?.showModal();
    return () => element?.close();
  }, []);
  return (
    <dialog
      ref={dialog}
      className="repertoire-delete-dialog"
      aria-labelledby="delete-repertoire-title"
      aria-describedby="delete-repertoire-description"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onCancel();
      }}
    >
      <h2 id="delete-repertoire-title">
        Удалить «{document.filename ?? document.title}»?
      </h2>
      <p id="delete-repertoire-description">
        Документ и все его версии будут удалены из ChessPush. PGN-файлы на диске
        и сыгранные партии сохранятся.
      </p>
      <footer>
        <button autoFocus disabled={busy} onClick={onCancel}>
          Отмена
        </button>
        <button className="danger" disabled={busy} onClick={onDelete}>
          {busy ? "Удаление…" : "Удалить"}
        </button>
      </footer>
    </dialog>
  );
}

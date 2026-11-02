import { useEffect, useReducer, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { errorMessage } from "../api";
import {
  KnowledgeEditor,
  type KnowledgeEditorActions,
} from "../knowledge/KnowledgeEditor";
import type { KnowledgeTarget } from "../knowledge/Knowledge";
import type {
  DocumentNavigation,
  DocumentPosition,
  KnowledgeDocument,
} from "../knowledge/types";
import {
  enqueue,
  getState,
  pauseAnalysis,
  requestSync,
  type StatisticsState,
} from "../statistics/api";
import type { AccountStatus } from "../lichess";
import { ReviewEngine } from "./ReviewEngine";
import {
  analysisLabel,
  listReviews,
  markReview,
  openReview,
  saveReviewKnowledge,
  type OpenReview,
  type ReviewFilter,
  type ReviewPage,
  type ReviewRow,
  type ReviewTarget,
} from "./api";
import "./review.css";

const date = (value: number | null) =>
  value == null
    ? "—"
    : new Date(value).toLocaleString("ru-RU", {
        day: "2-digit",
        month: "short",
        year: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      });
const speedLabel = (value: string) =>
  (
    ({
      bullet: "Bullet",
      blitz: "Blitz",
      rapid: "Rapid",
      classical: "Classical",
      correspondence: "Переписка",
    }) as Record<string, string>
  )[value] ?? value;
const statusClass = (value: string) =>
  value.endsWith("failed")
    ? "failed"
    : value === "succeeded"
      ? "done"
      : value === "running"
        ? "running"
        : "";

export function GameReview({
  active,
  account,
  initialTarget,
  onOpenKnowledge,
  onOpenGraph,
  onSettings,
  onAuthenticate,
}: {
  active: boolean;
  account: AccountStatus;
  initialTarget?: ReviewTarget;
  onOpenKnowledge: (target: KnowledgeTarget) => void;
  onOpenGraph: (position: DocumentPosition) => void;
  onSettings: () => void;
  onAuthenticate: () => void;
}) {
  const [page, setPage] = useState<ReviewPage | null>(null);
  const [filter, setFilter] = useState<ReviewFilter>({ limit: 25, offset: 0 });
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [generation, refresh] = useReducer((n) => n + 1, 0);
  const [engineState, setEngineState] = useState<StatisticsState | null>(null);
  const [opened, setOpened] = useState<OpenReview | null>(null);
  const [address, setAddress] = useState<{
    chapterId: string;
    nodeId: string;
  } | null>(null);
  const [target, setTarget] = useState<ReviewTarget | null>(null);
  const [extraction, setExtraction] = useState<{
    document: KnowledgeDocument;
    position: DocumentNavigation;
  } | null>(null);
  const request = useRef(0);
  const filterKey = JSON.stringify(filter);
  useEffect(() => {
    if (!active || !isTauri() || opened) return;
    let disposed = false;
    setLoading(true);
    const timer = setTimeout(
      () => {
        void listReviews(filter)
          .then((value) => {
            if (!disposed) {
              setPage(value);
              setError("");
            }
          })
          .catch((reason) => {
            if (!disposed) setError(errorMessage(reason));
          })
          .finally(() => {
            if (!disposed) setLoading(false);
          });
      },
      filter.query ? 180 : 0,
    );
    return () => {
      disposed = true;
      clearTimeout(timer);
    };
  }, [active, Boolean(opened), filterKey, generation]);
  useEffect(() => {
    if (!active || !isTauri()) return;
    let disposed = false;
    const stops: (() => void)[] = [];
    let timer: ReturnType<typeof setTimeout> | undefined;
    const update = () => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        refresh();
        void getState(filter.account_id)
          .then((value) => {
            if (!disposed) setEngineState(value);
          })
          .catch((reason) => {
            if (!disposed) setError(errorMessage(reason));
          });
      }, 180);
    };
    update();
    for (const event of [
      "statistics-updated",
      "game-review-updated",
      "knowledge-updated",
    ]) {
      void listen(event, update)
        .then((stop) => {
          if (disposed) stop();
          else stops.push(stop);
        })
        .catch((reason) => {
          if (!disposed) setError(errorMessage(reason));
        });
    }
    const fallback = setInterval(update, 15_000);
    return () => {
      disposed = true;
      clearTimeout(timer);
      clearInterval(fallback);
      stops.forEach((stop) => stop());
    };
  }, [active, filter.account_id]);
  useEffect(() => {
    if (initialTarget) void open(initialTarget);
  }, [initialTarget?.requestId]);

  async function open(next: ReviewTarget) {
    const version = ++request.current;
    setBusy(true);
    setError("");
    try {
      const value = await openReview(next);
      if (version !== request.current) return;
      const requested = value.source_nodes.find(
        (node) => node.ply === (next.ply ?? 0),
      );
      if (next.ply != null && !requested)
        throw new Error(
          "Исходный узел партии удалён из разбора. Откройте разбор из списка партий.",
        );
      setAddress({
        chapterId: requested?.chapter_id ?? value.chapter_id,
        nodeId: requested?.node_id ?? value.node_id,
      });
      setOpened(value);
      setTarget(next);
      window.scrollTo({ top: 0 });
    } catch (reason) {
      if (version === request.current) setError(errorMessage(reason));
    } finally {
      if (version === request.current) setBusy(false);
    }
  }
  function close() {
    request.current++;
    setBusy(false);
    setOpened(null);
    setTarget(null);
    setAddress(null);
    setExtraction(null);
    setError("");
    refresh();
    window.scrollTo({ top: 0 });
  }
  async function run(action: () => Promise<unknown>) {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      await action();
      refresh();
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }
  function change(patch: Partial<ReviewFilter>) {
    setFilter((previous) => ({ ...previous, ...patch, offset: 0 }));
  }
  const jobs = engineState?.engine.jobs ?? [];
  const currentJob = opened
    ? jobs.find(
        (job) =>
          job.game_id === opened.game.id &&
          job.profile_id === engineState?.engine.identity?.profile_id,
      )
    : null;
  const currentAnalysisStatus = engineState
    ? !engineState.engine.configured
      ? "not_configured"
      : (currentJob?.state ?? "not_queued")
    : (opened?.analysis_status ?? "not_queued");
  const tags = page?.tags ?? [];
  const offset = filter.offset ?? 0,
    limit = filter.limit ?? 25;
  const rowStatus = (row: ReviewRow) =>
    `${analysisLabel(row.analysis_status)}${row.analysis_status === "running" && row.analysis_progress ? ` ${row.analysis_progress.progress}/${row.analysis_progress.total}` : ""}`;
  const toolbar = (actions: KnowledgeEditorActions) => (
    <>
      <label className="review-reviewed">
        <input
          type="checkbox"
          checked={opened!.reviewed}
          disabled={actions.busy || busy}
          onChange={(event) => {
            const reviewed = event.target.checked;
            actions.commit(async () => {
              await markReview(
                opened!.game.id,
                opened!.game.account_id,
                opened!.game.movetext_revision,
                reviewed,
              );
              setOpened((previous) =>
                previous ? { ...previous, reviewed } : previous,
              );
            });
          }}
        />{" "}
        Разобрана
      </label>
      <button
        disabled={actions.busy || busy || !actions.navigation}
        onClick={() =>
          actions.commit((document, position) =>
            setExtraction({ document, position }),
          )
        }
      >
        Сохранить знание
      </button>
    </>
  );

  return (
    <div className="game-review-page" hidden={!active}>
      {error && (
        <div className="review-error error" role="alert">
          <span>{error}</span>
          <button
            onClick={() => {
              setError("");
              refresh();
            }}
          >
            Повторить
          </button>
        </div>
      )}
      {busy && !opened && <p role="status">Открывается партия…</p>}
      {opened && address ? (
        <KnowledgeEditor
          key={`${opened.document.id}:${target?.requestId ?? "list"}`}
          active={active}
          documentKind="analysis"
          documentId={opened.document.id}
          initialChapterId={address.chapterId}
          initialNodeId={address.nodeId}
          closeLabel="К партиям"
          onClose={close}
          onSaved={() => refresh()}
          onDocumentChange={(document) =>
            setOpened((previous) =>
              previous && previous.document.id === document.id
                ? { ...previous, document }
                : previous,
            )
          }
          onNavigatePosition={onOpenGraph}
          toolbar={toolbar}
          sidePanel={(actions) => (
            <>
              <div className="review-source" aria-label="Исходная партия">
                <span>
                  {opened.game.white.name} — {opened.game.black.name}
                </span>
                <span>
                  {opened.game.result} · {speedLabel(opened.game.speed)} ·{" "}
                  {date(opened.game.started_at_ms)}
                </span>
              </div>
              <ReviewEngine
                document={actions.document}
                navigation={actions.navigation}
                active={active}
                onSettings={onSettings}
                onPlay={actions.playMove}
                busy={actions.busy}
                profileKey={engineState?.engine.identity?.profile_id}
              />
              <div
                className="review-game-analysis"
                aria-label="Состояние анализа партии"
              >
                <span className={statusClass(currentAnalysisStatus)}>
                  {analysisLabel(currentAnalysisStatus)}
                  {currentJob?.state === "running"
                    ? ` ${currentJob.progress}/${currentJob.total}`
                    : ""}
                </span>
                {currentJob?.last_error && (
                  <span role="alert">{currentJob.last_error}</span>
                )}
                {!engineState?.engine.configured ? (
                  <button onClick={onSettings}>Настроить движок</button>
                ) : (
                  <button
                    disabled={
                      busy ||
                      currentJob?.state === "running" ||
                      currentJob?.state === "queued"
                    }
                    onClick={() => void run(() => enqueue([opened.game.id]))}
                  >
                    Анализировать партию
                  </button>
                )}
              </div>
            </>
          )}
        />
      ) : (
        <main
          className="review-library"
          aria-label="Разбор партий"
          aria-busy={loading}
        >
          <header className="review-library-header">
            <h1>
              Разбор партий <span>{page?.total ?? ""}</span>
            </h1>
            {engineState?.engine.configured && (
              <button
                disabled={busy}
                onClick={() =>
                  void run(() => pauseAnalysis(!engineState.engine.paused))
                }
              >
                {engineState.engine.paused
                  ? "Продолжить анализ"
                  : "Пауза анализа"}
              </button>
            )}
            <button
              disabled={busy || account.status !== "connected"}
              onClick={() => void run(requestSync)}
            >
              Обновить
            </button>
          </header>
          {!isTauri() ? (
            <p role="status">Разбор доступен в приложении ChessPush</p>
          ) : (
            <>
              <div className="review-filters">
                <input
                  type="search"
                  aria-label="Поиск партий"
                  placeholder="Игрок или дебют"
                  value={filter.query ?? ""}
                  onChange={(event) => change({ query: event.target.value })}
                />
                <select
                  aria-label="Цвет в партии"
                  value={filter.color ?? ""}
                  onChange={(event) =>
                    change({ color: event.target.value || null })
                  }
                >
                  <option value="">Обе стороны</option>
                  <option value="white">Белыми</option>
                  <option value="black">Чёрными</option>
                </select>
                <select
                  aria-label="Контроль партии"
                  value={filter.speed ?? ""}
                  onChange={(event) =>
                    change({ speed: event.target.value || null })
                  }
                >
                  <option value="">Все контроли</option>
                  {[
                    "bullet",
                    "blitz",
                    "rapid",
                    "classical",
                    "correspondence",
                  ].map((value) => (
                    <option key={value} value={value}>
                      {speedLabel(value)}
                    </option>
                  ))}
                </select>
                <select
                  aria-label="Статус разбора"
                  value={filter.reviewed == null ? "" : String(filter.reviewed)}
                  onChange={(event) =>
                    change({
                      reviewed:
                        event.target.value === ""
                          ? null
                          : event.target.value === "true",
                    })
                  }
                >
                  <option value="">Все партии</option>
                  <option value="false">Не разобраны</option>
                  <option value="true">Разобраны</option>
                </select>
                <select
                  aria-label="Тег партии"
                  value={filter.tag ?? ""}
                  onChange={(event) =>
                    change({ tag: event.target.value || null })
                  }
                >
                  <option value="">Все теги</option>
                  {tags.map((tag) => (
                    <option key={tag} value={tag}>
                      {tag}
                    </option>
                  ))}
                </select>
                {page && page.accounts.length > 1 && (
                  <select
                    aria-label="Аккаунт партий"
                    value={filter.account_id ?? page.selected_account_id ?? ""}
                    onChange={(event) =>
                      change({ account_id: event.target.value })
                    }
                  >
                    {page.accounts.map((value) => (
                      <option value={value.id} key={value.id}>
                        {value.username}
                      </option>
                    ))}
                  </select>
                )}
              </div>
              <div className="review-date-filters">
                <label>
                  С{" "}
                  <input
                    type="date"
                    aria-label="Партии с даты"
                    onChange={(event) =>
                      change({
                        since_ms: event.target.value
                          ? new Date(`${event.target.value}T00:00:00`).getTime()
                          : null,
                      })
                    }
                  />
                </label>
                <label>
                  По{" "}
                  <input
                    type="date"
                    aria-label="Партии до даты"
                    onChange={(event) =>
                      change({
                        until_ms: event.target.value
                          ? new Date(
                              `${event.target.value}T23:59:59.999`,
                            ).getTime()
                          : null,
                      })
                    }
                  />
                </label>
                {loading && <span role="status">Загрузка…</span>}
              </div>
              {!page?.accounts.length && !loading ? (
                <div className="review-empty">
                  <button className="primary" onClick={onAuthenticate}>
                    Подключить Lichess
                  </button>
                </div>
              ) : !page?.rows.length && !loading ? (
                <p className="review-empty" role="status">
                  {page?.total ? "Ничего не найдено" : "Партий нет"}
                </p>
              ) : (
                <div className="review-games-wrap">
                  <table aria-label="Свои партии">
                    <thead>
                      <tr>
                        <th>Дата</th>
                        <th>Партия</th>
                        <th>Результат</th>
                        <th>Контроль</th>
                        <th>Дебют</th>
                        <th>Теги</th>
                        <th>Анализ</th>
                        <th>Разбор</th>
                      </tr>
                    </thead>
                    <tbody>
                      {page?.rows.map((row) => (
                        <tr
                          key={`${row.account_id}:${row.game_id}`}
                          data-game-id={row.game_id}
                        >
                          <td>
                            <time
                              dateTime={
                                row.started_at_ms
                                  ? new Date(row.started_at_ms).toISOString()
                                  : undefined
                              }
                            >
                              {date(row.started_at_ms)}
                            </time>
                          </td>
                          <td>
                            <button
                              className="review-game-link"
                              disabled={busy}
                              onClick={() =>
                                void open({
                                  gameId: row.game_id,
                                  accountId: row.account_id,
                                  revision: row.source_revision,
                                  requestId: Date.now(),
                                })
                              }
                            >
                              <span
                                className={
                                  row.own_color === "white" ? "own-player" : ""
                                }
                              >
                                {row.white}
                              </span>
                              <span className="review-versus"> — </span>
                              <span
                                className={
                                  row.own_color === "black" ? "own-player" : ""
                                }
                              >
                                {row.black}
                              </span>
                            </button>
                          </td>
                          <td className="review-result">{row.result}</td>
                          <td>{speedLabel(row.speed)}</td>
                          <td
                            className="review-opening"
                            title={row.opening ?? ""}
                          >
                            {row.eco && <span>{row.eco} </span>}
                            {row.opening ?? "—"}
                          </td>
                          <td
                            className="review-row-tags"
                            title={row.tags.join(", ")}
                          >
                            {row.tags.join(", ") || "—"}
                          </td>
                          <td>
                            <span
                              title={row.analysis_error ?? undefined}
                              className={`review-analysis-status ${statusClass(row.analysis_status)}`}
                            >
                              {rowStatus(row)}
                            </span>
                          </td>
                          <td>
                            <span
                              className={
                                row.reviewed ? "review-is-reviewed" : ""
                              }
                              role="img"
                              aria-label={
                                row.reviewed ? "Разобрана" : "Не разобрана"
                              }
                              title={
                                row.reviewed ? "Разобрана" : "Не разобрана"
                              }
                            >
                              {row.reviewed ? "✓" : "—"}
                            </span>
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
              <nav className="review-pagination" aria-label="Страницы партий">
                <span role="status">
                  {page?.total
                    ? `${offset + 1}–${Math.min(offset + (page.rows.length ?? 0), page.total)} из ${page.total}`
                    : "0 партий"}
                </span>
                <button
                  aria-label="Предыдущая страница партий"
                  disabled={loading || !offset}
                  onClick={() =>
                    setFilter((previous) => ({
                      ...previous,
                      offset: Math.max(0, offset - limit),
                    }))
                  }
                >
                  ‹
                </button>
                <button
                  aria-label="Следующая страница партий"
                  disabled={loading || offset + limit >= (page?.total ?? 0)}
                  onClick={() =>
                    setFilter((previous) => ({
                      ...previous,
                      offset: offset + limit,
                    }))
                  }
                >
                  ›
                </button>
              </nav>
            </>
          )}
        </main>
      )}
      {extraction && (
        <SaveKnowledgeDialog
          value={extraction}
          onClose={() => setExtraction(null)}
          onSaved={(document) => {
            setExtraction(null);
            onOpenKnowledge({ documentId: document.id, requestId: Date.now() });
          }}
        />
      )}
    </div>
  );
}

function SaveKnowledgeDialog({
  value,
  onClose,
  onSaved,
}: {
  value: { document: KnowledgeDocument; position: DocumentNavigation };
  onClose: () => void;
  onSaved: (document: KnowledgeDocument) => void;
}) {
  const [title, setTitle] = useState(`${value.document.title} · Позиция`);
  const [scope, setScope] = useState<"position" | "branch" | "game">("branch");
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
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
      className="review-save-dialog"
      aria-labelledby="review-save-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onClose();
      }}
    >
      <h2 id="review-save-title">Сохранить знание</h2>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          setBusy(true);
          setError("");
          void saveReviewKnowledge(
            value.document.id,
            value.document.revision,
            value.position.chapter_id,
            value.position.node_id,
            title,
            scope,
          )
            .then(onSaved)
            .catch((reason) => setError(errorMessage(reason)))
            .finally(() => setBusy(false));
        }}
      >
        <label>
          Название
          <input
            autoFocus
            aria-label="Название сохраняемого знания"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            maxLength={200}
            disabled={busy}
          />
        </label>
        <label>
          Материал
          <select
            aria-label="Материал знания"
            value={scope}
            onChange={(event) => setScope(event.target.value as typeof scope)}
            disabled={busy}
          >
            <option value="position">Текущая позиция</option>
            <option value="branch">Позиция с вариантами</option>
            <option value="game">Весь разбор партии</option>
          </select>
        </label>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        <footer>
          <button type="button" onClick={onClose} disabled={busy}>
            Отмена
          </button>
          <button
            type="submit"
            className="primary"
            disabled={busy || !title.trim()}
          >
            {busy ? "Сохраняется…" : "Сохранить знание"}
          </button>
        </footer>
      </form>
    </dialog>
  );
}

import {
  lazy,
  Suspense,
  useCallback,
  useEffect,
  useReducer,
  useRef,
  useState,
} from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { errorMessage } from "../api";
import type { AccountStatus } from "../lichess";
import type { DocumentPosition } from "../knowledge/types";
import type { RepertoireAddress } from "../gameGraph/GameGraph";
import { BoardView } from "./BoardView";
import { getGame, requestSync, type Filter, type GameDetail } from "./api";
import {
  getOpeningSummary,
  type OpeningSummary,
  type OpeningSummaryRow,
} from "./openingSummary";
import { OpeningTables } from "./OpeningTables";
import "./statistics.css";
import "./simpleStatistics.css";

const GameGraph = lazy(() =>
  import("../gameGraph/GameGraph").then((module) => ({
    default: module.GameGraph,
  })),
);
type Section = "openings" | "graph";
const sectionOf = (section?: string): Section =>
  section === "graph" ? "graph" : "openings";
function savedFilter(): Filter {
  try {
    const value = JSON.parse(
      localStorage.getItem("chesspush.practice.filters.v1") ?? "null",
    );
    if (value && typeof value === "object")
      return {
        account_id:
          typeof value.account_id === "string" ? value.account_id : null,
        color: value.color === "black" ? "black" : "white",
        variant: "standard",
      };
  } catch {
    /* Preferences never affect stored games. */
  }
  return { color: "white", variant: "standard" };
}

export function Statistics({
  active,
  account,
  onAuthenticate,
  busy,
  initialSection,
  sectionRequest,
  onNavigateSection,
  onOpenRepertoire,
  openPositionRequest,
  onPositionRequestHandled,
  openGameRequest,
  onContextChange,
  onExternalGameClose,
}: {
  active: boolean;
  account: AccountStatus;
  onAuthenticate: () => void;
  busy: boolean;
  initialSection?: string;
  sectionRequest?: number;
  onNavigateSection?: (section: string) => void;
  onOpenSettings?: (section?: string) => void;
  onOpenRepertoire?: (address: RepertoireAddress) => void;
  openGameRequest?: {
    id: string;
    ply?: number;
    runId?: string;
    requestId?: number;
    accountId?: string;
    returnTo?: "home";
  };
  onExternalGameClose?: () => void;
  openPositionRequest?: DocumentPosition;
  onPositionRequestHandled?: () => void;
  onContextChange?: (context: {
    section: string;
    label: string;
    gameId?: string;
    ply?: number;
    runId?: string;
    accountId?: string;
  }) => void;
}) {
  const [tab, setTab] = useState<Section>(() => sectionOf(initialSection));
  const [filter, setFilter] = useState<Filter>(savedFilter);
  const color = filter.color === "black" ? "black" : "white";
  const [snapshot, setSnapshot] = useState<{
    white: OpeningSummary;
    black: OpeningSummary;
  } | null>(null);
  const [snapshotKey, setSnapshotKey] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [generation, refresh] = useReducer((value) => value + 1, 0);
  const [openingSelection, setOpeningSelection] = useState<Pick<
    OpeningSummaryRow,
    "eco" | "name" | "color"
  > | null>(null);
  const [detail, setDetail] = useState<GameDetail | null>(null);
  const [initialPly, setInitialPly] = useState(0);
  const [initialRunId, setInitialRunId] = useState("");
  const gameRequest = useRef(0);
  const returnScroll = useRef(0);
  const externalGame = useRef(false);
  const graphLabel = useRef("Граф партий");
  const contextCallback = useRef(onContextChange);
  contextCallback.current = onContextChange;
  const positionHandledCallback = useRef(onPositionRequestHandled);
  positionHandledCallback.current = onPositionRequestHandled;
  const queryKey = JSON.stringify([filter.account_id ?? null]);
  const summaries = snapshotKey === queryKey ? snapshot : null;
  const summary = summaries?.[color] ?? null;
  const accountId = filter.account_id ?? summary?.selected_account_id ?? null;

  useEffect(() => {
    try {
      localStorage.setItem(
        "chesspush.practice.filters.v1",
        JSON.stringify({
          account_id: filter.account_id ?? null,
          color,
          variant: "standard",
        }),
      );
    } catch {
      /* Preview storage may be unavailable. */
    }
  }, [filter.account_id, color]);
  useEffect(() => {
    setTab(sectionOf(initialSection));
    gameRequest.current++;
    setDetail(null);
  }, [initialSection, sectionRequest]);
  useEffect(() => {
    if (openPositionRequest) {
      setTab("graph");
      setOpeningSelection(null);
      setFilter((previous) => ({
        account_id: previous.account_id,
        color: previous.color ?? "white",
        variant: "standard",
      }));
    }
  }, [openPositionRequest]);
  useEffect(() => {
    if (active) refresh();
  }, [active]);
  useEffect(() => {
    if (!active || !isTauri()) return;
    let disposed = false;
    const unlisteners: (() => void)[] = [];
    let timer: ReturnType<typeof setTimeout> | undefined;
    for (const event of ["statistics-updated", "knowledge-updated"]) {
      void listen(event, () => {
        clearTimeout(timer);
        timer = setTimeout(refresh, 150);
      })
        .then((stop) => {
          if (disposed) stop();
          else unlisteners.push(stop);
        })
        .catch(() => {});
    }
    const fallback = setInterval(refresh, 60000);
    return () => {
      disposed = true;
      clearTimeout(timer);
      clearInterval(fallback);
      unlisteners.forEach((stop) => stop());
    };
  }, [active]);
  useEffect(() => {
    if (!active || !isTauri()) return;
    let disposed = false;
    setLoading(true);
    void Promise.all([
      getOpeningSummary(filter.account_id ?? null, "white"),
      getOpeningSummary(filter.account_id ?? null, "black"),
    ])
      .then(([white, black]) => {
        if (!disposed) {
          setSnapshot({ white, black });
          setSnapshotKey(queryKey);
          setError("");
        }
      })
      .catch((reason) => {
        if (!disposed) setError(errorMessage(reason));
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [
    active,
    account.id,
    account.status,
    filter.account_id,
    generation,
    queryKey,
  ]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let stop: (() => void) | undefined;
    void listen<{ account_id: string }>("statistics-reset", ({ payload }) => {
      if (accountId && payload.account_id !== accountId) return;
      gameRequest.current++;
      setDetail(null);
      setOpeningSelection(null);
      setFilter((previous) => ({
        account_id: previous.account_id,
        color: previous.color ?? "white",
        variant: "standard",
        line: [],
      }));
      setSnapshot(null);
      refresh();
    })
      .then((unlisten) => {
        if (disposed) unlisten();
        else stop = unlisten;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      stop?.();
    };
  }, [accountId]);

  function closeGame(returnToSource = true) {
    gameRequest.current++;
    setDetail(null);
    if (detail)
      requestAnimationFrame(() =>
        window.scrollTo({ top: returnScroll.current }),
      );
    if (returnToSource && externalGame.current) onExternalGameClose?.();
    else if (returnToSource && detail)
      contextCallback.current?.({
        section: tab,
        label: tab === "graph" ? graphLabel.current : "Дебюты",
      });
    externalGame.current = false;
  }
  async function openGame(
    id: string,
    ply = 0,
    runId?: string,
    explicitAccount?: string,
    external = false,
  ) {
    const request = ++gameRequest.current;
    returnScroll.current = window.scrollY;
    setError("");
    try {
      const game = await getGame(id, explicitAccount ?? accountId);
      if (request !== gameRequest.current) return;
      if (runId && !game.runs.some((run) => run.id === runId))
        throw new Error("Результат анализа устарел; обновите статистику");
      externalGame.current = external;
      setDetail(game);
      setInitialPly(ply);
      setInitialRunId(runId ?? game.runs[0]?.id ?? "");
      contextCallback.current?.({
        section: tab,
        label: `${game.game.white.name} — ${game.game.black.name}`,
        gameId: id,
        ply,
        runId,
        accountId: game.game.account_id,
      });
      window.scrollTo({ top: 0 });
    } catch (reason) {
      if (request === gameRequest.current) setError(errorMessage(reason));
    }
  }
  useEffect(() => {
    if (openGameRequest)
      void openGame(
        openGameRequest.id,
        openGameRequest.ply,
        openGameRequest.runId,
        openGameRequest.accountId,
        openGameRequest.returnTo === "home",
      );
  }, [openGameRequest]);
  const handleGraphContext = useCallback((label: string) => {
    graphLabel.current = label;
    contextCallback.current?.({ section: "graph", label });
  }, []);
  const handlePositionHandled = useCallback(() => {
    setFilter((previous) =>
      previous.line == null && previous.position_key == null
        ? previous
        : { ...previous, line: undefined, position_key: undefined },
    );
    positionHandledCallback.current?.();
  }, []);
  function navigate(section: Section) {
    setTab(section);
    closeGame(false);
    onNavigateSection?.(section);
    contextCallback.current?.({
      section,
      label: section === "graph" ? "Граф партий" : "Дебюты",
    });
  }
  function chooseColor(next: "white" | "black") {
    if (next === color) return;
    setOpeningSelection(null);
    setFilter({
      account_id: filter.account_id,
      color: next,
      variant: "standard",
      line: [],
    });
    closeGame(false);
  }
  function openOpening(row: OpeningSummaryRow) {
    setOpeningSelection({ eco: row.eco, name: row.name, color: row.color });
    setFilter((previous) => ({
      ...previous,
      color: row.color,
      line: [],
      position_key: undefined,
    }));
    navigate("graph");
  }
  const openingName = openingSelection
    ? [openingSelection.eco, openingSelection.name ?? "Дебют не определён"]
        .filter(Boolean)
        .join(" · ")
    : "";
  const graphGameIds = openingSelection
    ? (summary?.rows.find(
        (row) =>
          row.eco === openingSelection.eco &&
          row.name === openingSelection.name,
      )?.game_ids ?? [])
    : undefined;
  return (
    <main
      className="statistics simple-statistics"
      hidden={!active}
      aria-busy={loading}
    >
      {!detail && (
        <div className="simple-statistics-toolbar">
          <div className="stats-tabs" role="tablist" aria-label="Статистика">
            {(
              [
                ["openings", "Дебюты"],
                ["graph", "Граф партий"],
              ] as const
            ).map(([id, label]) => (
              <button
                key={id}
                role="tab"
                className={tab === id ? "active" : ""}
                aria-selected={tab === id}
                onClick={() => navigate(id)}
              >
                {label}
              </button>
            ))}
          </div>
          <div className="simple-statistics-controls">
            {summary && summary.accounts.length > 1 && (
              <select
                aria-label="Аккаунт статистики"
                value={accountId ?? ""}
                onChange={(event) => {
                  setOpeningSelection(null);
                  setFilter({
                    account_id: event.target.value,
                    color,
                    variant: "standard",
                    line: [],
                  });
                }}
              >
                {summary.accounts.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.username}
                  </option>
                ))}
              </select>
            )}
            {tab === "graph" && (
              <div className="simple-statistics-side" aria-label="Цвет фигур">
                <button
                  className={color === "white" ? "selected" : ""}
                  aria-pressed={color === "white"}
                  onClick={() => chooseColor("white")}
                >
                  Белыми
                </button>
                <button
                  className={color === "black" ? "selected" : ""}
                  aria-pressed={color === "black"}
                  onClick={() => chooseColor("black")}
                >
                  Чёрными
                </button>
              </div>
            )}
            <button
              disabled={
                busy || loading || account.status !== "connected" || !isTauri()
              }
              onClick={() => {
                setError("");
                void requestSync()
                  .then(refresh)
                  .catch((reason) => setError(errorMessage(reason)));
              }}
            >
              Обновить
            </button>
          </div>
        </div>
      )}
      {error && (
        <div className="error" role="alert">
          {error}
          <button onClick={refresh}>Повторить</button>
        </div>
      )}
      {detail ? (
        <BoardView
          game={detail.game}
          runs={detail.runs}
          events={detail.events}
          analyses={detail.analyses}
          initialPly={initialPly}
          preferredRunId={initialRunId}
          onClose={() => closeGame()}
          onPositionChange={(ply, runId) =>
            contextCallback.current?.({
              section: tab,
              label: `${detail.game.white.name} — ${detail.game.black.name}`,
              gameId: detail.game.id,
              ply,
              runId,
              accountId: detail.game.account_id,
            })
          }
        />
      ) : null}
      {!isTauri() ? (
        <div className="stats-empty">
          Статистика доступна в приложении ChessPush
        </div>
      ) : (
        <>
          {tab === "openings" && !detail && (
            <OpeningTables
              summaries={summaries}
              loading={loading}
              busy={busy}
              onAuthenticate={onAuthenticate}
              onOpenOpening={openOpening}
              contextKey={queryKey}
            />
          )}

          {tab === "graph" && (
            <div hidden={Boolean(detail)}>
              {openingName && (
                <div className="simple-opening-selection">
                  <span>{openingName}</span>
                  <button
                    onClick={() => {
                      setOpeningSelection(null);
                      setFilter({
                        account_id: accountId,
                        color,
                        variant: "standard",
                        line: [],
                      });
                    }}
                  >
                    Все дебюты
                  </button>
                </div>
              )}
              <Suspense
                fallback={<div className="stats-empty">Загрузка графа…</div>}
              >
                {summary ? (
                  <GameGraph
                    filter={{
                      ...filter,
                      color,
                      account_id: accountId,
                      game_ids: graphGameIds,
                    }}
                    generation={String(generation)}
                    onOpenGame={openGame}
                    onOpenRepertoire={onOpenRepertoire}
                    openPositionRequest={openPositionRequest}
                    onPositionRequestHandled={handlePositionHandled}
                    onContextChange={handleGraphContext}
                  />
                ) : (
                  <div className="stats-empty">Загрузка партий…</div>
                )}
              </Suspense>
            </div>
          )}
        </>
      )}
    </main>
  );
}

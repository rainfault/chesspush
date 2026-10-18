import {
  useCallback,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import { listen } from "@tauri-apps/api/event";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { SquareName } from "chessops/types";
import { INITIAL_FEN } from "chessops/fen";
import { ChessBoard } from "../chess/ChessBoard";
import { BoardSession } from "../chess/BoardSession";
import { uciInput } from "../chess/boardInput";
import { errorMessage } from "../api";
import type { Filter, DecisionEvent } from "../statistics/api";
import { number } from "../statistics/format";
import { listDocuments } from "../knowledge/api";
import type {
  DocumentSummary,
  DocumentPosition,
  KnowledgeLink,
} from "../knowledge/types";
import {
  graphQuery,
  loadGraphState,
  saveGraphState,
  type Projection,
  type GraphEdge,
} from "./api";
import {
  pushRoute,
  chosenContinuation,
  restoreHistory,
  type Route,
  type History,
} from "./navigation";
import {
  layoutBranches,
  GRAPH_NODE_WIDTH as DEFAULT_NODE_W,
  GRAPH_NODE_HEIGHT as NODE_H,
  GRAPH_REPLY_HEIGHT as REPLY_H,
  GRAPH_COLUMN as DEFAULT_COLUMN,
  GRAPH_PADDING as DEFAULT_PAD,
} from "./branchLayout";
import "./graph.css";

export interface RepertoireAddress {
  documentId?: string;
  chapterId?: string;
  nodeId?: string;
  initialFen?: string;
  initialLinks?: KnowledgeLink[];
}
const pieceRoles: Record<string, string> = {
  N: "knight",
  B: "bishop",
  R: "rook",
  Q: "queen",
  K: "king",
};
function MoveLabel({ san, color }: { san: string; color: string }) {
  const namedRole = pieceRoles[san[0]];
  const role = namedRole ?? (san.startsWith("O-O") ? "king" : "pawn");
  return (
    <>
      <span
        className="move-piece cg-wrap"
        aria-hidden="true"
        ref={(node) => {
          if (node) {
            const piece = document.createElement("piece");
            piece.className = `${color} ${role}`;
            node.replaceChildren(piece);
          }
        }}
      />
      <span>{namedRole ? san.slice(1) : san}</span>
    </>
  );
}
function Results({
  edge,
}: {
  edge: Pick<GraphEdge, "games" | "wins" | "draws" | "losses">;
}) {
  if (!edge.games) return null;
  return (
    <>
      <span
        className="branch-results"
        title={`Победы ${edge.wins}; ничьи ${edge.draws}; поражения ${edge.losses}`}
      >
        <span>В {edge.wins}</span>
        <span>Н {edge.draws}</span>
        <span>П {edge.losses}</span>
      </span>
      <span className="branch-result-bar" aria-hidden="true">
        <i style={{ flex: edge.wins }} />
        <i style={{ flex: edge.draws }} />
        <i style={{ flex: edge.losses }} />
      </span>
    </>
  );
}
export function GameGraph({
  filter,
  generation,
  onContextChange,
  openPositionRequest,
  onPositionRequestHandled,
}: {
  filter: Filter;
  generation: string;
  onOpenGame: (id: string, ply: number) => void;
  onOpenRepertoire?: (address: RepertoireAddress) => void;
  onContextChange?: (label: string) => void;
  openPositionRequest?: DocumentPosition;
  onPositionRequestHandled?: () => void;
  reviewEvents?: DecisionEvent[];
  onOpenMoment?: (id: string, ply: number, runId?: string) => void;
  onRequestReview?: () => void;
  reviewLoading?: boolean;
  reviewLoaded?: boolean;
}) {
  const [history, setHistory] = useState<History>(() =>
    restoreHistory(null, INITIAL_FEN),
  );
  const [hydrated, setHydrated] = useState(false),
    [projection, setProjection] = useState<Projection | null>(null),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(false);
  const [documents, setDocuments] = useState<DocumentSummary[]>([]),
    [visibleCount, setVisibleCount] = useState(5);
  const [hovered, setHovered] = useState<{
      uci: string;
      reply?: string;
    } | null>(null),
    [selectedSquare, setSelectedSquare] = useState<SquareName | null>(null);
  const [revision, refresh] = useReducer((n) => n + 1, 0),
    [painted, paint] = useReducer((n) => n + 1, 0);
  const session = useMemo(() => new BoardSession(), []),
    sequence = useRef(0),
    appliedContext = useRef(""),
    savedState = useRef<unknown>(null),
    focusAfterTransition = useRef(false),
    workspace = useRef<HTMLElement>(null),
    historyRail = useRef<HTMLDivElement>(null),
    graphScroll = useRef<HTMLDivElement>(null);
  const [stageWidth, setStageWidth] = useState(0);
  useEffect(() => {
    const element = graphScroll.current;
    if (!element) return;
    const observer = new ResizeObserver(() =>
      setStageWidth(element.clientWidth),
    );
    observer.observe(element);
    setStageWidth(element.clientWidth);
    return () => observer.disconnect();
  }, []);
  const compact = stageWidth > 0 && stageWidth < 620;
  const NODE_W = compact
    ? Math.max(128, Math.floor((stageWidth - 96) / 3))
    : DEFAULT_NODE_W;
  const COLUMN = compact ? NODE_W + 36 : DEFAULT_COLUMN;
  const PAD = compact ? 12 : DEFAULT_PAD;
  const account = filter.account_id ?? null,
    route = history.entries[history.cursor],
    filterKey = JSON.stringify(filter),
    side = filter.color === "black" ? "black" : "white";
  const repertoireIds = useMemo(
    () =>
      documents
        .filter(
          (d) =>
            d.kind === "repertoire" &&
            d.attachment &&
            d.attachment_active &&
            (d.side === side || d.side === "both"),
        )
        .map((d) => d.id),
    [documents, side],
  );
  const docRevision = documents
    .map((d) => `${d.id}:${d.revision}:${d.attachment_active}`)
    .join("|");
  const contextKey = JSON.stringify([
    route,
    filterKey,
    repertoireIds,
    docRevision,
  ]);
  const contextPending = appliedContext.current !== contextKey;
  const positionRequestKey = JSON.stringify(openPositionRequest);
  const navigate = useCallback((next: Route, keyboard = false) => {
    focusAfterTransition.current = keyboard;
    setSelectedSquare(null);
    setHovered(null);
    setHistory((h) => pushRoute(h, next));
  }, []);
  useEffect(() => {
    let alive = true;
    setHydrated(false);
    savedState.current = null;
    Promise.all([loadGraphState(account), listDocuments()])
      .then(([state, docs]) => {
        if (!alive) return;
        setDocuments(docs);
        setHistory(
          restoreHistory(
            (state as { history?: unknown } | null)?.history,
            INITIAL_FEN,
          ),
        );
        setHydrated(true);
      })
      .catch((e) => {
        if (alive) {
          setError(errorMessage(e));
          setHydrated(true);
        }
      });
    return () => {
      alive = false;
    };
  }, [account]);
  useEffect(() => {
    let disposed = false;
    let off: (() => void) | undefined;
    void listen("knowledge-updated", () => {
      void listDocuments()
        .then((docs) => {
          if (!disposed) {
            setDocuments(docs);
            refresh();
          }
        })
        .catch((e) => {
          if (!disposed) setError(errorMessage(e));
        });
    }).then((fn) => {
      if (disposed) fn();
      else off = fn;
    });
    return () => {
      disposed = true;
      off?.();
    };
  }, []);
  useEffect(() => {
    if (!hydrated || !openPositionRequest) return;
    const p = openPositionRequest;
    navigate({
      initialFen: p.initial_fen,
      line: p.path_uci,
      positionKey: p.position_key,
    });
    onPositionRequestHandled?.();
  }, [hydrated, positionRequestKey]);
  const externalPath = JSON.stringify([filter.line, filter.position_key]);
  useEffect(() => {
    if (!hydrated || !filter.line || openPositionRequest) return;
    navigate({
      initialFen: INITIAL_FEN,
      line: filter.line,
      positionKey: filter.position_key,
    });
    onPositionRequestHandled?.();
  }, [hydrated, externalPath]);
  useEffect(() => {
    if (!hydrated) return;
    const state = { history };
    savedState.current = state;
    const timer = setTimeout(
      () =>
        void saveGraphState(account, state).catch((e) =>
          setError(errorMessage(e)),
        ),
      300,
    );
    return () => clearTimeout(timer);
  }, [hydrated, history, account]);
  useEffect(
    () => () => {
      if (savedState.current)
        void saveGraphState(account, savedState.current).catch(() => {});
    },
    [account],
  );
  useEffect(() => {
    if (!hydrated) return;
    const token = ++sequence.current;
    setLoading(true);
    graphQuery({
      filter,
      initial_fen: route.initialFen,
      line: route.line,
      position_key: route.positionKey,
      depth: 1,
      top_k: 1,
      max_nodes: 2,
      repertoire_ids: repertoireIds,
      all_paths: false,
      preview_limit: 30,
      preview_top_k: 2,
    })
      .then((next) => {
        if (token !== sequence.current) return;
        appliedContext.current = contextKey;
        setProjection(next);
        setError("");
        try {
          if (next.path_nodes.length) session.loadReplay(next.path_nodes);
          else session.loadFen(next.selected_fen);
          paint();
        } catch (e) {
          setError(errorMessage(e));
        }
        if (focusAfterTransition.current)
          requestAnimationFrame(() => {
            workspace.current
              ?.querySelector<HTMLButtonElement>(".branch-node[data-level='1']")
              ?.focus({ preventScroll: true });
            focusAfterTransition.current = false;
          });
      })
      .catch((e) => {
        if (token === sequence.current) setError(errorMessage(e));
      })
      .finally(() => {
        if (token === sequence.current) setLoading(false);
      });
    return () => {
      sequence.current++;
    };
  }, [hydrated, contextKey, generation, revision, session]);
  useEffect(() => {
    session.orientation = side;
    paint();
  }, [side, account, session]);
  const routeKey = `${route.initialFen}|${route.line.join(" ")}|${filterKey}`;
  useEffect(() => {
    setVisibleCount(5);
    graphScroll.current?.scrollTo({ top: 0, left: 0, behavior: "instant" });
    setSelectedSquare(null);
    setHovered(null);
  }, [routeKey]);
  useEffect(() => {
    if (!projection || contextPending) return;
    onContextChange?.(
      projection.path_nodes
        .slice(1)
        .map((n) => n.san)
        .filter(Boolean)
        .join(" ") || "Начальная позиция",
    );
    historyRail.current
      ?.querySelector<HTMLElement>("[aria-current='step']")
      ?.scrollIntoView({
        block: "nearest",
        inline: "nearest",
        behavior: "instant",
      });
  }, [projection, contextPending, onContextChange]);
  const edges = projection?.continuations ?? [],
    selected = projection?.selected,
    turn = session.position.turn;
  const ordered = useMemo(
    () =>
      [...edges].sort(
        (a, b) =>
          b.games - a.games ||
          b.prepared.length - a.prepared.length ||
          a.uci.localeCompare(b.uci),
      ),
    [edges],
  );
  const visibleEdges = ordered.slice(0, visibleCount),
    layout = layoutBranches(visibleEdges, projection?.previews ?? []);
  const back = () => {
    if (route.line.length)
      navigate(
        { initialFen: route.initialFen, line: route.line.slice(0, -1) },
        true,
      );
  };
  const selectEdge = (edge: GraphEdge, reply?: GraphEdge, keyboard = false) => {
    if (contextPending) return;
    navigate(
      {
        initialFen: route.initialFen,
        line: [...route.line, edge.uci, ...(reply ? [reply.uci] : [])],
        positionKey: reply?.to ?? edge.to,
      },
      keyboard,
    );
  };
  const hoveredUci = hovered?.reply ?? hovered?.uci;
  const autoShapes: DrawShape[] =
    hoveredUci && !hovered?.reply
      ? [
          {
            orig: hoveredUci.slice(0, 2) as SquareName,
            dest: hoveredUci.slice(2, 4) as SquareName,
            brush: "blue",
          },
        ]
      : selectedSquare
        ? edges
            .filter((e) => e.uci.startsWith(selectedSquare))
            .map((e) => ({
              orig: e.uci.slice(2, 4) as SquareName,
              brush: "blue",
            }))
        : [];
  function keyboard(e: React.KeyboardEvent<HTMLElement>) {
    if (
      (e.target as HTMLElement).closest(
        "input,textarea,select,[contenteditable=true],[role=dialog]",
      )
    )
      return;
    if (e.key === "ArrowLeft") {
      e.preventDefault();
      back();
    }
    if (e.key === "ArrowRight") {
      e.preventDefault();
      const next = chosenContinuation(history);
      if (next) navigate(next, true);
      else if (edges.length === 1) selectEdge(edges[0], undefined, true);
      else
        workspace.current
          ?.querySelector<HTMLButtonElement>(".branch-node[data-level='1']")
          ?.focus();
    }
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      const nodes = [
        ...(workspace.current?.querySelectorAll<HTMLButtonElement>(
          ".branch-node[data-level='1']",
        ) ?? []),
      ];
      if (nodes.length) {
        e.preventDefault();
        const index = nodes.indexOf(
          document.activeElement as HTMLButtonElement,
        );
        nodes[
          (index + (e.key === "ArrowDown" ? 1 : nodes.length - 1)) %
            nodes.length
        ].focus();
      }
    }
  }
  const title = (edge: GraphEdge) =>
    `${edge.san}; уникальных партий ${edge.games}; посещений ${edge.visits}; частота ${number(edge.frequency, 1)}%; победы ${edge.wins}, ничьи ${edge.draws}, поражения ${edge.losses}${edge.prepared.length ? "; в репертуаре" : ""}`;
  const node = (edge: GraphEdge, y: number, parent?: GraphEdge) => (
    <button
      key={`${parent?.uci ?? "root"}/${edge.uci}`}
      className={`branch-node ${edge.prepared.length ? "in-repertoire" : ""} ${!edge.games ? "prepared-only" : ""} ${selectedSquare && !parent && edge.uci.startsWith(selectedSquare) ? "piece-selected" : ""}`}
      style={{
        left: PAD + COLUMN * (parent ? 2 : 1),
        top: y,
        height: parent ? REPLY_H : NODE_H,
        width: NODE_W,
      }}
      data-level={parent ? 2 : 1}
      data-uci={edge.uci}
      data-parent-uci={parent?.uci}
      aria-label={title(edge)}
      title={title(edge)}
      disabled={contextPending}
      onMouseEnter={() =>
        setHovered({
          uci: parent?.uci ?? edge.uci,
          reply: parent ? edge.uci : undefined,
        })
      }
      onMouseLeave={() => setHovered(null)}
      onFocus={() =>
        setHovered({
          uci: parent?.uci ?? edge.uci,
          reply: parent ? edge.uci : undefined,
        })
      }
      onBlur={() => setHovered(null)}
      onClick={(e) =>
        selectEdge(parent ?? edge, parent ? edge : undefined, e.detail === 0)
      }
    >
      <span className="branch-move">
        <MoveLabel
          san={edge.san}
          color={parent ? (turn === "white" ? "black" : "white") : turn}
        />
        {edge.prepared.length > 0 && (
          <span
            className="branch-prepared-mark"
            aria-label="В репертуаре"
            title="В репертуаре"
          >
            ✓
          </span>
        )}
      </span>
      <span className="branch-counts">
        {edge.games ? (
          <>
            <strong>Партий: {edge.games}</strong>
            <span>{number(edge.frequency, 0)}%</span>
          </>
        ) : (
          <span>Подготовлено</span>
        )}
      </span>
      <Results edge={edge} />
    </button>
  );
  const currentMove = projection?.path_nodes.at(-1);
  return (
    <section
      className="game-graph"
      aria-label="Граф партий"
      aria-busy={loading || contextPending}
      data-projection-line={projection?.line.join(" ")}
      onKeyDown={keyboard}
      ref={workspace}
      tabIndex={-1}
    >
      <div className="graph-workspace">
        <div className="graph-board">
          <ChessBoard
            session={session}
            revision={session.revision + painted}
            onChange={paint}
            readOnly={contextPending}
            allowedMoves={edges.map((e) => e.uci)}
            autoShapes={autoShapes}
            onSelect={(square) => {
              setHovered(null);
              setSelectedSquare(square);
              const last = ordered.reduce(
                (n, edge, index) =>
                  edge.uci.startsWith(square ?? "!") ? index : n,
                -1,
              );
              if (last >= visibleCount) setVisibleCount(last + 1);
            }}
            onMove={(orig, dest, promotion) => {
              const uci = uciInput(session, orig, dest, promotion),
                edge = edges.find((e) => e.uci === uci);
              if (edge) selectEdge(edge);
            }}
          />
          <div className="graph-board-actions">
            <button
              disabled={!route.line.length}
              onClick={back}
              aria-label="Назад по ветви"
            >
              ← Назад
            </button>
            <button
              disabled={!route.line.length}
              onClick={() =>
                navigate({ initialFen: route.initialFen, line: [] })
              }
            >
              К началу
            </button>
            <button
              aria-label="Перевернуть доску"
              onClick={() => {
                session.flip();
                paint();
              }}
            >
              ⇅
            </button>
          </div>
        </div>
        <div className="graph-branches" aria-label="Ветка и продолжения">
          <div
            className="graph-history"
            aria-label="История ветки"
            data-uci-line={route.line.join(" ")}
            ref={historyRail}
          >
            <button
              className="history-node"
              aria-current={!route.line.length ? "step" : undefined}
              onClick={() =>
                navigate({ initialFen: route.initialFen, line: [] })
              }
            >
              Начало
            </button>
            {route.line.map((uci, index) => {
              const n = projection?.path_nodes[index + 1];
              return (
                <button
                  key={index}
                  className="history-node"
                  data-ply={index + 1}
                  aria-label={`К позиции после ${n?.san ?? uci}, полуход ${index + 1}`}
                  aria-current={
                    index === route.line.length - 1 ? "step" : undefined
                  }
                  onClick={() =>
                    navigate({
                      initialFen: route.initialFen,
                      line: route.line.slice(0, index + 1),
                    })
                  }
                >
                  <span className="history-move-number">
                    {n?.actor === "white"
                      ? `${Math.ceil(n.absolute_ply / 2)}.`
                      : "…"}
                  </span>
                  <MoveLabel
                    san={n?.san ?? uci}
                    color={n?.actor ?? (index % 2 ? "black" : "white")}
                  />
                </button>
              );
            })}
          </div>
          {error && (
            <div className="graph-error" role="alert">
              {error}
              <button onClick={refresh}>Повторить</button>
            </div>
          )}
          <div className="graph-scroll" ref={graphScroll}>
            <div
              className="branch-diagram"
              style={{
                height: layout.height,
                minWidth: PAD * 2 + COLUMN * 2 + NODE_W,
              }}
            >
              <svg
                className="branch-connections"
                width="100%"
                height={layout.height}
                aria-hidden="true"
              >
                <defs>
                  <marker
                    id="branch-arrow"
                    markerWidth="5"
                    markerHeight="5"
                    refX="4"
                    refY="2.5"
                    orient="auto"
                  >
                    <path d="M0 0 L5 2.5 L0 5" />
                  </marker>
                </defs>
                {layout.rows.map((row) => (
                  <g key={row.edge.uci}>
                    <path
                      className={`${row.edge.prepared.length ? "prepared" : ""} ${hovered?.uci === row.edge.uci ? "active" : ""}`}
                      d={`M${PAD + NODE_W} ${layout.rootY + NODE_H / 2} C${PAD + NODE_W + 20} ${layout.rootY + NODE_H / 2},${PAD + COLUMN - 20} ${row.center},${PAD + COLUMN - 3} ${row.center}`}
                      markerEnd="url(#branch-arrow)"
                    />
                    {row.replies.map((reply) => (
                      <path
                        key={reply.edge.uci}
                        className={`${reply.edge.prepared.length ? "prepared" : ""} ${hovered?.uci === row.edge.uci && hovered.reply === reply.edge.uci ? "active" : ""}`}
                        d={`M${PAD + COLUMN + NODE_W} ${row.center} C${PAD + COLUMN + NODE_W + 20} ${row.center},${PAD + COLUMN * 2 - 20} ${reply.y + REPLY_H / 2},${PAD + COLUMN * 2 - 3} ${reply.y + REPLY_H / 2}`}
                        markerEnd="url(#branch-arrow)"
                      />
                    ))}
                  </g>
                ))}
              </svg>
              <div
                className="branch-node branch-current"
                style={{
                  left: PAD,
                  top: layout.rootY,
                  height: NODE_H,
                  width: NODE_W,
                }}
                aria-label="Текущая позиция"
              >
                <span className="branch-move">
                  {currentMove?.san ? (
                    <MoveLabel
                      san={currentMove.san}
                      color={currentMove.actor ?? "white"}
                    />
                  ) : (
                    "Начало"
                  )}
                </span>
                <span className="branch-counts">
                  <strong>
                    Партий:{" "}
                    {contextPending && !projection
                      ? "…"
                      : (selected?.games ?? 0)}
                  </strong>
                </span>
                {selected && <Results edge={selected} />}
              </div>
              {layout.rows.map((row) => (
                <div key={row.edge.uci}>
                  {node(row.edge, row.y)}
                  {row.replies.map((reply) =>
                    node(reply.edge, reply.y, row.edge),
                  )}
                  {row.hidden > 0 && (
                    <span
                      className="branch-hidden"
                      style={{
                        left: PAD + COLUMN * 2,
                        width: NODE_W,
                        top: (row.replies.at(-1)?.y ?? row.y) + REPLY_H + 2,
                      }}
                    >
                      Ещё {row.hidden}
                    </span>
                  )}
                </div>
              ))}
              {!loading && !contextPending && !edges.length && (
                <div
                  className="graph-empty"
                  style={{ left: PAD + COLUMN, top: layout.rootY }}
                >
                  {route.line.length
                    ? "Ветвь закончилась"
                    : "Завершённые партии появятся здесь"}
                </div>
              )}
            </div>
          </div>
          <div className="graph-footer">
            {loading && <span role="status">Загрузка…</span>}
            {ordered.length > visibleCount && (
              <button onClick={() => setVisibleCount((n) => n + 5)}>
                Ещё {ordered.length - visibleCount} продолжений ↓
              </button>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}

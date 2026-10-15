import { OpeningQualityDetails } from "./OpeningDetails";
import { EvidenceProvider, useEvidence, MoreEvidence } from "./Evidence";
import {
  OpeningViews,
  ExtendedViews,
  EngineEvidence,
  NodeEvaluations,
  Cases,
} from "./AnalyticsViews";
import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
} from "react";
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { ChessBoard } from "../chess/ChessBoard";
import { BoardSession } from "../chess/BoardSession";
import { errorMessage } from "../api";
import type { AccountStatus } from "../lichess";
import { BoardView } from "./BoardView";
import { number, timestamp, metricStatus, groupLabel } from "./format";
import {
  getReport,
  getState,
  getGame,
  requestSync,
  retryQuarantined,
  enqueue,
  pauseAnalysis,
  cancelAnalysis,
  type Filter,
  type Report,
  type StatisticsState,
  type GameDetail,
  type Metric,
  type ResultRow,
  type TimeRow,
  type TrendPoint,
} from "./api";
import "./statistics.css";

const tabs = [
  ["overview", "Обзор"],
  ["openings", "Дебюты"],
  ["quality", "Качество игры"],
  ["time", "Время"],
  ["endings", "Реализация и эндшпиль"],
  ["games", "Партии"],
] as const;
const metricIds = ["G03", "Q02", "O08", "E02", "E08"];
const phaseNames: Record<string, string> = {
  opening: "Дебют",
  middlegame: "Миттельшпиль",
  endgame: "Эндшпиль",
};
const eventNames: Record<string, string> = {
  significant: "Существенная потеря",
  significant_loss: "Существенная потеря",
  opportunity: "Возможность",
  missed_opportunity: "Упущенная возможность",
  advantage_lost: "Потеря преимущества",
  defense: "Защита",
  mate_lost: "Потеря матового продолжения",
};
function dateValue(ms: number | null | undefined) {
  if (ms == null) return "";
  return new Date(ms + 3 * 3600000).toISOString().slice(0, 10);
}
function dateMs(s: string, end = false) {
  return s ? Date.parse(s + "T00:00:00+03:00") + (end ? 86400000 : 0) : null;
}
function numeric(value: string) {
  return value === "" ? null : Number(value);
}

export function Statistics({
  active,
  account,
  onAuthenticate,
  busy,
  initialSection,
}: {
  active: boolean;
  account: AccountStatus;
  onAuthenticate: () => void;
  busy: boolean;
  initialSection?: string;
}) {
  const [tab, setTab] = useState("overview"),
    [filter, setFilter] = useState<Filter>({ variant: "standard" }),
    [report, setReport] = useState<Report | null>(null),
    [state, setState] = useState<StatisticsState | null>(null);
  const [error, setError] = useState(""),
    [loading, setLoading] = useState(false),
    [detail, setDetail] = useState<GameDetail | null>(null),
    [initialPly, setInitialPly] = useState(0),
    [initialRunId, setInitialRunId] = useState(""),
    [definition, setDefinition] = useState<Metric | null>(null),
    [compare, setCompare] = useState(false),
    [gameLimit, setGameLimit] = useState(100);
  const [chart, setChart] = useState<
    "games" | "score" | "rating" | "mean_loss" | "accuracy" | "median_spent"
  >("score");
  const [generation, refresh] = useReducer((n) => n + 1, 0);
  const request = useRef(0),
    gameRequest = useRef(0);
  function closeGame() {
    gameRequest.current++;
    setDetail(null);
  }
  useEffect(() => {
    if (initialSection) setTab(initialSection);
  }, [initialSection]);
  const session = useMemo(() => new BoardSession(), []);
  const [, paint] = useReducer((n) => n + 1, 0);
  const change = useCallback(
    (next: Partial<Filter>) =>
      setFilter((previous) => ({
        ...previous,
        ...("opening" in next && !("opening_label_id" in next)
          ? { opening_label_id: null }
          : {}),
        ...next,
      })),
    [],
  );
  useEffect(() => {
    if (!active || !isTauri()) return;
    let disposed = false;
    let inflight = false,
      again = false,
      againReport = false;
    let stop: (() => void) | undefined;
    const load = async (withReport = true) => {
      if (inflight) {
        again = true;
        againReport ||= withReport;
        return;
      }
      inflight = true;
      const revision = ++request.current;
      if (withReport) setLoading(true);
      try {
        const nextState = await getState(filter.account_id);
        if (!withReport) {
          if (!disposed && revision === request.current) setState(nextState);
          return;
        }
        const effective = {
          ...filter,
          account_id:
            filter.account_id ??
            nextState.active_account_id ??
            nextState.accounts[0]?.id ??
            null,
        };
        const next = await getReport(effective);
        if (!disposed && revision === request.current) {
          setState(nextState);
          setReport(next);
          setError("");
        }
      } catch (e) {
        if (!disposed && revision === request.current)
          setError(errorMessage(e));
      } finally {
        if (!disposed && revision === request.current) setLoading(false);
        inflight = false;
        if (again && !disposed) {
          again = false;
          const pendingReport = againReport;
          againReport = false;
          void load(pendingReport);
        }
      }
    };
    void load();
    const timer = setInterval(() => void load(false), 10000);
    const fallback = setInterval(() => void load(), 60000);
    void listen("statistics-updated", () => void load())
      .then((unsubscribe) => {
        if (disposed) unsubscribe();
        else stop = unsubscribe;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      clearInterval(timer);
      clearInterval(fallback);
      stop?.();
    };
  }, [active, filter, generation, account.id, account.status]);
  useLayoutEffect(() => {
    if (!report?.opening_tree.fen) return;
    try {
      session.loadFen(report.opening_tree.fen);
      session.orientation = filter.color === "black" ? "black" : "white";
      paint();
    } catch (e) {
      setError(errorMessage(e));
    }
  }, [report?.opening_tree.fen, session, filter.color]);
  async function action(fn: () => Promise<unknown>) {
    setError("");
    try {
      await fn();
      refresh();
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function openGame(id: string, ply = 0, runId?: string) {
    const token = ++gameRequest.current;
    setError("");
    try {
      const preferredProfile = filter.profile_id ?? report?.selected_profile_id;
      const data = await getGame(
        id,
        filter.account_id ?? report?.filter.account_id,
      );
      if (token !== gameRequest.current) return;
      if (runId && !data.runs.some((r) => r.id === runId))
        throw new Error("Результат анализа устарел; обновите статистику");
      setDetail(data);
      setInitialPly(ply);
      setInitialRunId(
        data.runs.find((r) =>
          runId ? r.id === runId : r.profile_id === preferredProfile,
        )?.id ??
          (preferredProfile ? "__missing__" : data.runs[0]?.id) ??
          "",
      );
    } catch (e) {
      if (token === gameRequest.current) setError(errorMessage(e));
    }
  }
  function showGames(ids: string[]) {
    change({ game_ids: ids });
    setTab("games");
    closeGame();
  }
  function resultTable(rows: ResultRow[], name: string) {
    return (
      <div className="stats-table-wrap">
        <table aria-label={name}>
          <thead>
            <tr>
              <th>Группа</th>
              <th>N</th>
              <th>W / D / L</th>
              <th>Очки, %</th>
              <th>Loss, п.п.</th>
              <th>Решений с анализом</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.key}>
                <td>
                  <button
                    className="stats-link"
                    onClick={() => showGames(row.game_ids)}
                  >
                    {groupLabel(row.label)}
                  </button>
                </td>
                <td>{row.games}</td>
                <td>
                  {row.wins} / {row.draws} / {row.losses}
                </td>
                <td>{number(row.score)}</td>
                <td>{number(row.mean_loss, 2)}</td>
                <td>{row.analysed}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }
  function timeTable(rows: TimeRow[], name: string) {
    return (
      <div className="stats-table-wrap">
        <table aria-label={name}>
          <thead>
            <tr>
              <th>Диапазон</th>
              <th>Решения</th>
              <th>Часы</th>
              <th>Медиана, с</th>
              <th>P25…P75, с</th>
              <th>Всего, с</th>
              <th>P90, с</th>
              <th>Loss, п.п.</th>
              <th>Потери ≥10 п.п.</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((row) => (
              <tr key={row.key}>
                <td>
                  <button
                    className="stats-link"
                    onClick={() => showGames(row.game_ids)}
                  >
                    {groupLabel(row.label)}
                  </button>
                </td>
                <td>{row.decisions}</td>
                <td>{row.clock_sample}</td>
                <td>{number(row.median_spent)}</td>
                <td>
                  {row.p25_spent == null
                    ? "—"
                    : number(row.p25_spent) + "…" + number(row.p75_spent)}
                </td>
                <td>{number(row.total_spent)}</td>
                <td>{number(row.p90_spent)}</td>
                <td>{number(row.mean_loss, 2)}</td>
                <td>
                  {row.analysed ? row.significant + "/" + row.analysed : "—"}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }
  function metricsTable(metrics: Metric[], name: string) {
    return (
      <div className="stats-table-wrap">
        <table aria-label={name}>
          <thead>
            <tr>
              <th>Показатель</th>
              <th>Значение</th>
              <th>N / доступно</th>
              <th>95% интервал</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {metrics.map((m) => (
              <tr key={m.id}>
                <td>
                  <button
                    className="stats-link"
                    onClick={() => setDefinition(m)}
                  >
                    {m.name}
                  </button>
                </td>
                <td>
                  {m.value == null
                    ? metricStatus(m.status)
                    : number(m.value, 2) + " " + m.unit}
                  {m.value != null &&
                    !["ready", "available", "complete"].includes(m.status) && (
                      <div className="stats-value-status">
                        {metricStatus(m.status)}
                      </div>
                    )}
                </td>
                <td>
                  {m.sample}/{m.eligible}
                </td>
                <td>
                  {m.low == null
                    ? "—"
                    : number(m.low, 2) + "…" + number(m.high, 2)}
                </td>
                <td>
                  <button
                    disabled={!m.game_ids.length}
                    onClick={() => showGames(m.game_ids)}
                    aria-label={"Партии: " + m.name}
                  >
                    Партии
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    );
  }
  const controls = Array.from(
    new Set([
      filter.control ?? "",
      ...(report?.games.map((g) => g.control) ?? []),
    ]),
  ).filter(Boolean);
  const families = Array.from(
    new Set(report?.openings.map((o) => o.family) ?? []),
  );
  const activeObservation =
    state?.accounts.find(
      (a) => a.id === (filter.account_id ?? state.active_account_id),
    ) ?? state?.accounts[0];
  return (
    <EvidenceProvider report={report}>
      <main className="statistics" hidden={!active} aria-busy={loading}>
        <header className="stats-header">
          <h1>Статистика</h1>
          <div className="stats-top-actions">
            <button
              onClick={() => setCompare(!compare)}
              className={compare ? "selected" : ""}
            >
              Сравнить периоды
            </button>
            <button
              disabled={
                !isTauri() ||
                busy ||
                account.status !== "connected" ||
                activeObservation?.id !== state?.active_account_id
              }
              onClick={() => void action(requestSync)}
            >
              Синхронизировать
            </button>
          </div>
        </header>
        <div className="stats-tabs" role="tablist" aria-label="Статистика">
          {tabs.map(([id, label]) => (
            <button
              key={id}
              role="tab"
              aria-selected={tab === id}
              className={tab === id ? "active" : ""}
              onClick={() => {
                setTab(id);
                closeGame();
              }}
            >
              {label}
            </button>
          ))}
        </div>
        {!isTauri() ? (
          <div className="stats-empty">
            Статистика доступна в приложении ChessPush
          </div>
        ) : (
          <>
            {state && !state.accounts.length ? (
              <div className="stats-empty">
                <button
                  className="primary"
                  onClick={onAuthenticate}
                  disabled={busy}
                >
                  Подключить Lichess
                </button>
              </div>
            ) : (
              <>
                <div className="stats-filters">
                  <label>
                    Аккаунт
                    <select
                      aria-label="Аккаунт статистики"
                      value={filter.account_id ?? activeObservation?.id ?? ""}
                      onChange={(e) => {
                        setFilter({
                          account_id: e.target.value,
                          variant: "standard",
                        });
                        closeGame();
                      }}
                    >
                      {state?.accounts.map((a) => (
                        <option key={a.id} value={a.id}>
                          {a.username}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    С
                    <input
                      aria-label="Начало периода"
                      type="date"
                      value={dateValue(filter.since_ms)}
                      onChange={(e) =>
                        change({ since_ms: dateMs(e.target.value) })
                      }
                    />
                  </label>
                  <label>
                    По
                    <input
                      aria-label="Конец периода"
                      type="date"
                      value={dateValue(
                        filter.until_ms == null
                          ? null
                          : filter.until_ms - 86400000,
                      )}
                      onChange={(e) =>
                        change({ until_ms: dateMs(e.target.value, true) })
                      }
                    />
                  </label>
                  <label>
                    Контроль
                    <select
                      aria-label="Контроль времени статистики"
                      value={filter.control ?? ""}
                      onChange={(e) =>
                        change({ control: e.target.value || null })
                      }
                    >
                      <option value="">Все</option>
                      {controls.map((c) => (
                        <option key={c} value={c}>
                          {c}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label>
                    Цвет
                    <select
                      aria-label="Цвет статистики"
                      value={filter.color ?? ""}
                      onChange={(e) =>
                        change({ color: e.target.value || null })
                      }
                    >
                      <option value="">Оба</option>
                      <option value="white">Белые</option>
                      <option value="black">Чёрные</option>
                    </select>
                  </label>
                  <details className="stats-extra">
                    <summary>Фильтры</summary>
                    <div className="stats-extra-fields">
                      <label>
                        Категория
                        <select
                          aria-label="Категория рейтинга"
                          value={filter.speed ?? ""}
                          onChange={(e) =>
                            change({ speed: e.target.value || null })
                          }
                        >
                          <option value="">Все</option>
                          {[
                            "ultraBullet",
                            "bullet",
                            "blitz",
                            "rapid",
                            "classical",
                            "correspondence",
                          ].map((s) => (
                            <option key={s} value={s}>
                              {groupLabel(s)}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label>
                        Фаза
                        <select
                          aria-label="Фаза игры"
                          value={filter.phase ?? ""}
                          onChange={(e) =>
                            change({ phase: e.target.value || null })
                          }
                        >
                          <option value="">Все</option>
                          {Object.entries(phaseNames).map(([key, label]) => (
                            <option key={key} value={key}>
                              {label}
                            </option>
                          ))}
                        </select>
                      </label>
                      <label>
                        Дебют
                        <input
                          aria-label="Фильтр дебюта"
                          value={filter.opening ?? ""}
                          onChange={(e) =>
                            change({ opening: e.target.value || null })
                          }
                        />
                      </label>
                      <label>
                        Рейтинг от
                        <input
                          type="number"
                          aria-label="Рейтинг соперника от"
                          value={filter.opponent_rating_min ?? ""}
                          onChange={(e) =>
                            change({
                              opponent_rating_min: numeric(e.target.value),
                            })
                          }
                        />
                      </label>
                      <label>
                        Рейтинг до
                        <input
                          type="number"
                          aria-label="Рейтинг соперника до"
                          value={filter.opponent_rating_max ?? ""}
                          onChange={(e) =>
                            change({
                              opponent_rating_max: numeric(e.target.value),
                            })
                          }
                        />
                      </label>
                      <label>
                        Партии
                        <select
                          aria-label="Тип партий"
                          value={
                            filter.rated == null ? "all" : String(filter.rated)
                          }
                          onChange={(e) =>
                            change({
                              rated:
                                e.target.value === "all"
                                  ? null
                                  : e.target.value === "true",
                            })
                          }
                        >
                          <option value="all">Все</option>
                          <option value="true">Рейтинговые</option>
                          <option value="false">Нерейтинговые</option>
                        </select>
                      </label>
                      <label>
                        Источник
                        <select
                          aria-label="Источник партий"
                          value={filter.source ?? ""}
                          onChange={(e) =>
                            change({ source: e.target.value || null })
                          }
                        >
                          <option value="">Все</option>
                          <option value="lichess">Lichess</option>
                        </select>
                      </label>
                      <label>
                        Вариант
                        <select
                          aria-label="Вариант шахмат"
                          value={filter.variant ?? ""}
                          onChange={(e) =>
                            change({ variant: e.target.value || null })
                          }
                        >
                          <option value="standard">Обычные шахматы</option>
                          <option value="">Все</option>
                        </select>
                      </label>
                      <label className="checkbox">
                        <input
                          type="checkbox"
                          checked={filter.include_uncertain ?? false}
                          onChange={(e) =>
                            change({ include_uncertain: e.target.checked })
                          }
                        />
                        Пограничные партии T0
                      </label>
                      <button
                        onClick={() =>
                          setFilter({
                            account_id: activeObservation?.id,
                            variant: "standard",
                          })
                        }
                      >
                        Сбросить фильтры
                      </button>
                    </div>
                  </details>
                </div>
                {filter.opening && (
                  <div className="stats-selection">
                    {filter.opening}
                    <button
                      onClick={() =>
                        change({
                          opening: null,
                          line: null,
                          position_key: null,
                        })
                      }
                    >
                      Все дебюты
                    </button>
                  </div>
                )}
                {filter.opening_label_id && (
                  <div className="stats-selection">
                    {report?.opening_analysis.hierarchy
                      .find((n) => n.id === filter.opening_label_id)
                      ?.path.join(" › ") ?? "Выбранная ветвь"}
                    <button
                      onClick={() =>
                        change({
                          opening_label_id: null,
                          opening: null,
                          line: null,
                          position_key: null,
                        })
                      }
                    >
                      Все ветви
                    </button>
                  </div>
                )}
                {filter.game_ids && (
                  <div className="stats-selection">
                    Выбрано партий: {filter.game_ids.length}
                    <button onClick={() => change({ game_ids: null })}>
                      Снять выборку
                    </button>
                  </div>
                )}
                <div className="stats-status" aria-live="polite">
                  {loading && <span>Обновление статистики…</span>}
                  <span>
                    {activeObservation
                      ? "Наблюдение с " + timestamp(activeObservation.t0_ms)
                      : ""}
                  </span>
                  <span>
                    Синхронизация:{" "}
                    {state?.sync
                      ? metricStatus(state.sync.status)
                      : loading
                        ? "загрузка"
                        : "—"}
                    {state?.sync?.last_success_ms
                      ? " · " + timestamp(state.sync.last_success_ms)
                      : ""}
                  </span>
                  {state?.sync && (
                    <span>
                      Ожидают сверки: {state.sync.pending_games} · Незакрытые
                      диапазоны: {state.sync.pending_ranges}
                    </span>
                  )}
                  {state?.sync?.boundary_uncertain && (
                    <span>Граница T0: неполное подтверждение</span>
                  )}
                  {state?.sync?.error_message && (
                    <span role="alert">
                      {state.sync.error_message}
                      {state.sync.retry_not_before_ms
                        ? " · повтор " +
                          timestamp(state.sync.retry_not_before_ms)
                        : ""}
                    </span>
                  )}
                </div>
                {error && (
                  <div className="error" role="alert">
                    {error}
                    <button onClick={refresh}>Повторить</button>
                  </div>
                )}
                {compare && (
                  <section className="stats-comparison">
                    <h2>Сравнение периодов</h2>
                    <div className="stats-filters">
                      <label>
                        Период B с
                        <input
                          aria-label="Сравнение с"
                          type="date"
                          value={dateValue(filter.comparison_since_ms)}
                          onChange={(e) =>
                            change({
                              comparison_since_ms: dateMs(e.target.value),
                            })
                          }
                        />
                      </label>
                      <label>
                        Период B по
                        <input
                          aria-label="Сравнение по"
                          type="date"
                          value={dateValue(
                            filter.comparison_until_ms == null
                              ? null
                              : filter.comparison_until_ms - 86400000,
                          )}
                          onChange={(e) =>
                            change({
                              comparison_until_ms: dateMs(e.target.value, true),
                            })
                          }
                        />
                      </label>
                    </div>
                    {report &&
                      metricsTable(report.comparison, "Сравнение периодов")}
                  </section>
                )}
                {detail ? (
                  <BoardView
                    game={detail.game}
                    runs={detail.runs}
                    events={detail.events}
                    analyses={detail.analyses}
                    initialPly={initialPly}
                    preferredRunId={initialRunId}
                    onClose={closeGame}
                  />
                ) : report ? (
                  <>
                    <div className="stats-coverage">
                      {report.total_games} партий · {report.analysed_decisions}/
                      {report.total_decisions} решений с анализом · Вне выборки:{" "}
                      {report.excluded_games}
                      {state?.engine.jobs.some(
                        (j) => j.state === "running",
                      ) && (
                        <span>
                          {state.engine.jobs
                            .filter((j) => j.state === "running")
                            .map((j) => "Анализ: " + j.progress + "/" + j.total)
                            .join(" · ")}
                        </span>
                      )}
                      {state?.engine.paused && <span>Анализ: пауза</span>}
                      <select
                        aria-label="Профиль статистики"
                        title={
                          report.profile_labels[
                            filter.profile_id ??
                              report.selected_profile_id ??
                              ""
                          ] ?? "Последний совместимый профиль"
                        }
                        value={
                          filter.profile_id ?? report.selected_profile_id ?? ""
                        }
                        onChange={(e) =>
                          change({ profile_id: e.target.value || null })
                        }
                      >
                        <option value="">Последний совместимый профиль</option>
                        {report.profile_ids.map((p) => (
                          <option key={p} value={p}>
                            {report.profile_labels[p] ?? p.slice(0, 12)}
                          </option>
                        ))}
                      </select>
                    </div>
                    {tab === "overview" && (
                      <section>
                        {metricsTable(
                          metricIds
                            .map((id) =>
                              report.metrics.find((m) => m.id === id),
                            )
                            .filter((m): m is Metric => Boolean(m)),
                          "Основные показатели",
                        )}
                        <div className="stats-chart-toolbar">
                          <h2>Динамика</h2>
                          <select
                            aria-label="Показатель динамики"
                            value={chart}
                            onChange={(e) =>
                              setChart(e.target.value as typeof chart)
                            }
                          >
                            <option value="games">Партии, шт.</option>
                            <option value="score">Набранные очки, %</option>
                            <option value="rating">Рейтинг</option>
                            <option value="mean_loss">Потеря q, п.п.</option>
                            <option value="accuracy">Accuracy, %</option>
                            <option value="median_spent">Время, с</option>
                          </select>
                        </div>
                        <div className="stats-actions">
                          <label>
                            Окно динамики
                            <select
                              aria-label="Окно динамики"
                              value={filter.trend_granularity ?? "week"}
                              onChange={(e) =>
                                change({ trend_granularity: e.target.value })
                              }
                            >
                              <option value="day">День</option>
                              <option value="week">Неделя</option>
                              <option value="month">Месяц</option>
                              <option value="rolling50">
                                Последние 50 партий
                              </option>
                            </select>
                          </label>
                        </div>
                        <TrendChart
                          points={report.trend}
                          metric={chart}
                          onSelect={showGames}
                        />
                        {resultTable(report.results, "Результаты по группам")}
                        <ExtendedViews
                          report={report}
                          section="overview"
                          onOpen={openGame}
                          onGames={showGames}
                        />
                        {metricsTable(
                          report.metrics.filter(
                            (m) =>
                              m.id.startsWith("G") && !metricIds.includes(m.id),
                          ),
                          "Результаты и рейтинг",
                        )}
                      </section>
                    )}
                    {tab === "openings" && (
                      <section>
                        <div className="stats-filters">
                          <label>
                            Семейство
                            <select
                              aria-label="Дебютное семейство"
                              value={
                                families.includes(filter.opening ?? "")
                                  ? (filter.opening ?? "")
                                  : ""
                              }
                              onChange={(e) =>
                                change({
                                  opening: e.target.value || null,
                                  line: null,
                                  position_key: null,
                                })
                              }
                            >
                              <option value="">Все</option>
                              {families.map((f) => (
                                <option key={f}>{f}</option>
                              ))}
                            </select>
                          </label>
                          <label>
                            Ходы
                            <select
                              aria-label="Автор продолжения"
                              value={filter.opening_actor ?? "all"}
                              onChange={(e) =>
                                change({ opening_actor: e.target.value })
                              }
                            >
                              <option value="all">Все</option>
                              <option value="own">Собственные</option>
                              <option value="opponent">
                                Ответы соперников
                              </option>
                            </select>
                          </label>
                          <button
                            className={
                              filter.tree_mode !== "line" ? "selected" : ""
                            }
                            onClick={() => change({ tree_mode: "position" })}
                          >
                            По позиции
                          </button>
                          <button
                            className={
                              filter.tree_mode === "line" ? "selected" : ""
                            }
                            onClick={() =>
                              change({ tree_mode: "line", position_key: null })
                            }
                          >
                            По линии
                          </button>
                        </div>
                        <div className="stats-board-layout">
                          <div>
                            <ChessBoard
                              session={session}
                              revision={session.revision}
                              onChange={() => {
                                const moves = session.uciMoves;
                                if (moves.length)
                                  change({
                                    line: [
                                      ...report.opening_tree.line,
                                      ...moves,
                                    ],
                                    opening_initial_fen:
                                      report.opening_tree.initial_fen,
                                    position_key: null,
                                  });
                                paint();
                              }}
                            />
                            <div className="board-controls">
                              <button
                                onClick={() =>
                                  change({ line: [], position_key: null })
                                }
                              >
                                В начало
                              </button>
                              <button
                                disabled={!report.opening_tree.line.length}
                                onClick={() =>
                                  change({
                                    line: report.opening_tree.line.slice(0, -1),
                                    position_key: null,
                                  })
                                }
                              >
                                Назад
                              </button>
                              <button
                                onClick={() => {
                                  session.flip();
                                  paint();
                                }}
                              >
                                Перевернуть
                              </button>
                            </div>
                          </div>
                          <div>
                            <h2>Продолжения</h2>
                            <NodeEvaluations
                              node={report.opening_analysis.node}
                              onOpen={openGame}
                            />
                            <div>
                              {report.opening_tree.games} партий ·{" "}
                              {report.opening_tree.visits} посещений ·{" "}
                              {report.opening_tree.distinct_paths} путей
                            </div>
                            <div className="stats-uci-line">
                              {report.opening_tree.line.join(" ") ||
                                "Начальная позиция"}
                            </div>
                            <table aria-label="Дерево продолжений">
                              <thead>
                                <tr>
                                  <th>Ход</th>
                                  <th>Автор</th>
                                  <th>N / посещения</th>
                                  <th>Loss, п.п. · n</th>
                                  <th>Доля, %</th>
                                  <th>Время, с · N</th>
                                  <th>Очки, %</th>
                                  <th></th>
                                </tr>
                              </thead>
                              <tbody>
                                {report.opening_tree.edges.map((edge) => (
                                  <tr key={edge.uci}>
                                    <td>
                                      <button
                                        className="stats-link"
                                        onClick={() =>
                                          change({
                                            opening_initial_fen:
                                              report.opening_tree.initial_fen,
                                            line: [
                                              ...report.opening_tree.line,
                                              edge.uci,
                                            ],
                                            position_key:
                                              filter.tree_mode === "line"
                                                ? null
                                                : edge.position_key,
                                          })
                                        }
                                      >
                                        {edge.san}
                                      </button>
                                    </td>
                                    <td>
                                      {edge.actor === "own"
                                        ? "Свой"
                                        : edge.actor === "opponent"
                                          ? "Соперник"
                                          : "Оба"}
                                    </td>
                                    <td>
                                      {edge.result.games} / {edge.visits}
                                    </td>
                                    <td>
                                      {number(edge.quality.mean_loss, 2)} ·{" "}
                                      {edge.quality.wdl_sample}
                                    </td>
                                    <td>{number(edge.frequency)}</td>
                                    <td>
                                      {number(edge.spent_median)} ·{" "}
                                      {edge.spent_sample}
                                    </td>
                                    <td>{number(edge.result.score)}</td>
                                    <td>
                                      <button
                                        onClick={() =>
                                          showGames(edge.result.game_ids)
                                        }
                                      >
                                        Партии
                                      </button>
                                      <OpeningQualityDetails
                                        quality={edge.quality}
                                        selectorBase={{
                                          kind: "edge",
                                          key: edge.uci,
                                        }}
                                        onOpen={openGame}
                                      />
                                      <Cases
                                        selector={{
                                          kind: "edge",
                                          key: edge.uci,
                                        }}
                                        items={edge.cases}
                                        onOpen={openGame}
                                      />
                                    </td>
                                  </tr>
                                ))}
                              </tbody>
                            </table>
                            {!!report.opening_tree.unplayed.length && (
                              <details>
                                <summary>
                                  Легальные ходы без партий (
                                  {report.opening_tree.unplayed.length})
                                </summary>
                                <div className="stats-unplayed">
                                  {report.opening_tree.unplayed.map((move) => (
                                    <button
                                      key={move.uci}
                                      onClick={() =>
                                        change({
                                          opening_initial_fen:
                                            report.opening_tree.initial_fen,
                                          line: [
                                            ...report.opening_tree.line,
                                            move.uci,
                                          ],
                                          position_key: null,
                                        })
                                      }
                                    >
                                      {move.san}
                                    </button>
                                  ))}
                                </div>
                              </details>
                            )}
                            <button
                              disabled={!report.opening_tree.game_ids.length}
                              onClick={() =>
                                showGames(report.opening_tree.game_ids)
                              }
                            >
                              Партии позиции
                            </button>
                          </div>
                        </div>
                        <div className="stats-table-wrap">
                          <table aria-label="Дебютные варианты">
                            <thead>
                              <tr>
                                <th>ECO</th>
                                <th>Вариант</th>
                                <th>N</th>
                                <th>W / D / L</th>
                                <th>Очки, %</th>
                                <th>95% интервал очков</th>
                                <th>Рейтинг соперников</th>
                                <th>Loss, п.п.</th>
                                <th>Выход q</th>
                                <th></th>
                              </tr>
                            </thead>
                            <tbody>
                              {report.openings.map((o) => (
                                <tr key={o.eco + o.name}>
                                  <td>{o.eco}</td>
                                  <td>
                                    <button
                                      className="stats-link"
                                      onClick={() =>
                                        change({
                                          opening: o.name,
                                          line: null,
                                          position_key: null,
                                        })
                                      }
                                    >
                                      {o.name}
                                    </button>
                                  </td>
                                  <td>{o.result.games}</td>
                                  <td>
                                    {o.result.wins}/{o.result.draws}/
                                    {o.result.losses}
                                  </td>
                                  <td>{number(o.result.score)}</td>
                                  <td>
                                    {o.result.score_low == null
                                      ? "—"
                                      : number(o.result.score_low) +
                                        "…" +
                                        number(o.result.score_high)}
                                  </td>
                                  <td>
                                    {number(o.result.opponent_rating_median, 0)}{" "}
                                    · N={o.result.rating_sample ?? 0}
                                  </td>
                                  <td>{number(o.result.mean_loss, 2)}</td>
                                  <td>
                                    {number(o.exit_q, 2)} ({o.exit_sample})
                                  </td>
                                  <td>
                                    <button
                                      onClick={() =>
                                        showGames(o.result.game_ids)
                                      }
                                    >
                                      Партии
                                    </button>
                                  </td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                        {metricsTable(
                          report.metrics.filter((m) => m.id.startsWith("O")),
                          "Показатели дебютов",
                        )}
                        <OpeningViews
                          report={report}
                          onOpen={openGame}
                          onGames={showGames}
                          onFilter={change}
                          renderMetrics={metricsTable}
                        />
                        <h2>Потери в дебюте</h2>
                        <div className="stats-table-wrap">
                          <table aria-label="Потери в дебюте">
                            <thead>
                              <tr>
                                <th>Позиция</th>
                                <th>Ход</th>
                                <th>N / партии</th>
                                <th>Средняя потеря, п.п.</th>
                                <th>Сумма, п.п.</th>
                                <th>Время, с</th>
                                <th></th>
                              </tr>
                            </thead>
                            <tbody>
                              {report.problems.map((p) => (
                                <tr key={p.position_key + ":" + p.played_uci}>
                                  <td>
                                    <button
                                      className="stats-link"
                                      onClick={() =>
                                        void openGame(
                                          p.first_game_id,
                                          Math.max(0, p.first_ply - 1),
                                        )
                                      }
                                    >
                                      {p.opening?.name ?? "Позиция"} · ply{" "}
                                      {p.first_ply}
                                    </button>
                                  </td>
                                  <td>{p.san}</td>
                                  <td>
                                    {p.occurrences} / {p.games}
                                  </td>
                                  <td>
                                    {number(p.mean_loss, 2)} · median{" "}
                                    {number(p.median_loss, 2)}
                                  </td>
                                  <td>{number(p.sum_loss, 2)}</td>
                                  <td>{number(p.median_spent)}</td>
                                  <td>
                                    <button
                                      onClick={() => showGames(p.game_ids)}
                                    >
                                      Партии
                                    </button>
                                    <Cases
                                      selector={{
                                        kind: "problem",
                                        key:
                                          p.position_key + "|" + p.played_uci,
                                      }}
                                      items={p.cases}
                                      before
                                      onOpen={openGame}
                                    />
                                    {p.comparison.length > 0 && (
                                      <details>
                                        <summary>Периоды</summary>
                                        {metricsTable(
                                          p.comparison,
                                          "Периоды проблемной линии",
                                        )}
                                      </details>
                                    )}
                                  </td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                      </section>
                    )}
                    {tab === "quality" && (
                      <section>
                        {metricsTable(
                          report.metrics.filter(
                            (m) =>
                              m.id.startsWith("Q") ||
                              m.id.startsWith("S01") ||
                              m.id.startsWith("P"),
                          ),
                          "Качество решений",
                        )}
                        <details>
                          <summary>
                            Распределение потерь модельного очка
                          </summary>
                          {(report.metrics.find((m) => m.id === "Q02")
                            ?.sample ?? 0) > 0 ? (
                            <div className="stats-table-wrap">
                              <table aria-label="Гистограмма Lq">
                                <thead>
                                  <tr>
                                    <th>Потеря, п.п.</th>
                                    <th>Решения</th>
                                    <th>
                                      0…
                                      {
                                        report.metrics.find(
                                          (m) => m.id === "Q02",
                                        )?.sample
                                      }
                                    </th>
                                    <th></th>
                                  </tr>
                                </thead>
                                <tbody>
                                  {report.analysis.histogram.map((bin) => (
                                    <tr key={bin.lo}>
                                      <td>
                                        {bin.hi == null
                                          ? "≥" + number(bin.lo)
                                          : "[" +
                                            number(bin.lo) +
                                            "; " +
                                            number(bin.hi) +
                                            ")"}
                                      </td>
                                      <td>{bin.decisions}</td>
                                      <td>
                                        <svg
                                          viewBox="0 0 200 16"
                                          width="160"
                                          height="16"
                                          role="img"
                                          aria-label={
                                            bin.decisions + " решений"
                                          }
                                        >
                                          <rect
                                            width={
                                              (200 * bin.decisions) /
                                              (report.metrics.find(
                                                (m) => m.id === "Q02",
                                              )?.sample ?? 1)
                                            }
                                            height="16"
                                            fill="var(--accent)"
                                          />
                                        </svg>
                                      </td>
                                      <td>
                                        <button
                                          disabled={!bin.game_ids.length}
                                          onClick={() =>
                                            showGames(bin.game_ids)
                                          }
                                        >
                                          Партии
                                        </button>
                                      </td>
                                    </tr>
                                  ))}
                                </tbody>
                              </table>
                            </div>
                          ) : (
                            <div className="stats-chart-empty">
                              Нет пригодных оценок
                            </div>
                          )}
                        </details>
                        {timeTable(report.phases, "Качество по фазам")}
                        <ExtendedViews
                          report={report}
                          section="quality"
                          onOpen={openGame}
                          onGames={showGames}
                        />
                        <EngineEvidence
                          analysis={report.analysis}
                          prefixes={["Q"]}
                          onOpen={openGame}
                        />
                        <h2>События</h2>
                        <EventTable events={report.events} onOpen={openGame} />
                        {state && (
                          <details>
                            <summary>Загрузка и валидация</summary>
                            <table aria-label="Покрытие загрузки">
                              <tbody>
                                <tr>
                                  <th>Принятые партии вне фильтра</th>
                                  <td>{report.excluded_games}</td>
                                </tr>
                                <tr>
                                  <th>Ожидают сверки</th>
                                  <td>{state.sync?.pending_games ?? "—"}</td>
                                </tr>
                                <tr>
                                  <th>Диапазоны загрузки</th>
                                  <td>{state.sync?.pending_ranges ?? "—"}</td>
                                </tr>
                                <tr>
                                  <th>Карантин</th>
                                  <td>
                                    {state.sync?.quarantined_games ?? "—"}
                                  </td>
                                </tr>
                                {Object.entries(
                                  state.quarantine.reduce<
                                    Record<string, number>
                                  >((m, q) => {
                                    m[q.reason] = (m[q.reason] ?? 0) + 1;
                                    return m;
                                  }, {}),
                                ).map(([reason, n]) => (
                                  <tr key={reason}>
                                    <th>{reason}</th>
                                    <td>{n}</td>
                                  </tr>
                                ))}
                              </tbody>
                            </table>
                          </details>
                        )}
                      </section>
                    )}
                    {tab === "time" && (
                      <section>
                        {metricsTable(
                          report.metrics.filter((m) => m.id.startsWith("T")),
                          "Временные показатели",
                        )}
                        <h2>Остаток часов до решения</h2>
                        {timeTable(report.time, "Качество по остатку времени")}
                        <h2>Расход часов на решение</h2>
                        {timeTable(
                          report.time_spent,
                          "Качество по расходу времени",
                        )}
                        {timeTable(report.phases, "Время по фазам")}
                        <ExtendedViews
                          report={report}
                          section="time"
                          onOpen={openGame}
                          onGames={showGames}
                        />
                        <EventTable
                          subset="time"
                          events={report.events.filter(
                            (e) =>
                              e.clock_seconds != null ||
                              e.spent_seconds != null,
                          )}
                          onOpen={openGame}
                        />
                      </section>
                    )}
                    {tab === "endings" && (
                      <section>
                        {metricsTable(
                          report.metrics.filter(
                            (m) => m.id.startsWith("E") || m.id.startsWith("S"),
                          ),
                          "Реализация и защита",
                        )}
                        <ExtendedViews
                          report={report}
                          section="endings"
                          onOpen={openGame}
                          onGames={showGames}
                        />
                        <EngineEvidence
                          analysis={report.analysis}
                          prefixes={["E", "S"]}
                          onOpen={openGame}
                        />
                        <h2>Типы окончаний</h2>
                        {resultTable(report.endings, "Типы окончаний")}
                        <EventTable
                          subset="endings"
                          events={report.events.filter(
                            (e) =>
                              e.phase === "endgame" || e.kind !== "decision",
                          )}
                          onOpen={openGame}
                        />
                      </section>
                    )}
                    {tab === "games" && (
                      <section>
                        <div className="stats-actions">
                          <button
                            disabled={!report.games.length}
                            onClick={() =>
                              void action(() =>
                                enqueue(report.games.map((g) => g.id)),
                              )
                            }
                          >
                            Анализировать выборку
                          </button>
                          <button
                            onClick={() =>
                              void action(() =>
                                pauseAnalysis(!state?.engine.paused),
                              )
                            }
                          >
                            {state?.engine.paused
                              ? "Продолжить анализ"
                              : "Пауза анализа"}
                          </button>
                        </div>
                        <div className="stats-table-wrap">
                          <table aria-label="Собственные партии">
                            <thead>
                              <tr>
                                <th>Дата</th>
                                <th>Соперник</th>
                                <th>Рейтинг</th>
                                <th>Цвет</th>
                                <th>Контроль, с+с</th>
                                <th>Результат</th>
                                <th>Дебют</th>
                                <th>Accuracy</th>
                                <th>Loss</th>
                                <th>Решений с анализом</th>
                                <th>Наблюдаемый интервал, с</th>
                              </tr>
                            </thead>
                            <tbody>
                              {report.games.slice(0, gameLimit).map((g) => (
                                <tr key={g.id}>
                                  <td>
                                    <button
                                      className="stats-link"
                                      onClick={() => void openGame(g.id)}
                                    >
                                      {timestamp(g.timestamp_ms)}
                                    </button>
                                  </td>
                                  <td>{g.opponent}</td>
                                  <td>{g.opponent_rating ?? "—"}</td>
                                  <td>
                                    {g.own_color === "white"
                                      ? "Белые"
                                      : "Чёрные"}
                                  </td>
                                  <td>{g.control}</td>
                                  <td>{g.result}</td>
                                  <td>{g.opening?.name ?? "—"}</td>
                                  <td>{number(g.accuracy)}</td>
                                  <td>{number(g.mean_loss, 2)}</td>
                                  <td>
                                    {g.analysed
                                      ? g.analysed + "/" + g.own_decisions
                                      : metricStatus(g.status)}
                                  </td>
                                  <td
                                    title={
                                      g.interval_quality === "exact_end"
                                        ? "Подтверждённое завершение − начало"
                                        : "Последний ход − начало; не длительность партии"
                                    }
                                  >
                                    {number(g.observed_interval_seconds, 0)}
                                    {g.interval_quality === "last_move_span"
                                      ? " · до хода"
                                      : ""}
                                  </td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                        {report.games.length > gameLimit && (
                          <button onClick={() => setGameLimit(gameLimit + 100)}>
                            Ещё 100 партий
                          </button>
                        )}
                        <details>
                          <summary>
                            Очередь анализа ({state?.engine.jobs.length ?? 0})
                          </summary>
                          {!state?.engine.configured && (
                            <div>Stockfish не настроен</div>
                          )}
                          <table aria-label="Очередь анализа">
                            <thead>
                              <tr>
                                <th>Партия</th>
                                <th>Состояние</th>
                                <th>Позиции</th>
                                <th></th>
                              </tr>
                            </thead>
                            <tbody>
                              {state?.engine.jobs.map((j) => (
                                <tr key={j.id}>
                                  <td>
                                    <button
                                      onClick={() => void openGame(j.game_id)}
                                    >
                                      {j.game_id}
                                    </button>
                                  </td>
                                  <td>
                                    {metricStatus(j.state)}
                                    {j.last_error ? " · " + j.last_error : ""}
                                  </td>
                                  <td>
                                    {j.progress}/{j.total}
                                  </td>
                                  <td>
                                    <button
                                      disabled={
                                        !["queued", "running"].includes(j.state)
                                      }
                                      onClick={() =>
                                        void action(() => cancelAnalysis(j.id))
                                      }
                                    >
                                      Отменить
                                    </button>
                                  </td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </details>
                        {!!state?.quarantine.length && (
                          <details className="stats-quarantine">
                            <summary>
                              Отклонённые загрузки ({state.quarantine.length})
                            </summary>
                            <table aria-label="Отклонённые загрузки">
                              <thead>
                                <tr>
                                  <th>ID</th>
                                  <th>Получено</th>
                                  <th>Причина</th>
                                  <th>Исходные данные</th>
                                  <th></th>
                                </tr>
                              </thead>
                              <tbody>
                                {state.quarantine.map((q) => (
                                  <tr key={q.account_id + q.external_id}>
                                    <td>{q.external_id}</td>
                                    <td>{timestamp(q.received_at_ms)}</td>
                                    <td>{q.reason}</td>
                                    <td>
                                      <details>
                                        <summary>JSON</summary>
                                        <pre>{q.raw_json}</pre>
                                      </details>
                                    </td>
                                    <td>
                                      <button
                                        disabled={!activeObservation?.enabled}
                                        onClick={() =>
                                          void action(() =>
                                            retryQuarantined(
                                              q.account_id,
                                              q.external_id,
                                            ),
                                          )
                                        }
                                      >
                                        Загрузить повторно
                                      </button>
                                    </td>
                                  </tr>
                                ))}
                              </tbody>
                            </table>
                          </details>
                        )}
                      </section>
                    )}
                    {report.total_games === 0 && (
                      <div className="stats-empty">
                        {loading
                          ? "Загрузка"
                          : "Нет партий в выбранном периоде"}
                      </div>
                    )}
                  </>
                ) : loading ? (
                  <div className="stats-empty">Загрузка</div>
                ) : null}
              </>
            )}
          </>
        )}
        {definition && (
          <div
            className="modal-backdrop"
            onMouseDown={() => setDefinition(null)}
          >
            <section
              className="stats-definition"
              role="dialog"
              aria-modal="true"
              aria-label="Определение показателя"
              onMouseDown={(e) => e.stopPropagation()}
            >
              <header>
                <h2>{definition.name}</h2>
                <button
                  onClick={() => setDefinition(null)}
                  aria-label="Закрыть определение"
                >
                  ×
                </button>
              </header>
              <p>{definition.definition}</p>
              <dl>
                <dt>Единицы</dt>
                <dd>{definition.unit || "число"}</dd>
                <dt>Выборка</dt>
                <dd>
                  {definition.sample}/{definition.eligible}
                </dd>
                <dt>Метод</dt>
                <dd>{definition.method}</dd>
              </dl>
            </section>
          </div>
        )}
      </main>
    </EvidenceProvider>
  );
}
function EventTable({
  events,
  onOpen,
  subset = "all",
}: {
  subset?: string;
  events: Report["events"];
  onOpen: (id: string, ply: number, runId?: string) => void;
}) {
  const evidence = useEvidence(events, { kind: "events", key: subset });
  return (
    <div className="stats-table-wrap">
      <table aria-label="События партий">
        <caption>События · {evidence.total}</caption>
        <thead>
          <tr>
            <th>Партия</th>
            <th>Ply</th>
            <th>Ход</th>
            <th>Событие</th>
            <th>Loss q, п.п.</th>
            <th>Часы, с</th>
            <th>Spent, с</th>
            <th>Лучший</th>
          </tr>
        </thead>
        <tbody>
          {evidence.items.map((e, i) => (
            <tr key={e.game_id + ":" + e.ply + ":" + e.kind + ":" + i}>
              <td>
                <button
                  className="stats-link"
                  onClick={() =>
                    onOpen(e.game_id, Math.max(0, e.ply - 1), e.run_id)
                  }
                >
                  {e.game_id.replace("lichess:", "")}
                </button>
              </td>
              <td>{e.ply}</td>
              <td>{e.san}</td>
              <td>{eventNames[e.kind] ?? groupLabel(e.kind)}</td>
              <td>{number(e.loss_q, 2)}</td>
              <td>{number(e.clock_seconds)}</td>
              <td>{number(e.spent_seconds)}</td>
              <td>{e.best_move ?? "—"}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <MoreEvidence value={evidence} />
    </div>
  );
}
function TrendChart({
  points,
  metric,
  onSelect,
}: {
  points: TrendPoint[];
  metric:
    "games" | "score" | "rating" | "mean_loss" | "accuracy" | "median_spent";
  onSelect: (ids: string[]) => void;
}) {
  const available = points.filter((p) => p[metric] != null);
  if (!available.length)
    return <div className="stats-chart-empty">Нет данных графика</div>;
  const groups = Array.from(new Set(available.map((p) => p.stratum))),
    values = available.map((p) => p[metric]!),
    min = metric === "rating" ? Math.floor(Math.min(...values) / 50) * 50 : 0,
    max =
      metric === "score" || metric === "accuracy"
        ? 100
        : Math.max(min + 1, ...values) * 1.08;
  const first = Math.min(...available.map((p) => p.timestamp_ms)),
    last = Math.max(first + 1, ...available.map((p) => p.timestamp_ms));
  const x = (t: number) => 65 + ((t - first) / (last - first)) * 875,
    y = (v: number) => 235 - ((v - min) / (max - min)) * 210;
  return (
    <div className="stats-chart">
      <svg viewBox="0 0 980 285" role="img" aria-label={"Динамика " + metric}>
        {[0, 0.25, 0.5, 0.75, 1].map((r) => (
          <g key={r}>
            <line
              x1="65"
              y1={y(min + (max - min) * r)}
              x2="940"
              y2={y(min + (max - min) * r)}
              className="stats-chart-grid"
            />
            <text x="55" y={y(min + (max - min) * r) + 4} textAnchor="end">
              {number(min + (max - min) * r, 1)}
            </text>
          </g>
        ))}
        {groups.map((group, i) => {
          let draw = "",
            gap = true;
          for (const point of points.filter((p) => p.stratum === group)) {
            const value = point[metric];
            if (value == null) {
              gap = true;
              continue;
            }
            draw +=
              (gap ? "M" : "L") + x(point.timestamp_ms) + " " + y(value) + " ";
            gap = false;
          }
          return (
            <path
              key={group}
              d={draw}
              fill="none"
              stroke={i % 2 ? "var(--success)" : "var(--accent)"}
              strokeWidth="2"
            />
          );
        })}
        {available.map((p, i) => (
          <circle
            key={i}
            cx={x(p.timestamp_ms)}
            cy={y(p[metric]!)}
            r="4"
            fill="var(--accent)"
            tabIndex={0}
            role="button"
            aria-label={
              timestamp(p.timestamp_ms) +
              ": " +
              number(p[metric]) +
              ", N " +
              p.games
            }
            onClick={() => onSelect(p.game_ids)}
            onKeyDown={(e) => {
              if (e.key === "Enter" || e.key === " ") onSelect(p.game_ids);
            }}
          >
            <title>
              {timestamp(p.timestamp_ms)}: {number(p[metric])} · {p.games}{" "}
              партий · {groupLabel(p.stratum)}
            </title>
          </circle>
        ))}
        <text x="65" y="270">
          {timestamp(first)}
        </text>
        <text x="940" y="270" textAnchor="end">
          {timestamp(last)}
        </text>
      </svg>
      <div className="stats-chart-legend">
        {groups.map(groupLabel).join(" · ")}
      </div>
    </div>
  );
}

import { useLayoutEffect, useMemo, useReducer, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { INITIAL_FEN } from "chessops/fen";
import { ChessBoard } from "../chess/ChessBoard";
import { BoardSession } from "../chess/BoardSession";
import {
  sortOpenings,
  type OpeningSummary,
  type OpeningSummaryRow,
  type OpeningSort,
} from "./openingSummary";

type Side = "white" | "black";
type Summaries = Record<Side, OpeningSummary>;
const columns: [OpeningSort, string, string?][] = [
  ["eco", "ECO"],
  ["name", "Дебют"],
  ["games", "Партии"],
  ["wins", "В", "Победы"],
  ["draws", "Н", "Ничьи"],
  ["losses", "П", "Поражения"],
  ["score", "Очки, %"],
  ["coverage", "Репертуар, %"],
];
const percent = (value: number | null) =>
  value == null
    ? "—"
    : `${value.toLocaleString("ru-RU", { maximumFractionDigits: 1 })}%`;

function OpeningTable({
  summary,
  query,
  queryRevision,
  preview,
  onPreview,
  onOpen,
}: {
  summary: OpeningSummary;
  query: string;
  queryRevision: number;
  preview: OpeningSummaryRow | null;
  onPreview: (row: OpeningSummaryRow) => void;
  onOpen: (row: OpeningSummaryRow) => void;
}) {
  const [sort, setSort] = useState<{
    key: OpeningSort;
    direction: "asc" | "desc";
  }>({
    key: "games",
    direction: "desc",
  });
  const sideName = summary.color === "white" ? "Белыми" : "Чёрными";
  const lowerSide = sideName.toLowerCase();
  const wrap = useRef<HTMLDivElement>(null);
  const [pageSize, setPageSize] = useState(3);
  const [pageState, setPageState] = useState({ key: "", index: 0 });
  const pageKey = JSON.stringify([queryRevision, sort.key, sort.direction]);
  const rows = useMemo(
    () =>
      sortOpenings(
        summary.rows.filter(
          (row) =>
            !query ||
            [row.eco, row.name ?? "Дебют не определён"].some((value) =>
              value?.toLocaleLowerCase("ru").includes(query),
            ),
        ),
        sort.key,
        sort.direction,
      ),
    [summary.rows, query, sort.key, sort.direction],
  );
  const pages = Math.max(1, Math.ceil(rows.length / pageSize));
  const page =
    pageState.key === pageKey ? Math.min(pageState.index, pages - 1) : 0;
  const first = page * pageSize;
  const visibleRows = rows.slice(first, first + pageSize);
  useLayoutEffect(() => {
    const element = wrap.current;
    if (!element) return;
    const measure = () => {
      const head =
        element.querySelector("thead")?.getBoundingClientRect().height ?? 34;
      const foot =
        element.querySelector("tfoot")?.getBoundingClientRect().height ?? 34;
      const next = Math.max(
        1,
        Math.floor((element.clientHeight - head - foot) / 34),
      );
      setPageSize((previous) => (previous === next ? previous : next));
    };
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  }, [Boolean(summary.total_games)]);
  function changeSort(key: OpeningSort) {
    setPageState({ key: "", index: 0 });
    setSort((previous) => ({
      key,
      direction:
        previous.key === key
          ? previous.direction === "desc"
            ? "asc"
            : "desc"
          : key === "name" || key === "eco"
            ? "asc"
            : "desc",
    }));
  }
  return (
    <section
      className="opening-side-table"
      aria-label={`Дебюты ${sideName.toLowerCase()}`}
      data-color={summary.color}
    >
      <header className="opening-side-heading">
        <h2>{sideName}</h2>
        <span>
          Партий: <strong>{summary.total_games.toLocaleString("ru-RU")}</strong>
        </span>
        <span
          className="opening-result-legend"
          aria-label={`Победы ${summary.wins}, ничьи ${summary.draws}, поражения ${summary.losses}`}
        >
          <span className="opening-wins" title="Победы">
            В {summary.wins.toLocaleString("ru-RU")}
          </span>
          <span className="opening-draws" title="Ничьи">
            Н {summary.draws.toLocaleString("ru-RU")}
          </span>
          <span className="opening-losses" title="Поражения">
            П {summary.losses.toLocaleString("ru-RU")}
          </span>
        </span>
      </header>
      {summary.total_games > 0 && (
        <div className="opening-result-bar" aria-hidden="true">
          <i
            className="opening-result-wins"
            style={{ flexGrow: summary.wins }}
          />
          <i
            className="opening-result-draws"
            style={{ flexGrow: summary.draws }}
          />
          <i
            className="opening-result-losses"
            style={{ flexGrow: summary.losses }}
          />
        </div>
      )}
      {!summary.total_games ? (
        <div className="stats-empty">
          Нет завершённых партий {sideName.toLowerCase()}.
        </div>
      ) : (
        <>
          <div className="stats-table-wrap" ref={wrap}>
            <table aria-label={`Статистика дебютов ${sideName.toLowerCase()}`}>
              <colgroup>
                <col className="opening-col-eco" />
                <col className="opening-col-name" />
                <col className="opening-col-games" />
                <col className="opening-col-wdl" />
                <col className="opening-col-wdl" />
                <col className="opening-col-wdl" />
                <col className="opening-col-score" />
                <col className="opening-col-coverage" />
              </colgroup>
              <thead>
                <tr>
                  {columns.map(([key, label, title]) => (
                    <th
                      key={key}
                      className={
                        key === "wins"
                          ? "opening-wins"
                          : key === "draws"
                            ? "opening-draws"
                            : key === "losses"
                              ? "opening-losses"
                              : undefined
                      }
                      aria-sort={
                        sort.key === key
                          ? sort.direction === "asc"
                            ? "ascending"
                            : "descending"
                          : "none"
                      }
                    >
                      <button
                        onClick={() => changeSort(key)}
                        title={
                          title ??
                          (key === "coverage"
                            ? `Доля своих сыгранных ходов в репертуаре до ${summary.coverage_depth_moves}-го хода`
                            : undefined)
                        }
                      >
                        {label}
                        <span aria-hidden="true">
                          {sort.key === key
                            ? sort.direction === "desc"
                              ? " ↓"
                              : " ↑"
                            : ""}
                        </span>
                      </button>
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {visibleRows.map((row) => (
                  <tr
                    key={JSON.stringify([row.color, row.eco, row.name])}
                    data-color={row.color}
                    data-eco={row.eco ?? ""}
                    data-preview-fen={row.preview_fen ?? ""}
                    className={
                      preview?.color === row.color &&
                      preview.eco === row.eco &&
                      preview.name === row.name
                        ? "previewed"
                        : ""
                    }
                    onMouseEnter={() => onPreview(row)}
                    onFocusCapture={() => onPreview(row)}
                  >
                    <td className="opening-eco">{row.eco ?? "—"}</td>
                    <td>
                      <button
                        className="stats-link"
                        title={row.name ?? "Дебют не определён"}
                        onClick={() => onOpen(row)}
                      >
                        {row.name ?? "Дебют не определён"}
                      </button>
                    </td>
                    <td>{row.games}</td>
                    <td className="opening-wins">{row.wins}</td>
                    <td className="opening-draws">{row.draws}</td>
                    <td className="opening-losses">{row.losses}</td>
                    <td className="opening-score">{percent(row.score)}</td>
                    <td
                      className="opening-coverage"
                      title={`${row.coverage.covered_decisions} из ${row.coverage.total_decisions} своих ходов в ${row.coverage.games} партиях; до ${row.coverage.depth_moves}-го хода; ${row.coverage.repertoire_documents} PGN`}
                    >
                      <span>{percent(row.coverage.percent)}</span>
                      {row.coverage.percent != null && (
                        <span
                          className="opening-coverage-bar"
                          aria-hidden="true"
                        >
                          <i style={{ width: `${row.coverage.percent}%` }} />
                        </span>
                      )}
                    </td>
                  </tr>
                ))}
                {!visibleRows.length && (
                  <tr>
                    <td colSpan={8} className="opening-no-results">
                      Ничего не найдено
                    </td>
                  </tr>
                )}
              </tbody>
              <tfoot>
                <tr>
                  <td></td>
                  <th scope="row">Всего</th>
                  <td>{summary.total_games}</td>
                  <td className="opening-wins">{summary.wins}</td>
                  <td className="opening-draws">{summary.draws}</td>
                  <td className="opening-losses">{summary.losses}</td>
                  <td className="opening-score">{percent(summary.score)}</td>
                  <td></td>
                </tr>
              </tfoot>
            </table>
          </div>
          <nav
            className="opening-pagination"
            aria-label={`Страницы дебютов ${lowerSide}`}
          >
            <span role="status" aria-live="polite">
              {rows.length
                ? `${first + 1}–${first + visibleRows.length} из ${rows.length}`
                : "0 из 0"}
            </span>
            <button
              disabled={page === 0}
              aria-label={`Предыдущая страница: ${lowerSide}`}
              onClick={() => setPageState({ key: pageKey, index: page - 1 })}
            >
              <svg viewBox="0 0 20 20" aria-hidden="true">
                <path d="m12 4-6 6 6 6" />
              </svg>
            </button>
            <button
              disabled={page >= pages - 1}
              aria-label={`Следующая страница: ${lowerSide}`}
              onClick={() => setPageState({ key: pageKey, index: page + 1 })}
            >
              <svg viewBox="0 0 20 20" aria-hidden="true">
                <path d="m8 4 6 6-6 6" />
              </svg>
            </button>
          </nav>
        </>
      )}
    </section>
  );
}
function OpeningPreview({ row }: { row: OpeningSummaryRow | null }) {
  const session = useMemo(() => new BoardSession(), []);
  const [revision, paint] = useReducer((value) => value + 1, 0);
  useLayoutEffect(() => {
    if (row?.preview_pgn) session.loadPgn(row.preview_pgn);
    else session.loadFen(row?.preview_fen ?? INITIAL_FEN);
    session.orientation = row?.color ?? "white";
    paint();
  }, [session, row?.preview_fen, row?.preview_pgn, row?.color]);
  const title = row ? (row.name ?? "Дебют не определён") : "Начальная позиция";
  return (
    <aside
      className={`opening-preview${row ? " has-opening" : ""}`}
      aria-label="Позиция дебюта"
      data-fen={session.fen}
      data-color={row?.color ?? "white"}
    >
      <header>
        <div>
          {row?.eco && <span className="opening-preview-eco">{row.eco}</span>}
          {row && <span>{row.color === "white" ? "Белыми" : "Чёрными"}</span>}
        </div>
        <h2 title={title}>{title}</h2>
      </header>
      <ChessBoard
        session={session}
        revision={revision}
        readOnly
        onChange={() => {}}
      />
    </aside>
  );
}
export function OpeningTables({
  summaries,
  loading,
  busy,
  onAuthenticate,
  onOpenOpening,
  contextKey,
}: {
  summaries: Summaries | null;
  loading: boolean;
  busy: boolean;
  onAuthenticate: () => void;
  onOpenOpening: (row: OpeningSummaryRow) => void;
  contextKey: string;
}) {
  const [selected, setSelected] = useState<{
    contextKey: string;
    color: Side;
    eco: string | null;
    name: string | null;
  } | null>(null);
  const explorer = useRef<HTMLDivElement>(null);
  const [availableHeight, setAvailableHeight] = useState(0);
  const [query, setQuery] = useState("");
  const [queryRevision, reviseQuery] = useReducer((value) => value + 1, 0);
  const search = query.trim().toLocaleLowerCase("ru");
  useLayoutEffect(() => {
    const element = explorer.current;
    if (!element) return;
    const measure = () => {
      if (!element.getClientRects().length) return;
      const next = Math.max(
        320,
        Math.floor(
          window.innerHeight -
            element.getBoundingClientRect().top -
            window.scrollY -
            16,
        ),
      );
      setAvailableHeight((previous) => (previous === next ? previous : next));
    };
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    if (element.parentElement) observer.observe(element.parentElement);
    window.addEventListener("resize", measure);
    measure();
    return () => {
      observer.disconnect();
      window.removeEventListener("resize", measure);
    };
  }, []);
  const preview =
    selected?.contextKey === contextKey
      ? (summaries?.[selected.color].rows.find(
          (row) => row.eco === selected.eco && row.name === selected.name,
        ) ?? null)
      : null;
  return (
    <div
      ref={explorer}
      className="simple-opening-results opening-explorer"
      style={
        availableHeight
          ? ({
              "--opening-available-height": `${availableHeight}px`,
            } as CSSProperties)
          : undefined
      }
    >
      <div className="opening-tables">
        {!summaries ? (
          <div className="stats-empty" role="status">
            {loading ? "Загрузка дебютов…" : ""}
          </div>
        ) : !summaries.white.accounts.length ? (
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
            <input
              type="search"
              className="opening-search"
              aria-label="Поиск дебютов"
              placeholder="ECO или название дебюта"
              value={query}
              onChange={(event) => {
                setQuery(event.target.value);
                reviseQuery();
              }}
            />
            <div className="opening-table-list">
              {(["white", "black"] as const).map((side) => (
                <OpeningTable
                  key={`${contextKey}:${side}`}
                  summary={summaries[side]}
                  query={search}
                  queryRevision={queryRevision}
                  preview={preview}
                  onPreview={(row) =>
                    setSelected({
                      contextKey,
                      color: row.color,
                      eco: row.eco,
                      name: row.name,
                    })
                  }
                  onOpen={onOpenOpening}
                />
              ))}
            </div>
          </>
        )}
      </div>
      <OpeningPreview row={preview} />
    </div>
  );
}

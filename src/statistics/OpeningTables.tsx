import { useLayoutEffect, useMemo, useReducer, useState } from "react";
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
  preview,
  onPreview,
  onOpen,
}: {
  summary: OpeningSummary;
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
  function changeSort(key: OpeningSort) {
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
      <h2>{sideName}</h2>
      {!summary.total_games ? (
        <div className="stats-empty">
          Нет завершённых партий {sideName.toLowerCase()}.
        </div>
      ) : (
        <div className="stats-table-wrap">
          <table aria-label={`Статистика дебютов ${sideName.toLowerCase()}`}>
            <thead>
              <tr>
                {columns.map(([key, label, title]) => (
                  <th
                    key={key}
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
              {sortOpenings(summary.rows, sort.key, sort.direction).map(
                (row) => (
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
                    <td>{row.eco ?? "—"}</td>
                    <td>
                      <button
                        className="stats-link"
                        onClick={() => onOpen(row)}
                      >
                        {row.name ?? "Дебют не определён"}
                      </button>
                    </td>
                    <td>{row.games}</td>
                    <td>{row.wins}</td>
                    <td>{row.draws}</td>
                    <td>{row.losses}</td>
                    <td>{percent(row.score)}</td>
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
                ),
              )}
            </tbody>
            <tfoot>
              <tr>
                <td></td>
                <th scope="row">Всего</th>
                <td>{summary.total_games}</td>
                <td>{summary.wins}</td>
                <td>{summary.draws}</td>
                <td>{summary.losses}</td>
                <td>{percent(summary.score)}</td>
                <td></td>
              </tr>
            </tfoot>
          </table>
        </div>
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
  const preview =
    selected?.contextKey === contextKey
      ? (summaries?.[selected.color].rows.find(
          (row) => row.eco === selected.eco && row.name === selected.name,
        ) ?? null)
      : null;
  return (
    <div className="simple-opening-results opening-explorer">
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
          (["white", "black"] as const).map((side) => (
            <OpeningTable
              key={side}
              summary={summaries[side]}
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
          ))
        )}
      </div>
      <OpeningPreview row={preview} />
    </div>
  );
}

import { Cases, NodeEvaluations } from "./AnalyticsViews";
import { OpeningQualityDetails } from "./OpeningDetails";
import type { Filter, Report } from "./api";
import { number } from "./format";

/** Numerical details address the same path as the map; there is no second board or navigation state. */
export function OpeningContinuationStats({
  report,
  onMap,
  onOpen,
  onGames,
}: {
  report: Report;
  onMap: (next: Partial<Filter>) => void;
  onOpen: (id: string, ply: number, runId?: string) => void;
  onGames: (ids: string[]) => void;
}) {
  return (
    <details>
      <summary>Числовые подробности продолжений</summary>
      <NodeEvaluations node={report.opening_analysis.node} onOpen={onOpen} />
      <p>
        {report.opening_tree.games} партий · {report.opening_tree.visits}{" "}
        посещений · {report.opening_tree.distinct_paths} путей
      </p>
      <div className="stats-table-wrap">
        <table aria-label="Числовые подробности продолжений">
          <thead>
            <tr>
              <th>Ход</th>
              <th>Автор</th>
              <th>Партии / посещения</th>
              <th>Потеря, п.п. · решений</th>
              <th>Частота, %</th>
              <th>Время, с · решений</th>
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
                      onMap({
                        opening_initial_fen: report.opening_tree.initial_fen,
                        line: [...report.opening_tree.line, edge.uci],
                        position_key: edge.position_key,
                      })
                    }
                  >
                    {edge.san}
                  </button>
                </td>
                <td>
                  {edge.actor === "own"
                    ? "Мой ход"
                    : edge.actor === "opponent"
                      ? "Соперник"
                      : "Обе стороны"}
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
                  {number(edge.spent_median)} · {edge.spent_sample}
                </td>
                <td>{number(edge.result.score)}</td>
                <td>
                  <button onClick={() => onGames(edge.result.game_ids)}>
                    Партии
                  </button>
                  <OpeningQualityDetails
                    quality={edge.quality}
                    selectorBase={{ kind: "edge", key: edge.uci }}
                    onOpen={onOpen}
                  />
                  <Cases
                    selector={{ kind: "edge", key: edge.uci }}
                    items={edge.cases}
                    onOpen={onOpen}
                  />
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </details>
  );
}

import type { GameSummary, Report, StatisticsState } from "./api";
import { number, timestamp, metricStatus } from "./format";
import { reviewCases } from "./review";
import { parseFen } from "chessops/fen";
function moveNumber(fen: string, ply: number) {
  try {
    return parseFen(fen).unwrap().fullmoves;
  } catch {
    return Math.ceil(ply / 2);
  }
}

type OpenGame = (id: string, ply?: number, runId?: string) => void;
export function analysisLabel(
  game: GameSummary,
  state: StatisticsState | null,
) {
  const jobs =
    state?.engine.jobs.filter((job) => job.game_id === game.id) ?? [];
  if (jobs.some((job) => job.state === "running")) return "Анализируется";
  if (jobs.some((job) => job.state === "queued"))
    return state?.engine.paused ? "В очереди · пауза" : "В очереди";
  if (game.analysed >= game.own_decisions && game.analysed > 0)
    return "Анализ готов";
  if (game.analysed)
    return `Частично · ${game.analysed}/${game.own_decisions} решений`;
  if (jobs.some((job) => job.state.endsWith("failed"))) return "Ошибка анализа";
  return "Без анализа";
}
export function PracticeGames({
  games,
  state,
  onOpen,
  recent = false,
}: {
  games: GameSummary[];
  state: StatisticsState | null;
  onOpen: OpenGame;
  recent?: boolean;
}) {
  return (
    <div className="stats-table-wrap practice-games">
      <table aria-label={recent ? "Последние партии" : "Собственные партии"}>
        <thead>
          <tr>
            <th>Дата</th>
            <th>Соперник</th>
            <th>Сторона</th>
            <th>Результат</th>
            {!recent && <th>Дебют</th>}
            <th>Анализ</th>
          </tr>
        </thead>
        <tbody>
          {games.map((game) => (
            <tr key={game.id}>
              <td>
                <button className="stats-link" onClick={() => onOpen(game.id)}>
                  {timestamp(game.timestamp_ms)}
                </button>
              </td>
              <td>
                {game.opponent}{" "}
                {game.opponent_rating != null && (
                  <span className="practice-rating">
                    {game.opponent_rating}
                  </span>
                )}
              </td>
              <td>{game.own_color === "white" ? "Белыми" : "Чёрными"}</td>
              <td>{game.result}</td>
              {!recent && <td>{game.opening?.name ?? "Не определён"}</td>}
              <td>{analysisLabel(game, state)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
export function PracticeReview({
  report,
  onOpen,
  limit = 4,
}: {
  report: Report;
  onOpen: OpenGame;
  limit?: number;
}) {
  const cases = reviewCases(report.events, report.games).slice(0, limit);
  return (
    <section className="practice-review" aria-label="Моменты для разбора">
      <h2>На разбор</h2>
      <div className="practice-context">
        С анализом {report.analysed_decisions} из {report.total_decisions}{" "}
        решений
      </div>
      {cases.length ? (
        <ul>
          {cases.map(({ event, game, count, key }) => (
            <li key={key}>
              <div>
                <strong>
                  {moveNumber(event.fen, event.ply)}
                  {event.actor === "black" ? "…" : "."} {event.san}
                </strong>{" "}
                · {game?.opponent}
                <br />
                <span>
                  {count > 1 ? `${count} случая · ` : ""}
                  {event.kind === "opportunity_missed" ||
                  event.kind === "missed_opportunity"
                    ? "Упущенная возможность · "
                    : ""}
                  Потеря по оценке движка {number(event.loss_q, 1)} п.п.
                </span>
              </div>
              <button
                onClick={() =>
                  onOpen(
                    event.game_id,
                    Math.max(0, event.ply - 1),
                    event.run_id,
                  )
                }
              >
                Открыть момент
              </button>
            </li>
          ))}
        </ul>
      ) : (
        <p>
          {report.analysed_decisions
            ? "В доступном анализе нет подтверждённых моментов для этой подборки."
            : "Моменты для разбора появятся после анализа партий."}
        </p>
      )}
    </section>
  );
}
export function PracticeOverview({
  report,
  state,
  onOpen,
  onMap,
  onGames,
  onDetails,
  onAnalysis,
}: {
  report: Report;
  state: StatisticsState | null;
  onOpen: OpenGame;
  onMap: () => void;
  onGames: () => void;
  onDetails: () => void;
  onAnalysis: () => void;
}) {
  const wins = report.games.filter((game) => game.outcome === "win").length;
  const draws = report.games.filter((game) => game.outcome === "draw").length;
  const losses = report.games.filter((game) => game.outcome === "loss").length;
  return (
    <section className="practice-overview">
      <div className="practice-summary">
        <strong>{report.total_games} партий</strong>
        <span>
          {wins} побед · {draws} ничьих · {losses} поражений
        </span>
        <button className="primary" onClick={onMap}>
          Открыть карту игры
        </button>
      </div>
      <PracticeReview report={report} onOpen={onOpen} />
      {!report.analysed_decisions && (
        <div className="practice-analysis-prompt">
          <span>
            Карта и результаты доступны. Для оценки решений нужен Stockfish.
          </span>
          <button onClick={onAnalysis}>
            {state?.engine.configured
              ? "Анализировать партии"
              : "Настроить анализ"}
          </button>
        </div>
      )}
      <header className="practice-section-title">
        <h2>Последние партии</h2>
        <button className="stats-link" onClick={onGames}>
          Все партии
        </button>
      </header>
      <PracticeGames
        games={report.games.slice(0, 4)}
        state={state}
        onOpen={onOpen}
        recent
      />
      <button onClick={onDetails}>Подробная статистика</button>
      {state?.engine.jobs.some((job) => job.state === "running") && (
        <span className="practice-context"> · {metricStatus("running")}</span>
      )}
    </section>
  );
}

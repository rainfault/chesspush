import type { Report, StatisticsState } from "./api";
import { cancelAnalysis, enqueue, pauseAnalysis } from "./api";
import { metricStatus } from "./format";
export function AnalysisQueue({
  state,
  report,
  onAction,
  onOpen,
  onSettings,
}: {
  state: StatisticsState | null;
  report: Report;
  onAction: (action: () => Promise<unknown>) => void;
  onOpen: (id: string) => void;
  onSettings: () => void;
}) {
  return (
    <section className="practice-queue" aria-label="Управление анализом">
      <div className="stats-actions">
        <button
          disabled={!report.games.length || !state?.engine.configured}
          onClick={() =>
            onAction(() => enqueue(report.games.map((game) => game.id)))
          }
        >
          Анализировать выборку
        </button>
        <button
          onClick={() => onAction(() => pauseAnalysis(!state?.engine.paused))}
        >
          {state?.engine.paused ? "Продолжить анализ" : "Пауза анализа"}
        </button>
        {!state?.engine.configured && (
          <button onClick={onSettings}>Настроить Stockfish</button>
        )}
      </div>
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
          {state?.engine.jobs.map((job) => (
            <tr key={job.id}>
              <td>
                <button
                  className="stats-link"
                  onClick={() => onOpen(job.game_id)}
                >
                  {report.games.find((game) => game.id === job.game_id)
                    ?.opponent ?? "Партия"}
                </button>
              </td>
              <td>
                {metricStatus(job.state)}
                {job.last_error && (
                  <span role="alert"> · {job.last_error}</span>
                )}
              </td>
              <td>
                {job.progress}/{job.total}
              </td>
              <td>
                {job.state.endsWith("failed") || job.state === "cancelled" ? (
                  <button
                    onClick={() => onAction(() => enqueue([job.game_id]))}
                  >
                    Повторить
                  </button>
                ) : (
                  <button
                    disabled={!["queued", "running"].includes(job.state)}
                    onClick={() => onAction(() => cancelAnalysis(job.id))}
                  >
                    Отменить
                  </button>
                )}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </section>
  );
}

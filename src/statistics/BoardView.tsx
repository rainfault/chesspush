import { makeSan } from "chessops/san";
import { parseUci } from "chessops/util";
import { normalizeMove } from "chessops/chess";
import { EvaluationChart } from "./EvaluationChart";
import type { EngineDetails } from "./analyticsTypes";
import { useLayoutEffect, useMemo, useReducer, useState } from "react";
import { ChessBoard } from "../chess/ChessBoard";
import { BoardSession } from "../chess/BoardSession";
import type { DecisionEvent, EngineRun, StoredGame } from "./api";
import { number, timestamp, groupLabel } from "./format";
interface Props {
  game: StoredGame;
  runs: EngineRun[];
  events: DecisionEvent[];
  analyses: Record<string, EngineDetails>;
  initialPly?: number;
  preferredRunId?: string;
  onClose: () => void;
  onPositionChange?: (ply: number, runId?: string) => void;
}
export function BoardView({
  game,
  runs,
  events,
  analyses,
  initialPly = 0,
  preferredRunId = "",
  onClose,
  onPositionChange,
}: Props) {
  const session = useMemo(() => new BoardSession(), [game.id]);
  const [, render] = useReducer((x) => x + 1, 0);
  const [error, setError] = useState("");
  const [showAlternative, setShowAlternative] = useState(false);
  const [runId, setRunId] = useState(preferredRunId || runs[0]?.id || "");
  useLayoutEffect(() => {
    try {
      session.loadReplay(game.replay.nodes);
      setRunId(preferredRunId || runs[0]?.id || "");
      session.orientation = game.own_color === "black" ? "black" : "white";
      session.seek(initialPly);
      setError("");
      render();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [game, initialPly, session, preferredRunId, runs]);
  const ply = session.cursor,
    run = runs.find((x) => x.id === runId),
    evaluation = run?.evaluations.find((x) => x.ply === ply),
    event = run
      ? events.find((x) => x.ply === ply + 1 && x.run_id === run.id)
      : undefined,
    node = game.replay.nodes.find((x) => x.ply === ply);
  const analysis = run ? analyses[run.id] : undefined;
  const state = analysis?.trajectory.find((p) => p.ply === ply),
    nextDecision = analysis?.trajectory.find((p) => p.ply === ply + 1);
  function seek(value: number) {
    session.seek(value);
    setShowAlternative(false);
    render();
    onPositionChange?.(session.cursor, run?.id);
  }
  return (
    <section className="stats-game-view" aria-label="Просмотр партии">
      <header className="stats-view-header">
        <h2>
          {game.white.name} — {game.black.name}
        </h2>
        <span>{game.result}</span>
        <button onClick={onClose}>Вернуться</button>
      </header>
      <div
        className="stats-board-layout"
        onKeyDown={(e) => {
          if ((e.target as HTMLElement).matches("input,select,textarea"))
            return;
          if (e.key === "ArrowLeft") {
            e.preventDefault();
            seek(ply - 1);
          }
          if (e.key === "ArrowRight") {
            e.preventDefault();
            seek(ply + 1);
          }
          if (e.key === "Home") {
            e.preventDefault();
            seek(0);
          }
          if (e.key === "End") {
            e.preventDefault();
            seek(session.total);
          }
        }}
        tabIndex={0}
      >
        <div>
          <ChessBoard
            session={session}
            revision={session.revision}
            onChange={render}
            readOnly
          />
          <div className="board-controls">
            <button aria-label="В начало партии" onClick={() => seek(0)}>
              ⏮
            </button>
            <button
              aria-label="Предыдущий ход"
              onClick={() => seek(ply - 1)}
              disabled={!ply}
            >
              ◀
            </button>
            <button
              aria-label="Следующий ход"
              onClick={() => seek(ply + 1)}
              disabled={ply >= session.total}
            >
              ▶
            </button>
            <button
              aria-label="В конец партии"
              onClick={() => seek(session.total)}
            >
              ⏭
            </button>
            <button
              onClick={() => {
                session.flip();
                render();
              }}
            >
              Перевернуть
            </button>
          </div>
        </div>
        <div className="stats-game-panel">
          <div>
            {timestamp(game.started_at_ms)} · {groupLabel(game.speed)} ·{" "}
            {groupLabel(game.status)}
          </div>
          <div>{game.replay.opening?.name ?? "—"}</div>
          <div className="stats-movelist" aria-label="Ходы партии">
            {game.replay.nodes
              .filter((n) => n.ply > 0)
              .map((n) => (
                <button
                  key={n.ply}
                  className={n.ply === ply ? "selected" : ""}
                  onClick={() => seek(n.ply)}
                  aria-current={n.ply === ply ? "step" : undefined}
                >
                  {n.actor === "white"
                    ? Math.ceil(n.absolute_ply / 2) + ". "
                    : ""}
                  {n.san}
                </button>
              ))}
          </div>
          {runs.length > 0 && (
            <label>
              Анализ
              <select
                aria-label="Анализ партии"
                value={run?.id ?? ""}
                onChange={(e) => {
                  setRunId(e.target.value);
                  setShowAlternative(false);
                  onPositionChange?.(ply, e.target.value);
                }}
              >
                {runId === "__missing__" && (
                  <option value="__missing__">
                    Нет анализа выбранного профиля
                  </option>
                )}
                {runs.map((r) => (
                  <option value={r.id} key={r.id}>
                    {r.engine_version} ·{" "}
                    {r.profile === "quick"
                      ? "Быстрый"
                      : r.profile === "deep"
                        ? "Углублённый"
                        : "Стандартный"}{" "}
                  </option>
                ))}
              </select>
            </label>
          )}
          {analysis && (
            <EvaluationChart
              points={analysis.trajectory}
              ply={ply}
              onSelect={seek}
            />
          )}
          {event && (
            <section className="practice-moment" aria-label="Выбранный момент">
              <h3>
                Перед ходом {session.position.fullmoves}
                {event.actor === "black" ? "…" : "."} {event.san}
              </h3>
              <p>
                Сыграно {event.san}
                {event.loss_q != null
                  ? " · потеря по оценке движка " +
                    number(event.loss_q, 1) +
                    " п.п."
                  : " · недостаточно пригодных оценок"}
              </p>
              {event.best_move && (
                <button
                  onClick={() => setShowAlternative(!showAlternative)}
                  aria-expanded={showAlternative}
                >
                  {showAlternative
                    ? "Скрыть альтернативу"
                    : "Показать проверенную альтернативу"}
                </button>
              )}
              {showAlternative && (
                <p className="practice-alternative">
                  {(() => {
                    const move = parseUci(event.best_move ?? "");
                    if (!move) return event.best_move;
                    const normalized = normalizeMove(session.position, move);
                    return session.position.isLegal(normalized)
                      ? makeSan(session.position, normalized)
                      : event.best_move;
                  })()}
                </p>
              )}
            </section>
          )}
          {!run && (
            <p className="practice-context">
              Для этого профиля ещё нет анализа. Партия и нотация доступны.
            </p>
          )}
          <details className="stats-game-technical">
            <summary>Подробности позиции и анализа</summary>
            <dl>
              <dt>Ход</dt>
              <dd>{ply}</dd>
              <dt>Фаза</dt>
              <dd>{groupLabel(node?.phase ?? "—")}</dd>
              <dt>Часы, с</dt>
              <dd>{number(node?.clock_seconds)}</dd>
              <dt>Затрачено, с</dt>
              <dd>{number(node?.spent_seconds)}</dd>
              <dt>Материал</dt>
              <dd>{number(node?.material.balance, 0)}</dd>
            </dl>
            <dl>
              <dt>
                q модели за {game.own_color === "white" ? "белых" : "чёрных"}, %
              </dt>
              <dd>{state?.q == null ? "—" : number(state.q * 100, 2)}</dd>
              <dt>Accuracy следующего решения, %</dt>
              <dd>{number(nextDecision?.accuracy, 2)}</dd>
              <dt>Класс Lichess Advice</dt>
              <dd>
                {nextDecision?.advice ? groupLabel(nextDecision.advice) : "—"}
              </dd>
              <dt>Оценка за белых</dt>
              <dd>
                {evaluation?.mate != null
                  ? "#" + evaluation.mate
                  : evaluation?.cp != null
                    ? number(evaluation.cp / 100, 2)
                    : "нет оценок"}
              </dd>
              <dt>Глубина</dt>
              <dd>{evaluation?.depth ?? "—"}</dd>
              <dt>W / D / L, ‰</dt>
              <dd>{evaluation?.wdl?.join(" / ") ?? "—"}</dd>
              <dt>Следующее решение</dt>
              <dd>{event?.san ?? "—"}</dd>
              <dt>Потеря после решения, п.п.</dt>
              <dd>{number(event?.loss_q, 2)}</dd>
              <dt>Лучший ход</dt>
              <dd>{event?.best_move ?? evaluation?.best_move ?? "—"}</dd>
            </dl>
            {evaluation?.pv.length ? (
              <div className="stats-pv" aria-label="Вариант движка">
                {evaluation.pv.join(" ")}
              </div>
            ) : null}
          </details>
          {node?.comment && <p>{node.comment}</p>}
          <details>
            <summary>PGN</summary>
            <textarea aria-label="Исходный PGN" readOnly value={game.raw_pgn} />
            <button
              onClick={() =>
                void navigator.clipboard
                  .writeText(game.raw_pgn)
                  .catch((e) =>
                    setError(e instanceof Error ? e.message : String(e)),
                  )
              }
            >
              Копировать PGN
            </button>
          </details>
          <details>
            <summary>FEN</summary>
            <textarea
              aria-label="FEN выбранной позиции"
              readOnly
              value={node?.fen ?? session.fen}
            />
          </details>
          {error && (
            <div className="error" role="alert">
              {error}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}

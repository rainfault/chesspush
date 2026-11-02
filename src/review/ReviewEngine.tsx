import { useEffect, useMemo, useReducer, useState } from "react";
import { Chess, normalizeMove } from "chessops/chess";
import { parseFen } from "chessops/fen";
import { parseUci } from "chessops/util";
import { makeSan } from "chessops/san";
import type { DocumentNavigation, KnowledgeDocument } from "../knowledge/types";
import { errorMessage } from "../api";
import {
  cancelReviewEvaluation,
  evaluateReview,
  type ReviewEvaluation,
} from "./api";

function line(fen: string, moves: string[]) {
  const position = Chess.fromSetup(parseFen(fen).unwrap()).unwrap();
  const result: { san: string; uci: string }[] = [];
  for (const uci of moves.slice(0, 18)) {
    const parsed = parseUci(uci);
    if (!parsed) break;
    const move = normalizeMove(position, parsed);
    if (!position.isLegal(move)) break;
    result.push({
      uci,
      san: `${position.turn === "white" ? `${position.fullmoves}. ` : !result.length ? `${position.fullmoves}… ` : ""}${makeSan(position, move)}`,
    });
    position.play(move);
  }
  return result;
}
export function ReviewEngine({
  document,
  navigation,
  active,
  onSettings,
  onPlay,
  busy,
  profileKey,
}: {
  document: KnowledgeDocument;
  navigation: DocumentNavigation | null;
  active: boolean;
  onSettings: () => void;
  onPlay?: (uci: string) => void;
  busy: boolean;
  profileKey?: string;
}) {
  const [enabled, setEnabled] = useState(false);
  const [generation, retry] = useReducer((n) => n + 1, 0);
  const contextKey = JSON.stringify([
    document.id,
    document.revision,
    navigation?.chapter_id,
    navigation?.node_id,
    profileKey,
  ]);
  const [resultState, setResult] = useState<{
    key: string;
    value: ReviewEvaluation;
  } | null>(null);
  const result =
    enabled && active && navigation && resultState?.key === contextKey
      ? resultState.value
      : null;
  const [pending, setPending] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    setResult(null);
    setError("");
    if (!enabled || !active || !navigation) {
      setPending(false);
      return;
    }
    let disposed = false,
      started = false;
    const request = crypto.randomUUID();
    setPending(true);
    const timer = setTimeout(() => {
      started = true;
      void evaluateReview(
        document.id,
        document.revision,
        navigation.chapter_id,
        navigation.node_id,
        request,
      )
        .then((value) => {
          if (!disposed) setResult({ key: contextKey, value });
        })
        .catch((reason) => {
          if (!disposed) setError(errorMessage(reason));
        })
        .finally(() => {
          if (!disposed) setPending(false);
        });
    }, 220);
    return () => {
      disposed = true;
      clearTimeout(timer);
      if (started) void cancelReviewEvaluation(request).catch(() => {});
    };
  }, [
    active,
    enabled,
    document.id,
    document.revision,
    navigation?.chapter_id,
    navigation?.node_id,
    generation,
    profileKey,
  ]);
  const variation = useMemo(() => {
    if (!result || !navigation) return [];
    try {
      return line(navigation.fen, result.evaluation.pv);
    } catch {
      return [];
    }
  }, [result, navigation]);
  const evaluation = result?.evaluation;
  const score =
    evaluation?.mate != null
      ? `#${evaluation.mate}`
      : evaluation?.cp != null
        ? `${evaluation.cp >= 0 ? "+" : ""}${(evaluation.cp / 100).toLocaleString("ru-RU", { minimumFractionDigits: 1, maximumFractionDigits: 2 })}`
        : "—";
  return (
    <section className="review-engine" aria-label="Анализ позиции">
      <header>
        <label>
          <input
            type="checkbox"
            checked={enabled}
            onChange={(event) => setEnabled(event.target.checked)}
          />{" "}
          Stockfish
        </label>
        {evaluation && (
          <strong className="review-evaluation">
            {evaluation.bound === "lower"
              ? "≥"
              : evaluation.bound === "upper"
                ? "≤"
                : ""}
            {score}
          </strong>
        )}
        <button
          onClick={onSettings}
          aria-label="Настройки движка"
          title="Настройки движка"
        >
          <svg
            viewBox="0 0 24 24"
            width="20"
            height="20"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
            aria-hidden="true"
          >
            <path d="m9 3 1-2h4l1 2 3 2 2 1-1 3v5l1 3-2 1-3 2-1 2h-4l-1-2-3-2-2-1 1-3V9L4 6l2-1z" />
            <circle cx="12" cy="12" r="4" />
          </svg>
        </button>
      </header>
      {enabled && (
        <>
          <div role="status" className="review-engine-state">
            {pending
              ? "Анализируется…"
              : result
                ? `${result.engine.name} · Глубина ${evaluation!.depth} · ${evaluation!.nodes.toLocaleString("ru-RU")} узлов`
                : ""}
          </div>
          {error && (
            <div role="alert" className="review-engine-error">
              <span>{error}</span>
              <button onClick={retry}>Повторить</button>
            </div>
          )}
          {variation.length > 0 && (
            <div className="review-pv" aria-label="Вариант движка">
              {variation.map((move, index) => (
                <span key={`${index}:${move.uci}`}>{move.san} </span>
              ))}
            </div>
          )}
          {variation[0] && onPlay && (
            <button
              className="review-play-engine"
              disabled={busy || pending || !navigation}
              onClick={() => onPlay(variation[0].uci)}
            >
              Сыграть {variation[0].san}
            </button>
          )}
        </>
      )}
    </section>
  );
}

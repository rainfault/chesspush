import { useEffect, useMemo, useRef, useState } from "react";
import type { Role, SquareName } from "chessops/types";
import type { DrawShape } from "@lichess-org/chessground/draw";
import { ChessBoard } from "../chess/ChessBoard";
import { BoardSession } from "../chess/BoardSession";
import { uciInput } from "../chess/boardInput";
import { errorMessage } from "../api";
import {
  indexChapter,
  moveLabel,
  repertoireChapters,
  repertoireChapterKey,
} from "./navigation";
import { loadDocument } from "./api";
import { RepertoireChapterSelect } from "./RepertoireChapterSelect";
import type { KnowledgeDocument } from "./types";
import {
  answerTraining,
  hintTraining,
  nextTraining,
  startTraining,
  trainingOverview,
  type TrainingMode,
  type TrainingOverview,
  type TrainingPrompt,
  type TrainingScope,
  type TrainingSession,
} from "./trainingApi";
import "./repertoireTraining.css";

interface Props {
  document: KnowledgeDocument;
  initialChapterId?: string;
  initialNodeId?: string;
  initialSourceDocumentId?: string;
  active: boolean;
  onClose: () => void;
}

const modeNames = {
  srs: "Освоение и повторение",
  full_drill: "Полный дрилл",
};

function savedSelection(
  document: KnowledgeDocument,
  chapterId?: string,
  nodeId?: string,
  sourceDocumentId?: string,
) {
  let saved: {
    chapter_id?: string;
    node_id?: string;
    source_document_id?: string;
    mode?: TrainingMode;
    new_limit?: number;
  } = {};
  try {
    saved = JSON.parse(
      localStorage.getItem(`chesspush.training.${document.side}`) ?? "{}",
    );
  } catch {
    /* Optional navigation preference. */
  }
  const mode =
    saved?.mode === "full_drill" ? ("full_drill" as const) : ("srs" as const);
  const newLimit = [0, 5, 10].includes(saved?.new_limit ?? -1)
    ? saved.new_limit!
    : 5;
  const sourceId =
    sourceDocumentId ?? (chapterId ? "" : saved?.source_document_id) ?? "";
  if (sourceId) {
    const source = repertoireChapters(document).find(
      (item) =>
        item.document_id === sourceId &&
        item.chapter_id === (chapterId ?? saved?.chapter_id),
    );
    if (source) {
      const selectedNode = nodeId ?? saved?.node_id ?? "";
      return {
        sourceDocumentId: source.document_id,
        chapterId: source.chapter_id,
        nodeId: selectedNode === source.root_node_id ? "" : selectedNode,
        mode,
        newLimit,
      };
    }
  }
  const chapter = document.chapters.find(
    (item) => item.id === (chapterId ?? saved?.chapter_id),
  );
  const selectedNode = nodeId ?? (chapterId ? "" : saved?.node_id) ?? "";
  return {
    sourceDocumentId: "",
    chapterId: chapter?.id ?? "",
    nodeId:
      chapter &&
      selectedNode !== chapter.root.id &&
      indexChapter(chapter).has(selectedNode)
        ? selectedNode
        : "",
    mode,
    newLimit,
  };
}

// A presentation adapter over the native context, using the existing board model.
function frames(prompt: TrainingPrompt) {
  return [{ fen: prompt.initial_fen, uci: null }, ...prompt.context];
}
function playUci(board: BoardSession, uci: string) {
  const roles: Record<string, Role> = {
    q: "queen",
    r: "rook",
    b: "bishop",
    n: "knight",
  };
  board.play(
    uci.slice(0, 2) as SquareName,
    uci.slice(2, 4) as SquareName,
    roles[uci[4]],
  );
}

export function RepertoireTraining({
  document,
  initialChapterId,
  initialNodeId,
  initialSourceDocumentId,
  active,
  onClose,
}: Props) {
  const [initial] = useState(() =>
    savedSelection(
      document,
      initialChapterId,
      initialNodeId,
      initialSourceDocumentId,
    ),
  );
  const [sourceDocumentId, setSourceDocumentId] = useState(
    initial.sourceDocumentId,
  );
  const [sourceDocument, setSourceDocument] =
    useState<KnowledgeDocument | null>(null);
  const [sourceLoading, setSourceLoading] = useState(false);
  const [sourceError, setSourceError] = useState("");
  const [sourceRetry, setSourceRetry] = useState(0);
  const [chapterId, setChapterId] = useState(initial.chapterId);
  const [nodeId, setNodeId] = useState(initial.nodeId);
  const [mode, setMode] = useState<TrainingMode>(initial.mode);
  const [newLimit, setNewLimit] = useState(initial.newLimit);
  const [overview, setOverview] = useState<TrainingOverview>();
  const [session, setSession] = useState<TrainingSession | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [animating, setAnimating] = useState(false);
  const [commentsOpen, setCommentsOpen] = useState(false);
  const [board] = useState(() => {
    const value = new BoardSession();
    value.orientation = document.side === "black" ? "black" : "white";
    return value;
  });
  const [revision, setRevision] = useState(board.revision);
  const request = useRef(0);
  const inFlight = useRef(false);
  const sourceGeneration = useRef(0);
  const previousPrompt = useRef("");
  const mounted = useRef(true);
  const catalog = useMemo(() => repertoireChapters(document), [document]);
  const sourceChoice = catalog.find(
    (item) =>
      item.document_id === sourceDocumentId && item.chapter_id === chapterId,
  );
  const selectedDocument = sourceDocumentId ? sourceDocument : document;
  const chapter = selectedDocument?.chapters.find(
    (item) => item.id === chapterId,
  );
  useEffect(() => {
    let disposed = false;
    setSourceError("");
    setSourceDocument(null);
    if (!sourceDocumentId) {
      setSourceLoading(false);
      return;
    }
    if (!sourceChoice) {
      setSourceDocumentId("");
      setChapterId("");
      setNodeId("");
      setSourceLoading(false);
      return;
    }
    setSourceLoading(true);
    void loadDocument(sourceDocumentId)
      .then((value) => {
        if (disposed) return;
        const loadedChapter = value.chapters.find(
          (item) => item.id === chapterId,
        );
        if (!loadedChapter)
          throw new Error("Глава изменена или удалена. Выберите её заново.");
        setSourceDocument(value);
        if (nodeId && !indexChapter(loadedChapter).has(nodeId)) setNodeId("");
      })
      .catch((reason) => {
        if (!disposed) setSourceError(errorMessage(reason));
      })
      .finally(() => {
        if (!disposed) setSourceLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [sourceDocumentId, chapterId, document, sourceRetry]);
  const chapterIndex = useMemo(
    () => chapter && indexChapter(chapter),
    [chapter],
  );
  const branches = useMemo(() => {
    if (!chapter || !chapterIndex) return [];
    return [...chapterIndex.values()]
      .filter(
        (entry) =>
          entry.node.id !== chapter.root.id &&
          (entry.node.id === nodeId ||
            (entry.node.children.length &&
              entry.parentId &&
              chapterIndex.get(entry.parentId)!.node.children.length > 1)),
      )
      .map((entry) => ({
        id: entry.node.id,
        label: [...entry.pathIds, entry.node.id]
          .map((id) => chapterIndex.get(id)!.node)
          .filter((node) => node.uci)
          .map(moveLabel)
          .join(" "),
      }));
  }, [chapterIndex, chapter, nodeId]);
  const scope = useMemo<TrainingScope>(
    () => ({
      side: document.side === "black" ? "black" : "white",
      ...(chapterId ? { chapter_id: chapterId } : {}),
      ...(chapterId && nodeId ? { node_id: nodeId } : {}),
      ...(sourceDocumentId ? { source_document_id: sourceDocumentId } : {}),
    }),
    [document.side, chapterId, nodeId, sourceDocumentId],
  );
  useEffect(() => {
    try {
      localStorage.setItem(
        `chesspush.training.${document.side}`,
        JSON.stringify({ ...scope, mode, new_limit: newLimit }),
      );
    } catch {
      /* SQLite owns training progress; this is only the selected scope. */
    }
  }, [scope, mode, newLimit, document.side]);

  async function refreshOverview() {
    const token = ++request.current;
    setLoading(true);
    try {
      const value = await trainingOverview(scope);
      if (mounted.current && token === request.current) {
        setOverview(value);
        setError("");
      }
    } catch (reason) {
      if (mounted.current && token === request.current)
        setError(errorMessage(reason));
    } finally {
      if (mounted.current && token === request.current) setLoading(false);
    }
  }
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      request.current++;
    };
  }, []);
  useEffect(() => {
    if (active) void refreshOverview();
    return () => {
      request.current++;
    };
  }, [scope, active, document]);
  useEffect(() => {
    // Native start/answer reconcile the session with the current PGN. Leave setup
    // open after a source edit so no stale answer is silently graded.
    sourceGeneration.current++;
    setSession(null);
  }, [document]);

  const prompt = session?.prompt;
  const settled =
    session?.feedback === "correct" || session?.feedback === "assisted";
  const answering =
    session?.feedback === "awaiting" || session?.feedback === "retry";
  const introducing = session?.task === "learn";
  const materialVisible =
    introducing ||
    session?.feedback === "retry" ||
    settled ||
    session?.mode === "full_drill";
  const promptIdentity =
    session && prompt
      ? `${session.id}:${prompt.decision_id}:${session.step ?? 0}`
      : "";
  const currentVisit = useRef("");
  currentVisit.current =
    active && session
      ? `${session.id}:${session.step ?? 0}:${session.feedback}`
      : "";
  useEffect(() => {
    setCommentsOpen(introducing || session?.feedback === "retry");
  }, [promptIdentity, introducing, session?.feedback]);
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | undefined;
    if (!active) {
      setAnimating(false);
      return;
    }
    if (!prompt || !session) {
      const selected =
        chapterIndex?.get(nodeId) ??
        (chapter && chapterIndex?.get(chapter.root.id));
      if (selected && chapterIndex)
        board.loadReplay(
          [...selected.pathIds, selected.node.id].map(
            (id) => chapterIndex.get(id)!.node,
          ),
        );
      else board.loadFen(document.chapters[0]?.initial_fen ?? board.fen);
      previousPrompt.current = "";
      setRevision(board.revision);
      setAnimating(false);
      return;
    }
    const replay = frames(prompt);
    const identity = promptIdentity;
    const finish = () => {
      board.loadReplay(replay);
      if (settled && prompt.attempted_uci) playUci(board, prompt.attempted_uci);
      setRevision(board.revision);
      setAnimating(false);
    };
    if (
      identity === previousPrompt.current ||
      window.matchMedia("(prefers-reduced-motion: reduce)").matches
    ) {
      previousPrompt.current = identity;
      finish();
      return;
    }
    previousPrompt.current = identity;
    // Restore the native context in one short opponent move, without replaying
    // already learned opening moves before every retrieval attempt.
    let cursor = Math.max(0, replay.length - 2);
    if (replay.at(-1)?.fen === board.fen) cursor = replay.length - 1;
    board.loadReplay(replay.slice(0, cursor + 1));
    setRevision(board.revision);
    setAnimating(cursor < replay.length - 1);
    if (cursor < replay.length - 1) timer = setTimeout(finish, 220);
    else finish();
    return () => clearTimeout(timer);
  }, [session, active, chapterIndex, nodeId, document, board, promptIdentity]);

  async function run(
    action: () => Promise<TrainingSession>,
    expectedGeneration = sourceGeneration.current,
  ) {
    if (
      inFlight.current ||
      !mounted.current ||
      expectedGeneration !== sourceGeneration.current
    )
      return;
    inFlight.current = true;
    const generation = expectedGeneration;
    setBusy(true);
    setError("");
    try {
      const value = await action();
      if (mounted.current && generation === sourceGeneration.current)
        setSession(value);
    } catch (reason) {
      if (mounted.current && generation === sourceGeneration.current) {
        setError(errorMessage(reason));
        if (prompt) {
          board.loadReplay(frames(prompt));
          if (settled && prompt.attempted_uci)
            playUci(board, prompt.attempted_uci);
          setRevision(board.revision);
        }
      }
    } finally {
      inFlight.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  function move(from: SquareName, to: SquareName, promotion?: Role) {
    if (!active || !session || !answering || inFlight.current || animating)
      return;
    const uci = uciInput(board, from, to, promotion);
    board.play(from, to, promotion);
    setRevision(board.revision);
    void run(() => answerTraining(session.id, uci));
  }
  function setup() {
    sourceGeneration.current++;
    setSession(null);
    void refreshOverview();
  }
  function close() {
    sourceGeneration.current++;
    onClose();
  }
  useEffect(() => {
    // Keep annotated positions available for reading. Ordinary continuations
    // advance automatically after the brief result, including opponent replies.
    if (
      !session ||
      !settled ||
      busy ||
      error ||
      animating ||
      !active ||
      (materialVisible && prompt?.comments.length)
    )
      return;
    const generation = sourceGeneration.current;
    const visit = currentVisit.current;
    const timer = setTimeout(() => {
      if (currentVisit.current === visit)
        void run(() => nextTraining(session.id), generation);
    }, 450);
    return () => clearTimeout(timer);
  }, [session, busy, animating, active, error]);
  useEffect(() => {
    if (!active || !session || !settled || busy || animating || error) return;
    const generation = sourceGeneration.current;
    const visit = currentVisit.current;
    const advance = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (
        event.code !== "Space" ||
        event.repeat ||
        event.altKey ||
        event.ctrlKey ||
        event.metaKey ||
        event.shiftKey ||
        target?.closest(
          "input, textarea, select, button, summary, [contenteditable], [role=dialog], dialog",
        )
      )
        return;
      if (
        currentVisit.current !== visit ||
        generation !== sourceGeneration.current
      )
        return;
      event.preventDefault();
      void run(() => nextTraining(session.id), generation);
    };
    window.addEventListener("keydown", advance);
    return () => window.removeEventListener("keydown", advance);
  }, [active, session, settled, busy, animating, error]);
  const continuing =
    mode === "srs" ? overview?.active_srs : overview?.active_full_drill;
  const finished = session?.finished && !prompt;
  useEffect(() => {
    if (finished && active) void refreshOverview();
  }, [finished, active, scope]);
  useEffect(() => {
    const dueAt = overview?.next_due_at_ms;
    if (!active || (session && !finished) || !dueAt) return;
    const generation = sourceGeneration.current;
    const visit = currentVisit.current;
    const timer = setTimeout(
      () => {
        if (
          mounted.current &&
          generation === sourceGeneration.current &&
          currentVisit.current === visit
        )
          void refreshOverview();
      },
      Math.min(2_147_483_647, Math.max(100, dueAt - Date.now() + 50)),
    );
    return () => clearTimeout(timer);
  }, [
    active,
    finished,
    session?.id,
    overview?.next_due_at_ms,
    scope,
    document,
  ]);
  const newPositions = overview?.new_positions ?? 0;
  const available =
    !!overview && (overview.due > 0 || (newLimit > 0 && newPositions > 0));
  const nextReviewAt = session?.next_review_at_ms ?? overview?.next_due_at_ms;
  const reviewDate = nextReviewAt
    ? new Date(nextReviewAt).toLocaleString("ru-RU", {
        day: "numeric",
        month: "long",
        hour: "2-digit",
        minute: "2-digit",
      })
    : null;
  const arrows: DrawShape[] =
    session?.feedback === "retry" || (introducing && answering)
      ? (prompt?.accepted_moves ?? []).map((move) => ({
          brush: "green",
          orig: move.uci.slice(0, 2) as SquareName,
          dest: move.uci.slice(2, 4) as SquareName,
        }))
      : [];

  return (
    <main className="repertoire-training" aria-label="Тренировка репертуара">
      <header>
        <button disabled={busy} onClick={close}>
          К репертуару
        </button>
        <h1>{sourceChoice?.title ?? "Тренировка репертуара"}</h1>
        <span>{scope.side === "white" ? "За белых" : "За чёрных"}</span>
      </header>
      <div className="training-layout">
        <section className="training-board" aria-label="Доска тренировки">
          <ChessBoard
            session={board}
            revision={revision}
            onChange={() => setRevision(board.revision)}
            onMove={move}
            allowedMoves={prompt?.legal_moves.map((move) => move.uci)}
            readOnly={!active || !answering || busy || animating}
            autoShapes={arrows}
          />
          <div className="training-board-controls">
            <button
              aria-label="Перевернуть доску тренировки"
              onClick={() => {
                board.flip();
                setRevision(board.revision);
              }}
            >
              <svg
                viewBox="0 0 24 24"
                width="22"
                height="22"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.6"
                aria-hidden="true"
              >
                <path d="M8 3v18m-4-4 4 4 4-4M16 21V3m-4 4 4-4 4 4" />
              </svg>
            </button>
          </div>
        </section>
        <section
          className="training-panel"
          aria-label="Управление тренировкой"
          aria-busy={busy || loading || sourceLoading}
        >
          {(error || sourceError) && (
            <div className="error training-error" role="alert">
              {error || sourceError}
              <button
                disabled={busy}
                onClick={() => {
                  if (sourceError) setSourceRetry((value) => value + 1);
                  if (session) setup();
                  else void refreshOverview();
                }}
              >
                Обновить
              </button>
            </div>
          )}
          {!session ? (
            <>
              <div
                className="training-modes"
                role="group"
                aria-label="Режим тренировки"
              >
                {(["srs", "full_drill"] as const).map((value) => (
                  <button
                    key={value}
                    aria-pressed={mode === value}
                    onClick={() => setMode(value)}
                  >
                    {modeNames[value]}
                  </button>
                ))}
              </div>
              <fieldset
                className="training-selection"
                disabled={busy || sourceLoading}
              >
                <RepertoireChapterSelect
                  chapters={catalog}
                  value={sourceChoice ? repertoireChapterKey(sourceChoice) : ""}
                  onChange={(value) => {
                    const choice = catalog.find(
                      (item) => repertoireChapterKey(item) === value,
                    );
                    setSourceDocumentId(choice?.document_id ?? "");
                    setChapterId(choice?.chapter_id ?? "");
                    setNodeId("");
                  }}
                />
                {!sourceDocumentId && (
                  <label>
                    Начальная позиция
                    <select
                      value={chapterId}
                      onChange={(event) => {
                        setChapterId(event.target.value);
                        setNodeId("");
                      }}
                    >
                      <option value="">Весь репертуар</option>
                      {document.chapters.map((item) => (
                        <option key={item.id} value={item.id}>
                          {item.title}
                        </option>
                      ))}
                    </select>
                  </label>
                )}
                {chapter && (
                  <label>
                    Ветка
                    <select
                      value={nodeId}
                      onChange={(event) => setNodeId(event.target.value)}
                    >
                      <option value="">Все варианты</option>
                      {branches.map((item) => (
                        <option key={item.id} value={item.id}>
                          {item.label}
                        </option>
                      ))}
                    </select>
                  </label>
                )}
                {mode === "srs" && (
                  <label className="training-new-limit">
                    Новых за сессию
                    <select
                      aria-label="Новых за сессию"
                      value={newLimit}
                      onChange={(event) =>
                        setNewLimit(Number(event.target.value))
                      }
                    >
                      <option value={0}>0</option>
                      <option value={5}>5</option>
                      <option value={10}>10</option>
                    </select>
                  </label>
                )}
              </fieldset>
              {loading || sourceLoading ? (
                <p role="status">Загрузка позиций…</p>
              ) : (
                overview && (
                  <div className="training-start">
                    <dl className="training-counts">
                      {mode === "srs" ? (
                        <>
                          <div>
                            <dt>К повторению</dt>
                            <dd>{overview.due}</dd>
                          </div>
                          {overview.new_positions !== undefined && (
                            <div>
                              <dt>Новых</dt>
                              <dd>{newPositions}</dd>
                            </div>
                          )}
                          {overview.learning !== undefined && (
                            <div>
                              <dt>В освоении</dt>
                              <dd>{overview.learning}</dd>
                            </div>
                          )}
                          {overview.scheduled !== undefined && (
                            <div>
                              <dt>По расписанию</dt>
                              <dd>{overview.scheduled}</dd>
                            </div>
                          )}
                        </>
                      ) : (
                        <div>
                          <dt>Позиций</dt>
                          <dd>{overview.total}</dd>
                        </div>
                      )}
                    </dl>
                    {continuing && (
                      <p>
                        Пройдено: {continuing.completed} / {continuing.total}
                      </p>
                    )}
                    <button
                      className="primary"
                      disabled={
                        busy ||
                        !!error ||
                        !!sourceError ||
                        !overview.total ||
                        (mode === "srs" && !available && !continuing)
                      }
                      onClick={() =>
                        void run(() =>
                          startTraining(scope, mode, false, newLimit),
                        )
                      }
                    >
                      {busy
                        ? "Загрузка…"
                        : continuing
                          ? "Продолжить"
                          : "Начать"}
                    </button>
                    {!overview.total && (
                      <p role="status">В этой ветке нет ходов для тренировки</p>
                    )}
                    {mode === "srs" &&
                      !available &&
                      !continuing &&
                      overview.total > 0 && <p role="status">На сегодня всё</p>}
                    {mode === "srs" && reviewDate && (
                      <p className="training-review-date">
                        Следующее повторение: {reviewDate}
                      </p>
                    )}
                  </div>
                )
              )}
            </>
          ) : (
            <>
              <div className="training-session-heading">
                <h2>{modeNames[session.mode]}</h2>
                <button disabled={busy} onClick={setup}>
                  Выбрать режим
                </button>
              </div>
              <div className="training-progress">
                <span>
                  Пройдено: {session.completed} / {session.total}
                </span>
                <span>Ошибок: {session.mistakes}</span>
              </div>
              <progress
                aria-label={
                  session.mode === "full_drill"
                    ? "Покрытие репертуара"
                    : "Прогресс повторения"
                }
                value={session.completed}
                max={Math.max(1, session.total)}
              />
              {session.mode === "srs" && (
                <div className="training-session-counts">
                  {session.new_count !== undefined && (
                    <span>Новых: {session.new_count}</span>
                  )}
                  {session.review_count !== undefined && (
                    <span>Повторение: {session.review_count}</span>
                  )}
                  {(session.pending_retests ?? 0) > 0 && (
                    <span>Проверок впереди: {session.pending_retests}</span>
                  )}
                </div>
              )}
              {session.task && !finished && (
                <div className="training-task">
                  {
                    {
                      learn: "Новая позиция",
                      review: "Повторение",
                      recall: "Проверка",
                      relearn: "Повтор ошибки",
                    }[session.task]
                  }
                </div>
              )}
              <div
                className={`training-feedback ${session.feedback}`}
                role="status"
                aria-live="polite"
              >
                {finished ? (
                  <h2>
                    {session.mode === "srs"
                      ? "Сессия завершена"
                      : "Полный дрилл завершён"}
                  </h2>
                ) : animating ? (
                  <p>Восстановление позиции…</p>
                ) : session.feedback === "correct" ? (
                  <p>Верно</p>
                ) : session.feedback === "assisted" ? (
                  <p>
                    {introducing ? "Ход разобран" : "Ход повторён с подсказкой"}
                  </p>
                ) : session.feedback === "retry" ? (
                  <>
                    <p>
                      Подготовлено:{" "}
                      {prompt?.accepted_moves
                        .map((move) => move.san)
                        .join(" / ")}
                    </p>
                    <p>Повторите ход на доске</p>
                  </>
                ) : introducing ? (
                  <>
                    <p>Запомните ход</p>
                    <div className="training-prepared-moves">
                      {prompt?.accepted_moves
                        .map((move) => move.san)
                        .join(" / ")}
                    </div>
                  </>
                ) : (
                  <p>
                    {session.task === "recall" || session.task === "relearn"
                      ? "Повторите без подсказки"
                      : scope.side === "white"
                        ? "Ход белых"
                        : "Ход чёрных"}
                  </p>
                )}
              </div>
              {prompt && (
                <div className="training-context" aria-label="Сыгранные ходы">
                  {prompt.context.slice(-6).map((move, i) => (
                    <span key={i}>{moveLabel(move)}</span>
                  ))}
                </div>
              )}
              {finished && session.mode === "srs" && (
                <dl className="training-counts training-results">
                  {session.first_try_correct !== undefined && (
                    <div>
                      <dt>С первого раза</dt>
                      <dd>{session.first_try_correct}</dd>
                    </div>
                  )}
                  {session.recovered !== undefined && (
                    <div>
                      <dt>После повторения</dt>
                      <dd>{session.recovered}</dd>
                    </div>
                  )}
                </dl>
              )}
              <div className="training-actions">
                {answering && !introducing && (
                  <button
                    disabled={busy || animating || session.feedback === "retry"}
                    onClick={() => void run(() => hintTraining(session.id))}
                  >
                    Подсказка
                  </button>
                )}
                {settled && (
                  <button
                    className="primary"
                    disabled={busy || animating}
                    onClick={() => void run(() => nextTraining(session.id))}
                  >
                    Дальше
                  </button>
                )}
                {finished && session.mode === "srs" && available && (
                  <button
                    className="primary"
                    disabled={busy || loading}
                    onClick={() =>
                      void run(() =>
                        startTraining(scope, "srs", true, newLimit),
                      )
                    }
                  >
                    Следующая сессия
                  </button>
                )}
                {finished && (
                  <button
                    className={
                      session.mode === "srs" && available
                        ? undefined
                        : "primary"
                    }
                    disabled={busy}
                    onClick={setup}
                  >
                    К выбору тренировки
                  </button>
                )}
                {finished && session.mode === "full_drill" && (
                  <button
                    disabled={busy}
                    onClick={() =>
                      void run(() => startTraining(scope, session.mode, true))
                    }
                  >
                    Повторить полный дрилл
                  </button>
                )}
              </div>
              {finished && session.mode === "srs" && reviewDate && (
                <p className="training-review-date">
                  Следующее повторение: {reviewDate}
                </p>
              )}
              {materialVisible && !!prompt?.comments.length && (
                <details
                  className="training-comments"
                  open={commentsOpen}
                  onToggle={(event) =>
                    setCommentsOpen(event.currentTarget.open)
                  }
                >
                  <summary>Комментарии PGN</summary>
                  {prompt.comments.map((comment, i) => (
                    <p key={i}>{comment}</p>
                  ))}
                </details>
              )}
            </>
          )}
        </section>
      </div>
    </main>
  );
}

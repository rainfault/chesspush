import { useEffect, useMemo, useState } from "react";
import type { DrawShape } from "@lichess-org/chessground/draw";
import type { Role, SquareName } from "chessops/types";
import { ChessBoard } from "../chess/ChessBoard";
import { BoardSession } from "../chess/BoardSession";
import { uciInput } from "../chess/boardInput";
import {
  indexChapter,
  moveLabel,
  repertoireChapters,
  repertoireChapterKey,
} from "./navigation";
import { loadDocument } from "./api";
import { errorMessage } from "../api";
import { RepertoireChapterSelect } from "./RepertoireChapterSelect";
import type { KnowledgeDocument, VariationNode } from "./types";

interface SourceNode {
  document_id: string;
  chapter_id: string;
  node_id: string;
  title: string;
  filename?: string;
}
interface Props {
  document: KnowledgeDocument;
  onClose: () => void;
  onExport: () => void;
  onOpenSource: (source: SourceNode) => void;
  onTrain: (
    chapterId: string,
    nodeId: string,
    sourceDocumentId?: string,
  ) => void;
}

/** Read-only presentation of the very same native VariationTree as the editor. */
export function CombinedRepertoire({
  document,
  onClose,
  onExport,
  onOpenSource,
  onTrain,
}: Props) {
  const [chapterId, setChapterId] = useState(document.chapters[0]?.id ?? "");
  const catalog = useMemo(() => repertoireChapters(document), [document]);
  const [sourceKey, setSourceKey] = useState("");
  const choice = catalog.find(
    (item) => repertoireChapterKey(item) === sourceKey,
  );
  const [sourceDocument, setSourceDocument] =
    useState<KnowledgeDocument | null>(null);
  const [sourceLoading, setSourceLoading] = useState(false);
  const [sourceError, setSourceError] = useState("");
  const chapter = choice
    ? sourceDocument?.chapters.find((item) => item.id === choice.chapter_id)
    : (document.chapters.find((item) => item.id === chapterId) ??
      document.chapters[0]);
  const [nodeId, setNodeId] = useState(chapter?.root.id ?? "");
  const [board] = useState(() => {
    const session = new BoardSession();
    session.orientation = document.side === "black" ? "black" : "white";
    return session;
  });
  const [revision, setRevision] = useState(0);
  const index = useMemo(() => chapter && indexChapter(chapter), [chapter]);
  const selected =
    index?.get(nodeId) ?? (chapter && index?.get(chapter.root.id));
  const node = selected?.node;
  const sourceMap = document.metadata.repertoire_sources as
    Record<string, SourceNode[]> | undefined;
  const sources: SourceNode[] = node
    ? choice
      ? [
          {
            document_id: choice.document_id,
            chapter_id: choice.chapter_id,
            node_id: node.id,
            title: choice.document_title,
            filename: choice.filename ?? undefined,
          },
        ]
      : (sourceMap?.[node.id] ?? [])
    : [];
  function sourceLabel(source: SourceNode) {
    const chapters = catalog.filter(
      (item) => item.document_id === source.document_id,
    );
    const title = chapters.find(
      (item) => item.chapter_id === source.chapter_id,
    )?.title;
    return `${source.filename ?? source.title}${chapters.length > 1 && title ? ` · ${title}` : ""}`;
  }

  useEffect(() => {
    let disposed = false;
    setSourceError("");
    setSourceDocument(null);
    if (!choice) {
      setSourceLoading(false);
      if (sourceKey) setSourceKey("");
      return;
    }
    setSourceLoading(true);
    void loadDocument(choice.document_id)
      .then((value) => {
        if (disposed) return;
        if (!value.chapters.some((item) => item.id === choice.chapter_id))
          throw new Error("Глава изменена или удалена. Выберите её заново.");
        setSourceDocument(value);
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
  }, [sourceKey, choice?.document_id, choice?.chapter_id, document]);

  useEffect(() => {
    if (!selected || !index) return;
    board.loadReplay(
      [...selected.pathIds, selected.node.id].map((id) => index.get(id)!.node),
    );
    setRevision(board.revision);
  }, [node?.id, chapter?.id, document.updated_at_ms]);

  function move(orig: SquareName, dest: SquareName, promotion?: Role) {
    const uci = uciInput(board, orig, dest, promotion);
    const child = node?.children.find((item) => item.uci === uci);
    if (child) setNodeId(child.id);
    else setRevision((value) => value + 1);
  }
  function previous() {
    if (selected?.parentId) setNodeId(selected.parentId);
  }
  function next() {
    if (node?.children[0]) setNodeId(node.children[0].id);
  }
  return (
    <main
      className="combined-repertoire"
      aria-label={document.title}
      onKeyDown={(event) => {
        if (!chapter) return;
        if ((event.target as HTMLElement).matches("input,textarea,select"))
          return;
        if (event.key === "ArrowLeft") {
          event.preventDefault();
          previous();
        }
        if (event.key === "ArrowRight") {
          event.preventDefault();
          next();
        }
        if (event.key === "Home") {
          event.preventDefault();
          setNodeId(chapter.root.id);
        }
      }}
    >
      <header>
        <button onClick={onClose}>← К файлам</button>
        <h1>{choice?.title ?? document.title}</h1>
        <button
          className="primary"
          disabled={!chapter || !node || sourceLoading}
          onClick={() =>
            chapter && node && onTrain(chapter.id, node.id, choice?.document_id)
          }
        >
          Тренировать ветку
        </button>
        <button onClick={onExport}>
          {choice ? "Экспорт общего PGN" : "Экспорт PGN"}
        </button>
      </header>
      <div className="combined-selection">
        <RepertoireChapterSelect
          chapters={catalog}
          value={sourceKey}
          onChange={(value) => {
            setSourceKey(value);
            setNodeId("");
          }}
        />
        {!choice && chapter && document.chapters.length > 1 && (
          <label className="combined-chapter">
            Начальная позиция{" "}
            <select
              value={chapter.id}
              onChange={(event) => {
                const next = document.chapters.find(
                  (item) => item.id === event.target.value,
                )!;
                setChapterId(next.id);
                setNodeId(next.root.id);
              }}
            >
              {document.chapters.map((item) => (
                <option key={item.id} value={item.id}>
                  {item.title}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>
      {sourceLoading && <p role="status">Загрузка главы…</p>}
      {sourceError && (
        <div className="error" role="alert">
          {sourceError}
          <button onClick={() => setSourceKey("")}>К общему репертуару</button>
        </div>
      )}
      {chapter && node && !sourceLoading && !sourceError && (
        <div className="combined-layout">
          <section className="combined-board" aria-label="Доска репертуара">
            <ChessBoard
              session={board}
              revision={revision}
              onChange={() => setRevision(board.revision)}
              onMove={move}
              allowedMoves={node.children.flatMap((item) =>
                item.uci ? [item.uci] : [],
              )}
              shapes={node.shapes as DrawShape[]}
            />
            <div className="combined-board-controls">
              <button
                aria-label="Начало репертуара"
                disabled={!selected?.parentId}
                onClick={() => setNodeId(chapter.root.id)}
              >
                ⏮
              </button>
              <button
                aria-label="Предыдущий ход репертуара"
                disabled={!selected?.parentId}
                onClick={previous}
              >
                ←
              </button>
              <button
                aria-label="Следующий ход репертуара"
                disabled={!node.children.length}
                onClick={next}
              >
                →
              </button>
              <button
                aria-label="Перевернуть доску репертуара"
                onClick={() => {
                  board.flip();
                  setRevision(board.revision);
                }}
              >
                ↕
              </button>
            </div>
          </section>
          <section
            className="combined-variations"
            aria-label="Варианты общего репертуара"
          >
            <div className="combined-path">
              <button
                aria-current={node.id === chapter.root.id ? "step" : undefined}
                onClick={() => setNodeId(chapter.root.id)}
              >
                Начало
              </button>
              {[...selected!.pathIds, node.id]
                .map((id) => index!.get(id)!.node)
                .filter((item) => item.uci)
                .map((item) => (
                  <button
                    key={item.id}
                    aria-current={item.id === node.id ? "step" : undefined}
                    onClick={() => setNodeId(item.id)}
                  >
                    {moveLabel(item)}
                  </button>
                ))}
            </div>
            <div className="combined-branches">
              {node.children.map((child) => (
                <button key={child.id} onClick={() => setNodeId(child.id)}>
                  <strong>{moveLabel(child)}</strong>
                  <span>{preview(child)}</span>
                </button>
              ))}
              {!node.children.length && (
                <span className="combined-leaf">Конец варианта</span>
              )}
            </div>
            {node.comments.map((comment, i) => (
              <p className="combined-comment" key={i}>
                {comment}
              </p>
            ))}
            {sources.length > 0 && (
              <div className="combined-sources">
                {sources.map((source) => (
                  <button
                    key={`${source.document_id}:${source.node_id}`}
                    onClick={() => onOpenSource(source)}
                    aria-label={`Открыть фрагмент ${sourceLabel(source)}`}
                  >
                    {sourceLabel(source)} ↗
                  </button>
                ))}
              </div>
            )}
          </section>
        </div>
      )}
    </main>
  );
}

function preview(node: VariationNode) {
  const moves: string[] = [];
  let cursor = node.children[0];
  while (cursor && moves.length < 4) {
    moves.push(moveLabel(cursor));
    cursor = cursor.children[0];
  }
  return moves.join(" ");
}

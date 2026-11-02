import { useRef, useState } from "react";
import { useKnowledgeInRepertoire } from "./api";
import type { KnowledgeEditorActions } from "./KnowledgeEditor";
import type { RepertoireTarget } from "./Repertoire";

export interface SourceGameTarget {
  gameId: string;
  accountId?: string;
  revision: number;
  ply: number;
}

export function KnowledgeRepertoireActions({
  actions,
  onOpenRepertoire,
  onOpenSourceGame,
}: {
  actions: KnowledgeEditorActions;
  onOpenRepertoire?: (target: RepertoireTarget) => void;
  onOpenSourceGame?: (target: SourceGameTarget) => void;
}) {
  const { document, navigation, busy, commit } = actions;
  const [choice, setChoice] = useState<{
    documentId: string;
    side: "" | "white" | "black";
  }>({ documentId: document.id, side: "" });
  const menu = useRef<HTMLDetailsElement>(null);
  const side =
    choice.documentId === document.id && choice.side
      ? choice.side
      : document.side === "white" || document.side === "black"
        ? document.side
        : "";
  const role = document.metadata.repertoire_attachment;
  const active =
    !!role &&
    typeof role === "object" &&
    "active" in role &&
    role.active === true;
  const chapter =
    document.chapters.find((item) => item.id === navigation?.chapter_id) ??
    document.chapters[0];
  const source = chapter?.links.find(
    (link) => link.target.type === "occurrence",
  )?.target;
  const reviewSource = document.metadata.review_source;
  const sourceAccount =
    reviewSource &&
    typeof reviewSource === "object" &&
    "account_id" in reviewSource &&
    typeof reviewSource.account_id === "string"
      ? reviewSource.account_id
      : undefined;

  return (
    <div className="knowledge-repertoire-actions">
      <details ref={menu} className="knowledge-repertoire-menu">
        <summary>{active ? "В репертуаре" : "В репертуар"}</summary>
        <div className="knowledge-menu-panel">
          <label>
            Сторона
            <select
              aria-label="Сторона репертуара знания"
              value={side}
              disabled={busy}
              onChange={(event) =>
                setChoice({
                  documentId: document.id,
                  side: event.target.value as "" | "white" | "black",
                })
              }
            >
              <option value="">Выберите сторону</option>
              <option value="white">За белых</option>
              <option value="black">За чёрных</option>
            </select>
          </label>
          <button
            className="primary"
            disabled={busy || !side}
            onClick={() => {
              if (!side) return;
              commit(async (current) => {
                const changed = await useKnowledgeInRepertoire(
                  current.id,
                  current.revision,
                  side,
                );
                menu.current?.removeAttribute("open");
                return changed;
              });
            }}
          >
            {active ? "Обновить репертуар" : "Добавить в репертуар"}
          </button>
        </div>
      </details>
      {active &&
        onOpenRepertoire &&
        (document.side === "white" || document.side === "black") && (
          <button
            disabled={busy || !navigation}
            onClick={() =>
              commit((current, position) => {
                if (current.side !== "white" && current.side !== "black")
                  return;
                onOpenRepertoire({
                  mode: "train",
                  side: current.side,
                  documentId: current.id,
                  chapterId: position.chapter_id,
                });
              })
            }
          >
            Повторить
          </button>
        )}
      {source?.type === "occurrence" && onOpenSourceGame && (
        <button
          disabled={busy}
          onClick={() =>
            commit(() =>
              onOpenSourceGame({
                gameId: source.game_id,
                accountId: sourceAccount,
                revision: source.revision,
                ply: source.ply,
              }),
            )
          }
        >
          Исходная партия
        </button>
      )}
    </div>
  );
}

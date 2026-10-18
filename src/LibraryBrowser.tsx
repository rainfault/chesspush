import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BoardSession } from "./chess/BoardSession";
import { ChessBoard } from "./chess/ChessBoard";
import { errorMessage } from "./api";
import type { ReplayNode } from "./statistics/api";
type Entry = {
  id: number;
  metadata: {
    white: string;
    black: string;
    result: string;
    date: string;
    opening: string;
  };
};
export function LibraryBrowser({
  fileId,
  title,
  onClose,
}: {
  fileId: number;
  title: string;
  onClose: () => void;
}) {
  const [entries, setEntries] = useState<Entry[]>([]),
    [total, setTotal] = useState(0),
    [error, setError] = useState("");
  const [busy, setBusy] = useState(false),
    [session] = useState(() => new BoardSession()),
    [revision, setRevision] = useState(0);
  const [selected, setSelected] = useState<Entry>(),
    [raw, setRaw] = useState("");
  const generation = useRef(0),
    dialog = useRef<HTMLElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLButtonElement>("button")?.focus();
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
      if (event.key === "Tab") {
        const items = Array.from(
          dialog.current?.querySelectorAll<HTMLElement>(
            "button:not(:disabled),textarea,summary",
          ) ?? [],
        ).filter((el) => el.getClientRects().length);
        if (event.shiftKey && document.activeElement === items[0]) {
          event.preventDefault();
          items.at(-1)?.focus();
        } else if (!event.shiftKey && document.activeElement === items.at(-1)) {
          event.preventDefault();
          items[0]?.focus();
        }
      }
    };
    document.addEventListener("keydown", key);
    return () => {
      generation.current++;
      document.removeEventListener("keydown", key);
      previous?.focus();
    };
  }, []);
  async function more(offset = entries.length) {
    setBusy(true);
    try {
      const p = await invoke<{ total: number; entries: Entry[] }>(
        "library_games",
        { fileId, offset },
      );
      setTotal(p.total);
      setEntries((v) => (offset ? [...v, ...p.entries] : p.entries));
      setError("");
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  useEffect(() => {
    void more(0);
  }, [fileId]);
  async function open(entry: Entry) {
    const request = ++generation.current;
    setBusy(true);
    try {
      const data = await invoke<{
        pgn: string;
        replay: { nodes: ReplayNode[] };
      }>("library_game", { id: entry.id });
      if (request !== generation.current) return;
      session.loadReplay(data.replay.nodes);
      session.seek(0);
      setRevision(session.revision);
      setRaw(data.pgn);
      setSelected(entry);
      setError("");
    } catch (e) {
      if (request === generation.current) setError(errorMessage(e));
    } finally {
      if (request === generation.current) setBusy(false);
    }
  }
  const change = () => setRevision(session.revision);
  return (
    <div className="modal-backdrop">
      <section
        ref={dialog}
        className="library-browser"
        role="dialog"
        aria-modal="true"
        aria-label="Партии библиотеки"
      >
        <header>
          <h2>
            {selected
              ? `${selected.metadata.white} — ${selected.metadata.black}`
              : title}
          </h2>
          <button onClick={onClose} aria-label="Закрыть партии библиотеки">
            Закрыть
          </button>
        </header>
        {error && (
          <p role="alert" className="error">
            {error}
          </p>
        )}
        {selected ? (
          <>
            <div className="library-game">
              <div>
                <ChessBoard
                  session={session}
                  revision={revision}
                  onChange={change}
                  readOnly
                />
                <div className="board-controls">
                  <button
                    onClick={() => {
                      session.seek(session.cursor - 1);
                      change();
                    }}
                    aria-label="Предыдущий ход"
                  >
                    ←
                  </button>
                  <button
                    onClick={() => {
                      session.seek(session.cursor + 1);
                      change();
                    }}
                    aria-label="Следующий ход"
                  >
                    →
                  </button>
                  <button
                    onClick={() => {
                      session.flip();
                      change();
                    }}
                  >
                    Перевернуть
                  </button>
                </div>
              </div>
              <div className="library-notation">
                <p>
                  {selected.metadata.result} · {selected.metadata.opening}
                </p>
                <button onClick={() => setSelected(undefined)}>
                  К списку партий
                </button>
                <div>
                  {session.sanMoves.map((san, i) => (
                    <button
                      key={i}
                      className={session.cursor === i + 1 ? "selected" : ""}
                      onClick={() => {
                        session.seek(i + 1);
                        change();
                      }}
                    >
                      {san}
                    </button>
                  ))}
                </div>
                <details>
                  <summary>Исходный PGN</summary>
                  <textarea
                    readOnly
                    value={raw}
                    aria-label="PGN библиотечной партии"
                  />
                </details>
              </div>
            </div>
          </>
        ) : (
          <>
            <p>{total} партий</p>
            <ul>
              {entries.map((entry) => (
                <li key={entry.id}>
                  <button disabled={busy} onClick={() => void open(entry)}>
                    <span>
                      {entry.metadata.white} — {entry.metadata.black}
                    </span>
                    <span>{entry.metadata.result}</span>
                    <span>{entry.metadata.opening}</span>
                  </button>
                </li>
              ))}
            </ul>
            {entries.length < total && (
              <button disabled={busy} onClick={() => void more()}>
                Ещё партии
              </button>
            )}
          </>
        )}
        {busy && <p role="status">Загрузка…</p>}
      </section>
    </div>
  );
}

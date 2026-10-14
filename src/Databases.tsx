import { useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { BoardSession } from "./chess/BoardSession";
import { ChessBoard } from "./chess/ChessBoard";
import {
  defaultFilters,
  errorMessage,
  files,
  type AppError,
  type LibraryFile,
  type ParseResult,
  type Progress,
} from "./api";

interface Opening {
  eco: string;
  name: string;
  pgn: string;
}
const basename = (path: string) => path.split(/[\\/]/).pop() ?? path;
export function Databases({ active }: { active: boolean }) {
  const [session] = useState(() => new BoardSession());
  const [revision, setRevision] = useState(0);
  const [source, setSource] = useState("");
  const [filters, setFilters] = useState(defaultFilters);
  const [separate, setSeparate] = useState(false);
  const [limit, setLimit] = useState(50),
    [start, setStart] = useState(0);
  const [tab, setTab] = useState("filters");
  const [library, setLibrary] = useState<LibraryFile[]>([]);
  const [busy, setBusy] = useState(false),
    [cancelling, setCancelling] = useState(false),
    [ready, setReady] = useState(false);
  const [progress, setProgress] = useState<Progress>({
    scanned: 0,
    matched: 0,
    cancelled: false,
  });
  const [result, setResult] = useState<ParseResult | null>(null),
    [error, setError] = useState("");
  const [query, setQuery] = useState(""),
    [opening, setOpening] = useState("");
  const [catalog, setCatalog] = useState<Opening[]>([]);
  const [editor, setEditor] = useState<"PGN" | "FEN" | null>(null),
    [text, setText] = useState("");
  const [editorError, setEditorError] = useState("");
  const job = useRef<string | null>(null);
  const changed = () => {
    setRevision(session.revision);
    setOpening("");
  };
  const refresh = () =>
    files()
      .then(setLibrary)
      .catch((e) => setError(errorMessage(e)));

  useEffect(() => {
    fetch("/openings.json")
      .then((r) => r.json())
      .then(setCatalog)
      .catch((e) => setError(errorMessage(e)));
    if (!isTauri()) return;
    let disposed = false;
    const removers: UnlistenFn[] = [];
    const register = async () => {
      removers.push(
        await listen<{ id: string; progress: Progress }>(
          "parser-progress",
          ({ payload }) => {
            if (payload.id === job.current) setProgress(payload.progress);
          },
        ),
      );
      removers.push(
        await listen<{
          id: string;
          result: { Ok?: ParseResult; Err?: AppError };
        }>("parser-finished", ({ payload }) => {
          if (payload.id !== job.current) return;
          if (payload.result.Ok) {
            setResult(payload.result.Ok);
            setProgress(payload.result.Ok.progress);
            setError(
              payload.result.Ok.error?.message ??
                payload.result.Ok.warning ??
                "",
            );
          } else
            setError(
              payload.result.Err?.message ?? "Операция завершилась с ошибкой",
            );
          job.current = null;
          setBusy(false);
          setCancelling(false);
          void refresh();
        }),
      );
      if (disposed) removers.forEach((fn) => fn());
      else {
        setReady(true);
        void refresh();
      }
    };
    void register().catch((e) => {
      removers.forEach((fn) => fn());
      if (!disposed) setError(errorMessage(e));
    });
    return () => {
      disposed = true;
      removers.forEach((fn) => fn());
    };
  }, []);
  useEffect(() => {
    if (!active) return;
    const key = (e: KeyboardEvent) => {
      if (
        editor ||
        (e.target instanceof HTMLElement &&
          ["INPUT", "TEXTAREA", "SELECT", "BUTTON"].includes(e.target.tagName))
      )
        return;
      if (e.key === "ArrowLeft") session.seek(session.cursor - 1);
      else if (e.key === "ArrowRight") session.seek(session.cursor + 1);
      else if (e.key === "Home") session.seek(0);
      else if (e.key === "End") session.seek(session.total);
      else if (e.key.toLowerCase() === "f") session.flip();
      else return;
      e.preventDefault();
      changed();
    };
    window.addEventListener("keydown", key);
    return () => window.removeEventListener("keydown", key);
  }, [active, editor]);
  async function choose(importAll = false) {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: importAll ? "PGN" : "PGN / ZST",
            extensions: importAll ? ["pgn"] : ["zst", "pgn"],
          },
        ],
      });
      if (typeof selected !== "string") return;
      await invoke("remember_file", { path: selected });
      setSource(selected);
      await refresh();
      if (importAll) await run(true, selected);
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function run(importAll = false, path = source) {
    if (!ready || busy) return;
    if (!importAll && !session.standardRoot) {
      setError("Для поиска по дебюту нужна стандартная начальная позиция");
      return;
    }
    const id = crypto.randomUUID();
    job.current = id;
    setBusy(true);
    setCancelling(false);
    setError("");
    setResult(null);
    setProgress({ scanned: 0, matched: 0, cancelled: false });
    try {
      await invoke("start_parse", {
        id,
        request: {
          source: path,
          filters: {
            ...filters,
            min_black_elo: separate
              ? filters.min_black_elo
              : filters.min_white_elo,
            max_black_elo: separate
              ? filters.max_black_elo
              : filters.max_white_elo,
            move_prefix: session.pgn,
            eco: session.pgn ? "" : filters.eco,
            exclude_bullet:
              filters.perf_type === "bullet" ? false : filters.exclude_bullet,
          },
          start_index: start,
          limit,
          import_all: importAll,
        },
      });
    } catch (e) {
      job.current = null;
      setBusy(false);
      setError(errorMessage(e));
    }
  }
  async function cancel() {
    try {
      await invoke("cancel_parse", { id: job.current });
      setCancelling(true);
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  function loadEditor() {
    try {
      if (editor === "FEN") session.loadFen(text);
      else session.loadPgn(text);
      changed();
      setEditor(null);
      setEditorError("");
      setError("");
    } catch (e) {
      setEditorError(errorMessage(e));
    }
  }
  const candidates = query.trim()
    ? catalog
        .filter((o) =>
          query
            .toLowerCase()
            .trim()
            .split(/\s+/)
            .every((t) => `${o.eco} ${o.name}`.toLowerCase().includes(t)),
        )
        .slice(0, 60)
    : [];
  function numberField(
    key:
      | "min_white_elo"
      | "max_white_elo"
      | "min_black_elo"
      | "max_black_elo"
      | "min_moves",
    label: string,
    max = 4000,
  ) {
    return (
      <label>
        {label}
        <input
          aria-label={label}
          type="number"
          min="0"
          max={max}
          value={filters[key]}
          onChange={(e) =>
            setFilters({ ...filters, [key]: e.target.valueAsNumber || 0 })
          }
        />
      </label>
    );
  }

  return (
    <main className="databases" hidden={!active}>
      <section className="board-column">
        <div className="opening-picker">
          <input
            aria-label="Найти дебют"
            placeholder={opening || "Найти дебют"}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          {candidates.length > 0 && (
            <div className="opening-results">
              {candidates.map((o) => (
                <button
                  key={`${o.eco}:${o.name}:${o.pgn}`}
                  onClick={() => {
                    try {
                      session.loadPgn(o.pgn);
                      changed();
                      setOpening(`${o.eco} · ${o.name}`);
                      setQuery("");
                    } catch (e) {
                      setError(errorMessage(e));
                    }
                  }}
                >
                  <span>{o.eco}</span>
                  {o.name}
                </button>
              ))}
            </div>
          )}
        </div>
        <ChessBoard session={session} revision={revision} onChange={changed} />
        <div className="board-controls">
          <button
            title="Перевернуть доску (F)"
            aria-label="Перевернуть доску"
            onClick={() => {
              session.flip();
              changed();
            }}
          >
            ⇅
          </button>
          <button
            title="Начальная позиция"
            aria-label="В начало"
            onClick={() => {
              session.seek(0);
              changed();
            }}
          >
            ⏮
          </button>
          <button
            title="Назад"
            aria-label="Назад"
            onClick={() => {
              session.seek(session.cursor - 1);
              changed();
            }}
          >
            ‹
          </button>
          <button
            title="Вперёд"
            aria-label="Вперёд"
            onClick={() => {
              session.seek(session.cursor + 1);
              changed();
            }}
          >
            ›
          </button>
          <button
            title="Последняя позиция"
            aria-label="В конец"
            onClick={() => {
              session.seek(session.total);
              changed();
            }}
          >
            ⏭
          </button>
          <button
            title="Очистить стрелки"
            aria-label="Очистить стрелки"
            onClick={() => {
              session.setShapes([]);
              session.revision++;
              changed();
            }}
          >
            ⌫
          </button>
          <button
            title="Новая позиция"
            aria-label="Сбросить доску"
            onClick={() => {
              session.reset();
              changed();
            }}
          >
            ↺
          </button>
          <button
            onClick={() => {
              setText(session.pgn);
              setEditorError("");
              setEditor("PGN");
            }}
          >
            PGN
          </button>
          <button
            onClick={() => {
              setText(session.fen);
              setEditorError("");
              setEditor("FEN");
            }}
          >
            FEN
          </button>
        </div>
        <div className="moves" aria-label="Ходы">
          {session.sanMoves.map((san, i) => (
            <button
              key={i}
              className={session.cursor === i + 1 ? "selected" : ""}
              onClick={() => {
                session.seek(i + 1);
                changed();
              }}
            >
              {san}
            </button>
          ))}
        </div>
      </section>
      <section className="parser-column">
        <button
          className="source-button"
          disabled={busy || !ready}
          title={source}
          onClick={() => void choose()}
        >
          {source ? basename(source) : "Выбрать PGN / ZST"}
          <span>⌄</span>
        </button>
        <div className="tabs">
          <button
            className={tab === "filters" ? "active" : ""}
            onClick={() => setTab("filters")}
          >
            Фильтры
          </button>
          <button
            className={tab === "files" ? "active" : ""}
            onClick={() => {
              setTab("files");
              if (ready) void refresh();
            }}
          >
            Файлы
          </button>
        </div>
        {tab === "filters" ? (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              void run();
            }}
          >
            <fieldset disabled={busy}>
              <div className="form-row">
                <label>
                  Сторона
                  <select
                    value={filters.color}
                    onChange={(e) =>
                      setFilters({ ...filters, color: e.target.value })
                    }
                  >
                    <option value="any">Любая</option>
                    <option value="white">Белые</option>
                    <option value="black">Чёрные</option>
                  </select>
                </label>
                <label>
                  Контроль
                  <select
                    value={filters.perf_type}
                    onChange={(e) =>
                      setFilters({ ...filters, perf_type: e.target.value })
                    }
                  >
                    <option value="any">Любой</option>
                    <option value="rapid">Rapid</option>
                    <option value="blitz">Blitz</option>
                    <option value="classical">Classical</option>
                    <option value="bullet">Bullet</option>
                  </select>
                </label>
              </div>
              <div className="form-row">
                {numberField(
                  "min_white_elo",
                  separate ? "Белые от" : "Рейтинг от",
                )}
                {numberField("max_white_elo", "До")}
              </div>
              <div className="form-row">
                <label>
                  Количество
                  <input
                    type="number"
                    min="1"
                    max="1000000"
                    value={limit}
                    onChange={(e) => setLimit(e.target.valueAsNumber || 1)}
                  />
                </label>
                <label>
                  Результат
                  <select
                    value={filters.result_mode}
                    onChange={(e) =>
                      setFilters({ ...filters, result_mode: e.target.value })
                    }
                  >
                    <option value="all">Любой</option>
                    <option value="selected_wins">Победы</option>
                    <option value="selected_losses">Поражения</option>
                    <option value="wins_draws">Победы и ничьи</option>
                    <option value="no_quick_losses">
                      Без быстрых поражений
                    </option>
                  </select>
                </label>
              </div>
              <details>
                <summary>Дополнительно</summary>
                <label className="checkbox">
                  <input
                    type="checkbox"
                    checked={separate}
                    onChange={(e) => setSeparate(e.target.checked)}
                  />
                  Разный рейтинг сторон
                </label>
                {separate && (
                  <div className="form-row">
                    {numberField("min_black_elo", "Чёрные от")}
                    {numberField("max_black_elo", "Чёрные до")}
                  </div>
                )}
                <div className="form-row">
                  {numberField("min_moves", "Минимум ходов", 1000)}
                  <label>
                    Пропустить партий
                    <input
                      type="number"
                      min="0"
                      max={Number.MAX_SAFE_INTEGER}
                      value={start}
                      onChange={(e) => setStart(e.target.valueAsNumber || 0)}
                    />
                  </label>
                </div>
                <div className="form-row">
                  <label>
                    ECO
                    <input
                      disabled={!!session.pgn}
                      value={filters.eco}
                      onChange={(e) =>
                        setFilters({ ...filters, eco: e.target.value })
                      }
                    />
                  </label>
                  <label>
                    Игрок
                    <input
                      value={filters.username}
                      onChange={(e) =>
                        setFilters({ ...filters, username: e.target.value })
                      }
                    />
                  </label>
                </div>
                <label className="checkbox">
                  <input
                    type="checkbox"
                    checked={
                      filters.exclude_bullet && filters.perf_type !== "bullet"
                    }
                    disabled={filters.perf_type === "bullet"}
                    onChange={(e) =>
                      setFilters({
                        ...filters,
                        exclude_bullet: e.target.checked,
                      })
                    }
                  />
                  Исключить Bullet
                </label>
                <label className="checkbox">
                  <input
                    type="checkbox"
                    checked={filters.include_timeout}
                    onChange={(e) =>
                      setFilters({
                        ...filters,
                        include_timeout: e.target.checked,
                      })
                    }
                  />
                  Партии с просрочкой времени
                </label>
              </details>
            </fieldset>
            <div className="run-actions">
              {busy ? (
                <button
                  type="button"
                  className="cancel"
                  disabled={cancelling}
                  onClick={() => void cancel()}
                >
                  {cancelling ? "Отмена…" : "Остановить"}
                </button>
              ) : (
                <button className="primary" disabled={!source || !ready}>
                  Найти партии
                </button>
              )}
            </div>
          </form>
        ) : (
          <div className="library">
            <button
              className="import-button"
              disabled={busy || !ready}
              onClick={() => void choose(true)}
            >
              Импорт PGN
            </button>
            {library.map((file) => (
              <div className="file-row" key={file.id}>
                <button
                  className="file-name"
                  title={file.path}
                  onClick={() =>
                    void invoke("reveal_file", { id: file.id }).catch((e) =>
                      setError(errorMessage(e)),
                    )
                  }
                >
                  {basename(file.path)}
                  {file.games > 0 && (
                    <span>{file.games.toLocaleString("ru-RU")}</span>
                  )}
                </button>
                <button
                  disabled={busy}
                  title="Выбрать источник"
                  aria-label={`Выбрать ${basename(file.path)}`}
                  onClick={() => {
                    setSource(file.path);
                    setTab("filters");
                  }}
                >
                  ↗
                </button>
              </div>
            ))}
          </div>
        )}
        {(busy || result) && (
          <div className="operation" aria-live="polite">
            {busy && <progress aria-label="Чтение базы" />}
            <div className="counts">
              <span>
                Обработано{" "}
                <strong>{progress.scanned.toLocaleString("ru-RU")}</strong>
              </span>
              <span>
                Найдено{" "}
                <strong>{progress.matched.toLocaleString("ru-RU")}</strong>
              </span>
            </div>
            {!busy && result && (
              <div className="result-status">
                {result.error
                  ? "Завершено с ошибкой"
                  : progress.cancelled
                    ? "Остановлено"
                    : "Готово"}{" "}
                · {(result.elapsed_ms / 1000).toFixed(1)} с
              </div>
            )}
            {result?.output && (
              <button
                className="result-file"
                onClick={() => {
                  const file = library.find((f) => f.path === result.output);
                  if (file)
                    void invoke("reveal_file", { id: file.id }).catch((e) =>
                      setError(errorMessage(e)),
                    );
                }}
              >
                {basename(result.output)}
              </button>
            )}
          </div>
        )}
        {error && (
          <div className="error" role="alert">
            {error}
            <button aria-label="Закрыть ошибку" onClick={() => setError("")}>
              ×
            </button>
          </div>
        )}
      </section>
      {editor && (
        <div className="modal-backdrop" onMouseDown={() => setEditor(null)}>
          <section
            className="editor-dialog"
            role="dialog"
            aria-modal="true"
            aria-label={editor}
            onMouseDown={(e) => e.stopPropagation()}
          >
            <header>
              <h2>{editor}</h2>
              <button aria-label="Закрыть" onClick={() => setEditor(null)}>
                ×
              </button>
            </header>
            <textarea
              autoFocus
              aria-label={`${editor} позиции`}
              value={text}
              onChange={(e) => {
                setText(e.target.value);
                setEditorError("");
              }}
            />
            {editorError && (
              <div className="error" role="alert">
                {editorError}
              </div>
            )}
            <button className="primary" onClick={loadEditor}>
              Применить
            </button>
          </section>
        </div>
      )}
    </main>
  );
}

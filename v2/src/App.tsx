import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { Databases } from "./Databases";
import { errorMessage } from "./api";
const pages = [
  ["databases", "Базы"],
  ["statistics", "Статистика"],
  ["knowledge", "Знания"],
  ["games", "Мои партии"],
  ["training", "Тренировка"],
] as const;
export function App() {
  const [page, setPage] = useState("databases"),
    [theme, setTheme] = useState("light"),
    [settings, setSettings] = useState(false),
    [error, setError] = useState("");
  const [now, setNow] = useState(new Date());
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 1000);
    return () => clearInterval(timer);
  }, []);
  useEffect(() => {
    if (isTauri())
      void invoke<string>("get_theme")
        .then(setTheme)
        .catch((e) => setError(errorMessage(e)));
  }, []);
  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);
  async function changeTheme(value: string) {
    try {
      if (isTauri()) await invoke("set_theme", { theme: value });
      setTheme(value);
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  return (
    <>
      <header className="app-header">
        <span className="wordmark">ChessPush</span>
        <nav aria-label="Разделы">
          <button
            className={page === "home" ? "active" : ""}
            title="Главная"
            aria-label="Главная"
            onClick={() => setPage("home")}
          >
            <svg
              viewBox="0 0 24 24"
              width="24"
              height="24"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.6"
            >
              <path d="m3 11 9-8 9 8M6 9v12h5v-7h3v7h4V9" />
            </svg>
          </button>
          {pages.map(([id, label]) => (
            <button
              key={id}
              className={page === id ? "active" : ""}
              onClick={() => setPage(id)}
            >
              {label}
            </button>
          ))}
        </nav>
        <button
          className="settings-button"
          title="Настройки"
          aria-label="Настройки"
          onClick={() => setSettings(true)}
        >
          <svg
            viewBox="0 0 24 24"
            width="25"
            height="25"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
          >
            <path d="m9 3 1-2h4l1 2 3 2 2 1-1 3v5l1 3-2 1-3 2-1 2h-4l-1-2-3-2-2-1 1-3V9L4 6l2-1z" />
            <circle cx="12" cy="12" r="4" />
          </svg>
        </button>
      </header>
      <Databases active={page === "databases"} />
      {page === "home" && (
        <main className="home">
          <time>
            {now.toLocaleTimeString("ru-RU", {
              hour: "2-digit",
              minute: "2-digit",
            })}
          </time>
        </main>
      )}
      {page !== "home" && page !== "databases" && (
        <main className="placeholder">
          <h1>{pages.find(([id]) => id === page)?.[1]}</h1>
        </main>
      )}
      {settings && (
        <div className="modal-backdrop" onMouseDown={() => setSettings(false)}>
          <section
            className="settings-dialog"
            role="dialog"
            aria-modal="true"
            aria-label="Настройки"
            onMouseDown={(e) => e.stopPropagation()}
          >
            <header>
              <h2>Настройки</h2>
              <button
                aria-label="Закрыть настройки"
                onClick={() => setSettings(false)}
              >
                ×
              </button>
            </header>
            <label>
              Тема
              <select
                aria-label="Тема"
                value={theme}
                onChange={(e) => void changeTheme(e.target.value)}
              >
                <option value="light">Светлая</option>
                <option value="dark">Тёмная</option>
              </select>
            </label>
            {error && (
              <div role="alert" className="error">
                {error}
              </div>
            )}
          </section>
        </div>
      )}
    </>
  );
}

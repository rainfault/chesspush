import { useCallback, useEffect, useRef, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Databases } from "./Databases";
import { Home } from "./Home";
import { Settings } from "./Settings";
import { errorMessage } from "./api";
import {
  authenticate,
  cancelAuthentication,
  connectToken,
  disconnectAccount,
  disconnectedAccount,
  refreshAccount,
  type AccountStatus,
} from "./lichess";

const pages = [
  ["databases", "Базы"],
  ["statistics", "Статистика"],
  ["knowledge", "Знания"],
  ["games", "Мои партии"],
  ["training", "Тренировка"],
] as const;

export function App() {
  const [page, setPage] = useState("home");
  const [theme, setTheme] = useState("light");
  const [settings, setSettings] = useState(false);
  const [error, setError] = useState("");
  const [account, setAccount] = useState<AccountStatus>(disconnectedAccount);
  const [busy, setBusy] = useState(false);
  const revision = useRef(0);
  const refreshing = useRef(false);
  const closeSettings = useCallback(() => setSettings(false), []);

  const refresh = useCallback(async () => {
    if (!isTauri() || refreshing.current) return;
    refreshing.current = true;
    const current = revision.current;
    try {
      const value = await refreshAccount();
      if (current === revision.current) setAccount(value);
    } catch {
      if (current === revision.current)
        setAccount((previous) => ({
          ...previous,
          status: "error",
          rapid: null,
          blitz: null,
          message: "Не удалось проверить подключение Lichess",
        }));
    } finally {
      refreshing.current = false;
    }
  }, []);

  useEffect(() => {
    if (!isTauri()) return;
    void invoke<string>("get_theme")
      .then(setTheme)
      .catch((e) => setError(errorMessage(e)));
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void listen<AccountStatus>("lichess-account-updated", (event) => {
      if (!disposed) {
        revision.current++;
        setAccount(event.payload);
      }
    })
      .then((stop) => {
        if (disposed) stop();
        else {
          unsubscribe = stop;
          void refresh();
        }
      })
      .catch(() => {
        if (!disposed) void refresh();
      });
    const timer = setInterval(() => void refresh(), 60_000);
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    window.addEventListener("online", onFocus);
    return () => {
      disposed = true;
      unsubscribe?.();
      clearInterval(timer);
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("online", onFocus);
    };
  }, [refresh]);

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

  async function runAccountAction(action: () => Promise<AccountStatus>) {
    setError("");
    if (!isTauri()) {
      setAccount((previous) => ({
        ...previous,
        status: "error",
        rapid: null,
        blitz: null,
        message: "Вход доступен в приложении ChessPush",
      }));
      return;
    }
    setBusy(true);
    const current = ++revision.current;
    try {
      const value = await action();
      if (current === revision.current) setAccount(value);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  const onAuthenticate = () => void runAccountAction(authenticate);
  const onCancel = () => void runAccountAction(cancelAuthentication);

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
          onClick={() => {
            setError("");
            setSettings(true);
          }}
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
        <Home
          account={account}
          onAuthenticate={onAuthenticate}
          onCancel={onCancel}
          busy={busy}
        />
      )}
      {page !== "home" && page !== "databases" && (
        <main className="placeholder">
          <h1>{pages.find(([id]) => id === page)?.[1]}</h1>
        </main>
      )}
      {settings && (
        <Settings
          account={account}
          theme={theme}
          busy={busy}
          error={error}
          onClose={closeSettings}
          onTheme={(value) => void changeTheme(value)}
          onToken={(token) => void runAccountAction(() => connectToken(token))}
          onAuthenticate={onAuthenticate}
          onCancel={onCancel}
          onDisconnect={() => void runAccountAction(disconnectAccount)}
        />
      )}
      {error && !settings && (
        <div className="global-error error" role="alert">
          {error}
          <button
            aria-label="Закрыть ошибку аккаунта"
            onClick={() => setError("")}
          >
            ×
          </button>
        </div>
      )}
    </>
  );
}

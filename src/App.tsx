import { Repertoire, type RepertoireTarget } from "./knowledge/Repertoire";
import type { DocumentPosition } from "./knowledge/types";
import { Statistics } from "./statistics/Statistics";
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
  ["home", "Главная"],
  ["statistics", "Статистика"],
  ["repertoire", "Репертуар"],
  ["databases", "Парсер ZST"],
] as const;

export function App() {
  const [page, setPage] = useState("home");
  const [sectionRequest, setSectionRequest] = useState(0);
  const [section, setSection] = useState("openings");
  const [settingsSection, setSettingsSection] = useState("account");
  const [repertoireTarget, setRepertoireTarget] = useState<RepertoireTarget>();
  const [openPositionRequest, setOpenPositionRequest] =
    useState<DocumentPosition>();
  const openSettings = (tab = "account") => {
    setSettingsSection(tab);
    setSettings(true);
  };
  const openMap = () => {
    setSectionRequest((n) => n + 1);
    setSection("graph");
    setPage("statistics");
  };
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
        <button
          className="wordmark"
          onClick={() => setPage("home")}
          aria-label="ChessPush — главная"
        >
          ChessPush
        </button>
        <nav aria-label="Разделы">
          {pages.map(([id, label]) => (
            <button
              key={id}
              className={page === id ? "active" : ""}
              onClick={() => {
                setPage(id);
                if (id === "repertoire")
                  setRepertoireTarget({ mode: "list", requestId: Date.now() });
              }}
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
            openSettings();
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
      <Statistics
        active={page === "statistics"}
        initialSection={section}
        sectionRequest={sectionRequest}
        onNavigateSection={setSection}
        onOpenSettings={openSettings}
        openPositionRequest={openPositionRequest}
        onPositionRequestHandled={() => setOpenPositionRequest(undefined)}
        onOpenRepertoire={(target) => {
          setRepertoireTarget({ ...target, requestId: Date.now() });
          setPage("repertoire");
        }}
        account={account}
        onAuthenticate={onAuthenticate}
        busy={busy}
      />
      {page === "home" && (
        <Home
          account={account}
          onAuthenticate={onAuthenticate}
          onCancel={onCancel}
          busy={busy}
        />
      )}
      <div hidden={page !== "repertoire"}>
        <Repertoire
          active={page === "repertoire"}
          initialTarget={repertoireTarget}
          onOpenGraph={(position) => {
            setOpenPositionRequest(position);
            openMap();
          }}
        />
      </div>
      {settings && (
        <Settings
          account={account}
          initialSection={settingsSection}
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
      {(error ||
        (["invalid_token", "error"].includes(account.status)
          ? account.message
          : null)) &&
        !settings && (
          <div className="global-error error" role="alert">
            {error || account.message}
            <button onClick={() => openSettings("account")}>
              Проверить подключение
            </button>
            {error && (
              <button
                aria-label="Закрыть ошибку аккаунта"
                onClick={() => setError("")}
              >
                ×
              </button>
            )}
          </div>
        )}
    </>
  );
}

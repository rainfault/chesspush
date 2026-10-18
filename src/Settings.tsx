import { StatisticsSettings } from "./statistics/StatisticsSettings";
import { EngineSettings } from "./statistics/EngineSettings";
import { useEffect, useRef, useState } from "react";
import type { AccountStatus } from "./lichess";

interface SettingsProps {
  account: AccountStatus;
  theme: string;
  busy: boolean;
  error: string;
  onClose: () => void;
  onTheme: (theme: string) => void;
  onToken: (token: string) => void;
  onAuthenticate: () => void;
  onCancel: () => void;
  onDisconnect: () => void;
  initialSection?: string;
}
const sections = [
  ["account", "Аккаунт"],
  ["appearance", "Внешний вид"],
  ["engine", "Движок"],
  ["statistics", "Статистика"],
] as const;
export function Settings({
  account,
  theme,
  busy,
  error,
  onClose,
  onTheme,
  onToken,
  onAuthenticate,
  onCancel,
  onDisconnect,
  initialSection = "account",
}: SettingsProps) {
  const [token, setToken] = useState("");
  const [section, setSection] = useState(initialSection);
  const dialog = useRef<HTMLElement>(null);
  const authenticating = account.status === "authenticating";
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.querySelector<HTMLElement>("button")?.focus();
    const key = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onClose();
      }
      if (event.key === "Tab") {
        const controls = Array.from(
          dialog.current?.querySelectorAll<HTMLElement>(
            "button:not(:disabled),input:not(:disabled),select:not(:disabled),summary,textarea",
          ) ?? [],
        ).filter((e) => e.getClientRects().length);
        const first = controls[0],
          last = controls.at(-1);
        if (event.shiftKey && document.activeElement === first) {
          event.preventDefault();
          last?.focus();
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault();
          first?.focus();
        }
      }
    };
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("keydown", key);
      previous?.focus();
    };
  }, [onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <section
        ref={dialog}
        className="settings-dialog product-settings"
        role="dialog"
        aria-modal="true"
        aria-label="Настройки"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header>
          <h2>Настройки</h2>
          <button aria-label="Закрыть настройки" onClick={onClose}>
            ×
          </button>
        </header>
        <div
          className="settings-tabs"
          role="tablist"
          aria-label="Разделы настроек"
        >
          {sections.map(([id, label]) => (
            <button
              key={id}
              role="tab"
              aria-selected={section === id}
              onClick={() => setSection(id)}
            >
              {label}
            </button>
          ))}
        </div>
        <div
          role="tabpanel"
          aria-label={sections.find(([id]) => id === section)?.[1]}
        >
          {section === "account" && (
            <>
              <h3>Lichess</h3>
              <p>
                {account.username
                  ? `Аккаунт: ${account.username}`
                  : "Аккаунт не подключён"}
              </p>
              {authenticating ? (
                <>
                  <p role="status">Завершите вход в браузере.</p>
                  <button
                    className="auth-button"
                    onClick={onCancel}
                    disabled={busy}
                  >
                    Отменить вход
                  </button>
                </>
              ) : (
                <button
                  className="primary auth-button"
                  onClick={onAuthenticate}
                  disabled={busy}
                >
                  {account.hasToken
                    ? "Подключить через браузер заново"
                    : "Войти через Lichess"}
                </button>
              )}
              <details>
                <summary>Подключить вручную по токену</summary>
                <form
                  onSubmit={(e) => {
                    e.preventDefault();
                    const value = token.trim();
                    if (!value || busy || authenticating) return;
                    setToken("");
                    onToken(value);
                  }}
                >
                  <label>
                    Токен
                    <input
                      aria-label="Токен"
                      type="password"
                      value={token}
                      onChange={(e) => setToken(e.target.value)}
                      autoComplete="off"
                      spellCheck={false}
                      maxLength={4096}
                    />
                  </label>
                  <button
                    className="token-connect"
                    type="submit"
                    disabled={!token.trim() || busy || authenticating}
                  >
                    Подключить
                  </button>
                </form>
              </details>
              {account.hasToken && (
                <button
                  className="disconnect-button"
                  onClick={onDisconnect}
                  disabled={busy || authenticating}
                >
                  Отключить аккаунт
                </button>
              )}
            </>
          )}
          {section === "appearance" && (
            <label>
              Тема
              <select
                aria-label="Тема"
                value={theme}
                onChange={(e) => onTheme(e.target.value)}
              >
                <option value="light">Светлая</option>
                <option value="dark">Тёмная</option>
              </select>
            </label>
          )}
          {section === "engine" && <EngineSettings />}
          {section === "statistics" && <StatisticsSettings />}
        </div>
        {(error || account.message) && (
          <div role="alert" className="error">
            {error || account.message}
          </div>
        )}
      </section>
    </div>
  );
}

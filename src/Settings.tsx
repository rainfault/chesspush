import { EngineSettings } from "./statistics/EngineSettings";
import { useEffect, useState } from "react";
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
}

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
}: SettingsProps) {
  const [token, setToken] = useState("");
  const authenticating = account.status === "authenticating";
  useEffect(() => {
    const close = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    document.addEventListener("keydown", close);
    return () => document.removeEventListener("keydown", close);
  }, [onClose]);
  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <section
        className="settings-dialog"
        role="dialog"
        aria-modal="true"
        aria-label="Настройки"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header>
          <h2>Настройки</h2>
          <button aria-label="Закрыть настройки" onClick={onClose}>
            ×
          </button>
        </header>
        <label>
          Ник
          <input
            aria-label="Ник"
            value={account.username ?? ""}
            readOnly
            autoComplete="off"
          />
        </label>
        <form
          onSubmit={(event) => {
            event.preventDefault();
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
              onChange={(event) => setToken(event.target.value)}
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
        {authenticating ? (
          <button className="auth-button" onClick={onCancel} disabled={busy}>
            Отменить вход
          </button>
        ) : (
          <button
            className="primary auth-button"
            onClick={onAuthenticate}
            disabled={busy}
          >
            Войти через Lichess
          </button>
        )}
        <label>
          Тема
          <select
            aria-label="Тема"
            value={theme}
            onChange={(event) => onTheme(event.target.value)}
          >
            <option value="light">Светлая</option>
            <option value="dark">Тёмная</option>
          </select>
        </label>
        <EngineSettings />
        {account.hasToken && (
          <button
            className="disconnect-button"
            onClick={onDisconnect}
            disabled={busy || authenticating}
          >
            Отключить аккаунт
          </button>
        )}
        {(error || account.message) && (
          <div role="alert" className="error">
            {error || account.message}
          </div>
        )}
      </section>
    </div>
  );
}

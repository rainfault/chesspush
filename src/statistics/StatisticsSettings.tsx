import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { errorMessage } from "../api";
import { getState, type StatisticsState } from "./api";
import "./statisticsSettings.css";

interface StatisticsPreferences {
  opening_coverage_depth_moves: number;
}
interface ResetResult {
  account_id: string;
  t0_ms: number;
  removed_games: number;
}
export function StatisticsSettings() {
  const [depth, setDepth] = useState("15");
  const [savedDepth, setSavedDepth] = useState(15);
  const [state, setState] = useState<StatisticsState | null>(null);
  const [accountId, setAccountId] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const [confirming, setConfirming] = useState(false);
  const value = Number(depth);
  const valid =
    /^\d+$/.test(depth) && Number.isInteger(value) && value >= 1 && value <= 40;
  const selected = state?.accounts.find((account) => account.id === accountId);
  useEffect(() => {
    if (!isTauri()) {
      setLoading(false);
      return;
    }
    let current = true;
    void Promise.all([
      invoke<StatisticsPreferences>("statistics_preferences"),
      getState(),
    ])
      .then(([preferences, next]) => {
        if (!current) return;
        setDepth(String(preferences.opening_coverage_depth_moves));
        setSavedDepth(preferences.opening_coverage_depth_moves);
        setState(next);
        setAccountId(next.active_account_id ?? next.accounts[0]?.id ?? "");
      })
      .catch((cause) => {
        if (current) setError(errorMessage(cause));
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, []);
  async function save() {
    if (!valid || loading || busy || value === savedDepth) return;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const next = await invoke<StatisticsPreferences>(
        "statistics_preferences_save",
        {
          preferences: { opening_coverage_depth_moves: value },
        },
      );
      setDepth(String(next.opening_coverage_depth_moves));
      setSavedDepth(next.opening_coverage_depth_moves);
      setMessage("Глубина покрытия сохранена");
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }
  async function reset() {
    if (!selected || busy) return;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      const result = await invoke<ResetResult>("statistics_reset", {
        accountId,
      });
      setConfirming(false);
      setMessage(
        `Статистика ${selected.username} сброшена. Новый период: ${new Date(result.t0_ms).toLocaleString("ru-RU")}`,
      );
      setState(await getState(accountId));
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section
      className="statistics-settings"
      aria-label="Настройки статистики"
      aria-busy={loading || busy}
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <label>
          Глубина покрытия дебютов, ходов
          <input
            aria-label="Глубина покрытия дебютов, ходов"
            type="number"
            min="1"
            max="40"
            step="1"
            aria-invalid={!valid}
            value={depth}
            disabled={loading || busy}
            onChange={(event) => {
              setDepth(event.target.value);
              setMessage("");
            }}
          />
        </label>
        <button
          className="primary"
          type="submit"
          disabled={loading || busy || !valid || value === savedDepth}
        >
          Сохранить глубину
        </button>
      </form>
      <div className="statistics-reset">
        <h3>Сброс статистики</h3>
        {state?.accounts.length ? (
          <>
            <label>
              Аккаунт статистики
              <select
                aria-label="Аккаунт статистики"
                value={accountId}
                disabled={busy}
                onChange={(event) => {
                  setAccountId(event.target.value);
                  setConfirming(false);
                  setMessage("");
                }}
              >
                {state.accounts.map((account) => (
                  <option key={account.id} value={account.id}>
                    {account.username}
                  </option>
                ))}
              </select>
            </label>
            {confirming ? (
              <div
                className="statistics-reset-confirm"
                role="alertdialog"
                aria-modal="false"
                aria-labelledby="statistics-reset-title"
                aria-describedby="statistics-reset-description"
              >
                <h3 id="statistics-reset-title">
                  Сбросить статистику {selected?.username}?
                </h3>
                <p id="statistics-reset-description">
                  Начать заново с текущего момента. Прежние партии исчезнут из
                  статистики и её графа. Подключение аккаунта, исходные партии,
                  анализ и репертуар сохранятся.
                </p>
                <div className="statistics-settings-actions">
                  <button
                    autoFocus
                    disabled={busy}
                    onClick={() => setConfirming(false)}
                  >
                    Отмена
                  </button>
                  <button
                    className="danger-button"
                    disabled={busy}
                    onClick={() => void reset()}
                  >
                    {busy ? "Сброс…" : "Сбросить статистику аккаунта"}
                  </button>
                </div>
              </div>
            ) : (
              <button
                className="danger-button"
                disabled={busy || loading || !selected}
                onClick={() => {
                  setConfirming(true);
                  setError("");
                  setMessage("");
                }}
              >
                Сбросить статистику
              </button>
            )}
          </>
        ) : (
          !loading && <p>Аккаунт не подключён</p>
        )}
      </div>
      {message && <p role="status">{message}</p>}
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
    </section>
  );
}

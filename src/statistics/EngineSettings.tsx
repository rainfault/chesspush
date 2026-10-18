import { useEffect, useState } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { errorMessage } from "../api";
import type { EngineConfig } from "./api";
export function EngineSettings() {
  const [config, setConfig] = useState<EngineConfig>({
    path: "",
    threads: 1,
    hash_mb: 128,
    profile: "standard",
    custom_nodes: null,
    custom_movetime_ms: null,
    syzygy_path: null,
  });
  const [busy, setBusy] = useState(false),
    [error, setError] = useState(""),
    [identity, setIdentity] = useState("");
  useEffect(() => {
    if (isTauri())
      void invoke<EngineConfig>("statistics_engine_config")
        .then(setConfig)
        .catch((e) => setError(errorMessage(e)));
  }, []);
  async function choose() {
    try {
      const file = await open({ multiple: false, title: "Stockfish" });
      if (typeof file === "string") setConfig({ ...config, path: file });
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function chooseTables() {
    try {
      const folder = await open({
        directory: true,
        multiple: false,
        title: "Syzygy",
      });
      if (typeof folder === "string")
        setConfig({ ...config, syzygy_path: folder });
    } catch (e) {
      setError(errorMessage(e));
    }
  }
  async function save(validate = false) {
    setBusy(true);
    setError("");
    setIdentity("");
    try {
      const result = await invoke<string>(
        validate ? "statistics_engine_validate" : "statistics_engine_save",
        { config },
      );
      setIdentity(result);
    } catch (e) {
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="engine-settings" aria-label="Stockfish">
      <h3>Stockfish</h3>
      <label>
        Исполняемый файл
        <div className="engine-path">
          <input
            aria-label="Путь к Stockfish"
            value={config.path}
            onChange={(e) => setConfig({ ...config, path: e.target.value })}
          />
          <button onClick={() => void choose()} aria-label="Выбрать Stockfish">
            …
          </button>
        </div>
      </label>
      <label>
        Режим
        <select
          aria-label="Режим анализа"
          value={config.profile}
          onChange={(e) =>
            setConfig({
              ...config,
              profile: e.target.value,
              custom_nodes: null,
              custom_movetime_ms: null,
            })
          }
        >
          <option value="quick">Быстрый</option>
          <option value="standard">Стандартный</option>
          <option value="deep">Углублённый</option>
        </select>
      </label>
      <details>
        <summary>Дополнительные настройки</summary>
        <div className="form-row">
          <label>
            Потоки
            <input
              aria-label="Потоки Stockfish"
              type="number"
              min="1"
              max="128"
              value={config.threads}
              onChange={(e) =>
                setConfig({ ...config, threads: Number(e.target.value) })
              }
            />
          </label>
          <label>
            Hash, MiB
            <input
              aria-label="Hash Stockfish"
              type="number"
              min="16"
              max="4096"
              value={config.hash_mb}
              onChange={(e) =>
                setConfig({ ...config, hash_mb: Number(e.target.value) })
              }
            />
          </label>
        </div>

        <div className="form-row">
          <label>
            Узлы
            <input
              aria-label="Лимит узлов Stockfish"
              type="number"
              min="100"
              max="2000000000"
              value={config.custom_nodes ?? ""}
              onChange={(e) =>
                setConfig({
                  ...config,
                  custom_nodes:
                    e.target.value === "" ? null : Number(e.target.value),
                })
              }
            />
          </label>
          <label>
            На поиск, мс
            <input
              aria-label="Лимит времени Stockfish"
              type="number"
              min="10"
              max="300000"
              value={config.custom_movetime_ms ?? ""}
              onChange={(e) =>
                setConfig({
                  ...config,
                  custom_movetime_ms:
                    e.target.value === "" ? null : Number(e.target.value),
                })
              }
            />
          </label>
        </div>
        <label>
          Каталог Syzygy
          <div className="engine-path">
            <input
              aria-label="Каталог Syzygy"
              value={config.syzygy_path ?? ""}
              onChange={(e) =>
                setConfig({ ...config, syzygy_path: e.target.value || null })
              }
            />
            <button
              aria-label="Выбрать каталог Syzygy"
              onClick={() => void chooseTables()}
            >
              …
            </button>
          </div>
        </label>
      </details>
      <div className="engine-actions">
        <button disabled={busy || !config.path} onClick={() => void save(true)}>
          Проверить движок
        </button>
        <button className="primary" disabled={busy} onClick={() => void save()}>
          Сохранить движок
        </button>
      </div>
      {identity && (
        <>
          <p role="status">Движок готов. Настройки сохранены.</p>
          <details>
            <summary>Технические подробности движка</summary>
            <output>{identity}</output>
          </details>
        </>
      )}
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
    </section>
  );
}

import { useEffect, useRef, useState } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import { errorMessage } from "../api";
import {
  backupProfile,
  cancelProfileRestore,
  prepareProfileRestore,
  previewProfile,
  profileStatus,
  restartForProfileRestore,
  type BackupSummary,
  type ProfileStatus,
} from "./api";
import "./profile.css";

const date = (value: number) => new Date(value).toLocaleString("ru-RU");
const bytes = (value: number) =>
  `${(value / (1024 * 1024)).toLocaleString("ru-RU", { maximumFractionDigits: 1 })} MiB`;
const labels: Record<string, string> = {
  games: "Партии",
  knowledge: "Знания",
  repertoire: "Репертуары",
  reviews: "Повторения",
  sessions: "Учебные сессии",
};

export function ProfileSettings({
  onRestoringChange,
}: {
  onRestoringChange?: (value: boolean) => void;
}) {
  const [state, setState] = useState<ProfileStatus | null>(null);
  const [preview, setPreview] = useState<BackupSummary | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [message, setMessage] = useState("");
  const operation = useRef(false),
    mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    if (isTauri())
      void profileStatus()
        .then((value) => {
          if (mounted.current) setState(value);
        })
        .catch((reason) => {
          if (mounted.current) setError(errorMessage(reason));
        });
    return () => {
      mounted.current = false;
    };
  }, []);
  async function run(action: () => Promise<void>) {
    if (operation.current) return;
    operation.current = true;
    setBusy(true);
    setError("");
    setMessage("");
    try {
      await action();
    } catch (reason) {
      if (mounted.current) setError(errorMessage(reason));
    } finally {
      operation.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  async function restart() {
    onRestoringChange?.(true);
    try {
      await restartForProfileRestore();
    } catch (reason) {
      onRestoringChange?.(false);
      throw reason;
    }
  }
  return (
    <section className="profile-settings" aria-label="Данные профиля">
      <h3>Резервные копии</h3>
      <div className="profile-actions">
        <button
          className="primary"
          disabled={busy || !isTauri() || state?.pending_restore}
          onClick={() =>
            void run(async () => {
              const path = await save({
                title: "Резервная копия профиля",
                defaultPath: `ChessPush-${new Date().toISOString().slice(0, 10)}.chesspush`,
                filters: [
                  { name: "Профиль ChessPush", extensions: ["chesspush"] },
                ],
              });
              if (!path) return;
              const result = await backupProfile(path);
              if (mounted.current) {
                setState((previous) => ({
                  last_backup: result,
                  restore_notice: previous?.restore_notice ?? null,
                  pending_restore: false,
                }));
                setMessage(`Резервная копия сохранена: ${result.path}`);
              }
            })
          }
        >
          Создать резервную копию
        </button>
        <button
          disabled={busy || !isTauri() || state?.pending_restore}
          onClick={() =>
            void run(async () => {
              const path = await open({
                title: "Восстановить профиль",
                multiple: false,
                filters: [
                  { name: "Профиль ChessPush", extensions: ["chesspush"] },
                ],
              });
              if (typeof path !== "string") return;
              const result = await previewProfile(path);
              if (mounted.current) setPreview(result);
            })
          }
        >
          Восстановить профиль
        </button>
      </div>
      {busy && <div role="status">Обработка профиля…</div>}
      {message && (
        <div className="profile-message" role="status">
          {message}
        </div>
      )}
      {error && (
        <div className="error" role="alert">
          {error}
        </div>
      )}
      {state?.pending_restore && (
        <div className="profile-pending">
          <strong>Восстановление подготовлено</strong>
          <div className="profile-actions">
            <button
              className="primary"
              disabled={busy}
              onClick={() => void run(restart)}
            >
              Перезапустить и восстановить
            </button>
            <button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  await cancelProfileRestore();
                  if (mounted.current) setState(await profileStatus());
                })
              }
            >
              Отменить восстановление
            </button>
          </div>
        </div>
      )}
      {state?.restore_notice && (
        <div className="profile-message" role="status">
          {state.restore_notice}
        </div>
      )}
      {state?.last_backup && (
        <div className="profile-last-backup">
          <h3>Последняя копия</h3>
          <BackupDetails value={state.last_backup} showPath />
        </div>
      )}
      {preview && (
        <RestoreDialog
          value={preview}
          busy={busy}
          onCancel={() => {
            if (!busy) setPreview(null);
          }}
          onRestore={() =>
            void run(async () => {
              const prepared = await prepareProfileRestore(preview);
              if (mounted.current) {
                setState((previous) => ({
                  last_backup: previous?.last_backup ?? null,
                  restore_notice: `Страховочная копия: ${prepared.safety_backup}`,
                  pending_restore: true,
                }));
                setPreview(null);
              }
              await restart();
            })
          }
        />
      )}
    </section>
  );
}
function BackupDetails({
  value,
  showPath = false,
}: {
  value: BackupSummary;
  showPath?: boolean;
}) {
  return (
    <dl className="profile-details">
      <div>
        <dt>Дата</dt>
        <dd>{date(value.created_at_ms)}</dd>
      </div>
      <div>
        <dt>Версия</dt>
        <dd>ChessPush {value.app_version}</dd>
      </div>
      {showPath && (
        <div>
          <dt>Файл</dt>
          <dd>{value.path}</dd>
        </div>
      )}
      {Object.entries(labels).map(([key, label]) => (
        <div key={key}>
          <dt>{label}</dt>
          <dd>{(value.counts[key] ?? 0).toLocaleString("ru-RU")}</dd>
        </div>
      ))}
      <div>
        <dt>Файлы профиля</dt>
        <dd>
          {value.files.toLocaleString("ru-RU")} · {bytes(value.bytes)}
        </dd>
      </div>
      <div>
        <dt>Внешние источники</dt>
        <dd>{value.external_files.toLocaleString("ru-RU")}</dd>
      </div>
      <div>
        <dt>Доступ Lichess</dt>
        <dd>Не входит в копию</dd>
      </div>
    </dl>
  );
}
function RestoreDialog({
  value,
  busy,
  onCancel,
  onRestore,
}: {
  value: BackupSummary;
  busy: boolean;
  onCancel: () => void;
  onRestore: () => void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    dialog.current?.showModal();
    return () => {
      dialog.current?.close();
      previous?.focus();
    };
  }, []);
  return (
    <dialog
      ref={dialog}
      className="profile-restore-dialog"
      aria-labelledby="profile-restore-title"
      onCancel={(event) => {
        event.preventDefault();
        if (!busy) onCancel();
      }}
      onKeyDownCapture={(event) => {
        if (event.key === "Escape" || event.key === "Tab")
          event.stopPropagation();
      }}
    >
      <h2 id="profile-restore-title">Заменить текущий профиль?</h2>
      <BackupDetails value={value} />
      <p>
        Текущие данные будут заменены. Перед восстановлением сохранится
        страховочная копия. ChessPush перезапустится.
      </p>
      <p>
        Внешние PGN/ZST и доступ Lichess на другом компьютере подключаются
        отдельно.
      </p>
      <div className="profile-actions">
        <button disabled={busy} autoFocus onClick={onCancel}>
          Отмена
        </button>
        <button className="primary" disabled={busy} onClick={onRestore}>
          Восстановить и перезапустить
        </button>
      </div>
    </dialog>
  );
}

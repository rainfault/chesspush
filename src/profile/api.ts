import { invoke } from "@tauri-apps/api/core";

export interface BackupSummary {
  format_version: number;
  created_at_ms: number;
  app_version: string;
  schema_version: number;
  files: number;
  bytes: number;
  external_files: number;
  counts: Record<string, number>;
  sha256: string;
  path: string;
  credentials_included: boolean;
}
export interface PreparedRestore {
  summary: BackupSummary;
  safety_backup: string;
}
export interface ProfileStatus {
  last_backup: BackupSummary | null;
  restore_notice: string | null;
  pending_restore: boolean;
}
export const profileUiKey = (key: string) =>
  [
    "chesspush.workspace.v1",
    "chesspush.repertoire.side",
    "chesspush.repertoire.context",
    "chesspush.practice.filters.v1",
  ].includes(key) ||
  key.startsWith("chesspush.knowledge.draft.") ||
  key.startsWith("chesspush.knowledge.orphan.") ||
  key.startsWith("chesspush.training.");
export function browserProfileState(): Record<string, string> {
  const state: Record<string, string> = {};
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (key && profileUiKey(key)) state[key] = localStorage.getItem(key) ?? "";
  }
  return state;
}
export const backupProfile = (path: string) =>
  invoke<BackupSummary>("profile_backup", {
    path,
    uiState: browserProfileState(),
  });
export const previewProfile = (path: string) =>
  invoke<BackupSummary>("profile_restore_preview", { path });
export const prepareProfileRestore = (summary: BackupSummary) =>
  invoke<PreparedRestore>("profile_restore_prepare", {
    path: summary.path,
    expectedSha256: summary.sha256,
    currentUiState: browserProfileState(),
  });
export const cancelProfileRestore = () =>
  invoke<void>("profile_restore_cancel");
export const restartForProfileRestore = () =>
  invoke<void>("profile_restart_restore");
export const profileStatus = () => invoke<ProfileStatus>("profile_status");

let initialization: Promise<void> | null = null;
/** Call before mounting any workspace that can write browser preferences. */
export function initializeProfileUi(): Promise<void> {
  if (initialization) return initialization;
  initialization = applyProfileUi().finally(() => {
    initialization = null;
  });
  return initialization;
}
async function applyProfileUi() {
  const restored = await invoke<Record<string, string> | null>(
    "profile_restored_ui",
  );
  if (restored === null) return;
  const previous: Record<string, string> = {};
  for (let i = 0; i < localStorage.length; i++) {
    const key = localStorage.key(i);
    if (key?.startsWith("chesspush."))
      previous[key] = localStorage.getItem(key) ?? "";
  }
  if (Object.keys(restored).some((key) => !profileUiKey(key)))
    throw new Error("Недопустимый ключ восстановленного интерфейса");
  const replace = (state: Record<string, string>) => {
    const keys = Array.from({ length: localStorage.length }, (_, i) =>
      localStorage.key(i),
    ).filter((key): key is string => Boolean(key?.startsWith("chesspush.")));
    keys.forEach((key) => localStorage.removeItem(key));
    for (const [key, value] of Object.entries(state)) {
      localStorage.setItem(key, value);
    }
  };
  try {
    replace(restored);
  } catch (error) {
    replace(previous);
    throw error;
  }
  await invoke("profile_restored_ui_ack");
}

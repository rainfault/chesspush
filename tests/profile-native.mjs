// Real application + SQLite/profile commands, including an actual application
// restart. Only native file-dialog answers are supplied by the test.
import { chromium, expect as baseExpect } from "@playwright/test";
import { readFile, writeFile, mkdir, stat } from "node:fs/promises";
import { resolve, join, sep, dirname, basename } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
const expect = baseExpect.configure({ timeout: 20_000 });
const data = process.env.CHESSPUSH_TEST_DATA,
  endpoint = process.env.CHESSPUSH_TEST_CDP;
if (!data || !endpoint || !resolve(data).startsWith(resolve(".qa") + sep))
  throw Error("Use the isolated native runner with a profile inside .qa");
const artifacts = resolve(process.env.CHESSPUSH_TRAINING_ARTIFACTS);
const ownerFile = join(dirname(data), `.${basename(data)}.profile-owner.json`);
const executable =
  process.env.CHESSPUSH_TEST_EXECUTABLE ??
  resolve("src-tauri/target/debug/chesspush.exe");
const run = promisify(execFile);
const ownerPid = async () => JSON.parse(await readFile(ownerFile, "utf8"));
await mkdir(artifacts, { recursive: true });
let browser = await chromium.connectOverCDP(endpoint);
let page = browser.contexts()[0].pages()[0];
const errors = [],
  checks = [];
function observe(target) {
  target.on("pageerror", (error) => errors.push(error.message));
}
observe(page);
async function ready() {
  await page.waitForFunction(
    () =>
      window.__TAURI_INTERNALS__?.invoke &&
      document.querySelector(".app-header"),
  );
  await page.setViewportSize({ width: 1440, height: 960 });
}
await ready();
async function native(command, args = {}) {
  return page.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
}
async function fileDialog(command, result) {
  await page.evaluate(
    ({ command, result }) => {
      const fetchOriginal = window.fetch.bind(window);
      window.fetch = (input, options) => {
        const url = new URL(
          typeof input === "string" ? input : input.url,
          location.href,
        );
        if (
          url.hostname === "ipc.localhost" &&
          decodeURIComponent(url.pathname.slice(1)) === command
        )
          return Promise.resolve(
            new Response(JSON.stringify(result), {
              headers: {
                "Content-Type": "application/json",
                "Tauri-Response": "ok",
              },
            }),
          );
        return fetchOriginal(input, options);
      };
      const webview = window.chrome?.webview;
      if (webview) {
        const postOriginal = webview.postMessage.bind(webview);
        webview.postMessage = (payload) => {
          let message;
          try {
            message =
              typeof payload === "string" ? JSON.parse(payload) : payload;
          } catch {}
          if (message?.cmd === command) {
            queueMicrotask(() =>
              window.__TAURI_INTERNALS__.runCallback(message.callback, result),
            );
            return;
          }
          return postOriginal(payload);
        };
      }
    },
    { command, result },
  );
}
async function settingsData() {
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await page.getByRole("tab", { name: "Данные", exact: true }).click();
  await expect(
    page.getByRole("region", { name: "Данные профиля" }),
  ).toBeVisible();
}
async function reconnect() {
  const deadline = Date.now() + 45_000;
  let last;
  while (Date.now() < deadline) {
    try {
      const next = await chromium.connectOverCDP(endpoint, { timeout: 1000 });
      const candidate = next.contexts()[0]?.pages()[0];
      if (candidate) {
        browser = next;
        page = candidate;
        observe(page);
        await ready();
        return;
      }
      await next.close();
    } catch (error) {
      last = error;
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw Error(`Restarted native profile did not become ready: ${last}`);
}
let restarted = false;
try {
  const originalOwner = await ownerPid();
  expect(Number.isSafeInteger(originalOwner)).toBe(true);
  await run(executable, [], {
    env: process.env,
    windowsHide: true,
    timeout: 15_000,
  });
  expect(await ownerPid()).toBe(originalOwner);
  expect(await native("profile_status")).toBeTruthy();
  checks.push(
    "A second native launch exits cleanly without taking the active profile lock",
  );
  const document = await native("knowledge_import", {
    request: {
      title: "До восстановления",
      kind: "study",
      side: "both",
      pgn: "1. e4 {Сохранённый план. [%cal Ge2e4]} e5 (1... c5) *",
    },
  });
  await page.evaluate(
    ({ id }) => {
      localStorage.setItem("chesspush.repertoire.side", "white");
      localStorage.setItem(
        `chesspush.knowledge.draft.${id}`,
        "Сохранённый черновик",
      );
    },
    { id: document.id },
  );
  await native("set_theme", { theme: "dark" });
  await settingsData();
  const archive = join(artifacts, "profile.chesspush");
  await fileDialog("plugin:dialog|save", archive);
  await page
    .getByRole("button", { name: "Создать резервную копию", exact: true })
    .click();
  await expect(
    page.getByRole("status").filter({ hasText: "Резервная копия сохранена:" }),
  ).toBeVisible();
  expect((await stat(archive)).size).toBeGreaterThan(0);
  const unsafeDestination = join(data, "unsafe.chesspush");
  let unsafeRejected = false;
  try {
    await native("profile_backup", { path: unsafeDestination, uiState: {} });
  } catch {
    unsafeRejected = true;
  }
  expect(unsafeRejected).toBe(true);
  await expect(stat(unsafeDestination)).rejects.toThrow();
  const summary = await native("profile_restore_preview", { path: archive });
  expect(summary.counts.knowledge).toBe(1);
  expect(summary.credentials_included).toBe(false);
  checks.push(
    "Native backup button writes a verified single-file profile with real shared-tree knowledge",
  );
  await page.screenshot({
    path: join(artifacts, "profile-backup-settings.png"),
    fullPage: true,
  });
  await native("knowledge_apply", {
    documentId: document.id,
    expectedRevision: document.revision,
    operation: { type: "set_tags", tags: ["Позже"] },
  });
  await native("knowledge_import", {
    request: {
      title: "После резервной копии",
      kind: "study",
      side: "both",
      pgn: "1. d4 d5 *",
    },
  });
  await native("set_theme", { theme: "light" });
  await page.evaluate(
    ({ id }) => {
      localStorage.setItem("chesspush.repertoire.side", "black");
      localStorage.setItem(
        `chesspush.knowledge.draft.${id}`,
        "Более новый черновик",
      );
      localStorage.setItem(
        "chesspush.knowledge.draft.extra",
        "Не должен остаться",
      );
    },
    { id: document.id },
  );
  // Real prepare/cancel leaves the current profile intact and retains its safety snapshot.
  const prepared = await native("profile_restore_prepare", {
    path: archive,
    expectedSha256: summary.sha256,
    currentUiState: { "chesspush.repertoire.side": "black" },
  });
  expect((await stat(prepared.safety_backup)).size).toBeGreaterThan(0);
  expect((await native("profile_status")).pending_restore).toBe(true);
  await native("profile_restore_cancel");
  expect(
    (await native("knowledge_list")).filter((item) => item.kind === "study"),
  ).toHaveLength(2);
  expect((await native("profile_status")).pending_restore).toBe(false);
  const corrupt = join(artifacts, "corrupt.chesspush");
  const bytes = await readFile(archive);
  await writeFile(corrupt, bytes.subarray(0, Math.floor(bytes.length / 2)));
  let rejected = false;
  try {
    await native("profile_restore_preview", { path: corrupt });
  } catch {
    rejected = true;
  }
  expect(rejected).toBe(true);
  expect((await native("profile_status")).pending_restore).toBe(false);
  checks.push(
    "Prepare creates a consistent safety snapshot; cancel and corrupt archive leave the active profile unchanged",
  );
  await fileDialog("plugin:dialog|open", archive);
  await page
    .getByRole("button", { name: "Восстановить профиль", exact: true })
    .click();
  const dialog = page.getByRole("dialog", {
    name: "Заменить текущий профиль?",
    exact: true,
  });
  await expect(dialog).toBeVisible();
  await page.setViewportSize({ width: 900, height: 700 });
  await page.screenshot({
    path: join(artifacts, "profile-restore-confirmation-900.png"),
    fullPage: true,
  });
  await dialog.getByRole("button", { name: "Отмена", exact: true }).click();
  expect(
    (await native("knowledge_list")).filter((item) => item.kind === "study"),
  ).toHaveLength(2);
  await page.setViewportSize({ width: 1440, height: 960 });
  await page
    .getByRole("button", { name: "Восстановить профиль", exact: true })
    .click();
  await expect(dialog).toBeVisible();
  const disconnected = new Promise((resolve) =>
    browser.once("disconnected", resolve),
  );
  await dialog
    .getByRole("button", { name: "Восстановить и перезапустить", exact: true })
    .click();
  await Promise.race([
    disconnected,
    new Promise((_, reject) =>
      setTimeout(
        () => reject(Error("Old native app did not close for restore")),
        30_000,
      ),
    ),
  ]);
  await reconnect();
  restarted = true;
  const restoredOwner = await ownerPid();
  expect(restoredOwner).not.toBe(originalOwner);
  await run(executable, [], {
    env: process.env,
    windowsHide: true,
    timeout: 15_000,
  });
  expect(await ownerPid()).toBe(restoredOwner);
  const restored = (await native("knowledge_list")).filter(
    (item) => item.kind === "study",
  );
  expect(restored.map((item) => item.id)).toEqual([document.id]);
  const loaded = await native("knowledge_load", {
    documentId: document.id,
    revision: null,
  });
  expect(loaded.revision).toBe(document.revision);
  expect(loaded.chapters).toEqual(document.chapters);
  expect(loaded.metadata.tags ?? []).toEqual([]);
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  const ui = await page.evaluate(
    ({ id }) => ({
      side: localStorage.getItem("chesspush.repertoire.side"),
      draft: localStorage.getItem(`chesspush.knowledge.draft.${id}`),
      extra: localStorage.getItem("chesspush.knowledge.draft.extra"),
    }),
    { id: document.id },
  );
  expect(ui).toEqual({
    side: "white",
    draft: "Сохранённый черновик",
    extra: null,
  });
  expect((await native("profile_status")).pending_restore).toBe(false);
  expect((await native("profile_status")).restore_notice).toContain(
    "Профиль восстановлен",
  );
  expect(await native("profile_restored_ui")).toBeNull();
  await settingsData();
  await expect(
    page.getByRole("status").filter({ hasText: "Профиль восстановлен." }),
  ).toBeVisible();
  await page.screenshot({
    path: join(artifacts, "profile-restored-settings.png"),
    fullPage: true,
  });
  checks.push(
    "Actual cooperative restart applies the snapshot before DB workers, restores original IDs/tree/revision/theme/UI drafts and acknowledges browser bootstrap",
  );
  expect(errors).toEqual([]);
  await writeFile(
    join(artifacts, "profile-native-results.json"),
    JSON.stringify({ pass: true, checks, errors }, null, 2),
  );
  console.log(
    `PASS full profile backup/restore native integration; artifacts: ${artifacts}`,
  );
} catch (error) {
  await page
    .screenshot({
      path: join(artifacts, "profile-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  await writeFile(
    join(artifacts, "profile-failure.txt"),
    await page
      .locator("body")
      .innerText()
      .catch(() => ""),
  ).catch(() => {});
  throw error;
} finally {
  // The shared harness owns the original PID. Close the restarted descendant
  // through Tauri so this disposable native profile cannot outlive the test.
  if (restarted && page && !page.isClosed())
    await native("plugin:window|close", { label: "main" }).catch(() => {});
  await browser.close().catch(() => {});
  // A restarted descendant inherits the harness pipes. Reap only the process
  // named by this disposable profile, after checking its executable identity.
  if (restarted) {
    const pid = await ownerPid().catch(() => null);
    if (Number.isSafeInteger(pid) && pid > 0) {
      const expected = resolve(executable)
        .replace(/^\\\\\?\\/, "")
        .replaceAll("'", "''");
      await run(
        "powershell.exe",
        [
          "-NoProfile",
          "-Command",
          `$profileTestProcess=Get-Process -Id ${pid} -ErrorAction SilentlyContinue; if($profileTestProcess){if($profileTestProcess.Path -ne '${expected}'){throw 'Unexpected test process identity'}; [void]$profileTestProcess.CloseMainWindow(); if(-not $profileTestProcess.WaitForExit(10000)){$profileTestProcess.Kill($true); $profileTestProcess.WaitForExit()}}`,
        ],
        { windowsHide: true },
      );
    }
  }
}

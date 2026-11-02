// Current React/Vite source: StrictMode boot, recoverable browser-storage errors,
// and the backup UI. Native archive/restart coverage lives in profile-native.
import { chromium, expect } from "@playwright/test";
import { createServer } from "vite";
import { mkdir, writeFile } from "node:fs/promises";
const artifacts = ".qa/profile-ui";
await mkdir(artifacts, { recursive: true });
const vite = await createServer({
  cacheDir: ".qa/profile-ui-vite-cache",
  server: { host: "127.0.0.1", port: 0, strictPort: false },
});
await vite.listen();
const browser = await chromium.launch({ channel: "msedge", headless: true });
const errors = [];
async function open(mode = "normal") {
  const page = await browser.newPage({
    viewport: { width: 1440, height: 960 },
  });
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(
    ({ mode }) => {
      window.isTauri = true;
      window.__profileCalls = [];
      const calls = window.__profileCalls;
      const summary = {
        format_version: 1,
        created_at_ms: Date.UTC(2026, 10, 2),
        app_version: "0.2.0-alpha.11",
        schema_version: 11,
        files: 12,
        bytes: 1234567,
        external_files: 2,
        counts: {
          games: 1234,
          knowledge: 18,
          repertoire: 2,
          reviews: 299,
          sessions: 12,
        },
        sha256: "synthetic",
        credentials_included: false,
        path: "C:\\Users\\Example\\Documents\\ChessPush backups\\Личный профиль подготовки и знания\\Резервная копия профиля ChessPush-2026-11-02.chesspush",
      };
      const restored = {
        "chesspush.repertoire.side": "white",
        "chesspush.knowledge.draft.fixture": "restored draft",
      };
      localStorage.setItem("chesspush.repertoire.side", "black");
      localStorage.setItem("chesspush.future.extra", "old value");
      localStorage.setItem("another.application", "keep");
      let reads = 0,
        active = false,
        failedQuota = false;
      const originalSet = Storage.prototype.setItem;
      if (mode === "quota")
        Storage.prototype.setItem = function (key, value) {
          if (!failedQuota && value === "restored draft") {
            failedQuota = true;
            throw new DOMException(
              "Synthetic quota exceeded",
              "QuotaExceededError",
            );
          }
          return originalSet.call(this, key, value);
        };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
      window.__TAURI_INTERNALS__ = {
        transformCallback() {
          return 1;
        },
        async invoke(command) {
          calls.push(command);
          if (command === "profile_restored_ui") {
            if (active) throw Error("Concurrent profile bootstrap");
            active = true;
            reads++;
            await new Promise((resolve) => setTimeout(resolve, 30));
            active = false;
            if (mode === "retry" && reads === 1)
              throw Error("Synthetic read failure");
            return restored;
          }
          if (command === "profile_restored_ui_ack") {
            if (
              localStorage.getItem("chesspush.knowledge.draft.fixture") !==
              "restored draft"
            )
              throw Error("Premature profile UI acknowledgement");
            return;
          }
          if (command.startsWith("plugin:event|")) return 1;
          if (
            ["statistics_recent", "knowledge_list", "library_files"].includes(
              command,
            )
          )
            return [];
          if (command === "get_theme") return "light";
          if (command === "account_status")
            return {
              status: "disconnected",
              username: null,
              rapid: null,
              blitz: null,
              message: null,
              hasToken: false,
            };
          if (command === "profile_status")
            return {
              last_backup: summary,
              restore_notice: null,
              pending_restore: false,
            };
          if (command === "plugin:dialog|open") return summary.path;
          if (command === "profile_restore_preview") return summary;
          throw Error(`Unexpected profile UI command: ${command}`);
        },
      };
    },
    { mode },
  );
  await page.goto(`http://127.0.0.1:${vite.httpServer.address().port}`);
  return page;
}
try {
  const page = await open();
  await expect(page.locator(".app-header")).toBeVisible();
  expect(
    await page.evaluate(
      () =>
        window.__profileCalls.filter((c) => c === "profile_restored_ui").length,
    ),
  ).toBe(1);
  expect(
    await page.evaluate(
      () =>
        window.__profileCalls.filter((c) => c === "profile_restored_ui_ack")
          .length,
    ),
  ).toBe(1);
  expect(
    await page.evaluate(() => ({
      side: localStorage.getItem("chesspush.repertoire.side"),
      extra: localStorage.getItem("chesspush.future.extra"),
      other: localStorage.getItem("another.application"),
    })),
  ).toEqual({ side: "white", extra: null, other: "keep" });
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await page.getByRole("tab", { name: "Данные", exact: true }).click();
  await expect(
    page.getByText("Не входит в копию", { exact: true }),
  ).toBeVisible();
  const file = page
    .locator(".profile-last-backup dd")
    .filter({ hasText: "C:\\Users\\Example" });
  await expect(file).toBeVisible();
  await page.setViewportSize({ width: 900, height: 700 });
  expect(await file.evaluate((e) => e.scrollWidth <= e.clientWidth)).toBe(true);
  await page.screenshot({
    path: `${artifacts}/profile-last-copy-path-900.png`,
    fullPage: true,
  });
  await page
    .getByRole("button", { name: "Восстановить профиль", exact: true })
    .click();
  const confirmation = page.getByRole("dialog", {
    name: "Заменить текущий профиль?",
    exact: true,
  });
  await expect(confirmation).toBeVisible();
  const commit = confirmation.getByRole("button", {
    name: "Восстановить и перезапустить",
    exact: true,
  });
  const bounds = await commit.boundingBox();
  expect(bounds.y + bounds.height).toBeLessThanOrEqual(700);
  await page.screenshot({
    path: `${artifacts}/profile-confirmation-900.png`,
    fullPage: true,
  });
  await page.keyboard.press("Escape");
  await expect(confirmation).toHaveCount(0);
  await expect(
    page.getByRole("dialog", { name: "Настройки", exact: true }),
  ).toBeVisible();
  await page.evaluate(() => (document.documentElement.dataset.theme = "dark"));
  await page.screenshot({
    path: `${artifacts}/profile-last-copy-dark-900.png`,
    fullPage: true,
  });
  await page.close();
  for (const mode of ["retry", "quota"]) {
    const retry = await open(mode);
    await expect(
      retry.getByRole("button", { name: "Повторить", exact: true }),
    ).toBeVisible();
    expect(
      await retry.evaluate(() =>
        window.__profileCalls.includes("profile_restored_ui_ack"),
      ),
    ).toBe(false);
    expect(
      await retry.evaluate(() =>
        localStorage.getItem("chesspush.repertoire.side"),
      ),
    ).toBe("black");
    expect(
      await retry.evaluate(() =>
        localStorage.getItem("chesspush.future.extra"),
      ),
    ).toBe("old value");
    await retry.getByRole("button", { name: "Повторить", exact: true }).click();
    await expect(retry.locator(".app-header")).toBeVisible();
    expect(
      await retry.evaluate(
        () =>
          window.__profileCalls.filter((c) => c === "profile_restored_ui")
            .length,
      ),
    ).toBe(2);
    expect(
      await retry.evaluate(
        () =>
          window.__profileCalls.filter((c) => c === "profile_restored_ui_ack")
            .length,
      ),
    ).toBe(1);
    await retry.close();
  }
  expect(errors).toEqual([]);
  await writeFile(
    `${artifacts}/results.json`,
    JSON.stringify(
      {
        pass: true,
        checks: [
          "StrictMode shares one bootstrap",
          "Read and quota failures retry without premature ACK",
          "Failed browser replace rolls back all prior preferences",
          "Restored unknown keys cleared; unrelated browser keys retained",
          "Long archive path and confirmation controls fit at 900",
          "Escape dismisses restore dialog while retaining Settings",
        ],
        errors,
      },
      null,
      2,
    ),
  );
  console.log(
    `PASS profile React UI and StrictMode bootstrap; artifacts: ${artifacts}`,
  );
} finally {
  await browser.close();
  await vite.close();
}

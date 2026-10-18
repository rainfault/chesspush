// Real React Settings, synthetic IPC only. This suite never opens a native user profile.
import { chromium, expect } from "@playwright/test";
import { createServer } from "vite";
import { mkdir, writeFile } from "node:fs/promises";
const output = ".qa/statistics-settings-ui";
await mkdir(output, { recursive: true });
const server = await createServer({
  server: { host: "127.0.0.1", port: 1437, strictPort: true },
});
await server.listen();
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1100, height: 900 } });
const errors = [],
  scenarios = [];
page.on("pageerror", (error) => errors.push(error.message));
await page.addInitScript(() => {
  const callbacks = new Map(),
    listeners = new Map(),
    calls = [];
  let id = 0,
    depth = 15,
    theme = "light",
    failReset = false;
  let accounts = [
    {
      id: "lichess:settingsone",
      external_id: "settingsone",
      username: "SettingsOne",
      t0_ms: 1000,
      enabled: true,
      generation: 1,
    },
    {
      id: "lichess:settingstwo",
      external_id: "settingstwo",
      username: "SettingsTwo",
      t0_ms: 2000,
      enabled: false,
      generation: 1,
    },
  ];
  window.isTauri = true;
  window.__settingsTest = {
    calls,
    failReset: (value) => (failReset = value),
    clearAccounts: () => (accounts = []),
  };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(_event, key) {
      listeners.delete(key);
    },
  };
  window.__TAURI_INTERNALS__ = {
    transformCallback(callback) {
      const key = ++id;
      callbacks.set(key, callback);
      return key;
    },
    async invoke(command, args) {
      calls.push({ command, args });
      switch (command) {
        case "plugin:event|listen": {
          const key = ++id;
          listeners.set(key, args);
          return key;
        }
        case "plugin:event|unlisten":
          listeners.delete(args.eventId);
          return;
        case "get_theme":
          return theme;
        case "set_theme":
          theme = args.theme;
          return;
        case "account_status":
          return {
            id: "settingsone",
            status: "connected",
            username: "SettingsOne",
            rapid: 1900,
            blitz: 1850,
            hasToken: true,
            message: null,
          };
        case "knowledge_list":
        case "statistics_recent":
        case "library_files":
          return [];
        case "statistics_state":
          return {
            accounts,
            active_account_id: accounts[0]?.id ?? null,
            sync: null,
            engine: { configured: false, jobs: [], paused: false },
            quarantine: [],
          };
        case "statistics_preferences":
          return { opening_coverage_depth_moves: depth };
        case "statistics_preferences_save":
          depth = args.preferences.opening_coverage_depth_moves;
          return { opening_coverage_depth_moves: depth };
        case "statistics_reset": {
          if (failReset)
            throw { code: "network", message: "Тестовая ошибка сброса" };
          const account = accounts.find(
            (account) => account.id === args.accountId,
          );
          if (!account) throw Error("Unknown synthetic account");
          account.t0_ms = 20000;
          return { account_id: account.id, t0_ms: 20000, removed_games: 3 };
        }
        default:
          throw Error(`Unexpected IPC ${command}`);
      }
    },
  };
});
try {
  await page.goto("http://127.0.0.1:1437");
  const settings = () =>
    page.getByRole("dialog", { name: "Настройки", exact: true });
  const open = async () => {
    await page.getByRole("button", { name: "Настройки", exact: true }).click();
    await settings()
      .getByRole("tab", { name: "Статистика", exact: true })
      .click();
    await expect(
      page.getByLabel("Глубина покрытия дебютов, ходов", { exact: true }),
    ).toBeEnabled();
  };
  const resets = () =>
    page.evaluate(() =>
      window.__settingsTest.calls.filter(
        (call) => call.command === "statistics_reset",
      ),
    );
  await open();
  const depth = page.getByLabel("Глубина покрытия дебютов, ходов", {
    exact: true,
  });
  await expect(depth).toHaveValue("15");
  const save = page.getByRole("button", {
    name: "Сохранить глубину",
    exact: true,
  });
  await expect(save).toBeDisabled();
  for (const invalid of ["0", "41", "1.5", ""]) {
    await depth.fill(invalid);
    await expect(save).toBeDisabled();
    await expect(depth).toHaveAttribute("aria-invalid", "true");
    await depth.press("Enter");
  }
  expect(
    await page.evaluate(() =>
      window.__settingsTest.calls.filter(
        (call) => call.command === "statistics_preferences_save",
      ),
    ),
  ).toHaveLength(0);
  await depth.fill("17");
  await depth.press("Enter");
  await expect(page.getByRole("status")).toContainText(
    "Глубина покрытия сохранена",
  );
  expect(
    await page.evaluate(() =>
      window.__settingsTest.calls.filter(
        (call) => call.command === "statistics_preferences_save",
      ),
    ),
  ).toEqual([
    {
      command: "statistics_preferences_save",
      args: { preferences: { opening_coverage_depth_moves: 17 } },
    },
  ]);
  scenarios.push("Depth default, min/max/integer validation, Enter saves once");
  await page
    .getByRole("button", { name: "Сбросить статистику", exact: true })
    .click();
  const confirmation = page.getByRole("alertdialog", {
    name: "Сбросить статистику SettingsOne?",
    exact: true,
  });
  await expect(confirmation).toContainText("Начать заново с текущего момента");
  await confirmation
    .getByRole("button", { name: "Отмена", exact: true })
    .click();
  expect(await resets()).toHaveLength(0);
  await page
    .getByLabel("Аккаунт статистики", { exact: true })
    .selectOption("lichess:settingstwo");
  await page
    .getByRole("button", { name: "Сбросить статистику", exact: true })
    .click();
  await expect(page.getByRole("alertdialog")).toHaveAccessibleName(
    "Сбросить статистику SettingsTwo?",
  );
  await page
    .getByRole("button", { name: "Сбросить статистику аккаунта", exact: true })
    .click();
  await expect(page.getByRole("status")).toContainText(
    "Статистика SettingsTwo сброшена",
  );
  expect(await resets()).toEqual([
    { command: "statistics_reset", args: { accountId: "lichess:settingstwo" } },
  ]);
  scenarios.push(
    "Explicit confirmation, cancel has no reset IPC, selected account identity",
  );
  await page.evaluate(() => window.__settingsTest.failReset(true));
  await page
    .getByRole("button", { name: "Сбросить статистику", exact: true })
    .click();
  await page
    .getByRole("button", { name: "Сбросить статистику аккаунта", exact: true })
    .click();
  await expect(page.getByRole("alert")).toContainText("Тестовая ошибка сброса");
  await expect(page.getByRole("status")).toHaveCount(0);
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await page.screenshot({ path: output + "/light-error.png" });
  await page.getByRole("button", { name: "Отмена", exact: true }).click();
  scenarios.push("Reset error retains confirmation and never reports success");
  await page
    .getByRole("button", { name: "Закрыть настройки", exact: true })
    .click();
  await open();
  await expect(depth).toHaveValue("17");
  await settings()
    .getByRole("tab", { name: "Внешний вид", exact: true })
    .click();
  await page.getByLabel("Тема", { exact: true }).selectOption("dark");
  await settings()
    .getByRole("tab", { name: "Статистика", exact: true })
    .click();
  await expect(depth).toHaveValue("17");
  await page.setViewportSize({ width: 900, height: 700 });
  await page.screenshot({ path: output + "/dark-settings.png" });
  expect(
    await settings().evaluate(
      (element) => element.getBoundingClientRect().right,
    ),
  ).toBeLessThanOrEqual(900);
  await page
    .getByRole("button", { name: "Закрыть настройки", exact: true })
    .click();
  await page.evaluate(() => window.__settingsTest.clearAccounts());
  await open();
  await expect(settings()).toContainText("Аккаунт не подключён");
  await expect(
    page.getByRole("button", { name: "Сбросить статистику", exact: true }),
  ).toHaveCount(0);
  scenarios.push(
    "Remount retains saved depth; dark/narrow Settings; no reset without observation",
  );
  expect(errors).toEqual([]);
  await writeFile(
    output + "/results.json",
    JSON.stringify(
      {
        result: "PASS",
        scenarios,
        pageErrors: errors,
        boundary: "synthetic IPC, no native user profile",
      },
      null,
      2,
    ),
  );
  console.log(
    JSON.stringify({
      result: "PASS",
      scenarios: scenarios.length,
      pageErrors: errors.length,
    }),
  );
} finally {
  await browser.close();
  await server.close();
}

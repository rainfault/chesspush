// Browser interaction checks against the real React UI with a synthetic IPC boundary.
// No account credentials or requests to Lichess are used here.
import { chromium, expect } from "@playwright/test";
import { mkdir } from "node:fs/promises";
await mkdir(".qa", { recursive: true });
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
await page.addInitScript(() => {
  let account = {
    status: "disconnected",
    username: null,
    rapid: null,
    blitz: null,
    message: null,
    hasToken: false,
  };
  const callbacks = new Map();
  const listeners = new Map();
  const calls = [];
  let next = 1;
  let theme = "light";
  window.isTauri = true;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener(_event, id) {
      listeners.delete(id);
    },
  };
  window.__testAccount = (value) => {
    account = value;
    for (const entry of listeners.values())
      if (entry.event === "lichess-account-updated")
        callbacks.get(entry.handler)?.({ payload: value });
  };
  window.__testCalls = calls;
  window.__TAURI_INTERNALS__ = {
    transformCallback(callback) {
      const id = next++;
      callbacks.set(id, callback);
      return id;
    },
    async invoke(command, args) {
      calls.push(command);
      switch (command) {
        case "plugin:event|listen": {
          const id = next++;
          listeners.set(id, args);
          return id;
        }
        case "plugin:event|unlisten":
          listeners.delete(args.eventId);
          return;
        case "get_theme":
          return theme;
        case "set_theme":
          theme = args.theme;
          return;
        case "statistics_recent":
        case "knowledge_list":
        case "library_files":
          return [];
        case "statistics_engine_config":
          return {
            path: "",
            threads: 1,
            hash_mb: 128,
            profile: "standard",
            custom_nodes: null,
            custom_movetime_ms: null,
            syzygy_path: null,
          };
        case "account_status":
          return account;
        case "start_lichess_auth":
          account = { ...account, status: "authenticating" };
          return account;
        case "cancel_lichess_auth":
          account = { ...account, status: "disconnected" };
          return account;
        case "connect_lichess_token":
          account = {
            status: "connected",
            username: "DemoPlayer",
            rapid: 1950,
            blitz: 1900,
            message: null,
            hasToken: true,
          };
          return account;
        case "disconnect_lichess":
          account = {
            status: "disconnected",
            username: null,
            rapid: null,
            blitz: null,
            message: null,
            hasToken: false,
          };
          return account;
        default:
          throw new Error(`Unexpected IPC command: ${command}`);
      }
    },
  };
});
try {
  await page.goto("http://127.0.0.1:1420");
  await expect(page.getByLabel("Текущее время")).toHaveText(/^\d{2}:\d{2}$/);
  await expect(page.locator(".home-rating-value")).toHaveCount(0);

  await page
    .getByRole("button", { name: "Войти через Lichess", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Ожидание входа…", exact: true }),
  ).toBeDisabled();
  await page
    .getByRole("button", { name: "Отменить вход", exact: true })
    .click();
  await expect(
    page.getByRole("button", { name: "Войти через Lichess", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await expect(
    page.getByRole("dialog", { name: "Настройки", exact: true }),
  ).toContainText("Аккаунт не подключён");
  await page.getByText("Подключить вручную по токену", { exact: true }).click();
  await expect(page.getByLabel("Токен", { exact: true })).toHaveAttribute(
    "type",
    "password",
  );
  await page
    .getByLabel("Токен", { exact: true })
    .fill("synthetic_test_credential");
  await page.getByRole("button", { name: "Подключить", exact: true }).click();
  await expect(page.getByLabel("Токен", { exact: true })).toHaveValue("");
  await expect(
    page.getByRole("dialog", { name: "Настройки", exact: true }),
  ).toContainText("Аккаунт: DemoPlayer");
  await page
    .getByRole("button", { name: "Закрыть настройки", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Rapid 1950", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("region", { name: "Blitz 1900", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".home time")).toHaveCount(1);
  await expect(page.locator(".home-resume,.home-material")).toHaveCount(0);
  const clockSize = await page
    .getByLabel("Текущее время")
    .evaluate((e) => parseFloat(getComputedStyle(e).fontSize));
  expect(clockSize).toBeGreaterThanOrEqual(80);
  expect(clockSize).toBeLessThanOrEqual(112);
  await page.screenshot({ path: ".qa/home-light.png" });
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await page.getByRole("tab", { name: "Внешний вид", exact: true }).click();
  await page.getByLabel("Тема", { exact: true }).selectOption("dark");
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.screenshot({ path: ".qa/home-dark.png" });
  await page.evaluate(() =>
    window.__testAccount({
      status: "offline",
      username: "DemoPlayer",
      rapid: 1950,
      blitz: 1900,
      message: null,
      hasToken: true,
    }),
  );
  await expect(page.locator(".home-rating-value")).toHaveCount(0);
  await expect(
    page.getByRole("button", { name: "Войти через Lichess", exact: true }),
  ).toBeVisible();
  await page.screenshot({ path: ".qa/home-offline.png" });
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await page
    .getByRole("button", { name: "Отключить аккаунт", exact: true })
    .click();
  await expect(
    page.getByRole("dialog", { name: "Настройки", exact: true }),
  ).toContainText("Аккаунт не подключён");
  await page.getByText("Подключить вручную по токену", { exact: true }).click();
  await expect(page.getByLabel("Токен", { exact: true })).toHaveValue("");
  await page.screenshot({ path: ".qa/settings-account.png" });
  const calls = await page.evaluate(() => window.__testCalls);
  for (const command of [
    "start_lichess_auth",
    "cancel_lichess_auth",
    "connect_lichess_token",
    "disconnect_lichess",
  ])
    if (!calls.includes(command)) throw new Error(`Missing command ${command}`);
  if (errors.length) throw new Error(errors.join("\n"));
  console.log(
    "PASS: restored clock/rating Home, scoped clock typography, browser login/cancel, masked manual token, offline, two themes, disconnect",
  );
} finally {
  await browser.close();
}

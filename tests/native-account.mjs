// Native account IPC verification, only with the isolated profile created by check-native.
import { chromium, expect } from "@playwright/test";
const endpoint = process.env.CHESSPUSH_CDP;
if (!endpoint || !process.env.CHESSPUSH_TEST_DATA)
  throw new Error("An isolated native test profile is required");
const browser = await chromium.connectOverCDP(endpoint);
const page = browser.contexts()[0].pages()[0];
const invoke = (command, args = {}) =>
  page.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
try {
  const account = await invoke("account_status");
  if (
    account.status !== "disconnected" ||
    account.hasToken ||
    account.rapid !== null ||
    account.blitz !== null
  )
    throw new Error("Fresh profile must have no account");
  if (
    Object.keys(account).some(
      (key) => key.toLowerCase() === "token" || key.includes("access_token"),
    )
  )
    throw new Error("Credential exposed by account DTO");
  await page.getByRole("button", { name: "Главная", exact: true }).click();
  await expect(page.getByLabel("Текущее время")).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Войти через Lichess", exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await page
    .getByLabel("Токен", { exact: true })
    .fill("invalid synthetic credential");
  await page.getByRole("button", { name: "Подключить", exact: true }).click();
  await expect(page.getByRole("dialog").getByRole("alert")).toHaveText(
    "Некорректный токен Lichess",
  );
  await expect(page.getByLabel("Токен", { exact: true })).toHaveValue("");
  await page
    .getByRole("button", { name: "Закрыть настройки", exact: true })
    .click();
  const cancelled = await invoke("cancel_lichess_auth");
  if (cancelled.status !== "disconnected")
    throw new Error("Native cancellation failed");
  const disconnected = await invoke("disconnect_lichess");
  if (disconnected.hasToken || disconnected.username !== null)
    throw new Error("Native disconnect failed");
  console.log(
    "PASS: native account IPC, fresh profile, Home, masked token validation/clearing, cancellation and disconnect; no personal credentials used",
  );
} finally {
  await browser.close();
}

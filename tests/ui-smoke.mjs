// Own application only. Run against Vite, or set CHESSPUSH_CDP for a test Tauri window.
import { chromium } from "@playwright/test";
import { mkdir } from "node:fs/promises";
await mkdir(".qa", { recursive: true });
const native = !!process.env.CHESSPUSH_CDP;
const browser = native
  ? await chromium.connectOverCDP(process.env.CHESSPUSH_CDP)
  : await chromium.launch({ channel: "msedge", headless: true });
const page = native
  ? browser.contexts()[0].pages()[0]
  : await browser.newPage({
      viewport: { width: 1440, height: 1000 },
      deviceScaleFactor: 1.25,
    });
page.setDefaultTimeout(10000);
page.on("console", (message) => {
  if (message.type() === "error") errors.push(message.text());
});
process.on("unhandledRejection", async (error) => {
  console.error(error);
  await page.screenshot({ path: ".qa/failure.png" });
  await browser.close();
  process.exit(1);
});
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
if (!native) await page.goto("http://127.0.0.1:1420");
await page.locator("cg-board").waitFor();
await page.getByRole("button", { name: "Настройки", exact: true }).click();
await page.getByLabel("Тема", { exact: true }).selectOption("light");
await page
  .getByRole("button", { name: "Закрыть настройки", exact: true })
  .click();
const board = page.locator(".board-frame > .cg-wrap");
async function square(key) {
  const b = await board.boundingBox();
  return {
    x: b.x + ((key.charCodeAt(0) - 97 + 0.5) * b.width) / 8,
    y: b.y + ((8 - Number(key[1]) + 0.5) * b.height) / 8,
  };
}
async function click(key) {
  const p = await square(key);
  await page.mouse.click(p.x, p.y);
}
async function drag(from, to, button = "left") {
  const a = await square(from),
    b = await square(to);
  await page.mouse.move(a.x, a.y);
  await page.mouse.down({ button });
  await page.mouse.move(b.x, b.y, { steps: 15 });
  await page.mouse.up({ button });
}
async function waitMoves(text) {
  await page
    .getByLabel("Ходы", { exact: true })
    .getByRole("button", { name: text, exact: true })
    .waitFor();
}
async function loadFen(fen) {
  await page.getByRole("button", { name: "FEN", exact: true }).click();
  await page.getByLabel("FEN позиции").fill(fen);
  await page.getByRole("button", { name: "Применить", exact: true }).click();
}
await click("e2");
await click("e4");
await waitMoves("1. e4");
console.log("click passed");
await drag("e7", "e5");
await waitMoves("e5");
console.log("drag passed");
await drag("g1", "f3", "right");
const d4 = await square("d4");
await page.mouse.click(d4.x, d4.y, { button: "right" });
await page.waitForFunction(
  () => document.querySelectorAll("svg.cg-shapes g > *").length >= 2,
);
await page
  .getByRole("button", { name: "Очистить стрелки", exact: true })
  .click();
await page.waitForFunction(
  () => document.querySelectorAll("svg.cg-shapes g > *").length === 0,
);
await page.getByRole("button", { name: "Назад", exact: true }).click();
await page.getByRole("button", { name: "Вперёд", exact: true }).click();
await page.getByRole("button", { name: "Главная", exact: true }).click();
await page.getByRole("button", { name: "Базы", exact: true }).click();
await waitMoves("e5");
await loadFen("INVALID");
const fenDialog = page.getByRole("dialog", { name: "FEN", exact: true });
const validation = fenDialog.getByRole("alert");
await validation.waitFor();
const unobstructed = await validation.evaluate((el) => {
  const bounds = el.getBoundingClientRect();
  return (
    document
      .elementFromPoint(
        bounds.x + bounds.width / 2,
        bounds.y + bounds.height / 2,
      )
      ?.closest('[role="alert"]') === el
  );
});
if (!unobstructed)
  throw new Error("FEN validation error is hidden behind the editor");
await fenDialog.getByRole("button", { name: "Закрыть", exact: true }).click();
await loadFen("7k/P7/8/8/8/8/8/7K w - - 0 1");
await drag("a7", "a8");
await page.getByRole("button", { name: "knight", exact: true }).click();
await waitMoves("1. a8=N");
await page.getByRole("button", { name: "Сбросить доску", exact: true }).click();
await page.getByLabel("Найти дебют").fill("caro kann");
await page.locator(".opening-results button").first().click();
await page.waitForTimeout(250);
await page.screenshot({ path: ".qa/light.png" });
await page.getByRole("button", { name: "Настройки", exact: true }).click();
await page.getByLabel("Тема", { exact: true }).selectOption("dark");
await page
  .getByRole("button", { name: "Закрыть настройки", exact: true })
  .click();
await page.screenshot({ path: ".qa/dark.png" });
if (!native) await page.setViewportSize({ width: 960, height: 760 });
await page.screenshot({ path: ".qa/compact.png" });
if (errors.length) throw new Error(errors.join("\n"));
console.log(
  "PASS: click, drag, arrows, circles, clear, seek, mounted navigation, visible FEN validation, promotion, openings, themes, resize",
);
if (!native) await browser.close();
else await browser.close();

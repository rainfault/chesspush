import { expect } from "@playwright/test";
import { readFile, writeFile, copyFile } from "node:fs/promises";
import { resolve } from "node:path";
import { bridge } from "./helpers/product-bridge.mjs";
const config = JSON.parse(await readFile(".qa/ui-atlas-tools/current.json")),
  h = await bridge(config, "light", {
    videoDirectory: "screens/product-ux/videos",
  }),
  p = h.page;
let result;
try {
  await p
    .locator(".app-header")
    .getByRole("button", { name: "Библиотека", exact: true })
    .click();
  await p
    .getByRole("button", { name: "Сохранённые файлы", exact: true })
    .click();
  await p.evaluate(
    (path) => (window.__atlas.responses["plugin:dialog|open"] = path),
    resolve("tests/fixtures/corpus.pgn"),
  );
  await p.getByRole("button", { name: "Импорт PGN", exact: true }).click();
  await expect(
    p.getByRole("button", { name: "Открыть партии", exact: true }),
  ).toBeVisible();
  await p.screenshot({
    path: "screens/product-ux/after/light-1440x960-library-result.png",
  });
  await p.getByRole("button", { name: "Открыть партии", exact: true }).click();
  await expect(p.locator(".library-browser li button").first()).toBeVisible();
  await p.locator(".library-browser li button").first().click();
  await expect(p.locator(".library-game cg-board")).toBeVisible();
  await p.getByRole("button", { name: "Следующий ход", exact: true }).click();
  await p.screenshot({
    path: "screens/product-ux/after/light-1440x960-library-game.png",
  });
  await p.keyboard.press("Escape");
  await expect(
    p.getByRole("dialog", { name: "Партии библиотеки" }),
  ).toBeHidden();
  await p
    .locator(".app-header")
    .getByRole("button", { name: "Моя игра", exact: true })
    .click();
  await p.getByRole("tab", { name: "Карта игры", exact: true }).click();
  await expect(p.locator(".continuation-chip").first()).toBeVisible();
  const root = p.getByRole("button", { name: "К началу", exact: true });
  if (await root.isEnabled()) await root.click();
  await expect(
    p.getByRole("button", { name: "Белые: e4", exact: true }),
  ).toBeEnabled();
  await p.getByRole("button", { name: "Белые: e4", exact: true }).hover();
  await p.waitForTimeout(350);
  await p.getByRole("button", { name: "Белые: e4", exact: true }).click();
  await expect(
    p.getByRole("button", { name: "Чёрные: e5", exact: true }),
  ).toBeEnabled();
  const b = await p.locator(".graph-board cg-board").boundingBox(),
    cell = b.width / 8;
  await p.mouse.click(b.x + 4.5 * cell, b.y + 1.5 * cell);
  await p.mouse.click(b.x + 4.5 * cell, b.y + 3.5 * cell);
  await expect(p.locator(".graph-path")).toContainText("e5");
  await p.getByRole("button", { name: "Назад по ветви", exact: true }).click();
  await expect(
    p.getByRole("button", { name: "Партии хода e5", exact: true }),
  ).toBeEnabled();
  await p.getByRole("button", { name: "Партии хода e5", exact: true }).click();
  await p
    .locator(".graph-evidence")
    .getByRole("button", { name: "Открыть партию", exact: true })
    .first()
    .click();
  await expect(
    p.getByRole("region", { name: "Просмотр партии" }),
  ).toBeVisible();
  await p.screenshot({
    path: "screens/product-ux/after/light-1440x960-branch-game.png",
  });
  await p.getByRole("button", { name: "Вернуться", exact: true }).click();
  await expect(p.locator(".graph-path")).toContainText("e4");
  result = {
    result: "PASS",
    scenarios: [
      "actual parser import→stored library list→shared Rust replay→seek",
      "vertex→board→back→edge game→exact context return",
    ],
    controlledBoundary:
      "native file picker selection only; PGN/parser/storage/graph/viewer are actual isolated Rust backend",
  };
} catch (e) {
  result = { result: "FAIL", error: String(e) };
  console.log((await p.locator("body").innerText()).slice(-3000));
  throw e;
} finally {
  const video = await p.video()?.path();
  await h.browser.close();
  if (video)
    await copyFile(video, "screens/product-ux/videos/map-board-game.webm");
  await writeFile(
    "screens/product-ux/scenario-results.json",
    JSON.stringify(result, null, 2),
  );
}

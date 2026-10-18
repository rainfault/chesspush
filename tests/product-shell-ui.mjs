import { expect } from "@playwright/test";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { bridge } from "./helpers/product-bridge.mjs";
const config = process.env.CHESSPUSH_CDP
  ? { cdp: process.env.CHESSPUSH_CDP }
  : JSON.parse(await readFile(".qa/ui-atlas-tools/current.json"));
const output = "screens/product-ux";
await mkdir(output + "/after", { recursive: true });
await mkdir(output + "/videos", { recursive: true });
const results = [],
  shots = [],
  errors = [];
for (const theme of ["light", "dark"]) {
  const h = await bridge(
      config,
      theme,
      theme === "light" ? { videoDirectory: output + "/videos" } : {},
    ),
    p = h.page;
  p.on("pageerror", (e) => errors.push(String(e)));
  p.setDefaultTimeout(15000);
  const nav = (label) =>
    p
      .locator(".app-header")
      .getByRole("button", { name: label, exact: true })
      .click();
  const shot = async (name) => {
    const size = p.viewportSize(),
      path = `after/${theme}-${size.width}x${size.height}-${name}.png`;
    await p.screenshot({ path: output + "/" + path });
    shots.push({
      theme,
      viewport: size,
      dpr: await p.evaluate(() => devicePixelRatio),
      path,
      provenance:
        "production frontend + isolated native Rust/SQLite; theme at IPC boundary",
    });
  };
  try {
    await expect(
      p.getByRole("button", { name: "Открыть карту игры", exact: true }),
    ).toBeVisible();
    await expect(
      p
        .locator(".app-header")
        .getByRole("button", { name: "Знания", exact: true }),
    ).toHaveCount(0);
    await expect(p.locator(".home-material").first().locator("li")).toHaveCount(
      4,
    );
    await p
      .locator(".home-material")
      .first()
      .getByRole("button")
      .filter({ hasText: "Opponent" })
      .first()
      .click();
    await expect(
      p.getByRole("region", { name: "Просмотр партии", exact: true }),
    ).toBeVisible();
    await p.getByRole("button", { name: "Следующий ход", exact: true }).click();
    await expect
      .poll(() =>
        p.evaluate(
          () => JSON.parse(localStorage.getItem("chesspush.workspace.v1")).ply,
        ),
      )
      .toBe(1);
    await p.getByRole("button", { name: "Вернуться", exact: true }).click();
    await expect(
      p.getByRole("heading", { name: "Моя подготовка" }),
    ).toBeVisible();
    results.push({
      theme,
      scenario: "Home → recent game → seek → resume address → return Home",
      result: "PASS",
    });
    for (const size of [
      { width: 1440, height: 960 },
      { width: 900, height: 700 },
      { width: 1920, height: 1080 },
    ]) {
      await p.setViewportSize(size);
      await nav("Главная");
      await shot("home");
      await nav("Моя игра");
      await p.getByRole("tab", { name: "Карта игры", exact: true }).click();
      await expect(p.locator(".continuation-chip").first()).toBeVisible();
      const rootButton = p.getByRole("button", {
        name: "К началу",
        exact: true,
      });
      if (await rootButton.isEnabled()) await rootButton.click();
      await expect(
        p.getByRole("region", { name: "Карта игры", exact: true }),
      ).toHaveAttribute("aria-busy", "false");
      await expect(p.locator(".continuation-chip")).toHaveCount(3);
      const board = await p.locator(".graph-board cg-board").boundingBox(),
        chips = await p.locator(".continuation-chip").all();
      expect(chips.length).toBeGreaterThanOrEqual(3);
      expect(board.y + board.height).toBeLessThanOrEqual(size.height);
      for (const chip of chips.slice(0, 3)) {
        const r = await chip.boundingBox();
        expect(r.y + r.height).toBeLessThanOrEqual(size.height);
        expect(r.width).toBeGreaterThanOrEqual(44);
        expect(r.height).toBeGreaterThanOrEqual(44);
      }
      await shot("map");
      await p.getByRole("tab", { name: "Обзор", exact: true }).click();
      await expect(p.locator(".practice-overview")).toBeVisible();
      await shot("overview");
      await p.getByRole("tab", { name: "Партии", exact: true }).click();
      await expect(
        p.getByRole("table", { name: "Собственные партии" }),
      ).toBeVisible();
      await shot("games");
      await nav("Репертуар");
      if (
        await p
          .getByRole("button", { name: "К репертуарам", exact: true })
          .isVisible()
      )
        await p
          .getByRole("button", { name: "К репертуарам", exact: true })
          .click();
      await expect(
        p.getByRole("main", { name: "Репертуар", exact: true }),
      ).toBeVisible();
      await shot("repertoire-list");
      await p
        .getByRole("main", { name: "Репертуар", exact: true })
        .getByRole("button")
        .filter({ hasText: "Репертуар для съёмки" })
        .first()
        .click();
      await expect(
        p.getByLabel("Варианты репертуара", { exact: true }),
      ).toBeVisible();
      await shot("repertoire-editor");
      await nav("Библиотека");
      await expect(
        p.getByRole("button", { name: "Получить партии", exact: true }),
      ).toBeVisible();
      await shot("library");
      await p.getByRole("button", { name: "Настройки", exact: true }).click();
      await expect(p.getByLabel("Токен", { exact: true })).toBeHidden();
      await shot("settings-account");
      await p.getByRole("tab", { name: "Движок", exact: true }).click();
      await expect(
        p.getByLabel("Потоки Stockfish", { exact: true }),
      ).toBeHidden();
      await shot("settings-engine");
      await p.keyboard.press("Escape");
      await expect(
        p.getByRole("dialog", { name: "Настройки", exact: true }),
      ).toBeHidden();
      results.push({
        theme,
        viewport: size,
        scenario:
          "map 3chips+board visible; overview/games/repertoire/library/settings",
        result: "PASS",
      });
    }
    await p.setViewportSize({ width: 1440, height: 960 });
    await nav("Библиотека");
    await p
      .getByRole("button", { name: "Сохранённые файлы", exact: true })
      .click();
    const links = p.getByRole("button", { name: /Открыть партии: / });
    if (await links.count()) {
      await links.first().click();
      await expect(
        p.getByRole("dialog", { name: "Партии библиотеки" }),
      ).toBeVisible();
      await p.locator(".library-browser li button").first().click();
      await expect(p.locator(".library-game cg-board")).toBeVisible();
      await shot("library-game");
      await p
        .getByRole("button", { name: "Закрыть партии библиотеки" })
        .click();
      results.push({
        theme,
        scenario: "stored library→bounded list→actual Rust replay",
        result: "PASS",
      });
    }
    await nav("Моя игра");
    await p.getByRole("tab", { name: "Карта игры", exact: true }).click();
    await expect(p.getByRole("region", { name: "Карта игры" })).toHaveAttribute(
      "aria-busy",
      "false",
    );
    const heap = await p.context().newCDPSession(p);
    await heap.send("Performance.enable");
    const metrics = await heap.send("Performance.getMetrics");
    results.push({
      theme,
      scenario: "renderer heap at map",
      JSHeapUsedSize: metrics.metrics.find((x) => x.name === "JSHeapUsedSize")
        ?.value,
    });
  } catch (e) {
    console.log((await p.locator("body").innerText()).slice(-5000));
    await p.screenshot({ path: output + "/after/failure.png" });
    throw e;
  } finally {
    await h.browser.close();
  }
}
expect(errors).toEqual([]);
await writeFile(
  output + "/shell-results.json",
  JSON.stringify({ results, shots, errors }, null, 2),
);
console.log(
  JSON.stringify({ passed: results.length, screenshots: shots.length, errors }),
);

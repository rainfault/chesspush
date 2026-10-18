import { expect } from "@playwright/test";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve } from "node:path";
import { bridge } from "./helpers/product-bridge.mjs";
const config = JSON.parse(
  await readFile(".qa/ui-atlas-tools/current.json", "utf8"),
);
if (!config.data?.includes(".qa")) throw Error("Isolated profile required");
const output = "screens/simplification/home-parser";
await mkdir(output, { recursive: true });
const results = [],
  errors = [],
  shots = [];
for (const theme of ["light", "dark"]) {
  const h = await bridge(config, theme),
    p = h.page;
  p.setDefaultTimeout(20000);
  p.on("pageerror", (e) => errors.push(e.message));
  try {
    const nav = (name) =>
      p
        .locator(".app-header")
        .getByRole("button", { name, exact: true })
        .click();
    await expect(p.getByLabel("Текущее время")).toBeVisible();
    await expect(p.locator(".home time")).toHaveCount(1);
    await expect(
      p.locator(".home-resume,.home-material,.home-start"),
    ).toHaveCount(0);
    await expect(p.locator(".home-rating-value")).toHaveCount(2);
    await expect(
      p
        .locator(".app-header")
        .getByRole("button", { name: "Парсер ZST", exact: true }),
    ).toBeVisible();
    await expect(
      p
        .locator(".app-header")
        .getByRole("button", { name: "Библиотека", exact: true }),
    ).toHaveCount(0);
    for (const size of [
      { width: 1440, height: 960 },
      { width: 900, height: 700 },
      { width: 1920, height: 1080 },
    ]) {
      await p.setViewportSize(size);
      const geometry = await p.locator(".home").evaluate((home) => {
        const clock = home.querySelector("time"),
          ratings = [...home.querySelectorAll(".home-rating-value")];
        return {
          clockSize: parseFloat(getComputedStyle(clock).fontSize),
          ratingSizes: ratings.map((e) =>
            parseFloat(getComputedStyle(e).fontSize),
          ),
          clock: clock.getBoundingClientRect().toJSON(),
          page: document.documentElement.scrollWidth,
          viewport: innerWidth,
        };
      });
      expect(geometry.clockSize).toBeGreaterThanOrEqual(80);
      expect(geometry.clockSize).toBeLessThanOrEqual(112);
      expect(geometry.ratingSizes).toEqual([34, 34]);
      expect(geometry.page).toBeLessThanOrEqual(size.width);
      expect(geometry.clock.bottom).toBeLessThan(size.height);
      const path =
        output +
        "/" +
        theme +
        "-" +
        size.width +
        "x" +
        size.height +
        "-home.png";
      await p.screenshot({ path, animations: "disabled" });
      shots.push({ path, theme, viewport: size, dpr: 1 });
    }
    results.push({
      theme,
      scenario:
        "restored clock+ratings Home, bounded clock typography, no recent-game blocks, Parser ZST label",
      result: "PASS",
    });
    await p.setViewportSize({ width: 1440, height: 960 });
    await nav("Парсер ZST");
    await expect(
      p.getByRole("heading", { name: "Парсер ZST", exact: true }),
    ).toBeVisible();
    await expect(p.locator(".databases cg-board")).toBeVisible();
    if (theme === "light") {
      await p.evaluate(
        (path) => (window.__atlas.responses["plugin:dialog|open"] = path),
        resolve("tests/fixtures/corpus.pgn.zst"),
      );
      await p.locator(".source-button").click();
      await expect(p.locator(".source-button")).toContainText("corpus.pgn.zst");
      await p.getByLabel("Рейтинг от", { exact: true }).fill("0");
      await p.getByText("Дополнительно", { exact: true }).click();
      await p.getByLabel("Минимум ходов", { exact: true }).fill("0");
      await p
        .getByRole("button", { name: "Получить партии", exact: true })
        .click();
      await expect(p.locator(".result-status")).toContainText("Готово");
      await expect(
        p.getByRole("button", { name: "Открыть партии", exact: true }),
      ).toBeVisible();
      await p.screenshot({
        path: output + "/light-zst-result.png",
        animations: "disabled",
      });
      await p
        .getByRole("button", { name: "Открыть партии", exact: true })
        .click();
      await expect(
        p.getByRole("dialog", { name: "Партии библиотеки", exact: true }),
      ).toBeVisible();
      await p.locator(".library-browser li button").first().click();
      await expect(p.locator(".library-game cg-board")).toBeVisible();
      await p
        .getByRole("button", { name: "Следующий ход", exact: true })
        .click();
      await expect(
        p.locator(".library-game .library-notation button.selected"),
      ).toHaveCount(1);
      await p.screenshot({
        path: output + "/light-zst-game.png",
        animations: "disabled",
      });
      await p.keyboard.press("Escape");
      results.push({
        scenario:
          "actual ZST decode→filtered PGN→stored file→shared game replay and seek",
        result: "PASS",
        controlledBoundary: "native source file-picker choice only",
      });
    }
    await p.screenshot({
      path: output + "/" + theme + "-parser.png",
      animations: "disabled",
    });
    await nav("Главная");
    await p.getByRole("button", { name: "Настройки", exact: true }).click();
    await expect(
      p.getByRole("tab", { name: "Диагностика", exact: true }),
    ).toHaveCount(0);
    await expect(p.getByRole("dialog")).toContainText("StatisticsFixture");
    await p.keyboard.press("Escape");
    results.push({
      theme,
      scenario: "account settings and Escape, no obsolete diagnostics route",
      result: "PASS",
    });
  } finally {
    await h.browser.close();
  }
}
expect(errors).toEqual([]);
await writeFile(
  output + "/results.json",
  JSON.stringify(
    {
      results,
      shots,
      errors,
      provenance:
        "production frontend + actual isolated Rust/SQLite/PGN/ZST; two themes; only native file chooser controlled",
    },
    null,
    2,
  ),
);
console.log(JSON.stringify({ passed: results.length, errors }));

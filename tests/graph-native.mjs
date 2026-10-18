import { chromium, expect } from "@playwright/test";
import { writeFile, readFile } from "node:fs/promises";
import { join } from "node:path";
import { runRepertoireChecks } from "./repertoire-native.mjs";
const endpoint = process.env.CHESSPUSH_CDP,
  data = process.env.CHESSPUSH_TEST_DATA,
  phase = process.env.CHESSPUSH_GRAPH_PHASE;
if (!endpoint || !data) throw Error("Isolated native profile required");
const browser = await chromium.connectOverCDP(endpoint),
  page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(20000);
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const invoke = (command, args = {}) =>
  page.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
const request = {
  filter: { account_id: "lichess:statistics_fixture", variant: "standard" },
};
const graph = (extra) =>
  invoke("game_graph_query", { request: { ...request, ...extra } });
try {
  await page.getByRole("button", { name: "Статистика", exact: true }).click();
  await expect(
    page.getByRole("table", { name: "Основные показатели" }),
  ).toBeVisible();
  await page.getByRole("tab", { name: "Граф партий", exact: true }).click();
  await expect(
    page.getByRole("region", { name: "Граф партий", exact: true }),
  ).toBeVisible();
  await expect(page.locator(".graph-facts")).toContainText("4 партий");
  await expect(
    page.getByRole("table", { name: "Продолжения графа" }),
  ).toBeVisible();
  await expect(page.getByRole("alert")).toHaveCount(0);
  const root = await graph();
  expect(root.selected.games).toBe(4);
  expect(root.selected.visits).toBe(4);
  if (phase === "first") {
    const metrics = { phase };
    const started = performance.now();
    await page.waitForTimeout(500);
    // Inspect actual canvas geometry through Cytoscape's container registration, then send physical click.
    await page.locator(".game-graph-canvas").scrollIntoViewIfNeeded();
    const point = await page
      .locator(".game-graph-canvas")
      .evaluate((element) => {
        const cy = element._cyreg.cy;
        const edge = cy
          .edges()
          .filter((e) => e.data("label").startsWith("Nf3"))[0];
        const n = edge.target();
        const p = n.renderedPosition();
        const b = element.getBoundingClientRect();
        return { x: b.x + p.x, y: b.y + p.y };
      });
    await page.mouse.click(point.x, point.y);
    await expect(page.getByLabel("Путь графа")).toContainText("Nf3");
    await expect(
      page.getByRole("table", { name: "Продолжения графа" }),
    ).toContainText("Nf6");
    const square = async (s) => {
      const b = await page.locator(".graph-board .cg-wrap").boundingBox();
      return {
        x: b.x + ((s.charCodeAt(0) - 97 + 0.5) * b.width) / 8,
        y: b.y + ((8 - Number(s[1]) + 0.5) * b.height) / 8,
      };
    };
    const from = await square("g8"),
      to = await square("f6");
    await page.mouse.click(from.x, from.y);
    await page.mouse.click(to.x, to.y);
    await expect(page.getByLabel("Путь графа")).toContainText("Nf6");
    expect((await graph()).eligible_games).toBe(4);
    await page
      .getByRole("button", { name: "Назад по графу", exact: true })
      .click();
    await expect(page.getByLabel("Путь графа")).not.toContainText("Nf6");
    await page
      .getByRole("button", { name: "Вперёд по графу", exact: true })
      .click();
    await expect(page.getByLabel("Путь графа")).toContainText("Nf6");
    await page.getByRole("button", { name: "Обзор", exact: true }).click();
    await page
      .getByLabel("Цвет статистики", { exact: true })
      .selectOption("black");
    await expect(page.locator(".graph-facts")).toContainText("1 партий");
    await page.getByLabel("Цвет статистики", { exact: true }).selectOption("");
    await expect(page.locator(".graph-facts")).toContainText("4 партий");
    await page
      .getByLabel("Контроль времени статистики", { exact: true })
      .selectOption("600+2");
    await expect(page.locator(".graph-facts")).toContainText("4 партий");
    const t = (await invoke("statistics_game", { id: "lichess:stTr0001" })).game
      .replay.nodes[4];
    await page.getByLabel("Поиск позиции FEN", { exact: true }).fill(t.fen);
    await page
      .getByRole("button", { name: "Найти позицию", exact: true })
      .click();
    await expect(page.locator(".graph-selected")).toContainText("N=2");
    await expect(
      page
        .getByRole("table", { name: "Партии позиции графа" })
        .locator("tbody tr"),
    ).toHaveCount(2);
    await page
      .getByRole("button", { name: "Выбрать путь", exact: true })
      .first()
      .click();
    await expect(page.getByLabel("Путь графа").getByRole("button")).toHaveCount(
      5,
    );
    await page
      .getByRole("table", { name: "Партии позиции графа" })
      .locator("tbody tr")
      .first()
      .locator("td button")
      .first()
      .click();
    await expect(
      page.getByRole("region", { name: "Просмотр партии" }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Закрыть партию", exact: true })
      .click();
    await expect(page.locator(".graph-selected")).toContainText("N=2");
    await page.getByRole("button", { name: "Обзор", exact: true }).click();
    metrics.navigation_ms = performance.now() - started;
    metrics.repertoire = await runRepertoireChecks(page, invoke, phase, data);
    await page.getByRole("button", { name: "Обзор", exact: true }).click();
    const manifest = JSON.parse(
      await readFile(join(data, "repertoire-native.json"), "utf8"),
    );
    await page
      .getByLabel("Репертуар графа", { exact: true })
      .selectOption(manifest.id);
    await expect(
      page.getByRole("table", { name: "Продолжения графа" }),
    ).toContainText("d4");
    const overlay = await graph({ repertoire_ids: [manifest.id] });
    expect(overlay.selected.games).toBe(4);
    expect(overlay.edges.some((e) => e.prepared.length)).toBe(true);
    await page.getByLabel("Репертуар", { exact: true }).uncheck();
    await expect(
      page.getByRole("table", { name: "Продолжения графа" }),
    ).not.toContainText("d4");
    await page.getByLabel("Репертуар", { exact: true }).check();
    await page
      .getByRole("button", { name: "Вписать граф", exact: true })
      .click();
    await page.waitForTimeout(300);
    await page.screenshot({
      path: join(data, "graph-light.png"),
      fullPage: true,
    });
    await invoke("set_theme", { theme: "dark" });
    await page.evaluate(
      () => (document.documentElement.dataset.theme = "dark"),
    );
    await page.waitForTimeout(150);
    await page.screenshot({
      path: join(data, "graph-dark.png"),
      fullPage: true,
    });
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.setViewportSize({ width: 1100, height: 850 });
    await page
      .getByRole("button", { name: "Фокус позиции", exact: true })
      .click();
    await page.screenshot({
      path: join(data, "graph-reduced-resize.png"),
      fullPage: true,
    });
    await page.getByRole("button", { name: "Обзор", exact: true }).click();
    await page.waitForTimeout(700);
    const camera = await page.locator(".game-graph-canvas").evaluate((el) => ({
      zoom: el._cyreg.cy.zoom(),
      pan: el._cyreg.cy.pan(),
    }));
    await writeFile(
      join(data, "graph-native-camera.json"),
      JSON.stringify(camera),
    );
    await writeFile(
      join(data, "graph-native-metrics.json"),
      JSON.stringify(metrics, null, 2),
    );
  } else {
    const restored = await invoke("game_graph_load_state", {
      accountId: "lichess:statistics_fixture",
    });
    expect(restored.repertoireIds.length).toBeGreaterThan(0);
    const expectedCamera = JSON.parse(
      await readFile(join(data, "graph-native-camera.json"), "utf8"),
    );
    await expect
      .poll(() =>
        page.locator(".game-graph-canvas").evaluate((el) => ({
          zoom: el._cyreg.cy.zoom(),
          pan: el._cyreg.cy.pan(),
        })),
      )
      .toEqual(expectedCamera);
    await runRepertoireChecks(page, invoke, phase, data);
  }
  expect(errors).toEqual([]);
  console.log(`PASS native graph/repertoire ${phase}; artifacts ${data}`);
} catch (e) {
  await page
    .screenshot({
      path: join(data, `graph-${phase}-failure.png`),
      fullPage: true,
    })
    .catch(() => {});
  console.error("Artifacts " + data);
  throw e;
} finally {
  await browser.close();
}

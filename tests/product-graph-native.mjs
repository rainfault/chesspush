import { chromium, expect } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";
const endpoint = process.env.CHESSPUSH_CDP,
  data = process.env.CHESSPUSH_TEST_DATA,
  phase = process.env.CHESSPUSH_GRAPH_PHASE ?? "first";
if (!endpoint || !data || !data.includes(".qa"))
  throw Error("An isolated native test profile is required");
const browser = await chromium.connectOverCDP(endpoint),
  page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(20000);
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
const invoke = async (command, args = {}) => {
  const envelope = JSON.parse(
    await page.evaluate(
      async ({ command, args }) => {
        try {
          return JSON.stringify({
            ok: true,
            value: await window.__TAURI_INTERNALS__.invoke(command, args),
          });
        } catch (error) {
          return JSON.stringify({ ok: false, error });
        }
      },
      { command, args },
    ),
  );
  if (!envelope.ok) throw envelope.error;
  return envelope.value;
};
const account = "lichess:statistics_fixture",
  filter = { account_id: account, variant: "standard", color: "white" };
const query = (extra = {}) =>
  invoke("game_graph_query", { request: { filter, ...extra } });
const manifestPath = join(data, "product-graph-state.json");
const map = page.getByRole("region", { name: "Карта игры", exact: true });
const path = page.getByLabel("Путь графа", { exact: true });
const line = () => path.getAttribute("data-uci-line");
const settled = async () => {
  await expect
    .poll(
      async () =>
        (await map.getAttribute("data-projection-line")) === (await line()),
    )
    .toBe(true);
  await expect(map).toHaveAttribute("aria-busy", "false");
};
const chip = (color, san) =>
  map.getByRole("button", { name: `${color}: ${san}`, exact: true });
async function square(name) {
  const rect = await page
    .locator(".graph-board .board-frame > .cg-wrap")
    .boundingBox();
  const black = (
    await page.locator(".graph-board .cg-wrap").first().getAttribute("class")
  ).includes("orientation-black");
  return {
    x:
      rect.x +
      (((black ? 104 - name.charCodeAt(0) : name.charCodeAt(0) - 97) + 0.5) *
        rect.width) /
        8,
    y:
      rect.y +
      (((black ? Number(name[1]) - 1 : 8 - Number(name[1])) + 0.5) *
        rect.height) /
        8,
  };
}
async function clickSquare(name) {
  const p = await square(name);
  await page.mouse.click(p.x, p.y);
}
const initialMapStarted = performance.now();
try {
  await page.setViewportSize({ width: 1440, height: 960 });
  await page
    .getByRole("button", { name: "Моя игра", exact: true })
    .first()
    .click();
  await page.getByRole("tab", { name: "Карта игры", exact: true }).click();
  await settled();
  if (phase === "persist") {
    const expected = JSON.parse(await readFile(manifestPath, "utf8"));
    await expect(path).toHaveAttribute("data-uci-line", expected.line);
    await expect(
      page.getByLabel("Репертуар графа", { exact: true }),
    ).toHaveValue(expected.documentId);
    const restored = await invoke("game_graph_load_state", {
      accountId: account,
    });
    expect(restored.repertoireIds).toContain(expected.documentId);
    expect(
      (await invoke("knowledge_load", { documentId: expected.documentId }))
        .title,
    ).toBe("Product graph repertoire");
    await page.screenshot({
      path: join(data, "product-graph-restored.png"),
      fullPage: true,
    });
    console.log(
      "PASS native graph: prefix and overlay survive application restart",
    );
  } else {
    const checks = {},
      timings = {
        initial_useful_map_ms: performance.now() - initialMapStarted,
      };
    await page.getByRole("button", { name: "Я белыми", exact: true }).click();
    await settled();
    await map.getByRole("button", { name: "Начало", exact: true }).click();
    await settled();
    await expect(map.locator(".continuation-chip")).toHaveCount(3);
    const boardRect = await map
      .locator(".graph-board .board-frame")
      .boundingBox();
    const chipRects = await map
      .locator(".continuation-chip")
      .evaluateAll((nodes) =>
        nodes.slice(0, 3).map((n) => {
          const r = n.getBoundingClientRect();
          return { top: r.top, bottom: r.bottom, height: r.height };
        }),
      );
    expect(boardRect.y + boardRect.height).toBeLessThanOrEqual(960);
    expect(chipRects.every((r) => r.bottom <= 960 && r.height >= 44)).toBe(
      true,
    );
    checks.initial_layout = true;
    await page.screenshot({ path: join(data, "product-graph-light-1440.png") });
    const focusedChip = map.locator(".continuation-chip").nth(1);
    await focusedChip.focus();
    const focusLabel = await focusedChip.getAttribute("aria-label");
    const orderBefore = await map
      .locator(".continuation-chip")
      .evaluateAll((nodes) => nodes.map((n) => n.getAttribute("aria-label")));
    await page.evaluate(() => {
      window.__productGraphRefresh = new Promise((resolve) => {
        const target = document.querySelector(".game-graph");
        let seenBusy = false;
        const timer = setTimeout(() => {
          observer.disconnect();
          resolve(false);
        }, 15000);
        const observer = new MutationObserver(() => {
          if (target.getAttribute("aria-busy") === "true") seenBusy = true;
          else if (seenBusy) {
            clearTimeout(timer);
            observer.disconnect();
            resolve(true);
          }
        });
        observer.observe(target, {
          attributes: true,
          attributeFilter: ["aria-busy"],
        });
      });
    });
    await invoke("plugin:event|emit", {
      event: "statistics-updated",
      payload: {},
    });
    expect(await page.evaluate(() => window.__productGraphRefresh)).toBe(true);
    expect(
      await page.evaluate(() =>
        document.activeElement?.getAttribute("aria-label"),
      ),
    ).toBe(focusLabel);
    expect(
      await map
        .locator(".continuation-chip")
        .evaluateAll((nodes) => nodes.map((n) => n.getAttribute("aria-label"))),
    ).toEqual(orderBefore);
    checks.background_refresh_preserves_focus = true;
    const initial = await line();
    await chip("Белые", "Nf3").hover();
    await expect(map.locator(".graph-board svg.cg-shapes g line")).toHaveCount(
      1,
    );
    expect(await line()).toBe(initial);
    checks.hover_no_navigation = true;
    const started = performance.now();
    await chip("Белые", "Nf3").click();
    await settled();
    expect(await line()).toBe("g1f3");
    timings.chip_move_ms = performance.now() - started;
    await clickSquare("g8");
    await expect(map.locator(".continuation-row.piece-selected")).toHaveCount(
      1,
    );
    await clickSquare("f6");
    await settled();
    expect(await line()).toBe("g1f3 g8f6");
    checks.board_same_transition = true;
    await map
      .getByRole("button", { name: "Назад по ветви", exact: true })
      .click();
    await settled();
    expect(await line()).toBe("g1f3");
    await map.focus();
    await page.keyboard.press("ArrowRight");
    await settled();
    expect(await line()).toBe("g1f3 g8f6");
    await page.keyboard.press("ArrowLeft");
    await settled();
    expect(await line()).toBe("g1f3");
    checks.prefix_back_keyboard = true;
    await chip("Чёрные", "Nf6").click();
    await settled();
    await chip("Белые", "g3").click();
    await settled();
    await chip("Чёрные", "g6").click();
    await settled();
    const exactWhite = await line();
    expect(exactWhite).toBe("g1f3 g8f6 g2g3 g7g6");
    await page.getByRole("button", { name: "Я чёрными", exact: true }).click();
    await settled();
    await expect(map).toContainText("В этой выборке ветвь не встречалась");
    await map
      .getByRole("button", { name: /Ещё \d+ путей к этой позиции/ })
      .click();
    await settled();
    await expect(map).toContainText("Со всеми порядками ходов");
    await chip("Белые", "Bg2").click();
    await expect(
      map.getByLabel("Партии позиции графа", { exact: true }),
    ).toBeVisible();
    await map
      .getByRole("button", { name: "Выбрать этот путь", exact: true })
      .first()
      .click();
    await settled();
    expect(await line()).toBe("g2g3 g7g6 g1f3 g8f6 f1g2");
    checks.transposition_real_path = true;
    await page.getByRole("button", { name: "Я белыми", exact: true }).click();
    await settled();
    await map.getByRole("button", { name: "Начало", exact: true }).click();
    await settled();
    const factualBefore = await query();
    const doc = await invoke("knowledge_import", {
      request: {
        title: "Product graph repertoire",
        side: "white",
        pgn: '[Event "Graph overlay"]\n\n1. e4 (1. d4 d5 2. c4) e5 (1... c5) 2. Nf3 *',
      },
    });
    await expect(
      page
        .getByLabel("Репертуар графа", { exact: true })
        .getByRole("option", { name: "Product graph repertoire" }),
    ).toHaveCount(1);
    await page
      .getByLabel("Репертуар графа", { exact: true })
      .selectOption(doc.id);
    await settled();
    const withLayer = await query({ repertoire_ids: [doc.id] });
    for (const key of ["games", "visits", "wins", "draws", "losses", "score"])
      expect(withLayer.selected[key]).toEqual(factualBefore.selected[key]);
    const d4 = withLayer.continuations.find((e) => e.uci === "d2d4");
    expect(d4.games).toBe(0);
    expect(d4.score).toBeNull();
    expect(d4.prepared.length).toBeGreaterThan(0);
    await map
      .getByRole("button", { name: /Подготовленные продолжения/ })
      .click();
    await expect(chip("Белые", "d4")).toBeVisible();
    await expect(map.locator(".prepared-only")).toContainText(
      "Пока не встречалось",
    );
    checks.independent_repertoire = true;
    await chip("Белые", "e4").click();
    await settled();
    await map
      .getByRole("button", { name: /Подготовленные продолжения/ })
      .click();
    await chip("Чёрные", "c5").click();
    await settled();
    await expect(map).toContainText(
      "Ответ есть в репертуаре; продолжение после него не задано.",
    );
    checks.repertoire_reply_without_preparation = true;
    await map.getByRole("button", { name: "Начало", exact: true }).click();
    await settled();

    await map
      .getByRole("button", { name: "Партии хода Nf3", exact: true })
      .click();
    await expect(
      map.getByLabel("Партии позиции графа", { exact: true }),
    ).toBeVisible();
    await map
      .getByRole("button", { name: "Открыть партию", exact: true })
      .first()
      .click();
    const viewer = page.getByRole("region", {
      name: "Просмотр партии",
      exact: true,
    });
    await expect(viewer).toBeVisible();
    await expect(
      viewer.locator('.move.selected, [aria-current="step"]').first(),
    ).toBeVisible();
    await page.getByRole("button", { name: "Вернуться", exact: true }).click();
    await expect(map).toBeVisible();
    expect(await line()).toBe("");
    checks.exact_evidence_return = true;
    await chip("Белые", "Nf3").click();
    await settled();
    await chip("Чёрные", "Nf6").click();
    await settled();
    await page.emulateMedia({ reducedMotion: "reduce" });
    await map
      .getByRole("button", { name: "Назад по ветви", exact: true })
      .click();
    await settled();
    expect(await line()).toBe("g1f3");
    checks.reduced_motion = true;
    await map.getByRole("button", { name: "Начало", exact: true }).click();
    await settled();
    await page.setViewportSize({ width: 900, height: 700 });
    await settled();
    const compactBoard = await map
      .locator(".graph-board .board-frame")
      .boundingBox();
    expect(compactBoard.y + compactBoard.height).toBeLessThanOrEqual(700);
    await page.screenshot({ path: join(data, "product-graph-light-900.png") });
    const compactChips = await map
      .locator(".continuation-chip")
      .evaluateAll((nodes) =>
        nodes.slice(0, 3).map((n) => n.getBoundingClientRect().bottom),
      );
    expect(compactChips.length).toBe(3);
    expect(compactChips.every((bottom) => bottom <= 700)).toBe(true);
    checks.compact_layout = true;
    await page.setViewportSize({ width: 1440, height: 960 });
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await map.getByRole("button", { name: "Начало", exact: true }).click();
    await settled();
    if (process.env.CHESSPUSH_GRAPH_CYCLE === "1") {
      const aggregate = await query({ all_paths: true });
      expect(aggregate.selected.visits).toBeGreaterThan(
        aggregate.selected.games,
      );
      await map
        .getByRole("button", { name: /Ещё \d+ путей к этой позиции/ })
        .click();
      await settled();
      await map
        .getByRole("button", { name: "Подробности позиции", exact: true })
        .click();
      await expect(map.locator(".graph-position-details")).toContainText(
        `${aggregate.selected.games} уникальных партий · ${aggregate.selected.visits} посещений`,
      );
      const cycle = [
        ["Белые", "Nf3"],
        ["Чёрные", "Nf6"],
        ["Белые", "Ng1"],
        ["Чёрные", "Ng8"],
        ["Белые", "Nf3"],
        ["Чёрные", "Nf6"],
        ["Белые", "Ng1"],
        ["Чёрные", "Ng8"],
      ];
      for (const [color, san] of cycle) {
        await chip(color, san).click();
        await settled();
      }
      expect((await line()).split(" ")).toHaveLength(8);
      await map
        .getByRole("button", { name: "Только эта ветвь", exact: true })
        .click();
      await settled();
      const visits = await query({
        line: (await line()).split(" "),
        max_nodes: 8,
        depth: 5,
      });
      expect(visits.selected.games).toBe(1);
      expect(visits.selected.visits).toBe(1);
      expect(visits.nodes.length).toBeLessThanOrEqual(8);
      await expect(map.locator(".continuation-chip")).toHaveCount(0);
      await map
        .getByRole("button", { name: "Назад по ветви", exact: true })
        .first()
        .click();
      await settled();
      expect((await line()).split(" ")).toHaveLength(7);
      checks.repeated_position = {
        unique_games: aggregate.selected.games,
        position_visits: aggregate.selected.visits,
        unrolled_plies: 8,
        bounded_overview_nodes: visits.nodes.length,
      };
      await page.screenshot({
        path: join(data, "product-graph-cycle-visits.png"),
        fullPage: true,
      });
      await map.getByRole("button", { name: "Начало", exact: true }).click();
      await settled();
    }
    const factualCount = (await query()).eligible_games;
    await clickSquare("h2");
    await clickSquare("h4");
    await settled();
    expect(await line()).toBe("h2h4");
    await expect(map).toContainText("В этой выборке ветвь не встречалась");
    expect((await query()).eligible_games).toBe(factualCount);
    checks.unknown_legal_move_no_fact = true;
    await map
      .getByRole("button", { name: "Назад по ветви", exact: true })
      .first()
      .click();
    await settled();
    const opera = (await invoke("statistics_game", { id: "lichess:stOp0001" }))
      .game;
    const percentiles = (samples) => {
      const a = [...samples].sort((a, b) => a - b);
      return {
        n: a.length,
        p50_ms: a[Math.max(0, Math.ceil(a.length * 0.5) - 1)],
        p95_ms:
          a[
            Math.min(a.length - 1, Math.max(0, Math.ceil(a.length * 0.95) - 1))
          ],
        max_ms: a.at(-1),
        samples_ms: samples,
        percentile_method: "nearest rank",
      };
    };
    for (const pass of ["first_pass_20", "repeat_pass_20"]) {
      await map.getByRole("button", { name: "Начало", exact: true }).click();
      await settled();
      const times = [];
      for (const node of opera.replay.nodes.slice(1, 21)) {
        const began = performance.now();
        await chip(
          node.actor === "white" ? "Белые" : "Чёрные",
          node.san,
        ).click();
        await settled();
        times.push(performance.now() - began);
      }
      timings[pass] = percentiles(times);
    }
    await map.getByRole("button", { name: "Начало", exact: true }).click();
    await settled();
    const evidenceStarted = performance.now();
    await map
      .getByRole("button", { name: "Партии этой позиции", exact: true })
      .click();
    await expect(
      map
        .getByLabel("Партии позиции графа", { exact: true })
        .getByRole("button", { name: "Открыть партию", exact: true })
        .first(),
    ).toBeVisible();
    timings.evidence_ms = performance.now() - evidenceStarted;
    const customFen = "7k/P7/8/8/8/8/8/7K w - - 0 1";
    const promotionDoc = await invoke("knowledge_import", {
      request: {
        title: "Promotion choices",
        side: "white",
        pgn: `[FEN "${customFen}"]\n[SetUp "1"]\n\n1. a8=Q+ (1. a8=N) (1. a8=R+) (1. a8=B) *`,
      },
    });
    await invoke("game_graph_save_state", {
      accountId: account,
      state: {
        history: { entries: [{ initialFen: customFen, line: [] }], cursor: 0 },
        repertoireIds: [promotionDoc.id],
      },
    });
    await page.reload();
    await page
      .getByRole("button", { name: "Моя игра", exact: true })
      .first()
      .click();
    await page.getByRole("tab", { name: "Карта игры", exact: true }).click();
    await settled();
    for (const [role, suffix] of [
      ["knight", "n"],
      ["rook", "r"],
      ["bishop", "b"],
      ["queen", "q"],
    ]) {
      await clickSquare("a7");
      await clickSquare("a8");
      const promotion = page.getByRole("dialog", {
        name: "Превращение пешки",
        exact: true,
      });
      await expect(promotion).toBeVisible();
      await expect(promotion.getByRole("button")).toHaveCount(4);
      await promotion.getByRole("button", { name: role, exact: true }).click();
      await settled();
      expect(await line()).toBe(`a7a8${suffix}`);
      await map
        .getByRole("button", { name: "Назад по ветви", exact: true })
        .first()
        .click();
      await settled();
    }
    checks.all_promotions = true;
    await invoke("game_graph_save_state", {
      accountId: account,
      state: {
        history: {
          entries: [{ initialFen: opera.replay.initial_fen, line: ["g1f3"] }],
          cursor: 0,
        },
        repertoireIds: [doc.id],
      },
    });
    await page.reload();
    await page
      .getByRole("button", { name: "Моя игра", exact: true })
      .first()
      .click();
    await page.getByRole("tab", { name: "Карта игры", exact: true }).click();
    await settled();
    const savedLine = await line();
    await page.waitForTimeout(600);
    const state = await invoke("game_graph_load_state", { accountId: account });
    expect(state.history.entries[state.history.cursor].line.join(" ")).toBe(
      savedLine,
    );
    await writeFile(
      manifestPath,
      JSON.stringify({ line: savedLine, documentId: doc.id }, null, 2),
    );
    await writeFile(
      join(data, "product-graph-checks.json"),
      JSON.stringify({ checks, timings }, null, 2),
    );
    console.log(JSON.stringify({ phase, checks, timings, artifacts: data }));
  }
  expect(errors).toEqual([]);
} catch (error) {
  await page
    .screenshot({
      path: join(data, `product-graph-${phase}-failure.png`),
      fullPage: true,
    })
    .catch(() => {});
  throw error;
} finally {
  await browser.close();
}

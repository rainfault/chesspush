import { chromium, expect } from "@playwright/test";
import { writeFile, readFile } from "node:fs/promises";
import { join } from "node:path";
const endpoint = process.env.CHESSPUSH_CDP,
  api = process.env.CHESSPUSH_TEST_API,
  data = process.env.CHESSPUSH_TEST_DATA,
  phase = process.env.CHESSPUSH_FLOW_PHASE;
if (!endpoint || !api || !data) throw Error("Isolated native flow required");
const browser = await chromium.connectOverCDP(endpoint),
  page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(20000);
const errors = [],
  measurements = {
    phase,
    kind: "synthetic API games, real Tauri and real Stockfish",
  };
page.on("pageerror", (e) => errors.push(e.message));
const invoke = (command, args = {}) =>
  page.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
const report = (filter = {}) =>
  invoke("statistics_report", { filter: { variant: "standard", ...filter } });
async function measure(name, fn) {
  const start = performance.now();
  const result = await fn();
  measurements[name] = performance.now() - start;
  return result;
}
try {
  if (phase === "connect") {
    await page.getByRole("button", { name: "Настройки", exact: true }).click();
    await page
      .getByText("Подключить вручную по токену", { exact: true })
      .click();
    await page
      .getByLabel("Токен", { exact: true })
      .fill("synthetic_flow_credential");
    await page.getByRole("button", { name: "Подключить", exact: true }).click();
    await expect(
      page.getByRole("dialog", { name: "Настройки", exact: true }),
    ).toContainText("Аккаунт: FlowFixture");
    await page
      .getByRole("button", { name: "Закрыть настройки", exact: true })
      .click();
    await page.getByRole("button", { name: "Моя игра", exact: true }).click();
    await expect(page.locator("main.statistics")).toContainText(
      "Новые завершённые партии появятся здесь",
    );
    const initial = await invoke("statistics_state");
    expect(initial.accounts).toHaveLength(1);
    expect((await report()).total_games).toBe(0);
    await writeFile(
      join(data, "flow-t0.json"),
      JSON.stringify(initial.accounts[0]),
    );
    await new Promise((ok) => setTimeout(ok, 25));
    await page.getByRole("tab", { name: "Карта игры", exact: true }).click();
    const published = performance.now();
    await fetch(api + "/_fixture/publish?index=0", { method: "POST" });
    // Deliberately do not press Sync: this assertion exercises the normal automatic interval.
    await expect
      .poll(async () => (await report()).total_games, {
        timeout: 85000,
        intervals: [500, 1000, 2000],
      })
      .toBe(1);
    await expect(page.locator(".graph-local-heading")).toContainText(
      "1 партия",
    );
    const liveGraph = await invoke("game_graph_query", {
      request: { filter: { variant: "standard" } },
    });
    expect(liveGraph.selected.games).toBe(1);
    await page.getByRole("tab", { name: "Обзор", exact: true }).click();
    measurements.automatic_discovery_ms = performance.now() - published;
    const received = await report();
    expect(received.metrics.find((m) => m.id === "G03").value).toBe(0);
    expect(received.openings.some((o) => o.result.games === 1)).toBe(true);
    await expect(page.locator(".practice-summary")).toContainText("1 партий");
    await page.getByRole("tab", { name: "Карта игры", exact: true }).click();
    await expect(
      page.getByRole("button", { name: "Белые: f3", exact: true }),
    ).toBeVisible();
    await page.getByRole("button", { name: "Белые: f3", exact: true }).click();
    await expect(page.getByLabel("Путь графа")).toHaveAttribute(
      "data-uci-line",
      "f2f3",
    );
    await page.getByRole("button", { name: "К началу", exact: true }).click();
    await expect(page.getByLabel("Путь графа")).toHaveAttribute(
      "data-uci-line",
      "",
    );
    await page.evaluate(() => {
      window.__flowFrames = [];
      window.__flowLast = performance.now();
      window.__flowTimer = setInterval(() => {
        const now = performance.now();
        window.__flowFrames.push(now - window.__flowLast);
        window.__flowLast = now;
      }, 16);
    });
    await measure("real_engine_quick_job_ms", async () => {
      await invoke("statistics_engine_save", {
        config: {
          path: process.env.CHESSPUSH_STOCKFISH,
          threads: 1,
          hash_mb: 16,
          profile: "quick",
          custom_nodes: 100000,
          custom_movetime_ms: 200,
          syzygy_path: null,
        },
      });
      await expect
        .poll(
          async () =>
            (await invoke("statistics_state")).engine.jobs.filter(
              (j) => j.state === "succeeded",
            ).length,
          { timeout: 90000 },
        )
        .toBe(1);
    });
    const quality = await report();
    expect(quality.games[0].accuracy).not.toBeNull();
    expect(quality.metrics.find((m) => m.id === "Q02").value).not.toBeNull();
    const critical = quality.events.find((e) => e.loss_q >= 10);
    expect(critical).toBeTruthy();
    await page
      .getByRole("button", { name: "Подробная статистика", exact: true })
      .click();
    await page.getByRole("button", { name: "Качество", exact: true }).click();
    await page.locator(".practice-topic-details > summary").click();
    const eventTable = page.getByRole("table", { name: "События партий" });
    await expect(eventTable).toBeVisible();
    await eventTable.getByRole("button").first().click();
    await expect(
      page.getByRole("region", { name: "Просмотр партии" }),
    ).toBeVisible();
    await page
      .getByRole("region", { name: "Просмотр партии" })
      .locator("summary")
      .filter({ hasText: /^FEN$/ })
      .click();
    expect(await page.getByLabel("FEN выбранной позиции").inputValue()).toBe(
      quality.events[0].fen,
    );
    await page
      .getByRole("button", { name: "Следующий ход", exact: true })
      .click();
    await expect(
      page.getByLabel("Ходы партии").locator('[aria-current="step"]'),
    ).toHaveCount(1);
    await page.getByRole("button", { name: "Вернуться", exact: true }).click();
    await page
      .getByLabel("Контроль времени статистики", { exact: true })
      .selectOption("300+2");
    await page.getByRole("button", { name: "Библиотека", exact: true }).click();
    await page.getByRole("button", { name: "Моя игра", exact: true }).click();
    await expect(
      page.getByLabel("Контроль времени статистики", { exact: true }),
    ).toHaveValue("300+2");
    measurements.heartbeat = await page.evaluate(() => {
      clearInterval(window.__flowTimer);
      const v = window.__flowFrames.sort((a, b) => a - b);
      return {
        sample: v.length,
        p99_ms: v[Math.floor(v.length * 0.99)] ?? null,
        max_ms: v.at(-1) ?? null,
      };
    });
    expect(measurements.heartbeat.max_ms).toBeLessThan(1500);
    console.log(
      "PASS flow connect: real account IPC, automatic API ingestion, SQLite results/openings, real engine, critical move/FEN and navigation state",
    );
  } else if (phase === "catchup") {
    await page.getByRole("button", { name: "Моя игра", exact: true }).click();
    await expect
      .poll(async () => (await report()).total_games, { timeout: 45000 })
      .toBe(3);
    const state = await invoke("statistics_state"),
      initial = JSON.parse(await readFile(join(data, "flow-t0.json"), "utf8"));
    expect(state.accounts[0].t0_ms).toBe(initial.t0_ms);
    const current = await report();
    expect(current.metrics.find((m) => m.id === "G03").value).toBeCloseTo(
      100 / 3,
      8,
    );
    const game = await invoke("statistics_game", {
      id: "lichess:flow0002",
      accountId: state.accounts[0].id,
    });
    const key = game.game.replay.nodes[4].position_key;
    expect(
      (await report({ position_key: key, tree_mode: "position" })).opening_tree
        .games,
    ).toBe(2);
    expect(
      (
        await report({
          line: ["g1f3", "g8f6", "g2g3", "g7g6"],
          tree_mode: "line",
        })
      ).opening_tree.games,
    ).toBe(1);
    await measure("filter_roundtrip_ms", async () => {
      await page
        .getByLabel("Цвет статистики", { exact: true })
        .selectOption("black");
      await expect(page.locator("main.statistics")).toContainText(
        "Нет партий по этим условиям",
      );
      await page
        .getByLabel("Цвет статистики", { exact: true })
        .selectOption("");
      await expect(page.locator(".practice-summary")).toContainText("3 партий");
    });
    await invoke("statistics_engine_save", {
      config: {
        path: process.env.CHESSPUSH_STOCKFISH,
        threads: 1,
        hash_mb: 16,
        profile: "deep",
        custom_nodes: 500000,
        custom_movetime_ms: 1000,
        syzygy_path: null,
      },
    });
    await expect
      .poll(
        async () =>
          (await invoke("statistics_state")).engine.jobs.some(
            (j) => j.state === "running" && j.progress > 0,
          ),
        { timeout: 60000 },
      )
      .toBe(true);
    console.log(
      "PASS flow catchup: offline published games discovered at startup, same T0, dedup, transpositions/path, filters, engine active before close",
    );
  } else {
    await page.getByRole("button", { name: "Моя игра", exact: true }).click();
    await expect
      .poll(async () => (await report()).total_games, { timeout: 45000 })
      .toBe(3);
    await expect
      .poll(
        async () => {
          const s = await invoke("statistics_state");
          return s.engine.jobs.filter(
            (j) =>
              j.profile_id === s.engine.identity.profile_id &&
              j.state === "succeeded",
          ).length;
        },
        { timeout: 180000 },
      )
      .toBe(3);
    const current = await report();
    expect(new Set(current.games.map((g) => g.id)).size).toBe(3);
    expect(current.games.every((g) => g.accuracy != null)).toBe(true);
    expect(current.analysis?.tablebase_status ?? "not_configured").toMatch(
      /not_configured|missing|unavailable|not_requested/,
    );
    for (const section of ["Обзор", "Карта игры", "Партии"]) {
      await page.getByRole("tab", { name: section, exact: true }).click();
      await expect(
        page.getByRole("tab", { name: section, exact: true }),
      ).toHaveAttribute("aria-selected", "true");
    }
    await page
      .getByRole("button", { name: "Подробная статистика", exact: true })
      .click();
    for (const section of [
      "Дебюты",
      "Качество",
      "Время",
      "Реализация и окончания",
      "Все показатели",
    ]) {
      await page.getByRole("button", { name: section, exact: true }).click();
      await expect(
        page.getByRole("button", { name: section, exact: true }),
      ).toHaveClass(/selected/);
    }
    await page.screenshot({
      path: join(data, "flow-completed.png"),
      fullPage: true,
    });
    await invoke("disconnect_lichess");
    expect((await report()).total_games).toBe(3);
    console.log(
      "PASS flow recover: analysis resumed/completed, saved source and profiles survive restart, main and detailed routes, disconnect keeps history",
    );
  }
  const cdp = await page.context().newCDPSession(page);
  await cdp.send("Performance.enable");
  const perf = await cdp.send("Performance.getMetrics");
  measurements.webview_metrics = Object.fromEntries(
    perf.metrics
      .filter((m) =>
        ["JSHeapUsedSize", "JSHeapTotalSize", "Nodes", "Documents"].includes(
          m.name,
        ),
      )
      .map((m) => [m.name, m.value]),
  );
  await writeFile(
    join(data, `flow-performance-${phase}.json`),
    JSON.stringify(measurements, null, 2),
  );
  if (errors.length) throw Error(errors.join("\n"));
} finally {
  await browser.close();
}

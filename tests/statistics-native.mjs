import { chromium, expect } from "@playwright/test";
import { join } from "node:path";
const endpoint = process.env.CHESSPUSH_CDP,
  profile = process.env.CHESSPUSH_TEST_DATA,
  phase = process.env.CHESSPUSH_STATS_PHASE ?? "first";
if (!endpoint || !profile) throw new Error("Isolated native profile required");
const browser = await chromium.connectOverCDP(endpoint);
const page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(20000);
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const invoke = (command, args = {}) =>
  page.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
try {
  await page.getByRole("button", { name: "Статистика", exact: true }).click();
  await expect(
    page.getByRole("table", { name: "Основные показатели" }),
  ).toBeVisible();
  const report = await invoke("statistics_report", {
    filter: { account_id: "lichess:statistics_fixture" },
  });
  expect(report.total_games).toBe(4);
  expect(report.metrics.find((x) => x.id === "G03").value).toBe(62.5);
  if (phase === "first") {
    expect(report.metrics.find((x) => x.id === "Q02").value).toBeNull();
    const detail = await invoke("statistics_game", {
      id: "lichess:stTr0001",
      accountId: "lichess:statistics_fixture",
    });
    const key = detail.game.replay.nodes[4].position_key;
    const merged = await invoke("statistics_report", {
      filter: { position_key: key, tree_mode: "position" },
    });
    expect(merged.opening_tree.games).toBe(2);
    const exact = await invoke("statistics_report", {
      filter: { line: ["g1f3", "g8f6", "g2g3", "g7g6"], tree_mode: "line" },
    });
    expect(exact.opening_tree.games).toBe(1);
    await page
      .getByLabel("Цвет статистики", { exact: true })
      .selectOption("black");
    await expect(page.locator(".stats-coverage")).toContainText("1 партий");
    await page.getByLabel("Цвет статистики", { exact: true }).selectOption("");
    await expect(page.locator(".stats-coverage")).toContainText("4 партий");
    await page.getByRole("tab", { name: "Дебюты", exact: true }).click();
    await expect(
      page.getByRole("table", { name: "Дерево продолжений" }),
    ).toBeVisible();
    await page
      .getByRole("table", { name: "Дерево продолжений" })
      .getByRole("button", { name: "Nf3", exact: true })
      .click();
    await expect(page.locator(".stats-uci-line")).toContainText("g1f3");
    await page.getByRole("button", { name: "По линии", exact: true }).click();
    await page.getByRole("button", { name: "Назад", exact: true }).click();
    await expect(page.locator(".stats-uci-line")).toContainText(
      "Начальная позиция",
    );
    await page.waitForTimeout(250);
    await page.screenshot({
      path: join(profile, "statistics-openings-light.png"),
      fullPage: true,
    });
    await page.getByRole("tab", { name: "Партии", exact: true }).click();
    await page
      .getByRole("table", { name: "Собственные партии" })
      .locator("tbody button")
      .first()
      .click();
    await expect(
      page.getByRole("region", { name: "Просмотр партии" }),
    ).toBeVisible();
    await page
      .getByRole("button", { name: "Следующий ход", exact: true })
      .click();
    await expect(
      page.getByLabel("Ходы партии").locator('[aria-current="step"]'),
    ).toHaveCount(1);
    await page
      .getByRole("button", { name: "Закрыть партию", exact: true })
      .click();
    await page.getByText("Отклонённые загрузки (1)", { exact: true }).click();
    await expect(
      page.getByRole("table", { name: "Отклонённые загрузки" }),
    ).toContainText("stBad001");
    await page
      .getByRole("button", { name: "Загрузить повторно", exact: true })
      .click();
    await expect
      .poll(async () => (await invoke("statistics_state")).sync.pending_games)
      .toBe(1);
    await page.getByRole("button", { name: "Настройки", exact: true }).click();
    await page.getByLabel("Тема", { exact: true }).selectOption("dark");
    await page
      .getByLabel("Путь к Stockfish")
      .fill(process.env.CHESSPUSH_STOCKFISH);
    await page.getByLabel("Потоки Stockfish").fill("1");
    await page.getByLabel("Hash Stockfish").fill("16");
    await page.getByLabel("Режим анализа").selectOption("quick");
    await page
      .getByRole("button", { name: "Проверить движок", exact: true })
      .click();
    await expect(page.locator(".engine-settings output")).toContainText(
      "Stockfish",
    );
    // A deliberately larger node cap makes pause/restart timing reproducible with a real engine.
    await invoke("statistics_engine_save", {
      config: {
        path: process.env.CHESSPUSH_STOCKFISH,
        threads: 1,
        hash_mb: 16,
        profile: "quick",
        custom_nodes: 500000,
        custom_movetime_ms: 1000,
      },
    });
    await page
      .getByRole("button", { name: "Закрыть настройки", exact: true })
      .click();
    await page.getByRole("tab", { name: "Партии", exact: true }).click();
    await expect
      .poll(
        async () => {
          const s = await invoke("statistics_state");
          return s.engine.jobs.some(
            (j) => j.state === "running" && j.progress > 0,
          );
        },
        { timeout: 90000 },
      )
      .toBe(true);
    await page
      .getByRole("button", { name: "Пауза анализа", exact: true })
      .click();
    await expect
      .poll(async () => {
        const s = await invoke("statistics_state");
        return (
          s.engine.paused && !s.engine.jobs.some((j) => j.state === "running")
        );
      })
      .toBe(true);
    await page.screenshot({
      path: join(profile, "statistics-paused-dark.png"),
      fullPage: true,
    });
    console.log(
      "PASS native first: real IPC results, no fake B, filters, transpositions/line, board viewer, themes, real Stockfish setup and pause",
    );
  } else if (phase === "interrupt") {
    const before = await invoke("statistics_state");
    expect(before.engine.paused).toBe(true);
    expect(before.engine.jobs.some((j) => j.progress > 0)).toBe(true);
    await page.getByRole("tab", { name: "Партии", exact: true }).click();
    await page
      .getByRole("button", { name: "Продолжить анализ", exact: true })
      .click();
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
      "PASS native interrupted: persisted pause restored, resumed real Stockfish is running before application shutdown",
    );
  } else if (phase === "resume") {
    const before = await invoke("statistics_state");
    expect(before.engine.paused).toBe(false);
    expect(before.engine.jobs.some((j) => j.progress > 0)).toBe(true);
    await page.getByRole("tab", { name: "Партии", exact: true }).click();
    await expect
      .poll(
        async () => {
          const s = await invoke("statistics_state");
          if (s.engine.jobs.some((j) => j.state === "permanent_failed"))
            throw new Error(JSON.stringify(s.engine.jobs));
          return s.engine.jobs.filter((j) => j.state === "succeeded").length;
        },
        { timeout: 180000, intervals: [500, 1000, 2000] },
      )
      .toBe(4);
    const completed = await invoke("statistics_report", {
      filter: { account_id: "lichess:statistics_fixture" },
    });
    expect(completed.total_games).toBe(4);
    expect(completed.metrics.find((x) => x.id === "Q02").value).not.toBeNull();
    expect(
      completed.games.some((g) => g.accuracy !== null && g.analysed > 0),
    ).toBe(true);
    expect(completed.endings.length).toBeGreaterThan(0);
    expect(completed.events.length).toBeGreaterThan(0);
    expect(completed.events.length).toBeLessThanOrEqual(20);
    const evidence = await invoke("statistics_evidence", {
      filter: completed.filter,
      revision: completed.evidence_revision,
      kind: "engine",
      key: "Q05",
      offset: 0,
      limit: 100,
    });
    expect(evidence.total).toBe(completed.evidence_counts["engine:Q05"]);
    expect(evidence.items.length).toBeGreaterThan(0);
    expect(
      evidence.items.every((c) => c.metric_id === "Q05" && c.position_ply >= 0),
    ).toBe(true);
    const stale = await page.evaluate(
      (args) =>
        window.__TAURI_INTERNALS__.invoke("statistics_evidence", args).then(
          () => null,
          (error) => error,
        ),
      {
        filter: completed.filter,
        revision: "stale",
        kind: "engine",
        key: "Q05",
        offset: 0,
        limit: 100,
      },
    );
    expect(stale.code).toBe("statistics_changed");
    await page.getByRole("tab", { name: "Качество игры", exact: true }).click();
    const evidenceDetails = page
      .locator("details")
      .filter({
        has: page
          .locator("summary")
          .filter({ hasText: /^Измерения и позиции/ }),
      })
      .first();
    await evidenceDetails.locator("summary").first().click();
    await page.getByLabel("Показатель списка позиций").selectOption("Q05");
    await expect
      .poll(async () => {
        const rows = page
          .getByRole("table", { name: "Измерения решений" })
          .locator("tbody tr");
        if ((await rows.count()) === evidence.items.length)
          return evidence.items.length;
        const more = evidenceDetails.getByRole("button", {
          name: "Ещё случаи",
          exact: true,
        });
        if ((await more.isVisible()) && (await more.isEnabled()))
          await more.click({ timeout: 1000 }).catch(() => {});
        return rows.count();
      })
      .toBe(evidence.items.length);
    await page.getByRole("tab", { name: "Время", exact: true }).click();
    await expect(
      page.getByRole("table", { name: "Временные условия" }),
    ).toBeVisible();
    await page
      .getByRole("tab", { name: "Реализация и эндшпиль", exact: true })
      .click();
    await page
      .locator("summary")
      .filter({ hasText: /^Материал, пешечные структуры и рокировки/ })
      .click();
    await expect(
      page.getByRole("table", { name: "Позиционные признаки" }),
    ).toContainText("S10");

    for (const name of ["Качество игры", "Время", "Реализация и эндшпиль"]) {
      await page.getByRole("tab", { name, exact: true }).click();
      await expect(page.locator(".stats-table-wrap").first()).toBeVisible();
    }
    await page.getByRole("tab", { name: "Дебюты", exact: true }).click();
    await expect(
      page.getByRole("table", { name: "Потери в дебюте" }),
    ).toBeVisible();
    await page.screenshot({
      path: join(profile, "statistics-openings-completed-dark.png"),
      fullPage: true,
    });
    await page.getByRole("tab", { name: "Обзор", exact: true }).click();
    await page.screenshot({
      path: join(profile, "statistics-completed-dark.png"),
      fullPage: true,
    });
    await page
      .getByRole("button", { name: "Сравнить периоды", exact: true })
      .click();
    await page
      .getByLabel("Сравнение с", { exact: true })
      .fill(new Date(Date.now() - 8 * 86400000).toISOString().slice(0, 10));
    await page
      .getByLabel("Сравнение по", { exact: true })
      .fill(new Date().toISOString().slice(0, 10));
    await expect(
      page
        .getByRole("table", { name: "Сравнение периодов" })
        .locator("tbody tr"),
    ).not.toHaveCount(0);
    console.log(
      "PASS native resume: persisted paused job, real SF completion, all core B values, endgame/events, six tabs and period comparison",
    );
  } else {
    const current = await invoke("statistics_state");
    expect(
      current.engine.jobs.filter((j) => j.state === "succeeded").length,
    ).toBe(4);
    expect(report.metrics.find((x) => x.id === "Q02").value).not.toBeNull();
    expect(
      current.engine.jobs.filter(
        (j) => j.state === "running" || j.state === "queued",
      ).length,
    ).toBe(0);
    const ids = current.engine.jobs.map((j) => j.id);
    await invoke("statistics_engine_enqueue", {
      gameIds: report.games.map((g) => g.id),
    });
    expect(
      (await invoke("statistics_state")).engine.jobs.map((j) => j.id),
    ).toEqual(ids);
    const previousProfile = report.selected_profile_id;
    await invoke("statistics_engine_save", {
      config: {
        path: process.env.CHESSPUSH_STOCKFISH,
        threads: 1,
        hash_mb: 16,
        profile: "standard",
        custom_nodes: 1000,
        custom_movetime_ms: 100,
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
      .toBe(8);
    await expect
      .poll(
        async () =>
          (
            await invoke("statistics_report", {
              filter: { variant: "standard" },
            })
          ).profile_ids.length,
      )
      .toBe(2);
    await expect(
      page.getByLabel("Профиль статистики").locator("option"),
    ).toHaveCount(3);
    await page.getByLabel("Профиль статистики").selectOption(previousProfile);
    await page.getByRole("tab", { name: "Партии", exact: true }).click();
    await page
      .getByRole("table", { name: "Собственные партии" })
      .locator("tbody button")
      .first()
      .click();
    const selectedRun = await page
      .getByLabel("Анализ партии", { exact: true })
      .inputValue();
    const opened = await invoke("statistics_game", {
      id: report.games[0].id,
      accountId: "lichess:statistics_fixture",
    });
    expect(opened.runs.find((r) => r.id === selectedRun).profile_id).toBe(
      previousProfile,
    );
    await page
      .getByRole("button", { name: "Закрыть партию", exact: true })
      .click();
    await page.getByRole("tab", { name: "Качество игры", exact: true }).click();
    await expect(page.locator("main.statistics")).toHaveAttribute(
      "aria-busy",
      "false",
    );
    const oldReport = await invoke("statistics_report", {
      filter: { variant: "standard", profile_id: previousProfile },
    });
    const firstEvent = oldReport.events[0];
    await page
      .getByRole("table", { name: "События партий" })
      .getByRole("button")
      .first()
      .click();
    expect(
      await page.getByLabel("Анализ партии", { exact: true }).inputValue(),
    ).toBe(firstEvent.run_id);
    await page
      .getByRole("region", { name: "Просмотр партии" })
      .locator("summary")
      .filter({ hasText: /^FEN$/ })
      .click();
    await expect(page.getByLabel("FEN выбранной позиции")).toHaveValue(
      firstEvent.fen,
    );
    console.log(
      "PASS native persistence: game/analysis history across second restart, profile cache reused without duplicate jobs",
    );
  }
  if (errors.length) throw new Error(errors.join("\n"));
} finally {
  await browser.close();
}

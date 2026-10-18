// Explicit native Overview projection and preserved full-analytics navigation.
import { chromium, expect as baseExpect } from "@playwright/test";
const expect = baseExpect.configure({ timeout: 120000 });
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
const endpoint = process.env.CHESSPUSH_CDP,
  data = process.env.CHESSPUSH_TEST_DATA;
if (!endpoint || !data?.includes(".qa"))
  throw Error("Isolated native fixture required");
const browser = await chromium.connectOverCDP(endpoint),
  page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(120000);
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
let session;
try {
  await page.setViewportSize({ width: 1440, height: 960 });
  const map = page.getByRole("region", { name: "Карта игры", exact: true });
  await expect(map).toHaveAttribute("aria-busy", "false");
  const previousLine = await page
    .getByLabel("Путь графа", { exact: true })
    .getAttribute("data-uci-line");
  const commandsStart = await page.evaluate(() => {
    window.__reportLongtasks = [];
    window.__reportObserver = new PerformanceObserver((list) => {
      window.__reportLongtasks.push(
        ...list
          .getEntries()
          .map((e) => ({ start: e.startTime, duration: e.duration })),
      );
    });
    window.__reportObserver.observe({ type: "longtask" });
    return window.__graphCommands.length;
  });
  session = await page.context().newCDPSession(page);
  await session.send("Performance.enable");
  const before = await page.evaluate(() => performance.now());
  const metric = await session.send("Performance.getMetrics");
  const after = await page.evaluate(() => performance.now());
  const offset =
    metric.metrics.find((m) => m.name === "Timestamp").value * 1000 -
    (before + after) / 2;
  await session.send("Tracing.start", {
    categories: "devtools.timeline",
    transferMode: "ReturnAsStream",
  });
  const overviewStart = performance.now();
  await page.getByRole("tab", { name: "Обзор", exact: true }).click();
  await expect(page.locator(".practice-summary strong")).toHaveText(
    "1000 партий",
  );
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.__graphCommands.some(
          (c) =>
            c.command === "statistics_report" &&
            c.view === "practice" &&
            c.endedAt,
        ),
      ),
    )
    .toBe(true);
  const overviewReady = performance.now() - overviewStart;
  await page.screenshot({ path: join(data, "graph-report-overview.png") });
  await page
    .getByRole("button", { name: "Подробная статистика", exact: true })
    .first()
    .click();
  const detailsStart = performance.now();
  await page
    .getByRole("button", { name: "Результаты и динамика", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Результаты и динамика", exact: true }),
  ).toBeVisible();
  await page.getByText("Показатели и исходные случаи", { exact: true }).click();
  const results = page.getByRole("table", {
    name: "Результаты по группам",
    exact: true,
  });
  await expect(results).toBeVisible();
  await expect
    .poll(() => results.locator("tbody tr").count())
    .toBeGreaterThan(0);
  await expect
    .poll(() =>
      page.evaluate(() =>
        window.__graphCommands.some(
          (c) =>
            c.command === "statistics_report" && c.view === "full" && c.endedAt,
        ),
      ),
    )
    .toBe(true);
  const detailReady = performance.now() - detailsStart;
  const resultRows = await results.locator("tbody tr").count();
  const actualRows = await results.locator("tbody tr").allTextContents();
  expect(actualRows.join(" ")).toContain("1000");
  await expect(
    page.getByRole("img", { name: "Динамика score", exact: true }).first(),
  ).toBeVisible();
  await page.screenshot({
    path: join(data, "graph-report-full-results.png"),
    fullPage: true,
  });
  await page.getByRole("tab", { name: "Обзор", exact: true }).click();
  await expect(page.locator(".practice-summary strong")).toHaveText(
    "1000 партий",
  );
  await page.getByRole("tab", { name: "Карта игры", exact: true }).click();
  await expect(map).toHaveAttribute("aria-busy", "false");
  await expect(page.getByLabel("Путь графа", { exact: true })).toHaveAttribute(
    "data-uci-line",
    previousLine,
  );
  const done = new Promise((resolve) =>
    session.once("Tracing.tracingComplete", resolve),
  );
  await session.send("Tracing.end");
  const { stream } = await done;
  let raw = "";
  while (true) {
    const c = await session.send("IO.read", { handle: stream });
    raw += c.data;
    if (c.eof) break;
  }
  await session.send("IO.close", { handle: stream });
  const events = JSON.parse(raw).traceEvents;
  const resources = new Map();
  for (const e of events) {
    const d = e.args?.data;
    if (
      e.name === "ResourceSendRequest" &&
      d.url?.endsWith("/statistics_report")
    )
      resources.set(d.requestId, {
        requestId: d.requestId,
        startedAt: e.ts / 1000 - offset,
      });
    const r = resources.get(d?.requestId);
    if (r && e.name === "ResourceReceiveResponse")
      r.headersAt = e.ts / 1000 - offset;
    if (r && e.name === "ResourceFinish")
      Object.assign(r, {
        decodedBodyBytes: d.decodedBodyLength,
        finishedAt: e.ts / 1000 - offset,
      });
  }
  const commands = await page.evaluate(
    (start) =>
      window.__graphCommands
        .slice(start)
        .filter((c) => c.command === "statistics_report"),
    commandsStart,
  );
  for (const command of commands) {
    const nearest = [...resources.values()].sort(
      (a, b) =>
        Math.abs(a.startedAt - command.startedAt) -
        Math.abs(b.startedAt - command.startedAt),
    )[0];
    Object.assign(command, {
      decodedBodyBytes: nearest?.decodedBodyBytes,
      jsonElapsedMs: command.endedAt - command.parseStartedAt,
    });
  }
  expect(commands.some((c) => c.view === "practice")).toBe(true);
  expect(commands.some((c) => c.view === "full")).toBe(true);
  const practice = commands.find((c) => c.view === "practice"),
    full = commands.find((c) => c.view === "full");
  expect(practice.decodedBodyBytes).toBeLessThan(full.decodedBodyBytes);
  const longtasks = await page.evaluate(() => {
    window.__reportObserver.disconnect();
    return window.__reportLongtasks;
  });
  const result = {
    longtasks,
    fixture: "1000 legal synthetic games; no Stockfish runs",
    checks: {
      explicit_practice_overview: true,
      full_results_nonempty: true,
      returned_overview: true,
      same_graph_prefix: true,
    },
    overview_ready_ms: overviewReady,
    full_results_ready_ms: detailReady,
    result_rows: resultRows,
    actual_result_rows: actualRows,
    previousLine,
    commands,
    errors,
  };
  await writeFile(
    join(data, "graph-report-results.json"),
    JSON.stringify(result, null, 2),
  );
  console.log(
    JSON.stringify({
      ...result,
      actual_result_rows: result.actual_result_rows.slice(0, 8),
    }),
  );
  expect(errors).toEqual([]);
} catch (error) {
  await page
    .screenshot({
      path: join(data, "graph-report-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  throw error;
} finally {
  await session?.detach().catch(() => {});
  await browser.close();
}

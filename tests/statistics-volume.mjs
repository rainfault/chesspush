import { chromium, expect as baseExpect } from "@playwright/test";
import { realpath, writeFile } from "node:fs/promises";
import { isAbsolute, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const endpoint = process.env.CHESSPUSH_CDP;
const source = process.env.CHESSPUSH_TEST_DATA;
const output = process.env.CHESSPUSH_VOLUME_ARTIFACTS;
const accountId = process.env.CHESSPUSH_VOLUME_ACCOUNT;
if (!endpoint || !source || !output || !accountId)
  throw Error("Owned native volume harness required");
const address = new URL(endpoint);
if (
  address.protocol !== "http:" ||
  !["127.0.0.1", "localhost"].includes(address.hostname)
)
  throw Error("Volume CDP must use loopback");
const root = await realpath(fileURLToPath(new URL("../", import.meta.url)));
const qa = await realpath(join(root, ".qa"));
const data = await realpath(source);
const artifacts = await realpath(output);
for (const directory of [data, artifacts]) {
  const rel = relative(qa, directory);
  if (!rel || rel === ".." || rel.startsWith(".." + sep) || isAbsolute(rel))
    throw Error("Volume paths must stay inside .qa");
}
const browser = await chromium.connectOverCDP(endpoint);
const page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(45000);
const expect = baseExpect.configure({ timeout: 45000 });
const errors = [];
const measurements = {
  kind: "1001 synthetic legal games; persisted clones of one real Stockfish analysis, not independent chess observations",
  fixture_games: 1001,
  worker_paused: true,
  timings: {},
  tabs: [],
  webview: {},
};
page.on("pageerror", (error) => errors.push(error.message));
const cdp = await page.context().newCDPSession(page);
await cdp.send("Performance.enable");
const invoke = (command, args = {}) =>
  page.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
const report = (filter = {}) =>
  invoke("statistics_report", {
    filter: { account_id: accountId, variant: "standard", ...filter },
  });
async function measure(name, fn) {
  const start = performance.now();
  const result = await fn();
  measurements.timings[name] = performance.now() - start;
  return result;
}
async function memory(name) {
  const perf = await cdp.send("Performance.getMetrics");
  const counters = await cdp.send("Memory.getDOMCounters");
  measurements.webview[name] = {
    ...Object.fromEntries(
      perf.metrics
        .filter((m) =>
          ["JSHeapUsedSize", "JSHeapTotalSize", "Nodes", "Documents"].includes(
            m.name,
          ),
        )
        .map((m) => [m.name, m.value]),
    ),
    dom: counters,
  };
}
try {
  await page.waitForFunction(() => !!window.__TAURI_INTERNALS__);
  const before = await invoke("statistics_state", { accountId });
  expect(before.engine.paused).toBe(true);
  expect(before.engine.jobs.some((j) => j.state === "running")).toBe(false);
  await memory("before_statistics");
  await page.evaluate(() => {
    window.__statisticsVolumeBeats = [];
    window.__statisticsVolumeLast = performance.now();
    window.__statisticsVolumeTimer = setInterval(() => {
      const now = performance.now();
      window.__statisticsVolumeBeats.push(now - window.__statisticsVolumeLast);
      window.__statisticsVolumeLast = now;
    }, 16);
  });
  const baseline = await measure("native_report_ipc_ms", () => report());
  expect(baseline.total_games).toBe(1001);
  expect(new Set(baseline.games.map((g) => g.id)).size).toBe(1001);
  expect(baseline.analysed_decisions).toBeGreaterThan(0);
  expect(baseline.selected_profile_id).toBeTruthy();
  expect(baseline.evidence_revision).toBeTruthy();
  measurements.report_bytes = Buffer.byteLength(JSON.stringify(baseline));
  measurements.analysed_decisions = baseline.analysed_decisions;
  measurements.profile_id = baseline.selected_profile_id;
  const expectedRapid = baseline.games.filter(
    (g) => g.speed === "rapid",
  ).length;
  expect(expectedRapid).toBeGreaterThan(0);
  expect(expectedRapid).toBeLessThan(1001);
  const filtered = await measure("native_filter_ipc_ms", () =>
    report({ speed: "rapid" }),
  );
  expect(filtered.total_games).toBe(expectedRapid);
  await measure("initial_statistics_ui_ms", async () => {
    await page.getByRole("button", { name: "Статистика", exact: true }).click();
    await expect(
      page.getByRole("table", { name: "Основные показатели", exact: true }),
    ).toBeVisible();
    await expect(page.locator(".stats-coverage")).toContainText("1001 партий");
  });
  await memory("overview");
  for (const name of [
    "Обзор",
    "Дебюты",
    "Качество игры",
    "Время",
    "Реализация и эндшпиль",
    "Партии",
  ]) {
    await measure("tab_" + name + "_ms", async () => {
      const tab = page.getByRole("tab", { name, exact: true });
      await tab.click();
      await expect(tab).toHaveAttribute("aria-selected", "true");
      await expect(page.locator(".stats-coverage")).toContainText(
        "1001 партий",
      );
      await page.waitForTimeout(48);
    });
    if (name === "Качество игры") {
      await page
        .locator("summary")
        .filter({ hasText: /^Распределение потерь модельного очка/ })
        .click();
      const bins = page
        .getByRole("table", { name: "Гистограмма Lq" })
        .locator("tbody tr");
      await expect(bins).toHaveCount(baseline.analysis.histogram.length);
      await expect(bins.first()).toContainText(
        String(baseline.analysis.histogram[0].decisions),
      );
    }
    measurements.tabs.push(name);
  }
  await memory("all_tabs");
  await page.locator("details.stats-extra > summary").click();
  await measure("filter_rapid_ui_ms", async () => {
    await page
      .getByLabel("Категория рейтинга", { exact: true })
      .selectOption("rapid");
    await expect(page.locator(".stats-coverage")).toContainText(
      expectedRapid + " партий",
    );
  });
  await measure("filter_reset_ui_ms", async () => {
    await page
      .getByLabel("Категория рейтинга", { exact: true })
      .selectOption("");
    await expect(page.locator(".stats-coverage")).toContainText("1001 партий");
  });
  const filters = page.locator("details.stats-extra[open] > summary");
  if (await filters.isVisible()) await filters.click();
  await page.getByRole("tab", { name: "Дебюты", exact: true }).click();
  await expect(
    page.getByRole("table", { name: "Дерево продолжений", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Ещё случаи", exact: true }),
  ).toHaveCount(0);
  for (const theme of ["light", "dark"]) {
    await page.getByRole("button", { name: "Настройки", exact: true }).click();
    await page.getByLabel("Тема", { exact: true }).selectOption(theme);
    await page
      .getByRole("button", { name: "Закрыть настройки", exact: true })
      .click();
    await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
    await page.screenshot({
      path: join(artifacts, `statistics-volume-${theme}.png`),
      fullPage: true,
    });
  }
  await memory("after_filters_and_themes");
  await page.waitForTimeout(64);
  measurements.heartbeat = await page.evaluate(() => {
    clearInterval(window.__statisticsVolumeTimer);
    const samples = window.__statisticsVolumeBeats
      .slice()
      .sort((a, b) => a - b);
    return {
      interval_ms: 16,
      sample: samples.length,
      p50_ms: samples[Math.floor((samples.length - 1) * 0.5)] ?? null,
      p99_ms: samples[Math.floor((samples.length - 1) * 0.99)] ?? null,
      max_ms: samples.at(-1) ?? null,
    };
  });
  expect(measurements.tabs).toHaveLength(6);
  expect(measurements.heartbeat.sample).toBeGreaterThan(0);
  expect(measurements.heartbeat.max_ms).toBeLessThan(1500);
  const after = await invoke("statistics_state", { accountId });
  expect(after.engine.paused).toBe(true);
  expect(after.engine.jobs.some((j) => j.state === "running")).toBe(false);
  if (errors.length) throw Error(errors.join("\n"));
  measurements.status = "passed";
  console.log(
    "PASS volume: 1001 games, persisted analysis, native IPC/filter, six tabs, light/dark, paused worker and 16ms heartbeat <1500ms",
  );
} catch (error) {
  measurements.status = "failed";
  measurements.error = String(error?.stack ?? error);
  await page
    .screenshot({
      path: join(artifacts, "statistics-volume-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  throw error;
} finally {
  await page
    .evaluate(() => clearInterval(window.__statisticsVolumeTimer))
    .catch(() => {});
  measurements.page_errors = errors;
  await writeFile(
    join(artifacts, "statistics-volume-performance.json"),
    JSON.stringify(measurements, null, 2),
  );
  await browser.close();
}

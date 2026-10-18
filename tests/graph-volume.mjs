// Native product-map volume scenario. Primary fixtures are never personal data.
import { chromium, expect } from "@playwright/test";
import { writeFile } from "node:fs/promises";
import { join } from "node:path";
const endpoint = process.env.CHESSPUSH_CDP,
  data = process.env.CHESSPUSH_TEST_DATA;
if (!endpoint || !data?.includes(".qa"))
  throw Error("An isolated synthetic profile is required");
const browser = await chromium.connectOverCDP(endpoint),
  page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(30000);
const errors = [];
page.on("pageerror", (e) => errors.push(e.message));
const invoke = async (command, args = {}) => {
  const r = JSON.parse(
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
  if (!r.ok) throw r.error;
  return r.value;
};
const filter = {
  account_id: "lichess:syntheticgraph",
  variant: "standard",
  color: "white",
};
const query = (line = []) =>
  invoke("game_graph_query", {
    request: {
      filter,
      line,
      depth: 1,
      top_k: 1,
      max_nodes: 2,
      all_paths: false,
    },
  });
const map = page.getByRole("region", { name: "Карта игры", exact: true }),
  path = page.getByLabel("Путь графа", { exact: true });
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
const percentile = (samples, q) =>
  [...samples].sort((a, b) => a - b)[
    Math.min(samples.length - 1, Math.max(0, Math.ceil(samples.length * q) - 1))
  ];
const sample = (samples) => ({
  n: samples.length,
  p50_ms: percentile(samples, 0.5),
  p95_ms: percentile(samples, 0.95),
  max_ms: Math.max(...samples),
  samples_ms: samples,
  percentile_method: "nearest rank",
});
async function square(name) {
  const b = await map
    .locator(".graph-board .board-frame > .cg-wrap")
    .boundingBox();
  return {
    x: b.x + ((name.charCodeAt(0) - 97 + 0.5) * b.width) / 8,
    y: b.y + ((8 - Number(name[1]) + 0.5) * b.height) / 8,
  };
}
async function atRoot() {
  await map.getByRole("button", { name: "Начало", exact: true }).click();
  await settled();
}
try {
  const profiling = process.env.CHESSPUSH_GRAPH_PROFILE === "1";
  let traceSession;
  if (profiling) {
    await page.evaluate(() => {
      window.__graphCommands = [];
      const original = window.fetch;
      window.fetch = function (resource, options) {
        const url = String(resource);
        if (
          !url.startsWith("http://ipc.localhost/") &&
          !url.startsWith("https://ipc.localhost/")
        )
          return original.apply(this, arguments);
        let view;
        try {
          view = JSON.parse(options?.body ?? "{}").view;
        } catch {}
        const entry = {
          command: new URL(url).pathname.slice(1),
          view,
          startedAt: performance.now(),
        };
        window.__graphCommands.push(entry);
        return original.apply(this, arguments).then((response) => {
          entry.headersAt = performance.now();
          entry.contentLength = response.headers.get("content-length");
          const json = response.json.bind(response);
          response.json = function () {
            entry.parseStartedAt = performance.now();
            return json().then((value) => {
              entry.endedAt = performance.now();
              entry.summary =
                value && typeof value === "object"
                  ? {
                      keys: Object.keys(value),
                      games: value.games?.length,
                      total_games: value.total_games,
                      events: value.events?.length,
                    }
                  : typeof value;
              return value;
            });
          };
          return response;
        });
      };
    });
    traceSession = await page.context().newCDPSession(page);
    await traceSession.send("Tracing.start", {
      categories: "devtools.timeline,v8,disabled-by-default-v8.compile",
      transferMode: "ReturnAsStream",
    });
  }
  await page.setViewportSize({ width: 1440, height: 960 });
  const started = performance.now();
  await page
    .getByRole("button", { name: "Моя игра", exact: true })
    .first()
    .click();
  await page.getByRole("tab", { name: "Карта игры", exact: true }).click();
  await settled();
  await page.getByRole("button", { name: "Я белыми", exact: true }).click();
  await atRoot();
  const firstUseful = performance.now() - started,
    root = await query();
  expect(root.eligible_games).toBeGreaterThanOrEqual(1000);
  expect(root.selected.games).toBe(root.eligible_games);
  expect(root.selected.visits).toBe(root.selected.games);
  expect(root.continuations.length).toBeGreaterThan(5);
  await expect(map.locator(".continuation-chip")).toHaveCount(5);
  await expect(map.locator(".game-graph-canvas")).toHaveCount(0);
  const board = await map.locator(".graph-board .board-frame").boundingBox();
  expect(board.y + board.height).toBeLessThanOrEqual(960);
  const firstThree = await map
    .locator(".continuation-chip")
    .evaluateAll((nodes) =>
      nodes.slice(0, 3).map((n) => n.getBoundingClientRect().bottom),
    );
  expect(firstThree.every((y) => y <= 960)).toBe(true);
  await page.screenshot({ path: join(data, "graph-volume-light.png") });
  await page.evaluate(() => {
    window.__graphFrames = [];
    window.__graphLast = performance.now();
    window.__graphTimer = setInterval(() => {
      const now = performance.now();
      window.__graphFrames.push(now - window.__graphLast);
      window.__graphLast = now;
    }, 16);
  });
  const rare = root.continuations.at(-1),
    from = await square(rare.uci.slice(0, 2)),
    to = await square(rare.uci.slice(2, 4));
  await page.mouse.click(from.x, from.y);
  await page.mouse.click(to.x, to.y);
  await settled();
  expect(await line()).toBe(rare.uci);
  const rareBranch = await query([rare.uci]);
  expect(rareBranch.selected.games).toBe(rare.games);
  expect(rareBranch.selected.games).toBeGreaterThan(0);
  await atRoot();
  let expansions = 0;
  while (
    await map.getByRole("button", { name: /^Ещё \d+ продолжений$/ }).count()
  ) {
    await map.getByRole("button", { name: /^Ещё \d+ продолжений$/ }).click();
    expansions++;
  }
  await expect(map.locator(".continuation-chip")).toHaveCount(
    root.continuations.length,
  );
  expect(
    (await query()).continuations.map((e) => [e.uci, e.frequency, e.games]),
  ).toEqual(root.continuations.map((e) => [e.uci, e.frequency, e.games]));
  await page.screenshot({
    path: join(data, "graph-volume-expanded.png"),
    fullPage: true,
  });
  if (profiling)
    await page.evaluate(() => {
      const profile = (window.__graphProfile = {
        supportedLongTask:
          PerformanceObserver.supportedEntryTypes.includes("longtask"),
        longtasks: [],
        steps: [],
        heartbeats: [],
        current: null,
      });
      if (profile.supportedLongTask) {
        profile.observer = new PerformanceObserver((list) => {
          for (const entry of list.getEntries())
            profile.longtasks.push({
              start: entry.startTime,
              duration: entry.duration,
              name: entry.name,
              attribution: [...(entry.attribution ?? [])].map((a) => ({
                name: a.name,
                containerType: a.containerType,
                containerName: a.containerName,
              })),
            });
        });
        profile.observer.observe({ type: "longtask", buffered: true });
      }
      document.addEventListener(
        "click",
        (e) => {
          if (profile.current && e.target.closest?.(".continuation-chip")) {
            profile.current.clickAt = performance.now();
            profile.current.trusted = e.isTrusted;
          }
        },
        true,
      );
      profile.mutation = new MutationObserver(() => {
        const current = profile.current;
        if (!current?.clickAt) return;
        const map = document.querySelector(".game-graph"),
          path = document
            .querySelector("[data-uci-line]")
            .getAttribute("data-uci-line");
        if (map.getAttribute("aria-busy") === "true" && !current.busyAt)
          current.busyAt = performance.now();
        if (
          map.getAttribute("aria-busy") === "false" &&
          path !== current.initialLine &&
          map.getAttribute("data-projection-line") === path &&
          !current.readyAt
        ) {
          current.readyAt = performance.now();
          current.finalLine = path;
        }
      });
      profile.mutation.observe(document.querySelector(".game-graph"), {
        attributes: true,
        subtree: true,
        attributeFilter: ["aria-busy", "data-projection-line", "data-uci-line"],
      });
      let last = performance.now();
      profile.interval = setInterval(() => {
        const now = performance.now();
        profile.heartbeats.push({ at: now, gap: now - last });
        last = now;
      }, 16);
    });
  let cpuSession, cpuClock;
  if (profiling) {
    cpuSession = await page.context().newCDPSession(page);
    await cpuSession.send("Profiler.enable");
    await cpuSession.send("Profiler.setSamplingInterval", { interval: 1000 });
    await cpuSession.send("Performance.enable");
    const before = await page.evaluate(() => performance.now());
    const metrics = await cpuSession.send("Performance.getMetrics");
    const after = await page.evaluate(() => performance.now());
    const timestamp =
      metrics.metrics.find((m) => m.name === "Timestamp").value * 1000;
    cpuClock = {
      before,
      after,
      timestamp,
      offsetMs: timestamp - (before + after) / 2,
      uncertaintyMs: (after - before) / 2,
    };
    await cpuSession.send("Profiler.start");
  }
  const timings = {};
  for (const pass of ["first_pass_20", "repeat_pass_20"]) {
    await atRoot();
    const samples = [];
    for (let i = 0; i < 20; i++) {
      const chip = map.locator(".continuation-chip").first();
      await expect(chip).toBeVisible();
      const start = performance.now();
      if (profiling)
        await page.evaluate(
          ({ pass, index }) => {
            const p = window.__graphProfile;
            p.current = {
              pass,
              index,
              armedAt: performance.now(),
              initialLine: document
                .querySelector("[data-uci-line]")
                .getAttribute("data-uci-line"),
            };
            p.steps.push(p.current);
          },
          { pass, index: i },
        );
      const clickStart = performance.now();
      await chip.click();
      const clickReturned = performance.now();
      await settled();
      const done = performance.now();
      samples.push(done - start);
      if (profiling)
        await page.evaluate(
          (values) => {
            Object.assign(window.__graphProfile.current, values);
            window.__graphProfile.current.observedAt = performance.now();
            window.__graphProfile.current = null;
          },
          {
            driverClickMs: clickReturned - clickStart,
            driverReadyPollMs: done - clickReturned,
            nodeTotalMs: done - start,
          },
        );
    }
    timings[pass] = sample(samples);
  }
  let cpuProfile, timelineIpc;
  if (profiling) {
    ({ profile: cpuProfile } = await cpuSession.send("Profiler.stop"));
    await writeFile(
      join(data, "graph-volume-cpu.cpuprofile"),
      JSON.stringify(cpuProfile),
    );
    await cpuSession.detach();
    const traceDone = new Promise((resolve) =>
      traceSession.once("Tracing.tracingComplete", resolve),
    );
    await traceSession.send("Tracing.end");
    const { stream } = await traceDone;
    let traceText = "";
    while (true) {
      const chunk = await traceSession.send("IO.read", { handle: stream });
      traceText += chunk.data;
      if (chunk.eof) break;
    }
    await traceSession.send("IO.close", { handle: stream });
    await writeFile(join(data, "graph-volume-timeline.json"), traceText);
    const events = JSON.parse(traceText).traceEvents;
    const pending = new Map(),
      longEvents = [],
      resources = new Map();
    for (const e of events) {
      const key = `${e.pid}:${e.tid}`;
      if (e.ph === "B") {
        if (!pending.has(key)) pending.set(key, []);
        pending.get(key).push(e);
      } else if (e.ph === "E" && pending.get(key)?.length) {
        const begin = pending.get(key).pop();
        if (e.ts - begin.ts >= 20000)
          longEvents.push({ ...begin, dur: e.ts - begin.ts });
      } else if (e.ph === "X" && e.dur >= 20000) longEvents.push(e);
      const d = e.args?.data;
      if (e.name === "ResourceSendRequest" && d.url?.includes("ipc.localhost/"))
        resources.set(d.requestId, {
          command: new URL(d.url).pathname.slice(1),
          requestId: d.requestId,
          startedAt: e.ts / 1000 - cpuClock.offsetMs,
        });
      const r = resources.get(d?.requestId);
      if (r && e.name === "ResourceReceiveResponse")
        r.headersAt = e.ts / 1000 - cpuClock.offsetMs;
      if (r && e.name === "ResourceFinish")
        Object.assign(r, {
          decodedBodyLength: d.decodedBodyLength,
          encodedDataLength: d.encodedDataLength,
          finishedAt: e.ts / 1000 - cpuClock.offsetMs,
        });
    }
    timelineIpc = [...resources.values()];
    await writeFile(
      join(data, "graph-volume-ipc-summary.json"),
      JSON.stringify(timelineIpc, null, 2),
    );
    await writeFile(
      join(data, "graph-volume-timeline-long.json"),
      JSON.stringify(
        longEvents.map((e) => ({
          ...e,
          browserStart: e.ts / 1000 - cpuClock.offsetMs,
        })),
        null,
        2,
      ),
    );
    await traceSession.detach();
  }
  await atRoot();
  const evidenceStart = performance.now();
  await map
    .getByRole("button", { name: "Партии этой позиции", exact: true })
    .click();
  await expect(
    map
      .getByLabel("Партии позиции графа", { exact: true })
      .getByRole("button", { name: "Открыть партию", exact: true }),
  ).toHaveCount(50);
  timings.evidence_ms = performance.now() - evidenceStart;
  await map
    .getByRole("button", { name: "Закрыть список", exact: true })
    .click();
  await map.getByRole("button", { name: "Общая карта", exact: true }).click();
  await expect
    .poll(() =>
      map
        .locator(".game-graph-canvas")
        .evaluate((el) => el._cyreg?.cy?.nodes().length ?? 0),
    )
    .toBeGreaterThan(1);
  const overview = await map.locator(".game-graph-canvas").evaluate((el) => ({
    nodes: el._cyreg.cy.nodes().length,
    edges: el._cyreg.cy.edges().length,
    dragEnabled: el._cyreg.cy.nodes().some((n) => n.grabbable()),
  }));
  expect(overview.nodes).toBeLessThanOrEqual(100);
  expect(overview.dragEnabled).toBe(false);
  await map
    .getByRole("button", { name: "К продолжениям", exact: true })
    .click();
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await page.getByRole("tab", { name: "Внешний вид", exact: true }).click();
  await page.getByLabel("Тема", { exact: true }).selectOption("dark");
  await page
    .getByRole("button", { name: "Закрыть настройки", exact: true })
    .click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", "dark");
  await page.screenshot({ path: join(data, "graph-volume-dark.png") });
  await page.setViewportSize({ width: 900, height: 700 });
  const compactBoard = await map
    .locator(".graph-board .board-frame")
    .boundingBox();
  expect(compactBoard.y + compactBoard.height).toBeLessThanOrEqual(700);
  await page.screenshot({ path: join(data, "graph-volume-dark-900.png") });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await map.locator(".continuation-chip").first().click();
  await settled();
  expect((await line()).split(" ")).toHaveLength(1);
  const frames = await page.evaluate(() => {
    clearInterval(window.__graphTimer);
    return window.__graphFrames;
  });
  let profileSummary;
  if (profiling) {
    const profile = await page.evaluate(() => {
      const p = window.__graphProfile;
      p.observer?.disconnect();
      p.mutation.disconnect();
      clearInterval(p.interval);
      return {
        supportedLongTask: p.supportedLongTask,
        longtasks: p.longtasks,
        steps: p.steps,
        heartbeats: p.heartbeats,
        commands: window.__graphCommands,
      };
    });
    if (process.env.CHESSPUSH_GRAPH_EXPECT_NO_REPORT === "1")
      expect(
        profile.commands.filter((c) => c.command === "statistics_report"),
        "Switching from Overview to Map must not launch an already disposed full-report request",
      ).toEqual([]);
    for (const step of profile.steps) {
      step.click_to_busy_ms = step.busyAt - step.clickAt;
      step.click_to_ready_ms = step.readyAt - step.clickAt;
      step.driver_autowait_before_click_ms = step.clickAt - step.armedAt;
      step.longtasks = profile.longtasks.filter(
        (t) =>
          t.start <= step.observedAt && t.start + t.duration >= step.armedAt,
      );
      step.heartbeat_max_ms = Math.max(
        0,
        ...profile.heartbeats
          .filter((t) => t.at >= step.armedAt && t.at <= step.observedAt)
          .map((t) => t.gap),
      );
    }
    if (cpuProfile) {
      const nodes = new Map(cpuProfile.nodes.map((n) => [n.id, n]));
      const parents = new Map();
      for (const n of cpuProfile.nodes)
        for (const child of n.children ?? []) parents.set(child, n.id);
      const describe = (id) => {
        const n = nodes.get(id),
          f = n.callFrame;
        return {
          id,
          functionName: f.functionName,
          url: f.url,
          lineNumber: f.lineNumber + 1,
          columnNumber: f.columnNumber + 1,
        };
      };
      const windows = profile.longtasks.map((task) => {
        const self = new Map(),
          inclusive = new Map();
        let cursor = cpuProfile.startTime / 1000 - cpuClock.offsetMs;
        for (let i = 0; i < cpuProfile.samples.length; i++) {
          const duration = cpuProfile.timeDeltas[i] / 1000,
            end = cursor + duration;
          const overlap = Math.max(
            0,
            Math.min(end, task.start + task.duration) -
              Math.max(cursor, task.start),
          );
          cursor = end;
          if (!overlap) continue;
          let id = cpuProfile.samples[i];
          self.set(id, (self.get(id) ?? 0) + overlap);
          while (id) {
            inclusive.set(id, (inclusive.get(id) ?? 0) + overlap);
            id = parents.get(id);
          }
        }
        const rank = (values) =>
          [...values]
            .sort((a, b) => b[1] - a[1])
            .slice(0, 25)
            .map(([id, ms]) => ({
              ...describe(id),
              ms,
              stack: (() => {
                const stack = [];
                let parent = id;
                while (parent) {
                  stack.push(describe(parent));
                  parent = parents.get(parent);
                }
                return stack;
              })(),
            }));
        return { ...task, self: rank(self), inclusive: rank(inclusive) };
      });
      await writeFile(
        join(data, "graph-volume-cpu-summary.json"),
        JSON.stringify(
          {
            clock: cpuClock,
            profileStartTime: cpuProfile.startTime,
            profileEndTime: cpuProfile.endTime,
            samples: cpuProfile.samples.length,
            longtaskWindows: windows,
          },
          null,
          2,
        ),
      );
    }
    profileSummary = {
      supportedLongTask: profile.supportedLongTask,
      longtasks: profile.longtasks.length,
      worst_step: [...profile.steps].sort(
        (a, b) => b.nodeTotalMs - a.nodeTotalMs,
      )[0],
    };
    await writeFile(
      join(data, "graph-volume-profile.json"),
      JSON.stringify(profile, null, 2),
    );
  }
  const result = {
    kind: `${root.eligible_games} independent legal synthetic games, native product map`,
    initial_useful_map_ms: firstUseful,
    profile: profileSummary,
    observed_continuations: root.continuations.length,
    rare_move: rare.uci,
    rare_games: rare.games,
    expansions,
    overview,
    ...timings,
    heartbeat_p99_ms: percentile(frames, 0.99),
    heartbeat_max_ms: Math.max(...frames),
    heartbeat_samples: frames.length,
  };
  await writeFile(
    join(data, "graph-volume-metrics.json"),
    JSON.stringify(result, null, 2),
  );
  console.log(JSON.stringify(result));
  expect(errors).toEqual([]);
} catch (error) {
  await page
    .screenshot({
      path: join(data, "graph-volume-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  throw error;
} finally {
  await browser.close();
}

// Five named Study chapters remain selectable over the same native PGN domain.
// Run through the isolated repertoire-training runner with
// CHESSPUSH_TRAINING_TEST_SCRIPT=tests/repertoire-multichapter-ui.mjs.
import { chromium, expect } from "@playwright/test";
import { createServer } from "vite";
import react from "@vitejs/plugin-react";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { DatabaseSync } from "node:sqlite";

const cdp = process.env.CHESSPUSH_TEST_CDP,
  profile = process.env.CHESSPUSH_TEST_DATA;
if (!cdp || !profile)
  throw Error("Use the isolated repertoire-training runner");
const artifacts = resolve(
  process.env.CHESSPUSH_TRAINING_ARTIFACTS ?? ".qa/repertoire-multichapter",
);
await mkdir(artifacts, { recursive: true });
const nativeBrowser = await chromium.connectOverCDP(cdp);
const nativePage = nativeBrowser.contexts()[0].pages()[0];
await nativePage.waitForFunction(
  () =>
    typeof window.__TAURI_INTERNALS__?.invoke === "function" &&
    !!document.querySelector(".app-header"),
);
async function native(command, args = {}) {
  const result = JSON.parse(
    await nativePage.evaluate(
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
  if (!result.ok) throw result.error;
  return result.value;
}
const imported = await native("knowledge_attach_file", {
  path: resolve("tests/fixtures/repertoire-five-chapters.pgn"),
  side: "white",
});
const custom = await native("knowledge_attach_file", {
  path: resolve("tests/fixtures/repertoire-five-chapters-fen.pgn"),
  side: "white",
});
const titles = [
  "Five chapters: Queen's Gambit",
  "Five chapters: London",
  "Five chapters: Reti",
  "Five chapters: Italian",
  "Five chapters: English",
];
expect(imported.chapters.map((item) => item.title)).toEqual(titles);
expect(custom.chapters).toHaveLength(1);
const combined = await native("knowledge_repertoire", { side: "white" });
expect(
  combined.metadata.source_chapters
    .filter((item) => item.document_id === imported.id)
    .map((item) => item.title),
).toEqual(titles);
expect(
  combined.metadata.source_chapters.some(
    (item) =>
      item.document_id === custom.id &&
      item.initial_fen === custom.chapters[0].initial_fen,
  ),
).toBe(true);
function decisions(document) {
  const map = new Map();
  let occurrences = 0;
  function visit(node) {
    if (node.fen.split(" ")[1] === "w" && node.children.length) {
      occurrences++;
      const moves = map.get(node.position_key) ?? new Set();
      node.children.forEach((child) => moves.add(child.uci));
      map.set(node.position_key, moves);
    }
    node.children.forEach(visit);
  }
  document.chapters.forEach((chapter) => visit(chapter.root));
  return { map, occurrences };
}
const globalDecisions = decisions(combined);
const sourceChapters = [
  ...imported.chapters.map((chapter) => ({ document: imported, chapter })),
  ...custom.chapters.map((chapter) => ({ document: custom, chapter })),
];
expect(
  sourceChapters.reduce(
    (sum, { chapter }) => sum + decisions({ chapters: [chapter] }).map.size,
    0,
  ),
).toBeGreaterThan(globalDecisions.map.size);

const vite = await createServer({
  configFile: false,
  cacheDir: ".qa/repertoire-multichapter-vite-cache",
  optimizeDeps: { include: ["react", "react-dom/client"] },
  plugins: [react()],
  appType: "custom",
  server: {
    host: "127.0.0.1",
    port: 0,
    watch: { ignored: ["**/.qa/**", "**/src-tauri/target/**"] },
  },
});
vite.middlewares.use(async (req, res, next) => {
  if (!req.url?.startsWith("/chapter-harness")) return next();
  res.setHeader("Content-Type", "text/html");
  res.end(
    await vite.transformIndexHtml(
      "/chapter-harness",
      `<html><head></head><body><div id="root"></div><script type="module">
import React from 'react';
import {createRoot} from 'react-dom/client';
import {Repertoire} from '/src/knowledge/Repertoire.tsx';
import '/src/theme.css';
createRoot(document.getElementById('root')).render(React.createElement(Repertoire,{active:true,initialTarget:{mode:'list'}}));
</script></body></html>`,
    ),
  );
});
await vite.listen();
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
const errors = [],
  checks = [];
page.on("pageerror", (error) => errors.push(error.message));
await page.exposeFunction("__chapterNative", async (command, args) => {
  try {
    return JSON.stringify({ ok: true, value: await native(command, args) });
  } catch (error) {
    return JSON.stringify({ ok: false, error });
  }
});
await page.addInitScript(() => {
  window.isTauri = true;
  window.__chapterTest = { last: null, calls: [] };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback() {
      return 1;
    },
    async invoke(command, args) {
      if (command.startsWith("plugin:event|")) return 1;
      const result = JSON.parse(await window.__chapterNative(command, args));
      if (!result.ok) throw result.error;
      if (command.startsWith("training_")) {
        window.__chapterTest.calls.push({ command, args, value: result.value });
        if (command !== "training_overview")
          window.__chapterTest.last = result.value;
      }
      return result.value;
    },
  };
});
const trainer = page.locator(".repertoire-training"),
  viewer = page.locator(".combined-repertoire");
const last = () => page.evaluate(() => window.__chapterTest.last);
const record = (message) => {
  checks.push(message);
  console.log("PASS", message);
};
async function capture(name) {
  // Chessground animates legal-move rollback and context changes for 200 ms.
  // Capture the settled position instead of a piece between two squares.
  await page.waitForTimeout(350);
  await page.screenshot({ path: join(artifacts, name) });
}
const chapterSelect = (owner) =>
  owner.getByRole("combobox", { name: "Глава", exact: true });
async function list() {
  if (await trainer.isVisible())
    await trainer
      .getByRole("button", { name: "К репертуару", exact: true })
      .click();
  if (await viewer.isVisible())
    await viewer
      .getByRole("button", { name: "← К файлам", exact: true })
      .click();
  await expect(page.locator(".repertoire-list")).toBeVisible();
}
async function openTraining(title = null) {
  await list();
  await page.getByRole("button", { name: "Тренировать", exact: true }).click();
  await expect(trainer).toBeVisible();
  await chapterSelect(trainer).selectOption(title ? { label: title } : "");
}
async function start(mode) {
  await trainer
    .getByRole("button", {
      name: mode === "srs" ? "Интервальное повторение" : "Полный дрилл",
      exact: true,
    })
    .click();
  await page.evaluate(() => {
    window.__chapterTest.last = null;
  });
  await trainer.getByRole("button", { name: "Начать", exact: true }).click();
  await expect.poll(async () => (await last())?.feedback).toBeTruthy();
  return last();
}
async function play(
  uci,
  owner = trainer,
  boardSelector = ".training-board cg-board",
) {
  const state = await last();
  await expect(owner.locator(".training-feedback")).toHaveClass(
    new RegExp(state.feedback),
  );
  await expect(owner.locator(".training-feedback")).not.toContainText(
    "Восстановление",
  );
  if ((await last())?.feedback === "awaiting")
    await expect(
      owner.getByRole("button", { name: "Подсказка", exact: true }),
    ).toBeEnabled();
  const board = owner.locator(boardSelector),
    box = await board.boundingBox();
  const point = (name) => ({
    x: box.x + ((name.charCodeAt(0) - 97 + 0.5) * box.width) / 8,
    y: box.y + ((8 - Number(name[1]) + 0.5) * box.height) / 8,
  });
  const from = point(uci.slice(0, 2)),
    to = point(uci.slice(2, 4));
  const count = await page.evaluate(() => window.__chapterTest.calls.length);
  await page.mouse.click(from.x, from.y);
  await page.mouse.click(to.x, to.y);
  await expect
    .poll(() => page.evaluate(() => window.__chapterTest.calls.length))
    .toBeGreaterThan(count);
  return last();
}
async function advance(previous) {
  const current = await last();
  if (
    current.finished ||
    current.prompt?.decision_id !== previous.prompt?.decision_id
  )
    return current;
  try {
    await trainer
      .getByRole("button", { name: "Дальше", exact: true })
      .click({ timeout: 1500 });
  } catch (error) {
    const now = await last();
    if (
      now.finished ||
      now.prompt?.decision_id !== previous.prompt?.decision_id
    )
      return now;
    throw error;
  }
  await expect
    .poll(async () => {
      const now = await last();
      return (
        now.finished || now.prompt?.decision_id !== previous.prompt?.decision_id
      );
    })
    .toBe(true);
  return last();
}
async function complete(map, scope, seen = new Set()) {
  let session = await last();
  const total = session.total;
  for (let guard = 0; !session.finished && guard < 100; guard++) {
    expect(session.scope.source_document_id ?? null).toBe(
      scope?.document.id ?? null,
    );
    expect(session.scope.chapter_id ?? null).toBe(scope?.chapter.id ?? null);
    if (session.feedback === "correct" || session.feedback === "assisted") {
      session = await advance(session);
      continue;
    }
    const prompt = session.prompt,
      moves = map.get(prompt.position_key);
    if (scope) expect(prompt.initial_fen).toBe(scope.chapter.initial_fen);
    expect(
      moves,
      `Unexpected position outside ${scope?.chapter.title ?? "whole repertoire"}`,
    ).toBeTruthy();
    expect(
      seen.has(prompt.position_key),
      "A completed transposition was repeated",
    ).toBe(false);
    seen.add(prompt.position_key);
    session = await play([...moves][0]);
    expect(["correct", "assisted"]).toContain(session.feedback);
    expect(
      session.prompt.accepted_moves.map((move) => move.uci).sort(),
    ).toEqual([...moves].sort());
  }
  expect(session.finished).toBe(true);
  expect(session.completed).toBe(total);
  expect(seen.size).toBe(total);
  return { session, seen };
}
function srsRows() {
  const db = new DatabaseSync(join(profile, "chesspush.sqlite3"), {
    readOnly: true,
  });
  try {
    return db
      .prepare("SELECT * FROM repertoire_training_srs ORDER BY decision_id")
      .all();
  } finally {
    db.close();
  }
}
function semantic(node) {
  return {
    uci: node.uci,
    comments: node.comments,
    starting_comments: node.starting_comments ?? [],
    nags: node.nags,
    shapes: node.shapes,
    children: node.children.map(semantic),
  };
}
try {
  await page.goto(
    `http://127.0.0.1:${vite.httpServer.address().port}/chapter-harness`,
  );
  await page
    .getByRole("button", { name: "Открыть общий репертуар", exact: true })
    .click();
  await expect(chapterSelect(viewer).locator("option")).toHaveCount(7);
  const london = sourceChapters.find(
    (item) => item.chapter.title === "Five chapters: London",
  );
  await chapterSelect(viewer).selectOption({ label: london.chapter.title });
  await expect(viewer.locator(".combined-branches strong")).toHaveCount(1);
  await expect(viewer.locator(".combined-branches strong")).toContainText("d4");
  for (const san of ["d4", "d5", "Nf3", "Nf6", "Bf4"])
    await viewer
      .locator(".combined-branches")
      .getByRole("button", { name: new RegExp(`^\\d+[. …]+${san}(?:\\s|$)`) })
      .click();
  await expect(viewer.locator(".combined-comment")).toContainText(
    "Develop the bishop before e3.",
  );
  await viewer
    .getByRole("button", { name: "Начало репертуара", exact: true })
    .click();
  await viewer
    .getByRole("button", { name: "Тренировать ветку", exact: true })
    .click();
  await expect(chapterSelect(trainer).locator("option:checked")).toHaveText(
    london.chapter.title,
  );
  let session = await start("full_drill");
  const londonMap = decisions({ chapters: [london.chapter] }).map;
  expect(session.total).toBe(londonMap.size);
  session = await play("e2e4");
  expect(session.feedback).toBe("retry");
  expect(session.prompt.accepted_moves.map((move) => move.uci)).toEqual([
    "d2d4",
  ]);
  for (const width of [1440, 900]) {
    await page.setViewportSize({ width, height: 960 });
    for (const theme of ["light", "dark"]) {
      await page.evaluate((value) => {
        document.documentElement.dataset.theme = value;
      }, theme);
      await capture(`selected-chapter-training-${theme}-${width}.png`);
    }
  }
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "light";
  });
  session = await play("d2d4");
  expect(session.feedback).toBe("assisted");
  await complete(
    londonMap,
    london,
    new Set([london.chapter.root.position_key]),
  );
  record(
    "Five named Study chapters are selectable; viewer and launched chapter drill exclude unrelated prepared moves and preserve PGN comments",
  );
  const noSrs = srsRows();
  expect(noSrs).toEqual([]);
  for (const entry of sourceChapters) {
    const map = decisions({ chapters: [entry.chapter] }).map;
    if (entry !== london) {
      await openTraining(entry.chapter.title);
      session = await start("full_drill");
      expect(session.total).toBe(map.size);
      await complete(map, entry);
    }
    const before = srsRows();
    await openTraining(entry.chapter.title);
    session = await start("srs");
    expect(session.total).toBeGreaterThan(0);
    expect(session.total).toBeLessThanOrEqual(map.size);
    if (entry === london) expect(session.total).toBeLessThan(map.size);
    await complete(map, entry);
    expect(srsRows().length).toBeGreaterThanOrEqual(before.length);
  }
  record(
    "Both SRS and Full Drill test only each selected chapter, including RAV alternatives, transpositions and the custom-FEN source",
  );
  const beforeGlobalDrill = srsRows();
  await openTraining();
  session = await start("full_drill");
  expect(session.total).toBe(globalDecisions.map.size);
  await complete(globalDecisions.map, null);
  expect(srsRows()).toEqual(beforeGlobalDrill);
  record(
    "All-chapter Full Drill deduplicates shared and transposed decisions globally and preserves SRS schedules",
  );

  const exportPath = join(artifacts, "five-chapters-export.pgn");
  const sourcePgn = await native("knowledge_export", {
    documentId: imported.id,
  });
  await writeFile(exportPath, sourcePgn);
  const roundtrip = await native("knowledge_import", {
    request: {
      title: "Five-chapter roundtrip",
      side: "white",
      pgn: await readFile(exportPath, "utf8"),
    },
  });
  expect(roundtrip.chapters.map((item) => item.title)).toEqual(titles);
  expect(roundtrip.chapters.map((item) => semantic(item.root))).toEqual(
    imported.chapters.map((item) => semantic(item.root)),
  );
  await page.reload();
  const reloaded = await native("knowledge_load", { documentId: imported.id });
  expect(reloaded.chapters).toEqual(imported.chapters);
  await page
    .getByRole("button", { name: "Открыть общий репертуар", exact: true })
    .click();
  await chapterSelect(viewer).selectOption({ label: custom.chapters[0].title });
  await expect(viewer.locator(".combined-branches strong")).toContainText("g3");
  await expect(viewer.locator(".combined-branches strong")).toHaveCount(1);
  const customBoard = viewer.locator(".combined-board cg-board");
  await expect
    .poll(() =>
      customBoard.locator("piece.white.knight").evaluateAll((nodes) =>
        nodes.some((node) => {
          const square = node.getBoundingClientRect(),
            board = node.closest("cg-board").getBoundingClientRect();
          return (
            Math.abs(
              (square.x + square.width / 2 - board.x) / board.width - 5.5 / 8,
            ) < 0.02 &&
            Math.abs(
              (square.y + square.height / 2 - board.y) / board.height - 5.5 / 8,
            ) < 0.02
          );
        }),
      ),
    )
    .toBe(true);
  const customBox = await customBoard.boundingBox();
  await page.mouse.click(
    customBox.x + (6.5 * customBox.width) / 8,
    customBox.y + (6.5 * customBox.height) / 8,
  );
  await page.mouse.click(
    customBox.x + (6.5 * customBox.width) / 8,
    customBox.y + (5.5 * customBox.height) / 8,
  );
  await expect(viewer.locator(".combined-comment")).toContainText(
    "Prepare Bg2 from this exact position.",
  );
  await capture("custom-fen-chapter-light-1440.png");
  record(
    "Custom-FEN chapter preview displays the actual starting position and responds to real Chessground input",
  );
  await chapterSelect(viewer).selectOption({ label: "Five chapters: Italian" });
  await expect(viewer.locator(".combined-branches strong")).toContainText("e4");
  await capture("named-chapter-light-1440.png");
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "dark";
  });
  await capture("named-chapter-dark-1440.png");
  await page.setViewportSize({ width: 900, height: 960 });
  await capture("named-chapter-dark-900.png");
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "light";
  });
  await capture("named-chapter-light-900.png");
  record(
    "All five original names, RAV and comments survive reload and PGN export/reimport",
  );
  expect(errors).toEqual([]);
  await writeFile(
    join(artifacts, "multichapter-report.json"),
    JSON.stringify(
      {
        checks,
        errors,
        chapters: titles,
        scopedDecisionCount: sourceChapters.reduce(
          (sum, { chapter }) =>
            sum + decisions({ chapters: [chapter] }).map.size,
          0,
        ),
        globalDecisionCount: globalDecisions.map.size,
      },
      null,
      2,
    ),
  );
} catch (error) {
  await page
    .screenshot({
      path: join(artifacts, "multichapter-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  await writeFile(
    join(artifacts, "multichapter-failure.json"),
    JSON.stringify(
      {
        error: String(error),
        session: await last().catch(() => null),
        html: await page.content().catch(() => ""),
      },
      null,
      2,
    ),
  );
  throw error;
} finally {
  await page.context().close();
  await browser.close();
  await nativeBrowser.close();
  await vite.close();
}

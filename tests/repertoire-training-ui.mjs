// Actual Rust/SQLite commands and real Chessground mouse input in an isolated
// native profile. No training answers or scheduling responses are mocked.
import { chromium, expect } from "@playwright/test";
import { createServer } from "vite";
import react from "@vitejs/plugin-react";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { DatabaseSync } from "node:sqlite";

const cdp = process.env.CHESSPUSH_TEST_CDP;
if (!cdp || !process.env.CHESSPUSH_TEST_DATA)
  throw new Error(
    "Use tools/check-repertoire-training.mjs or set isolated CHESSPUSH_TEST_CDP and CHESSPUSH_TEST_DATA",
  );
const artifacts = resolve(
  process.env.CHESSPUSH_TRAINING_ARTIFACTS ?? ".qa/repertoire-training",
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
  const envelope = JSON.parse(
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
  if (!envelope.ok) throw envelope.error;
  return envelope.value;
}
await native("knowledge_attach_file", {
  path: resolve("tests/fixtures/repertoire-training-white.pgn"),
  side: "white",
});
await native("knowledge_attach_file", {
  path: resolve("tests/fixtures/repertoire-training-black.pgn"),
  side: "black",
});
const white = await native("knowledge_repertoire", { side: "white" });
const black = await native("knowledge_repertoire", { side: "black" });
function decisions(document, side) {
  const result = new Map(),
    occurrences = new Map();
  function visit(node) {
    if (node.fen.split(" ")[1] === side[0] && node.children.length) {
      const entry = result.get(node.position_key) ?? new Map();
      for (const child of node.children)
        if (child.uci) entry.set(child.uci, child.san);
      result.set(node.position_key, entry);
      occurrences.set(
        node.position_key,
        (occurrences.get(node.position_key) ?? 0) + 1,
      );
    }
    node.children.forEach(visit);
  }
  document.chapters.forEach((chapter) => visit(chapter.root));
  return { result, occurrences };
}
const whiteDecisions = decisions(white, "white"),
  blackDecisions = decisions(black, "black");
expect(
  [...whiteDecisions.occurrences.values()].some((count) => count > 1),
).toBe(true);
expect(
  [...whiteDecisions.result.values()].some(
    (moves) => moves.has("c2c4") && moves.has("e2e3"),
  ),
).toBe(true);

const vite = await createServer({
  configFile: false,
  cacheDir: ".qa/repertoire-training-vite-cache",
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
  if (!req.url?.startsWith("/training-harness")) return next();
  res.setHeader("Content-Type", "text/html");
  res.end(
    await vite.transformIndexHtml(
      "/training-harness",
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
await page.exposeFunction("__trainingNative", async (command, args) => {
  try {
    return JSON.stringify({ ok: true, value: await native(command, args) });
  } catch (error) {
    return JSON.stringify({ ok: false, error });
  }
});
await page.addInitScript(() => {
  window.isTauri = true;
  window.__trainingTest = { calls: [], last: null };
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback() {
      return 1;
    },
    async invoke(command, args) {
      if (command.startsWith("plugin:event|")) return 1;
      const envelope = JSON.parse(await window.__trainingNative(command, args));
      if (!envelope.ok) throw envelope.error;
      if (command.startsWith("training_")) {
        window.__trainingTest.calls.push({
          command,
          args,
          value: envelope.value,
        });
        if (command !== "training_overview")
          window.__trainingTest.last = envelope.value;
      }
      return envelope.value;
    },
  };
});
const trainer = page.locator(".repertoire-training");
const board = page.locator(".training-board cg-board");
const last = () => page.evaluate(() => window.__trainingTest.last);
const record = (text) => {
  checks.push(text);
  console.log("PASS", text);
};
async function sessionFeedback(...values) {
  await expect.poll(async () => (await last())?.feedback).toBeTruthy();
  await expect
    .poll(async () => values.includes((await last())?.feedback))
    .toBe(true);
  return last();
}
async function square(name) {
  const box = await board.boundingBox();
  if (!box) throw new Error("Training board unavailable");
  const orientation = await board.evaluate((element) =>
    element.closest(".cg-wrap").classList.contains("orientation-black")
      ? "black"
      : "white",
  );
  const file = name.charCodeAt(0) - 97,
    rank = Number(name[1]) - 1;
  return {
    x:
      box.x +
      (((orientation === "white" ? file : 7 - file) + 0.5) * box.width) / 8,
    y:
      box.y +
      (((orientation === "white" ? 7 - rank : rank) + 0.5) * box.height) / 8,
  };
}
async function play(uci, drag = false) {
  const state = await last();
  await expect(trainer.locator(".training-feedback")).toHaveClass(
    new RegExp(state.feedback),
  );
  await expect(trainer.locator(".training-feedback")).not.toContainText(
    "Восстановление",
  );
  if (state.feedback === "awaiting")
    await expect(
      trainer.getByRole("button", { name: "Подсказка", exact: true }),
    ).toBeEnabled();
  const from = await square(uci.slice(0, 2)),
    to = await square(uci.slice(2, 4));
  const prior = await page.evaluate(() => window.__trainingTest.calls.length);
  if (drag) {
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 12 });
    await page.mouse.up();
  } else {
    await page.mouse.click(from.x, from.y);
    await page.mouse.click(to.x, to.y);
  }
  await expect
    .poll(() => page.evaluate(() => window.__trainingTest.calls.length))
    .toBeGreaterThan(prior);
}
async function openTrainer(side = "white") {
  if (await trainer.isVisible())
    await trainer
      .getByRole("button", { name: "К репертуару", exact: true })
      .click();
  const backToFiles = page.getByRole("button", {
    name: "← К файлам",
    exact: true,
  });
  if (await backToFiles.isVisible()) await backToFiles.click();
  const sideButton = page.getByRole("button", {
    name: side === "white" ? "За белых" : "За чёрных",
    exact: true,
  });
  if (await sideButton.isVisible()) await sideButton.click();
  await page.getByRole("button", { name: "Тренировать", exact: true }).click();
  await expect(trainer).toBeVisible();
  await trainer.getByLabel("Начальная позиция").selectOption("");
}
async function start(mode, resume = false) {
  await trainer
    .getByRole("button", {
      name: mode === "srs" ? "Интервальное повторение" : "Полный дрилл",
      exact: true,
    })
    .click();
  await page.evaluate(() => {
    window.__trainingTest.last = null;
  });
  await trainer
    .getByRole("button", {
      name: resume ? "Продолжить" : "Начать",
      exact: true,
    })
    .click();
  await sessionFeedback("awaiting", "retry", "correct", "assisted", "finished");
  await expect(board).toBeVisible();
  return last();
}
async function next(previous) {
  previous ??= await last();
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
    const latest = await last();
    if (
      latest.finished ||
      latest.prompt?.decision_id !== previous.prompt?.decision_id
    )
      return latest;
    throw error;
  }
  await expect
    .poll(async () => {
      const value = await last();
      return (
        value.finished ||
        value.prompt?.decision_id !== previous.prompt?.decision_id
      );
    })
    .toBe(true);
  return last();
}
function expectedMoves(prompt, map) {
  const expected = map.get(prompt.position_key);
  expect(expected, `Unknown prompt position ${prompt.fen}`).toBeTruthy();
  return [...expected.keys()];
}
async function complete(map, initialSeen = new Set()) {
  const seen = new Set(initialSeen);
  let session = await last();
  for (let guard = 0; !session.finished && guard < 100; guard++) {
    if (session.feedback === "correct" || session.feedback === "assisted") {
      session = await next(session);
      continue;
    }
    expect(
      seen.has(session.prompt.position_key),
      "A previously covered transposed decision was repeated",
    ).toBe(false);
    seen.add(session.prompt.position_key);
    const choices = expectedMoves(session.prompt, map);
    const uci =
      session.mode === "srs" || session.scope.side === "black"
        ? choices.at(-1)
        : choices[0];
    await play(uci, guard % 2 === 0);
    session = await sessionFeedback("correct", "assisted");
  }
  expect(session.finished).toBe(true);
  expect(session.completed).toBe(session.total);
  expect(seen.size).toBe(session.total);
  return { session, seen };
}
function srsSnapshot() {
  const db = new DatabaseSync(
    join(process.env.CHESSPUSH_TEST_DATA, "chesspush.sqlite3"),
    { readOnly: true },
  );
  try {
    return db
      .prepare("SELECT * FROM repertoire_training_srs ORDER BY decision_id")
      .all();
  } finally {
    db.close();
  }
}
async function captureNativeApp() {
  // The actual application shell is inspected separately from the component
  // harness so the native navigation height is included in the geometry check.
  await nativePage.setViewportSize({ width: 1440, height: 960 });
  await nativePage
    .locator(".app-header")
    .getByRole("button", { name: "Репертуар", exact: true })
    .click();
  await nativePage
    .getByRole("button", { name: "За белых", exact: true })
    .click();
  await nativePage
    .getByRole("button", { name: "Тренировать", exact: true })
    .click();
  const nativeTrainer = nativePage.locator(".repertoire-training");
  await nativeTrainer.getByLabel("Начальная позиция").selectOption("");
  await nativeTrainer
    .getByRole("button", { name: "Полный дрилл", exact: true })
    .click();
  await nativeTrainer
    .getByRole("button", { name: "Начать", exact: true })
    .click();
  await expect(
    nativeTrainer.getByRole("button", { name: "Подсказка", exact: true }),
  ).toBeEnabled();
  await nativeTrainer
    .getByRole("button", { name: "Подсказка", exact: true })
    .click();
  await expect(nativeTrainer.locator(".training-feedback.retry")).toContainText(
    "Повторите ход",
  );
  async function theme(value) {
    await nativePage
      .getByRole("button", { name: "Настройки", exact: true })
      .click();
    await nativePage
      .getByRole("tab", { name: "Внешний вид", exact: true })
      .click();
    await nativePage.getByLabel("Тема", { exact: true }).selectOption(value);
    await nativePage
      .getByRole("button", { name: "Закрыть настройки", exact: true })
      .click();
    await expect(nativePage.locator("html")).toHaveAttribute(
      "data-theme",
      value,
    );
  }
  for (const width of [1440, 900]) {
    await nativePage.setViewportSize({ width, height: 960 });
    for (const value of ["light", "dark"]) {
      await theme(value);
      const boardBox = await nativeTrainer.locator("cg-board").boundingBox();
      const controls = await nativeTrainer
        .locator(".training-board-controls")
        .boundingBox();
      expect(boardBox.x + boardBox.width).toBeLessThanOrEqual(width + 1);
      expect(controls.y + controls.height).toBeLessThanOrEqual(961);
      await nativePage.screenshot({
        path: join(artifacts, `native-${value}-${width}.png`),
      });
    }
  }
  record(
    "Actual native App navigation, trainer entry and persisted theme settings fit the board and controls at 1440 and 900 pixels",
  );
}
try {
  await page.goto(
    `http://127.0.0.1:${vite.httpServer.address().port}/training-harness`,
  );
  await openTrainer();
  let session = await start("full_drill");
  expect(session.total).toBe(whiteDecisions.result.size);
  const initialSrs = srsSnapshot();
  const first = session.prompt;
  await expect(
    trainer.getByRole("button", { name: "Подсказка", exact: true }),
  ).toBeEnabled();
  const beforeDrawing = await page.evaluate(
    () => window.__trainingTest.calls.length,
  );
  const arrowFrom = await square("g1"),
    arrowTo = await square("f3"),
    circle = await square("d4");
  await page.mouse.move(arrowFrom.x, arrowFrom.y);
  await page.mouse.down({ button: "right" });
  await page.mouse.move(arrowTo.x, arrowTo.y, { steps: 12 });
  await page.mouse.up({ button: "right" });
  await page.mouse.click(circle.x, circle.y, { button: "right" });
  await expect
    .poll(() => page.locator(".training-board svg.cg-shapes g > *").count())
    .toBeGreaterThanOrEqual(2);
  expect(await page.evaluate(() => window.__trainingTest.calls.length)).toBe(
    beforeDrawing,
  );
  record(
    "Official Chessground arrow and circle interaction does not submit a training answer",
  );
  expect(first.legal_moves.some((move) => move.uci === "a2a3")).toBe(true);
  await play("a2a3");
  session = await sessionFeedback("retry");
  expect(session.mistakes).toBe(1);
  await expect(trainer.locator(".training-feedback")).toContainText(
    "Повторите ход",
  );
  expect(session.prompt.accepted_moves.length).toBeGreaterThan(1);
  await trainer.locator(".training-comments > summary").click();
  await expect(trainer.locator(".training-comments")).toContainText(
    "Occupy the centre.",
  );
  await page.screenshot({
    path: join(artifacts, "full-drill-wrong-light-1440.png"),
  });
  await play("d2d4");
  session = await sessionFeedback("assisted");
  expect(session.mistakes).toBe(1);
  const seen = new Set([first.position_key]);
  record(
    "Wrong legal move reveals preparation; real board retry stays assisted and records one error",
  );
  session = await next();
  const second = session.prompt;
  await trainer.getByRole("button", { name: "Подсказка", exact: true }).click();
  session = await sessionFeedback("retry");
  expect(session.prompt.accepted_moves.length).toBeGreaterThan(0);
  const hintedMistakes = session.mistakes;
  await play(expectedMoves(session.prompt, whiteDecisions.result)[0], true);
  session = await sessionFeedback("assisted");
  expect(session.mistakes).toBe(hintedMistakes);
  seen.add(second.position_key);
  record("Hint and drag retry cannot become an unassisted success");
  await expect
    .poll(async () => (await last()).prompt?.decision_id, { timeout: 7000 })
    .not.toBe(second.decision_id);
  session = await last();
  expect(session.prompt.context.some((move) => move.uci === "d7d5")).toBe(true);
  record(
    "An unannotated correct retry automatically plays the opponent continuation without a Next click",
  );
  const id = session.id,
    covered = session.completed;
  await page.reload();
  await openTrainer();
  session = await start("full_drill", true);
  expect(session.id).toBe(id);
  expect(session.completed).toBe(covered);
  const full = await complete(whiteDecisions.result, seen);
  expect(full.session.mistakes).toBe(hintedMistakes);
  expect(srsSnapshot()).toEqual(initialSrs);
  record(
    "Full Drill resumes persisted coverage, visits every unique decision including rare replies, and leaves SRS untouched",
  );

  await openTrainer();
  await trainer
    .getByLabel("Начальная позиция")
    .selectOption(white.chapters[0].id);
  function findUci(node, uci) {
    if (node.uci === uci) return node;
    for (const child of node.children) {
      const found = findUci(child, uci);
      if (found) return found;
    }
  }
  const rareBranch = findUci(white.chapters[0].root, "g7g6");
  await trainer.getByLabel("Ветка").selectOption(rareBranch.id);
  const rare = decisions(
    { chapters: [{ ...white.chapters[0], root: rareBranch }] },
    "white",
  );
  session = await start("full_drill");
  expect(session.total).toBe(rare.result.size);
  expect(session.total).toBe(2);
  await complete(rare.result);
  expect(srsSnapshot()).toEqual(initialSrs);
  record(
    "Chapter and branch selection restrict coverage to the chosen rare variation; PGN comments remain accessible",
  );

  await openTrainer();
  session = await start("srs");
  expect(session.total).toBe(whiteDecisions.result.size);
  const [accepted] = expectedMoves(session.prompt, whiteDecisions.result);
  const wrong = session.prompt.legal_moves.find(
    (move) =>
      !expectedMoves(session.prompt, whiteDecisions.result).includes(move.uci),
  );
  expect(wrong).toBeTruthy();
  await play(wrong.uci);
  await sessionFeedback("retry");
  const failedSrs = srsSnapshot();
  expect(failedSrs).not.toEqual(initialSrs);
  await play(accepted, true);
  session = await sessionFeedback("assisted");
  expect(srsSnapshot()).toEqual(failedSrs);
  await complete(whiteDecisions.result, new Set([session.prompt.position_key]));
  const overview = await native("training_overview", {
    scope: { side: "white" },
  });
  expect(overview.due).toBe(0);
  expect(overview.next_due_at_ms).toBeGreaterThan(Date.now());
  record(
    "SRS persists failure once, excludes guided retry from success, and schedules all completed positions",
  );

  // Advance one isolated fixture card to its review date; the remaining cards
  // keep the real schedules produced by the preceding SRS session.
  const overdue = [...rare.result.keys()].find((key) =>
    rare.result.get(key).has("b1c3"),
  );
  const fixtureDb = new DatabaseSync(
    join(process.env.CHESSPUSH_TEST_DATA, "chesspush.sqlite3"),
  );
  try {
    expect(
      fixtureDb
        .prepare(
          "UPDATE repertoire_training_srs SET due_at_ms=0 WHERE position_key=?",
        )
        .run(overdue).changes,
    ).toBe(1);
  } finally {
    fixtureDb.close();
  }
  await openTrainer();
  await trainer
    .getByLabel("Начальная позиция")
    .selectOption(white.chapters[0].id);
  await trainer.getByLabel("Ветка").selectOption(rareBranch.id);
  session = await start("srs");
  expect(session.total).toBe(1);
  expect(session.prompt.position_key).toBe(overdue);
  expect(session.prompt.context.length).toBeGreaterThanOrEqual(4);
  await complete(new Map([[overdue, rare.result.get(overdue)]]));
  record(
    "SRS branch selection reviews only the overdue decision and automatically restores context across mastered positions",
  );

  await openTrainer("black");
  session = await start("full_drill");
  expect(session.total).toBe(blackDecisions.result.size);
  expect(session.prompt.fen.split(" ")[1]).toBe("b");
  expect(session.prompt.context.some((move) => move.uci === "e2e4")).toBe(true);
  await expect(
    trainer.getByRole("button", { name: "Подсказка", exact: true }),
  ).toBeEnabled();
  await expect(page.locator(".training-board .cg-wrap")).toHaveClass(
    /orientation-black/,
  );
  await page.screenshot({
    path: join(artifacts, "black-drill-light-1440.png"),
  });
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "dark";
  });
  await page.screenshot({ path: join(artifacts, "black-drill-dark-1440.png") });
  await page.setViewportSize({ width: 900, height: 700 });
  await page.screenshot({ path: join(artifacts, "black-drill-dark-900.png") });
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "light";
  });
  await page.screenshot({ path: join(artifacts, "black-drill-light-900.png") });
  await page.setViewportSize({ width: 900, height: 960 });
  await page.screenshot({
    path: join(artifacts, "black-drill-light-900x960.png"),
  });
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "dark";
  });
  await page.screenshot({
    path: join(artifacts, "black-drill-dark-900x960.png"),
  });
  const box = await board.boundingBox();
  expect(box.width).toBeGreaterThan(300);
  expect(box.x + box.width).toBeLessThanOrEqual(901);
  const srsBeforeBlack = srsSnapshot();
  await complete(blackDecisions.result);
  expect(srsSnapshot()).toEqual(srsBeforeBlack);
  record(
    "Black repertoire automatically supplies white context, uses black board orientation and covers both prepared replies",
  );
  if (process.env.CHESSPUSH_TRAINING_NATIVE_CAPTURE !== "0")
    await captureNativeApp();
  expect(errors).toEqual([]);
  await writeFile(
    join(artifacts, "report.json"),
    JSON.stringify(
      {
        checks,
        whiteDecisions: whiteDecisions.result.size,
        blackDecisions: blackDecisions.result.size,
        errors,
      },
      null,
      2,
    ),
  );
} catch (error) {
  await page
    .screenshot({ path: join(artifacts, "failure.png"), fullPage: true })
    .catch(() => {});
  await writeFile(
    join(artifacts, "failure.json"),
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

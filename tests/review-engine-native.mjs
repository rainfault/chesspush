// Real app UI + Rust + official Stockfish. Run through the existing isolated
// profile lifecycle with CHESSPUSH_TRAINING_TEST_SCRIPT set to this file.
import { chromium, expect as baseExpect } from "@playwright/test";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { DatabaseSync } from "node:sqlite";
import { createHash, randomUUID } from "node:crypto";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve, join, sep } from "node:path";

const expect = baseExpect.configure({ timeout: 25_000 });
const run = promisify(execFile);
const profile = process.env.CHESSPUSH_TEST_DATA;
const endpoint = process.env.CHESSPUSH_TEST_CDP;
if (!profile || !endpoint || !resolve(profile).startsWith(resolve(".qa") + sep))
  throw new Error("Use the existing isolated native test-profile runner");
const stockfish = resolve(
  process.env.CHESSPUSH_STOCKFISH ??
    ".qa/stockfish19/release/stockfish/stockfish-windows-x86-64-universal.exe",
);
if (!stockfish.startsWith(resolve(".qa") + sep))
  throw new Error("The real-engine fixture must be inside ignored .qa");
const artifacts = resolve(process.env.CHESSPUSH_TRAINING_ARTIFACTS);
await mkdir(artifacts, { recursive: true });
const fixtureExecutable = process.env.CHESSPUSH_REVIEW_FIXTURE_EXECUTABLE;
if (
  fixtureExecutable &&
  !resolve(fixtureExecutable).startsWith(resolve("src-tauri/target") + sep)
)
  throw new Error(
    "Compiled fixture must belong to this checkout's target directory",
  );
const seeded = await run(
  fixtureExecutable ?? "cargo",
  fixtureExecutable
    ? [profile]
    : [
        "run",
        "--manifest-path",
        "src-tauri/Cargo.toml",
        "--no-default-features",
        "--example",
        "statistics_fixture",
        "--",
        profile,
      ],
  {
    cwd: resolve("."),
    windowsHide: true,
    timeout: 180_000,
    maxBuffer: 4 * 1024 * 1024,
  },
);
process.stdout.write(seeded.stdout);
process.stderr.write(seeded.stderr);
const browser = await chromium.connectOverCDP(endpoint);
const page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(25_000);
await page.setViewportSize({ width: 1440, height: 960 });
const errors = [],
  checks = [];
page.on("pageerror", (error) => errors.push(error.message));
await page.waitForFunction(
  () =>
    window.__TAURI_INTERNALS__?.invoke && document.querySelector(".app-header"),
);
const calls = [];
async function native(command, args = {}) {
  const envelope = JSON.parse(
    await page.evaluate(
      async ({ command, args }) => {
        try {
          return JSON.stringify({
            ok: true,
            data: await window.__TAURI_INTERNALS__.invoke(command, args),
          });
        } catch (error) {
          return JSON.stringify({ ok: false, error });
        }
      },
      { command, args },
    ),
  );
  if (!envelope.ok)
    throw new Error(`${command}: ${JSON.stringify(envelope.error)}`);
  return envelope.data;
}
function database(read) {
  const db = new DatabaseSync(join(profile, "chesspush.sqlite3"), {
    readOnly: true,
  });
  try {
    db.exec("PRAGMA busy_timeout=5000");
    return read(db);
  } finally {
    db.close();
  }
}
function facts() {
  return database((db) =>
    Object.fromEntries(
      [
        "games",
        "statistics_games",
        "statistics_payloads",
        "statistics_account_games",
      ].map((table) => {
        const rows = db
          .prepare(`SELECT * FROM "${table}"`)
          .all()
          .map((r) => JSON.stringify(r))
          .sort();
        return [
          table,
          createHash("sha256").update(JSON.stringify(rows)).digest("hex"),
        ];
      }),
    ),
  );
}
async function engineProcesses() {
  const name = stockfish.split(/[\\/]/).at(-1).replaceAll("'", "''");
  const result = await run(
    "powershell.exe",
    [
      "-NoProfile",
      "-Command",
      `@(Get-CimInstance Win32_Process -Filter "Name='${name}'" | Select-Object ProcessId,ParentProcessId,ExecutablePath) | ConvertTo-Json -Compress`,
    ],
    { windowsHide: true, timeout: 20_000 },
  );
  if (!result.stdout.trim()) return [];
  const values = JSON.parse(result.stdout);
  return (Array.isArray(values) ? values : [values]).filter(
    (p) => p.ExecutablePath?.toLowerCase() === stockfish.toLowerCase(),
  );
}
const baselinePids = new Set((await engineProcesses()).map((p) => p.ProcessId));
const ownedEngines = async () =>
  (await engineProcesses()).filter((p) => !baselinePids.has(p.ProcessId));
const nav = (name) =>
  page
    .getByRole("navigation", { name: "Разделы", exact: true })
    .getByRole("button", { name, exact: true })
    .click();
const engine = () =>
  page.getByRole("region", { name: "Анализ позиции", exact: true });
const selected = (id) => page.locator(`[data-knowledge-node="${id}"]`);
async function selectNode(id) {
  await selected(id).click();
  await expect(selected(id)).toHaveAttribute("aria-current", "step");
  await expect(page.locator(".knowledge-board-panel")).toHaveAttribute(
    "aria-busy",
    "false",
  );
}
const score = (evaluation) =>
  `${evaluation.bound === "lower" ? "≥" : evaluation.bound === "upper" ? "≤" : ""}${evaluation.mate != null ? `#${evaluation.mate}` : `${evaluation.cp >= 0 ? "+" : ""}${(evaluation.cp / 100).toLocaleString("ru-RU", { minimumFractionDigits: 1, maximumFractionDigits: 2 })}`}`;
async function settled(nodeId) {
  await expect(selected(nodeId)).toHaveAttribute("aria-current", "step");
  await expect(engine().getByRole("status")).toContainText(
    /Stockfish 19 · Глубина [1-9]/,
  );
  const current = await native("knowledge_load", {
    documentId: reviewContext.document.id,
    revision: null,
  });
  const args = {
    documentId: current.id,
    revision: current.revision,
    chapterId: reviewContext.chapter_id,
    nodeId,
    requestId: randomUUID(),
  };
  // The UI search has finished. Read the same canonical context from the real
  // backend cache to verify displayed score/depth without replacing IPC.
  const value = { args, result: await native("game_review_evaluate", args) };
  calls.push(value);
  await expect(engine().locator(".review-evaluation")).toHaveText(
    score(value.result.evaluation),
  );
  await expect(engine().getByRole("status")).toContainText(
    `Глубина ${value.result.evaluation.depth}`,
  );
  return value;
}
let appPid, reviewContext;
try {
  const before = facts();
  await native("statistics_engine_pause", { paused: true });
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  const settings = page.getByRole("dialog", { name: "Настройки", exact: true });
  await settings.getByRole("tab", { name: "Движок", exact: true }).click();
  await settings
    .getByRole("textbox", { name: "Путь к Stockfish", exact: true })
    .fill(stockfish);
  await settings
    .getByRole("combobox", { name: "Режим анализа", exact: true })
    .selectOption("quick");
  await settings.getByText("Дополнительные настройки", { exact: true }).click();
  await settings
    .getByRole("spinbutton", { name: "Hash Stockfish", exact: true })
    .fill("32");
  await settings
    .getByRole("spinbutton", { name: "Лимит узлов Stockfish", exact: true })
    .fill("1000000");
  await settings
    .getByRole("spinbutton", { name: "Лимит времени Stockfish", exact: true })
    .fill("1000");
  await settings
    .getByRole("button", { name: "Сохранить движок", exact: true })
    .click();
  await expect(settings.getByRole("status")).toHaveText(
    "Движок готов. Настройки сохранены.",
  );
  await settings
    .getByRole("button", { name: "Закрыть настройки", exact: true })
    .click();
  const configured = await native("statistics_engine_config");
  expect(configured.path).toBe(stockfish);
  checks.push(
    "Official Stockfish configured and validated through actual Settings UI",
  );

  const list = await native("game_review_list", { request: { limit: 100 } });
  const source = list.rows.find((r) => r.game_id.includes("stTr0001"));
  await nav("Разбор партий");
  await page
    .locator(`[data-game-id="${source.game_id}"]`)
    .getByRole("button")
    .click();
  await expect(
    page.getByRole("region", { name: "Редактор разбора партии", exact: true }),
  ).toBeVisible();
  const review = await native("game_review_open", {
    gameId: source.game_id,
    accountId: source.account_id,
    sourceRevision: source.source_revision,
  });
  reviewContext = review;
  const last = review.source_nodes.at(-1);
  await selectNode(last.node_id);
  await engine()
    .getByRole("checkbox", { name: "Stockfish", exact: true })
    .check();
  const first = await settled(last.node_id);
  expect(first.result.engine.name).toMatch(/Stockfish 19/);
  expect(first.result.evaluation.depth).toBeGreaterThan(0);
  expect(first.result.evaluation.nodes).toBeGreaterThan(0);
  expect(first.result.evaluation.pv.length).toBeGreaterThan(0);
  expect(first.result.evaluation.ply).toBe(last.ply);
  await expect(
    engine().getByLabel("Вариант движка", { exact: true }),
  ).not.toBeEmpty();
  const position = await native("knowledge_navigate", {
    documentId: review.document.id,
    chapterId: last.chapter_id,
    nodeId: last.node_id,
    revision: review.document.revision,
  });
  const best = first.result.evaluation.pv[0];
  expect(position.legal_moves.some((move) => move.uci === best)).toBe(true);
  const cache = database((db) =>
    db
      .prepare("SELECT context_json FROM statistics_engine_observations")
      .all()
      .map((r) => JSON.parse(r.context_json)),
  );
  expect(
    cache.some(
      (c) =>
        c[2] === position.initial_fen &&
        JSON.stringify(c[3]) === JSON.stringify(position.path_uci) &&
        c[5] === first.result.engine.profile_id,
    ),
  ).toBe(true);
  checks.push(
    "Actual score/depth/PV uses canonical full game history and distinct budget-specific engine cache",
  );
  await page.screenshot({
    path: join(artifacts, "review-engine-light.png"),
    fullPage: true,
  });

  await engine()
    .getByRole("button", { name: /^Сыграть / })
    .click();
  await expect
    .poll(
      async () =>
        (
          await native("knowledge_load", {
            documentId: review.document.id,
            revision: null,
          })
        ).revision,
    )
    .toBeGreaterThan(review.document.revision);
  await expect(page.locator(".knowledge-header [role=status]")).toHaveText(
    "Сохранено",
  );
  const changed = await native("knowledge_load", {
    documentId: review.document.id,
    revision: null,
  });
  const walk = (node) =>
    node.id === last.node_id ? node : node.children.map(walk).find(Boolean);
  const added = walk(changed.chapters[0].root).children.find(
    (node) => node.uci === best,
  );
  expect(added).toBeTruthy();
  await expect(selected(added.id)).toHaveAttribute("aria-current", "step");
  expect(facts()).toEqual(before);
  checks.push(
    "Play PV adds a legal saved analysis branch while every factual game/payload/membership row stays unchanged",
  );

  await engine()
    .getByRole("checkbox", { name: "Stockfish", exact: true })
    .uncheck();
  await expect.poll(async () => (await ownedEngines()).length).toBe(0);
  // A fresh profile keeps both race positions cold, so a genuine in-flight
  // request is superseded instead of merely hitting the saved cache.
  await native("statistics_engine_save", {
    config: {
      ...configured,
      custom_nodes: 20_000_000,
      custom_movetime_ms: 5000,
    },
  });
  const raceOld = review.source_nodes[2],
    raceNew = review.source_nodes[4];
  await selectNode(raceOld.node_id);
  await engine()
    .getByRole("checkbox", { name: "Stockfish", exact: true })
    .check();
  await expect
    .poll(async () => (await ownedEngines()).length)
    .toBeGreaterThan(0);
  const superseded = (await ownedEngines())[0];
  await selectNode(raceNew.node_id);
  await expect(engine().locator(".review-evaluation")).toHaveCount(0);
  await expect
    .poll(
      async () =>
        (await ownedEngines()).some(
          (p) => p.ProcessId === superseded.ProcessId,
        ),
      { timeout: 3000 },
    )
    .toBe(false);
  const final = await settled(raceNew.node_id);
  expect(final.args.nodeId).toBe(raceNew.node_id);
  await expect(selected(raceNew.node_id)).toHaveAttribute(
    "aria-current",
    "step",
  );
  checks.push(
    "Fast navigation cancels the prior request; the displayed result belongs to the selected node and revision",
  );

  await engine()
    .getByRole("checkbox", { name: "Stockfish", exact: true })
    .uncheck();
  await expect(engine().locator(".review-evaluation")).toHaveCount(0);
  await expect.poll(async () => (await ownedEngines()).length).toBe(0);
  expect(facts()).toEqual(before);
  expect(errors).toEqual([]);
  await writeFile(
    join(artifacts, "review-engine-results.json"),
    JSON.stringify(
      {
        pass: false,
        checks,
        engine: first.result.engine,
        first: first.result.evaluation,
        final: final.result.evaluation,
        calls,
        facts: before,
        errors,
      },
      null,
      2,
    ),
  );

  // The parent PID is obtained from this test's live engine child; this never
  // closes a user-owned installed ChessPush process.
  const shutdownNode = review.source_nodes[1];
  await selectNode(shutdownNode.node_id);
  await engine()
    .getByRole("checkbox", { name: "Stockfish", exact: true })
    .check();
  await expect
    .poll(async () => (await ownedEngines()).length)
    .toBeGreaterThan(0);
  const running = await ownedEngines();
  appPid = running[0].ParentProcessId;
  await run(
    "powershell.exe",
    [
      "-NoProfile",
      "-Command",
      `$reviewApp=Get-Process -Id ${appPid} -ErrorAction Stop; if($reviewApp.Path -ne '${resolve("src-tauri/target/debug/chesspush.exe").replaceAll("'", "''")}'){throw 'Unexpected engine parent'}; [void]$reviewApp.CloseMainWindow()`,
    ],
    { windowsHide: true, timeout: 20_000 },
  );
  await expect.poll(async () => (await ownedEngines()).length).toBe(0);
  await expect
    .poll(async () => {
      const result = await run(
        "powershell.exe",
        [
          "-NoProfile",
          "-Command",
          `if(Get-Process -Id ${appPid} -ErrorAction SilentlyContinue){'running'}else{'closed'}`,
        ],
        { windowsHide: true, timeout: 20_000 },
      );
      return result.stdout.trim();
    })
    .toBe("closed");
  checks.push(
    "Closing this isolated ChessPush during search waits for owned UCI cleanup; no Stockfish child remains",
  );
  const output = JSON.parse(
    await (
      await import("node:fs/promises")
    ).readFile(join(artifacts, "review-engine-results.json"), "utf8"),
  );
  output.checks = checks;
  output.pass = true;
  await writeFile(
    join(artifacts, "review-engine-results.json"),
    JSON.stringify(output, null, 2),
  );
  console.log(
    `PASS native review + official Stockfish, cancellation and shutdown; artifacts: ${artifacts}`,
  );
} catch (error) {
  await page
    .screenshot({
      path: join(artifacts, "review-engine-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  await writeFile(
    join(artifacts, "review-engine-failure.txt"),
    await page
      .locator("body")
      .innerText()
      .catch(() => "closed"),
  );
  throw error;
} finally {
  await browser.close().catch(() => {});
}

// Product repertoire acceptance against the actual isolated Rust/SQLite backend.
// Only native open/save picker selections are controlled by this browser harness.
import { expect } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { bridge } from "./helpers/product-bridge.mjs";
const config = JSON.parse(await readFile(".qa/ui-atlas-tools/current.json"));
const directory = resolve(".qa/repertoire-fragments-ui");
const screenshots = resolve("screens/simplification/repertoire");
await mkdir(directory, { recursive: true });
await mkdir(screenshots, { recursive: true });
const h = await bridge(config, "light"),
  p = h.page;
const errors = [];
p.on("pageerror", (error) => errors.push(error.message));
const result = {
  checks: [],
  fixtureProfile: config.directory,
  controlledBoundary:
    "Native open/save dialog selection only; file reads, legal PGN parse, classification, merge, save, export, graph projection and SQLite are actual Rust commands.",
};
const pgms = {
  "QA-ruy.pgn":
    '[Event "Ruy fragment"]\n[ECO "Z99"]\n[Opening "Invented"]\n\n1.e4 $1 {plan A [%cal Ge2e4]} e5 2.Nf3 Nc6 3.Bb5 a6 *',
  "QA-sicilian.pgn":
    '[Event "Sicilian fragment"]\n\n1.e4 c5 2.Nf3 d6 (2...Nc6 $220 {plan B}) *',
  "QA-fen.pgn":
    '[Event "Italian FEN fragment"]\n[SetUp "1"]\n[FEN "r1bqkbnr/pppp1ppp/2n5/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R w KQkq - 0 1"]\n\n1.Bc4 {from FEN} Bc5 (1...Nf6) *',
  "QA-french-black.pgn": '[Event "Black French"]\n\n1.e4 e6 2.d4 d5 *',
  "QA-broken.pgn": "1.e4 e5 2.Ke4 *",
};
for (const [name, text] of Object.entries(pgms))
  await writeFile(join(directory, name), text);
function factualHash() {
  const db = new DatabaseSync(join(config.data, "chesspush.sqlite3"), {
    readOnly: true,
  });
  const records = {};
  try {
    for (const table of [
      "games",
      "statistics_games",
      "statistics_payloads",
      "statistics_account_games",
      "statistics_analysis_runs",
    ])
      records[table] = db
        .prepare(`SELECT * FROM ${table} ORDER BY rowid`)
        .all();
    records.t0 = db
      .prepare("SELECT id,t0_ms FROM statistics_accounts ORDER BY id")
      .all();
    return createHash("sha256").update(JSON.stringify(records)).digest("hex");
  } finally {
    db.close();
  }
}
function leafPaths(document) {
  const paths = [];
  function walk(node, line) {
    if (!node.children.length) paths.push(line.join(" "));
    for (const child of node.children) walk(child, [...line, child.uci]);
  }
  for (const chapter of document.chapters) walk(chapter.root, []);
  return paths.sort();
}
const beforeHash = factualHash();
async function picker(files) {
  await p.evaluate(
    (files) => {
      window.__atlas.responses["plugin:dialog|open"] = files;
    },
    files.map((file) => join(directory, file)),
  );
}
async function openList() {
  await p
    .locator(".app-header")
    .getByRole("button", { name: "Репертуар", exact: true })
    .click();
  if (
    await p
      .getByRole("button", { name: "К файлам репертуара", exact: true })
      .isVisible()
  )
    await p
      .getByRole("button", { name: "К файлам репертуара", exact: true })
      .click();
  if (
    await p.getByRole("button", { name: "← К файлам", exact: true }).isVisible()
  )
    await p.getByRole("button", { name: "← К файлам", exact: true }).click();
  await expect(p.locator(".repertoire-list")).toBeVisible();
}
async function clickMove(san) {
  await p
    .locator(".combined-branches")
    .getByRole("button", { name: new RegExp(`^\\d+[. …]+${san}(?:\\s|$)`) })
    .click();
}
try {
  await openList();
  await p.getByRole("button", { name: "За белых", exact: true }).click();
  await picker(["QA-ruy.pgn", "QA-sicilian.pgn", "QA-fen.pgn"]);
  await p
    .getByRole("button", { name: /^Прикрепить PGN(?: за (?:белых|чёрных))?$/ })
    .click();
  await expect(
    p.locator(".repertoire-tile-filename", { hasText: "QA-fen.pgn" }),
  ).toBeVisible({ timeout: 30000 });
  let documents = await h.native("knowledge_list");
  const white = documents.filter(
    (item) =>
      item.attachment_active &&
      item.side === "white" &&
      item.filename?.startsWith("QA-"),
  );
  expect(white.length).toBe(3);
  const ruy = white.find((item) => item.filename === "QA-ruy.pgn");
  expect(
    ruy.openings.some((item) => item.name.includes("Ruy Lopez")),
  ).toBeTruthy();
  expect(
    ruy.openings.every(
      (item) => item.eco !== "Z99" && item.name !== "Invented",
    ),
  ).toBeTruthy();
  const fen = white.find((item) => item.filename === "QA-fen.pgn");
  expect(fen.openings).toEqual([]);
  for (const summary of white) {
    const source = await h.native("knowledge_load", { documentId: summary.id });
    expect(source.metadata.repertoire_attachment.source_pgn).toBe(
      pgms[summary.filename],
    );
  }
  await p.screenshot({
    path: join(screenshots, "light-attached-tiles.png"),
    fullPage: true,
  });
  result.checks.push({
    scenario:
      "Multi-file attach, automatic exact-position ECO, untrusted headers, explicit side and raw PGN preservation",
    result: "PASS",
  });
  await p
    .getByRole("button", { name: "Открыть общий репертуар", exact: true })
    .click();
  await expect(p.locator(".combined-repertoire cg-board")).toBeVisible();
  await clickMove("e4");
  await expect(p.locator(".combined-branches")).toContainText("e5");
  await expect(p.locator(".combined-branches")).toContainText("c5");
  const box = await p.locator(".combined-board cg-board").boundingBox(),
    cell = box.width / 8;
  await p.mouse.click(box.x + 4.5 * cell, box.y + 1.5 * cell);
  await p.mouse.click(box.x + 4.5 * cell, box.y + 3.5 * cell);
  await expect(p.locator(".combined-path")).toContainText("e5");
  await clickMove("Nf3");
  await clickMove("Nc6");
  await expect(p.locator(".combined-branches")).toContainText("Bb5");
  await expect(p.locator(".combined-branches")).toContainText("Bc4");
  await p.screenshot({
    path: join(screenshots, "light-merged-fen-and-opening.png"),
    fullPage: true,
  });
  await clickMove("Bc4");
  await expect(p.locator(".combined-path")).toContainText("3. Bc4");
  await p
    .getByRole("button", { name: "Открыть фрагмент QA-fen.pgn", exact: true })
    .click();
  const sourceBeforeEdit = await h.native("knowledge_load", {
    documentId: fen.id,
  });
  await expect(
    p.getByLabel("Комментарий к позиции", { exact: true }),
  ).toHaveValue(
    sourceBeforeEdit.chapters[0].root.children[0].comments.join("\n\n"),
  );
  const comment = `FEN fragment edit ${Date.now()}`;
  await p.getByLabel("Комментарий к позиции", { exact: true }).fill(comment);
  await p.getByRole("button", { name: "Сохранить", exact: true }).click();
  await expect(
    p.getByRole("button", { name: "Сохранить", exact: true }),
  ).toBeDisabled();
  await p
    .getByRole("button", { name: "К файлам репертуара", exact: true })
    .click();
  await p
    .getByRole("button", { name: "Открыть общий репертуар", exact: true })
    .click();
  await clickMove("e4");
  await clickMove("e5");
  await clickMove("Nf3");
  await clickMove("Nc6");
  await clickMove("Bc4");
  await expect(p.locator(".combined-comment")).toContainText(comment);
  result.checks.push({
    scenario:
      "Shared-prefix merge + FEN fragment graft, board navigation, source node edit and rebuilt union",
    result: "PASS",
  });
  const output = join(directory, "combined-white.pgn");
  await p.evaluate((path) => {
    window.__atlas.responses["plugin:dialog|save"] = path;
  }, output);
  await p.getByRole("button", { name: "Экспорт PGN", exact: true }).click();
  await expect
    .poll(async () => {
      try {
        return (await readFile(output, "utf8")).includes(comment);
      } catch {
        return false;
      }
    })
    .toBe(true);
  const exported = await readFile(output, "utf8");
  expect(exported).toContain("$220");
  expect(exported).toContain("plan B");
  expect(exported).toContain(comment);
  const combined = await h.native("knowledge_repertoire", { side: "white" });
  const roundtrip = await h.native("knowledge_import", {
    request: { title: "QA combined roundtrip", side: "white", pgn: exported },
  });
  expect(leafPaths(roundtrip)).toEqual(leafPaths(combined));
  result.checks.push({
    scenario:
      "Native save PGN + reimport preserves all leaf lines, RAV, NAG 220, comments and FEN context",
    result: "PASS",
  });
  await p.getByRole("button", { name: "← К файлам", exact: true }).click();
  await p
    .getByRole("button", { name: "Отсоединить QA-sicilian.pgn", exact: true })
    .click();
  await expect(
    p.locator(".repertoire-tile-filename", { hasText: "QA-sicilian.pgn" }),
  ).toBeHidden();
  documents = await h.native("knowledge_list");
  const sicilian = documents.find(
    (item) => item.filename === "QA-sicilian.pgn",
  );
  expect(sicilian.attachment_active).toBe(false);
  const noSicilian = await h.native("knowledge_repertoire", { side: "white" });
  expect(noSicilian.metadata.source_document_ids).not.toContain(sicilian.id);
  const detachedSource = await h.native("knowledge_load", {
    documentId: sicilian.id,
  });
  expect(detachedSource.metadata.repertoire_attachment.source_pgn).toBe(
    pgms["QA-sicilian.pgn"],
  );
  await p.getByText(/Отсоединённые файлы/).click();
  await p
    .locator(".repertoire-existing li", { hasText: "QA-sicilian.pgn" })
    .getByRole("button", { name: "Прикрепить снова", exact: true })
    .click();
  await expect(
    p.locator(".repertoire-tile-filename", { hasText: "QA-sicilian.pgn" }),
  ).toBeVisible();
  const beforeCount = (await h.native("knowledge_list")).length;
  await picker(["QA-ruy.pgn", "QA-broken.pgn"]);
  await p
    .getByRole("button", { name: /^Прикрепить PGN(?: за (?:белых|чёрных))?$/ })
    .click();
  await expect(p.getByRole("alert")).toContainText("QA-broken.pgn");
  expect((await h.native("knowledge_list")).length).toBe(beforeCount);
  result.checks.push({
    scenario:
      "Detach/reattach keeps source and node identities; repeated file idempotent; malformed PGN reports error without partial document",
    result: "PASS",
  });
  await p.getByRole("button", { name: "За чёрных", exact: true }).click();
  await picker(["QA-french-black.pgn"]);
  await p
    .getByRole("button", { name: /^Прикрепить PGN(?: за (?:белых|чёрных))?$/ })
    .click();
  await expect(
    p.locator(".repertoire-tile-filename", { hasText: "QA-french-black.pgn" }),
  ).toBeVisible();
  await expect(
    p.locator(".repertoire-tile-filename", { hasText: "QA-ruy.pgn" }),
  ).toBeHidden();
  const black = await h.native("knowledge_repertoire", { side: "black" });
  expect(black.metadata.source_document_ids).not.toContain(ruy.id);
  await p.reload();
  await p
    .locator(".app-header")
    .getByRole("button", { name: "Репертуар", exact: true })
    .click();
  await expect(
    p.getByRole("button", { name: "За чёрных", exact: true }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(
    p.locator(".repertoire-tile-filename", { hasText: "QA-french-black.pgn" }),
  ).toBeVisible();
  await p.evaluate(() => {
    document.documentElement.dataset.theme = "dark";
  });
  await p.screenshot({
    path: join(screenshots, "dark-attached-black.png"),
    fullPage: true,
  });
  const afterHash = factualHash();
  expect(afterHash).toBe(beforeHash);
  result.checks.push({
    scenario:
      "Separate white/black coverage, reload restores attached files and selected side, factual games/T0/engine runs remain byte-equivalent",
    result: "PASS",
    beforeHash,
    afterHash,
  });
  expect(errors).toEqual([]);
  result.result = "PASS";
} catch (error) {
  result.result = "FAIL";
  result.error = String(error);
  result.pageErrors = errors;
  await p
    .screenshot({ path: join(screenshots, "failure.png"), fullPage: true })
    .catch(() => {});
  console.log((await p.locator("body").innerText()).slice(-5000));
  throw error;
} finally {
  await writeFile(
    join(directory, "results.json"),
    JSON.stringify(result, null, 2),
  );
  await h.browser.close();
}
console.log(JSON.stringify(result, null, 2));

// Deletion acceptance uses an isolated native Rust/SQLite profile. No real user data.
import { expect } from "@playwright/test";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { bridge } from "./helpers/product-bridge.mjs";
const config = JSON.parse(await readFile(".qa/ui-atlas-tools/current.json"));
const profile = resolve(config.data);
if (!profile.startsWith(resolve(".qa") + "\\"))
  throw Error("This test only mutates isolated .qa profiles");
const out = resolve(".qa/repertoire-delete-ui");
await mkdir(out, { recursive: true });
const h = await bridge(config, "light"),
  p = h.page;
const errors = [];
p.on("pageerror", (error) => errors.push(error.message));
const checks = [];
function snapshot() {
  const db = new DatabaseSync(join(config.data, "chesspush.sqlite3"), {
    readOnly: true,
  });
  db.exec("PRAGMA busy_timeout=5000");
  try {
    const rows = {};
    for (const table of [
      "games",
      "statistics_games",
      "statistics_payloads",
      "statistics_account_games",
      "statistics_analysis_runs",
      "statistics_engine_observations",
      "statistics_move_evaluations",
    ])
      rows[table] = db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all();
    rows.t0 = db
      .prepare("SELECT id,t0_ms FROM statistics_accounts ORDER BY id")
      .all();
    return createHash("sha256").update(JSON.stringify(rows)).digest("hex");
  } finally {
    db.close();
  }
}
function documentCounts(id) {
  const db = new DatabaseSync(join(config.data, "chesspush.sqlite3"), {
    readOnly: true,
  });
  db.exec("PRAGMA busy_timeout=5000");
  try {
    return [
      "knowledge_documents",
      "knowledge_document_versions",
      "knowledge_node_index",
    ].map((table) =>
      Number(
        db
          .prepare(
            `SELECT count(*) AS n FROM ${table} WHERE ${table === "knowledge_documents" ? "id" : "document_id"}=?`,
          )
          .get(id).n,
      ),
    );
  } finally {
    db.close();
  }
}
const factualBefore = snapshot();
const path = join(out, "delete-attached.pgn"),
  pgn = "1.a3 a6 2.h3 h6 *";
await writeFile(path, pgn);
const document = await h.native("knowledge_attach_file", {
  path,
  side: "white",
});
const legacy = await h.native("knowledge_import", {
  request: { title: "Delete legacy source", side: "white", pgn: "1.b3 b6 *" },
});
async function openList() {
  await p
    .locator(".app-header")
    .getByRole("button", { name: "Репертуар", exact: true })
    .click();
  await expect(p.locator(".repertoire-list")).toBeVisible();
  await p.getByRole("button", { name: "За белых", exact: true }).click();
}
async function targetDelete(name) {
  await p.getByRole("button", { name: `Удалить ${name}`, exact: true }).click();
  await expect(p.getByRole("dialog")).toBeVisible();
  await expect(p.getByRole("dialog")).toContainText(name);
}
async function confirm() {
  await p
    .getByRole("dialog")
    .getByRole("button", { name: "Удалить", exact: true })
    .click();
  await expect(p.getByRole("dialog")).not.toBeVisible();
}
try {
  await openList();
  const beforeCounts = documentCounts(document.id);
  await targetDelete("delete-attached.pgn");
  await expect(
    p.getByRole("dialog").getByRole("button", { name: "Отмена", exact: true }),
  ).toBeFocused();
  await p.keyboard.press("Escape");
  await expect(p.getByRole("dialog")).not.toBeVisible();
  expect(documentCounts(document.id)).toEqual(beforeCounts);
  await targetDelete("delete-attached.pgn");
  await p
    .getByRole("dialog")
    .getByRole("button", { name: "Отмена", exact: true })
    .click();
  expect(documentCounts(document.id)).toEqual(beforeCounts);
  checks.push({
    scenario:
      "Named confirmation defaults focus to Cancel; Escape and Cancel preserve document, versions and index",
    result: "PASS",
  });

  // Simulate another editor committing while the old confirmation is open.
  await targetDelete("delete-attached.pgn");
  const edited = await h.native("knowledge_apply", {
    documentId: document.id,
    expectedRevision: document.revision,
    operation: { type: "rename", title: "Changed elsewhere", side: "white" },
  });
  await confirm();
  await expect(p.locator(".repertoire-list-error")).toContainText("изменён");
  expect(
    (await h.native("knowledge_load", { documentId: document.id })).revision,
  ).toBe(edited.revision);
  await p
    .locator(".repertoire-list-error")
    .getByRole("button", { name: "Повторить" })
    .click();
  checks.push({
    scenario:
      "Stale deletion confirmation is rejected after concurrent edit, refreshed source survives",
    result: "PASS",
  });

  const graph = await h.native("game_graph_query", {
    request: {
      filter: { color: "white", variant: "standard" },
      line: [],
      all_paths: false,
      repertoire_ids: [document.id],
    },
  });
  expect(
    graph.continuations.some(
      (edge) =>
        edge.uci === "a2a3" &&
        edge.prepared.some((source) => source.document_id === document.id),
    ),
  ).toBe(true);
  await targetDelete("delete-attached.pgn");
  await p.screenshot({
    path: join(out, "delete-confirmation-light.png"),
    fullPage: true,
  });
  await p.evaluate(() => {
    document.documentElement.dataset.theme = "dark";
  });
  await p.setViewportSize({ width: 900, height: 900 });
  const dialogBox = await p.getByRole("dialog").boundingBox();
  expect(dialogBox.x).toBeGreaterThanOrEqual(0);
  expect(dialogBox.x + dialogBox.width).toBeLessThanOrEqual(900);
  await p.screenshot({
    path: join(out, "delete-confirmation-dark-900.png"),
    fullPage: true,
  });
  await p.evaluate(() => {
    document.documentElement.dataset.theme = "light";
  });
  await p.setViewportSize({ width: 1440, height: 960 });
  await confirm();
  await expect(
    p.getByRole("button", { name: "Удалить delete-attached.pgn", exact: true }),
  ).toHaveCount(0);
  expect(documentCounts(document.id)).toEqual([0, 0, 0]);
  expect(await readFile(path, "utf8")).toBe(pgn);
  const union = await h.native("knowledge_repertoire", { side: "white" });
  expect(union.metadata.source_document_ids ?? []).not.toContain(document.id);
  const ids = (await h.native("knowledge_list"))
    .filter((d) => d.attachment_active && ["white", "both"].includes(d.side))
    .map((d) => d.id);
  const graphAfter = await h.native("game_graph_query", {
    request: {
      filter: { color: "white", variant: "standard" },
      line: [],
      all_paths: false,
      repertoire_ids: ids,
    },
  });
  expect(
    graphAfter.continuations.every((edge) =>
      edge.prepared.every((source) => source.document_id !== document.id),
    ),
  ).toBe(true);
  expect(graphAfter.selected.games).toBe(graph.selected.games);
  checks.push({
    scenario:
      "Confirmed attachment deletion removes all versions/index, union and prepared overlay; original file and actual game count remain",
    result: "PASS",
  });

  const detached = await h.native("knowledge_attach_file", {
    path,
    side: "white",
  });
  await h.native("knowledge_set_attachment", {
    documentId: detached.id,
    expectedRevision: detached.revision,
    active: false,
  });
  const details = p
    .locator(".repertoire-existing")
    .filter({ has: p.locator("summary", { hasText: "Отсоединённые файлы" }) });
  await details.locator("summary").click();
  await targetDelete("delete-attached.pgn");
  await confirm();
  expect(documentCounts(detached.id)).toEqual([0, 0, 0]);
  const legacyDetails = p.locator(".repertoire-existing").filter({
    has: p.locator("summary", { hasText: "Ранее созданные документы" }),
  });
  await legacyDetails.locator("summary").click();
  await targetDelete("Delete legacy source");
  await confirm();
  expect(documentCounts(legacy.id)).toEqual([0, 0, 0]);
  checks.push({
    scenario:
      "Detached sources and legacy repertoire documents both support explicit permanent deletion",
    result: "PASS",
  });
  const factualAfter = snapshot();
  expect(factualAfter).toBe(factualBefore);
  expect(errors).toEqual([]);
  checks.push({
    scenario:
      "All factual PGN, payloads, account T0 and engine data unchanged; zero JS errors",
    result: "PASS",
  });
  await writeFile(
    join(out, "results.json"),
    JSON.stringify(
      {
        checks,
        fixtureProfile: config.directory,
        errors,
        factualBefore,
        factualAfter,
      },
      null,
      2,
    ),
  );
  console.log(
    JSON.stringify({
      checks: checks.length,
      errors: errors.length,
      directory: out,
    }),
  );
} catch (error) {
  await p
    .screenshot({ path: join(out, "failure.png"), fullPage: true })
    .catch(() => {});
  throw error;
} finally {
  await h.browser.close();
}

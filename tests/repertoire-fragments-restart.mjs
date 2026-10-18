// Read-only process-restart acceptance for previously attached native fixtures.
import { expect } from "@playwright/test";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";
import { bridge } from "./helpers/product-bridge.mjs";
const directory = resolve(".qa/repertoire-fragments-ui");
const snapshotPath = join(directory, "before-restart.json");
const config = JSON.parse(await readFile(".qa/ui-atlas-tools/current.json"));
const h = await bridge(config, "light"),
  page = h.page;
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
function leafPaths(document) {
  const result = [];
  function walk(node, path) {
    if (!node.children.length) result.push(path.join(" "));
    for (const child of node.children) walk(child, [...path, child.uci]);
  }
  for (const chapter of document.chapters) walk(chapter.root, []);
  return result.sort();
}
function treeIds(document) {
  const result = [];
  function walk(node) {
    result.push(node.id);
    node.children.forEach(walk);
  }
  for (const chapter of document.chapters) {
    result.push(chapter.id);
    walk(chapter.root);
  }
  return result;
}
async function snapshot() {
  const summaries = (await h.native("knowledge_list"))
    .filter((item) =>
      [
        "QA-ruy.pgn",
        "QA-sicilian.pgn",
        "QA-fen.pgn",
        "QA-french-black.pgn",
      ].includes(item.filename),
    )
    .sort((a, b) => a.id.localeCompare(b.id));
  const sources = [];
  for (const summary of summaries) {
    const document = await h.native("knowledge_load", {
      documentId: summary.id,
    });
    sources.push({
      id: document.id,
      revision: document.revision,
      side: document.side,
      filename: summary.filename,
      active: summary.attachment_active,
      rawPGN: document.metadata.repertoire_attachment.source_pgn,
      originalPGNHash: createHash("sha256")
        .update(document.metadata.repertoire_attachment.source_pgn)
        .digest("hex"),
      nodeIds: treeIds(document),
      documentHash: createHash("sha256")
        .update(JSON.stringify(document))
        .digest("hex"),
      leafPaths: leafPaths(document),
    });
  }
  const white = await h.native("knowledge_repertoire", { side: "white" });
  const black = await h.native("knowledge_repertoire", { side: "black" });
  return {
    sources,
    white: {
      chapters: white.chapters.length,
      leafPaths: leafPaths(white),
      nodeIds: treeIds(white),
      sourceIds: white.metadata.source_document_ids,
    },
    black: {
      chapters: black.chapters.length,
      leafPaths: leafPaths(black),
      nodeIds: treeIds(black),
      sourceIds: black.metadata.source_document_ids,
    },
  };
}
let result;
try {
  if (process.argv.includes("--snapshot")) {
    const state = await snapshot();
    expect(state.sources).toHaveLength(4);
    expect(state.sources.every((source) => source.active)).toBe(true);
    await mkdir(directory, { recursive: true });
    await writeFile(
      snapshotPath,
      JSON.stringify(
        {
          nativePid: config.pid,
          profile: config.directory,
          dataDirectory: config.data,
          state,
        },
        null,
        2,
      ),
    );
    console.log(`Read-only pre-restart snapshot saved: ${snapshotPath}`);
  } else {
    const before = JSON.parse(await readFile(snapshotPath));
    expect(config.pid).not.toBe(before.nativePid);
    expect(config.data).toBe(before.dataDirectory);
    const after = await snapshot();
    expect(after).toEqual(before.state);
    expect(after.white.chapters).toBe(1);
    expect(
      after.white.leafPaths.some((path) =>
        path.startsWith("e2e4 e7e5 g1f3 b8c6 f1b5"),
      ),
    ).toBe(true);
    expect(
      after.white.leafPaths.some((path) =>
        path.startsWith("e2e4 e7e5 g1f3 b8c6 f1c4"),
      ),
    ).toBe(true);
    expect(after.black.leafPaths).toEqual(["e2e4 e7e6 d2d4 d7d5"]);
    await page
      .locator(".app-header")
      .getByRole("button", { name: "Репертуар", exact: true })
      .click();
    await page.getByRole("button", { name: "За чёрных", exact: true }).click();
    await expect(
      page.locator(".repertoire-tile-filename", {
        hasText: "QA-french-black.pgn",
      }),
    ).toBeVisible();
    await expect(
      page.locator(".repertoire-tile-filename", { hasText: "QA-ruy.pgn" }),
    ).toBeHidden();
    await page.getByRole("button", { name: "За белых", exact: true }).click();
    for (const name of ["QA-ruy.pgn", "QA-sicilian.pgn", "QA-fen.pgn"])
      await expect(
        page.locator(".repertoire-tile-filename", { hasText: name }),
      ).toBeVisible();
    await page
      .getByRole("button", { name: "Открыть общий репертуар", exact: true })
      .click();
    await expect(page.locator(".combined-board cg-board")).toBeVisible();
    for (const san of ["e4", "e5", "Nf3", "Nc6"])
      await page
        .locator(".combined-branches")
        .getByRole("button", { name: new RegExp(`^\\d+[. …]+${san}(?:\\s|$)`) })
        .click();
    await expect(page.locator(".combined-branches")).toContainText("Bb5");
    await expect(page.locator(".combined-branches")).toContainText("Bc4");
    await page.screenshot({
      path: "screens/simplification/repertoire/light-after-process-restart.png",
      fullPage: true,
    });
    expect(errors).toEqual([]);
    result = {
      result: "PASS",
      oldPid: before.nativePid,
      newPid: config.pid,
      profile: config.directory,
      checks: [
        "Actual native process identity changed; the same SQLite profile was reopened",
        "All four attachment IDs/revisions/active flags/original PGNs/node IDs/document hashes are unchanged",
        "White merged Ruy/Sicilian/FEN branches and black French union match exactly",
        "Native tiles and board navigation to Bb5/Bc4 work after process restart",
      ],
      pageErrors: errors,
    };
    await writeFile(
      join(directory, "restart-results.json"),
      JSON.stringify(result, null, 2),
    );
    console.log(JSON.stringify(result, null, 2));
  }
} catch (error) {
  if (!process.argv.includes("--snapshot"))
    await writeFile(
      join(directory, "restart-results.json"),
      JSON.stringify(
        { result: "FAIL", error: String(error), pageErrors: errors },
        null,
        2,
      ),
    );
  throw error;
} finally {
  await h.browser.close();
}

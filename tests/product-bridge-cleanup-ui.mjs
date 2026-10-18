// Focused event-bridge cleanup regression. Emits events only; never connects an
// account, resets statistics or edits documents in this isolated native profile.
import { chromium, expect } from "@playwright/test";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { bridge } from "./helpers/product-bridge.mjs";
const config = JSON.parse(
  await readFile(".qa/ui-atlas-tools/current.json", "utf8"),
);
if (!resolve(config.data).startsWith(resolve(".qa") + "\\"))
  throw Error("Only isolated QA profiles");
const out = resolve(".qa/product-bridge-cleanup");
await mkdir(out, { recursive: true });
function snapshot() {
  const db = new DatabaseSync(join(config.data, "chesspush.sqlite3"), {
    readOnly: true,
  });
  try {
    const result = {};
    for (const table of [
      "games",
      "statistics_games",
      "statistics_account_games",
      "statistics_payloads",
      "knowledge_documents",
      "knowledge_document_versions",
      "knowledge_node_index",
    ])
      result[table] = db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all();
    result.accounts = db
      .prepare(
        "SELECT id,t0_ms,generation,enabled FROM statistics_accounts ORDER BY id",
      )
      .all();
    return createHash("sha256").update(JSON.stringify(result)).digest("hex");
  } finally {
    db.close();
  }
}
const before = snapshot();
const observer = await chromium.connectOverCDP(config.cdp);
const nativePage = observer.contexts()[0].pages()[0];
const errors = [],
  warnings = [],
  checks = [];
const consoleProbe = (message) => {
  if (
    /Couldn.t find callback id|__atlasForward.*not exposed/.test(message.text())
  )
    warnings.push(message.text());
};
nativePage.on("pageerror", (error) => errors.push(error.message));
nativePage.on("console", consoleProbe);
await nativePage.waitForFunction(() => window.__TAURI_INTERNALS__?.invoke);
await nativePage.evaluate(() => {
  const original =
    window.__atlasOriginalInvoke ??
    window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
  window.__atlasCleanupProbe = {
    original,
    previous: window.__atlasOriginalInvoke,
    unlistens: [],
    rejections: [],
  };
  window.__atlasOriginalInvoke = async (command, args) => {
    if (command === "plugin:event|unlisten")
      window.__atlasCleanupProbe.unlistens.push(args);
    return original(command, args);
  };
  window.__atlasCleanupProbe.onRejection = (event) =>
    window.__atlasCleanupProbe.rejections.push(
      String(event.reason?.message ?? event.reason),
    );
  window.addEventListener(
    "unhandledrejection",
    window.__atlasCleanupProbe.onRejection,
  );
});
const emit = (page, amount) =>
  page.evaluate(async (amount) => {
    const invoke =
      window.__atlasOriginalInvoke ??
      window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__);
    for (let i = 0; i < amount; i++)
      await invoke("plugin:event|emit", {
        event: "knowledge-updated",
        payload: { cleanup_probe: true, index: i },
      });
  }, amount);
try {
  for (let round = 0; round < 2; round++) {
    const h = await bridge(config, "light");
    h.page.on("pageerror", (error) => errors.push(error.message));
    await h.page.evaluate(() => {
      const dispatch = window.__atlasDispatch;
      window.__atlasCleanupReceived = 0;
      window.__atlasDispatch = (event, payload) => {
        if (event === "knowledge-updated") window.__atlasCleanupReceived++;
        return dispatch(event, payload);
      };
    });
    await emit(nativePage, 1);
    await expect
      .poll(() => h.page.evaluate(() => window.__atlasCleanupReceived))
      .toBeGreaterThan(0);
    checks.push({
      round,
      name: "native event reaches production renderer before close",
      passed: true,
    });
    await Promise.all([emit(nativePage, 25), h.browser.close()]);
    // close must be idempotent, including while an event burst races cleanup.
    await h.browser.close();
    const fresh = await chromium.connectOverCDP(config.cdp);
    const freshPage = fresh.contexts()[0].pages()[0];
    freshPage.on("pageerror", (error) => errors.push(error.message));
    freshPage.on("console", consoleProbe);
    await emit(freshPage, 25);
    await freshPage.waitForTimeout(350);
    const state = await freshPage.evaluate(() => ({
      remaining: Object.keys(window.__atlasBridgeForwarders ?? {}),
      bindings: Object.keys(window).filter((key) =>
        key.startsWith("__atlasForward"),
      ),
      unlistens: window.__atlasCleanupProbe.unlistens,
      rejections: window.__atlasCleanupProbe.rejections,
    }));
    expect(state.remaining).toEqual([]);
    expect(state.bindings).toEqual([]);
    expect(state.unlistens).toHaveLength((round + 1) * 6);
    for (const subscription of state.unlistens) {
      expect(typeof subscription.event).toBe("string");
      expect(typeof subscription.eventId).toBe("number");
    }
    expect(state.rejections).toEqual([]);
    expect(errors).toEqual([]);
    expect(warnings).toEqual([]);
    checks.push({
      round,
      name: "fresh CDP events after close have no stale forwarding callback or unhandled rejection",
      passed: true,
      unlistens: state.unlistens.length,
    });
    await fresh.close();
  }
  expect(snapshot()).toBe(before);
  checks.push({
    name: "games, memberships, account boundary, documents and revisions unchanged",
    passed: true,
  });
  await writeFile(
    join(out, "results.json"),
    JSON.stringify(
      {
        passed: true,
        checks,
        errors,
        warnings,
        factual_before: before,
        factual_after: snapshot(),
        note: "The fixed failure was harness cleanup noise; production page errors remain zero.",
      },
      null,
      2,
    ),
  );
  console.log(JSON.stringify({ passed: true, checks, errors, warnings }));
} finally {
  await nativePage
    .evaluate(() => {
      const probe = window.__atlasCleanupProbe;
      window.removeEventListener("unhandledrejection", probe.onRejection);
      if (probe.previous === undefined) delete window.__atlasOriginalInvoke;
      else window.__atlasOriginalInvoke = probe.previous;
      delete window.__atlasCleanupProbe;
    })
    .catch(() => {});
  await observer.close();
}

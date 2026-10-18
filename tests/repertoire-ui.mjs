// Focused UI regression against a running isolated native backend.
// CHESSPUSH_TEST_CDP points to the test WebView2 CDP endpoint; production data is never opened.
import { chromium, expect } from "@playwright/test";
import { createServer } from "vite";
import react from "@vitejs/plugin-react";
import { mkdir, writeFile } from "node:fs/promises";
const cdp = process.env.CHESSPUSH_TEST_CDP;
if (!cdp)
  throw new Error(
    "Set CHESSPUSH_TEST_CDP to an isolated test profile CDP endpoint",
  );
const nativeBrowser = await chromium.connectOverCDP(cdp);
const nativePage = nativeBrowser.contexts()[0].pages()[0];
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
const original = await native("knowledge_import", {
  request: {
    title: "UX editor regression",
    side: "white",
    pgn: '[Event "UX chapter one"]\n\n1. e4 $1 {[%cal Ge2e4]} (1. d4 $220 d5) e5 2. Nf3 *\n\n[Event "UX chapter two"]\n\n1. c4 e5 *',
  },
});
const documentId = original.id;
console.log("Native fixture created", documentId);
const vite = await createServer({
  configFile: false,
  cacheDir: ".qa/repertoire-vite-cache",
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
  if (!req.url?.startsWith("/repertoire-harness")) return next();
  res.setHeader("Content-Type", "text/html");
  res.end(
    await vite.transformIndexHtml(
      "/repertoire-harness",
      `<html><head></head><body><div id="root"></div><script type="module">
import React from 'react';
import {createRoot} from 'react-dom/client';
import {Repertoire} from '/src/knowledge/Repertoire.tsx';
import '/src/theme.css';
function Harness() {
  const [active,setActive]=React.useState(true), [target,setTarget]=React.useState({documentId:${JSON.stringify(documentId)}});
  window.__repertoireRoute={setActive,setTarget};
  window.__repertoireContexts ??= [];
  return React.createElement('div',{hidden:!active},React.createElement(Repertoire,{active,initialTarget:target,onContextChange:context=>window.__repertoireContexts.push(context)}));
}
createRoot(document.getElementById('root')).render(React.createElement(Harness));
</script></body></html>`,
    ),
  );
});
await vite.listen();
const port = vite.httpServer.address().port;
const browser = await chromium.launch({ channel: "msedge", headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 960 } });
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
page.on("dialog", (dialog) => dialog.accept());
await page.exposeFunction("__repertoireNative", async (command, args) => {
  try {
    return JSON.stringify({ ok: true, value: await native(command, args) });
  } catch (error) {
    return JSON.stringify({ ok: false, error });
  }
});
await page.addInitScript(() => {
  window.isTauri = true;
  window.__repertoireTest = { fail: false, wait: 0, calls: [] };
  window.__TAURI_INTERNALS__ = {
    transformCallback() {
      return 1;
    },
    async invoke(command, args) {
      window.__repertoireTest.calls.push({ command, args });
      if (command.startsWith("plugin:event|")) return 1;
      if (command === "knowledge_apply") {
        if (window.__repertoireTest.wait)
          await new Promise((resolve) =>
            setTimeout(resolve, window.__repertoireTest.wait),
          );
        if (window.__repertoireTest.fail)
          throw {
            code: "fixture_io",
            message: "Тест: не удалось записать документ",
          };
      }
      const envelope = JSON.parse(
        await window.__repertoireNative(command, args),
      );
      if (!envelope.ok) throw envelope.error;
      return envelope.value;
    },
  };
});
await mkdir(".qa/product-ux-repertoire", { recursive: true });
const passed = [];
const record = (message) => {
  passed.push(message);
  console.log("PASS", message);
};
try {
  await page.goto(`http://127.0.0.1:${port}/repertoire-harness`);
  await expect(
    page.getByRole("heading", { name: "UX editor regression", exact: true }),
  ).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await expect(
    page.getByLabel("FEN выбранной позиции репертуара"),
  ).not.toBeVisible();
  const firstChapter = original.chapters[0],
    e4 = firstChapter.root.children[0];
  await page.locator(`[data-knowledge-node="${e4.id}"]`).click();
  await page
    .getByLabel("Комментарий к позиции")
    .fill("Текст, который должен пережить ошибку и перезапуск");
  await page.evaluate(() => {
    window.__repertoireTest.fail = true;
  });
  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await expect(page.getByRole("alert")).toContainText("Тест:");
  await expect(page.getByLabel("Комментарий к позиции")).toHaveValue(
    "Текст, который должен пережить ошибку и перезапуск",
  );
  const next = e4.children[0];
  await page.locator(`[data-knowledge-node="${next.id}"]`).click();
  await expect(
    page.locator(`[data-knowledge-node="${e4.id}"]`),
  ).toHaveAttribute("aria-current", "step");
  await expect(page.getByLabel("Комментарий к позиции")).toHaveValue(
    "Текст, который должен пережить ошибку и перезапуск",
  );
  await page.screenshot({
    path: ".qa/product-ux-repertoire/save-failure-retained.png",
  });
  record(
    "Save failure keeps comment and selected node when navigation is attempted",
  );
  await page.reload();
  await expect(
    page.getByRole("status").filter({ hasText: "Восстановлены" }),
  ).toBeVisible();
  await expect(page.getByLabel("Комментарий к позиции")).toHaveValue(
    "Текст, который должен пережить ошибку и перезапуск",
  );
  const remote = await native("knowledge_apply", {
    documentId,
    expectedRevision: original.revision,
    operation: {
      type: "annotate",
      chapter_id: firstChapter.id,
      node_id: e4.id,
      comments: ["Remote edit"],
      nags: [1],
      shapes: e4.shapes,
    },
  });
  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await expect(page.getByRole("alert")).toBeVisible();
  await page
    .getByRole("button", { name: "Сверить с сохранённой версией", exact: true })
    .click();
  await expect(page.getByLabel("Комментарий к позиции")).toHaveValue(
    "Текст, который должен пережить ошибку и перезапуск",
  );
  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: /^Сохранено$/ }),
  ).toBeVisible();
  record(
    "Reload restores draft; real revision conflict is explicitly reconciled without losing text",
  );
  await page.evaluate(() => {
    window.__repertoireTest.wait = 400;
  });
  await page.getByLabel("Комментарий к позиции").fill("First snapshot");
  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await page.getByLabel("Комментарий к позиции").fill("Typed while saving");
  await expect(
    page.getByRole("status").filter({ hasText: "Есть изменения" }),
  ).toBeVisible();
  await expect(page.getByLabel("Комментарий к позиции")).toHaveValue(
    "Typed while saving",
  );
  await page.evaluate(() => {
    window.__repertoireTest.wait = 0;
  });
  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: /^Сохранено$/ }),
  ).toBeVisible();
  const saved = await native("knowledge_load", { documentId, revision: null });
  expect(saved.chapters[0].root.children[0].comments).toEqual([
    "Typed while saving",
  ]);
  record(
    "Typing during an in-flight save survives and is persisted by the next save",
  );
  await page.getByRole("button", { name: /Варианты после начала/ }).click();
  const alternative = saved.chapters[0].root.children[1];
  await page.locator(`[data-knowledge-node="${alternative.id}"]`).click();
  await expect(
    page.locator(`[data-knowledge-node="${alternative.id}"]`),
  ).toContainText("$220");
  await page.locator(".knowledge-annotation-menu > summary").click();
  await page
    .getByRole("button", { name: "! — Хороший ход", exact: true })
    .click();
  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await expect(
    page.getByRole("status").filter({ hasText: /^Сохранено$/ }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Удалить ветвь", exact: true })
    .click();
  await expect(
    page.getByRole("button", {
      name: "Удалить ветвь и продолжения",
      exact: true,
    }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Отмена", exact: true }).click();
  expect(
    (await native("knowledge_load", { documentId, revision: null })).chapters[0]
      .root.children,
  ).toHaveLength(2);
  record(
    "Local alternative expansion, symbolic NAG editing, unknown NAG retention and deletion cancellation",
  );
  await page.locator(".knowledge-document-menu > summary").click();
  await page.getByRole("button", { name: "Экспорт PGN", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Экспорт PGN", exact: true });
  await expect(dialog).toBeVisible();
  const pgn = await page.getByLabel("Экспортированный PGN").inputValue();
  expect(pgn).toContain("$220");
  expect(pgn).toContain("[%cal Ge2e4]");
  const imported = await native("knowledge_import", {
    request: { title: "UX roundtrip", side: "white", pgn },
  });
  const source = await native("knowledge_load", { documentId, revision: null });
  function semantic(node) {
    return {
      uci: node.uci,
      comments: node.comments,
      nags: node.nags,
      shapes: node.shapes,
      children: node.children.map(semantic),
    };
  }
  expect(imported.chapters.map((chapter) => semantic(chapter.root))).toEqual(
    source.chapters.map((chapter) => semantic(chapter.root)),
  );
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
  record(
    "Native PGN round-trip preserves two chapters, RAV, comments, supported unknown NAGs and arrows",
  );
  await page.locator(".knowledge-annotation-menu > summary").click();
  await page.screenshot({
    path: ".qa/product-ux-repertoire/editor-light-1440.png",
  });
  for (const [width, height] of [
    [900, 700],
    [1920, 1080],
  ]) {
    await page.setViewportSize({ width, height });
    await expect(page.locator(".knowledge-board-panel")).toBeVisible();
    const box = await page.locator(".knowledge-board-panel").boundingBox();
    expect(box.x + box.width).toBeLessThanOrEqual(width);
    await page.screenshot({
      path: `.qa/product-ux-repertoire/editor-light-${width}.png`,
    });
  }
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "dark";
  });
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.screenshot({
    path: ".qa/product-ux-repertoire/editor-dark-1440.png",
  });
  const selectedContext = await page.evaluate(() =>
    window.__repertoireContexts.at(-1),
  );
  await page.evaluate(() => {
    window.__repertoireRoute.setActive(false);
    window.__repertoireContexts = [];
  });
  await expect(page.locator(".knowledge-workspace")).not.toBeVisible();
  await page.evaluate(() => window.__repertoireRoute.setActive(true));
  await expect
    .poll(() => page.evaluate(() => window.__repertoireContexts.at(-1)))
    .toEqual(selectedContext);
  await page.evaluate(() =>
    window.__repertoireRoute.setTarget({ mode: "list", requestId: Date.now() }),
  );
  await expect(
    page.getByRole("heading", { name: "Репертуар", exact: true }),
  ).toBeVisible();
  await page.evaluate(
    ({ documentId, chapterId, nodeId }) =>
      window.__repertoireRoute.setTarget({
        documentId,
        chapterId,
        nodeId,
        requestId: Date.now(),
      }),
    { documentId, chapterId: firstChapter.id, nodeId: alternative.id },
  );
  await expect(
    page.locator(`[data-knowledge-node="${alternative.id}"]`),
  ).toHaveAttribute("aria-current", "step");
  record(
    "Explicit list requests close an existing editor; hidden/visible workspace re-emits the exact document/chapter/node context",
  );
  await page
    .getByLabel("Комментарий к позиции")
    .fill("Recovered text from a remotely deleted node");
  await expect
    .poll(() =>
      page.evaluate(
        (documentId) =>
          localStorage.getItem(`chesspush.knowledge.draft.${documentId}`),
        documentId,
      ),
    )
    .toContain("Recovered text");
  const beforeDelete = await native("knowledge_load", {
    documentId,
    revision: null,
  });
  await native("knowledge_apply", {
    documentId,
    expectedRevision: beforeDelete.revision,
    operation: {
      type: "remove_variation",
      chapter_id: firstChapter.id,
      node_id: alternative.id,
    },
  });
  await page.reload();
  await expect(page.getByLabel("Черновик удалённого узла")).toHaveValue(
    "Recovered text from a remotely deleted node",
  );
  expect(
    (await native("knowledge_load", { documentId, revision: null })).chapters[0]
      .root.comments,
  ).toEqual([]);
  record(
    "A draft whose node was deleted remotely is recovered separately and never reassigned to the root position",
  );
  await page
    .getByRole("button", { name: "К репертуарам", exact: true })
    .click();
  await expect(
    page.getByRole("heading", { name: "Репертуар", exact: true }),
  ).toBeVisible();
  await expect(
    page
      .locator(".repertoire-list li button")
      .filter({ hasText: "UX editor regression" })
      .first(),
  ).toContainText("Белыми");
  await page.evaluate(() => {
    document.documentElement.dataset.theme = "dark";
  });
  await page.screenshot({
    path: ".qa/product-ux-repertoire/list-dark-1440.png",
  });
  await page.getByRole("button", { name: "Создать", exact: true }).click();
  await page
    .getByLabel("Название репертуара", { exact: true })
    .fill("UX created from workspace");
  await page
    .getByLabel("Сторона репертуара", { exact: true })
    .selectOption("black");
  await page.getByRole("button", { name: "Создать", exact: true }).click();
  await expect(
    page.getByRole("heading", {
      name: "UX created from workspace",
      exact: true,
    }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "К репертуарам", exact: true })
    .click();
  await expect(
    page
      .locator(".repertoire-list li button")
      .filter({ hasText: "UX created from workspace" })
      .first(),
  ).toContainText("Чёрными");
  await page
    .getByRole("button", { name: "Импортировать PGN", exact: true })
    .click();
  const importTask = page.getByRole("dialog", {
    name: "Импорт PGN",
    exact: true,
  });
  await expect(importTask).toBeVisible();
  await importTask
    .getByLabel("Название импортируемого репертуара")
    .fill("UX imported from workspace");
  await importTask.getByLabel("PGN для импорта").fill("1. e4 badmove *");
  await importTask
    .getByRole("button", { name: "Импортировать репертуар", exact: true })
    .click();
  await expect(importTask.getByRole("alert")).toBeVisible();
  await importTask.getByLabel("PGN для импорта").fill(pgn);
  await importTask
    .getByRole("button", { name: "Импортировать репертуар", exact: true })
    .click();
  await expect(
    page.getByRole("heading", {
      name: "UX imported from workspace",
      exact: true,
    }),
  ).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  record(
    "Standalone workspace lists title/side/date and creates/imports actual persisted native documents; import errors stay in the bounded task",
  );
  const longPgn =
    '[Event "UX long document"]\n\n' +
    Array.from(
      { length: 100 },
      (_, i) => `${i * 2 + 1}. Nf3 Nf6 ${i * 2 + 2}. Ng1 Ng8`,
    ).join(" ") +
    " *";
  const longDocument = await native("knowledge_import", {
    request: { title: "UX 400-ply deep link", side: "both", pgn: longPgn },
  });
  let deepest = longDocument.chapters[0].root;
  while (deepest.children[0]) deepest = deepest.children[0];
  expect(deepest.ply).toBe(400);
  await page.evaluate(
    ({ documentId, chapterId, nodeId }) =>
      window.__repertoireRoute.setTarget({
        documentId,
        chapterId,
        nodeId,
        requestId: Date.now(),
      }),
    {
      documentId: longDocument.id,
      chapterId: longDocument.chapters[0].id,
      nodeId: deepest.id,
    },
  );
  await expect(
    page.getByRole("heading", { name: "UX 400-ply deep link", exact: true }),
  ).toBeVisible();
  await expect(
    page.locator(`[data-knowledge-node="${deepest.id}"]`),
  ).toHaveAttribute("aria-current", "step");
  await expect(
    page.locator(`[data-knowledge-node="${deepest.id}"]`),
  ).toBeInViewport();
  await page.screenshot({
    path: ".qa/product-ux-repertoire/deep-link-400-dark-1440.png",
  });
  record(
    "A 400-ply native PGN document opens its exact stable node beyond the initial 300-move projection, with selected notation in view",
  );
  if (errors.length) throw new Error(errors.join("\n"));
  await writeFile(
    ".qa/product-ux-repertoire/results.json",
    JSON.stringify(
      {
        passed,
        viewport: { width: 1440, height: 960, dpr: 1 },
        method:
          "Vite React UI → native IPC isolated backend; save failure injected at IPC boundary, revision conflict real SQLite",
      },
      null,
      2,
    ),
  );
  console.log(`PASS: ${passed.length} repertoire UI scenarios`, passed);
} finally {
  await browser.close();
  await vite.close();
  await nativeBrowser.close();
}

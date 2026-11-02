// Actual application, Rust commands, SQLite persistence and Chessground input.
// Only OS file-dialog responses are supplied by the test.
import { chromium, expect as baseExpect } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
const expect = baseExpect.configure({ timeout: 15_000 });

if (!process.env.CHESSPUSH_TEST_DATA || !process.env.CHESSPUSH_TEST_CDP)
  throw new Error(
    "Run tools/check-knowledge-native.mjs with an isolated profile",
  );
const artifacts = resolve(process.env.CHESSPUSH_TRAINING_ARTIFACTS);
await mkdir(artifacts, { recursive: true });
const browser = await chromium.connectOverCDP(process.env.CHESSPUSH_TEST_CDP);
const page = browser.contexts()[0].pages()[0];
await page.setViewportSize({ width: 1440, height: 960 });
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
await page.waitForFunction(
  () =>
    window.__TAURI_INTERNALS__?.invoke && document.querySelector(".app-header"),
);
async function native(command, args = {}) {
  return page.evaluate(
    ({ command, args }) => window.__TAURI_INTERNALS__.invoke(command, args),
    { command, args },
  );
}
async function summaries() {
  return native("knowledge_list");
}
async function load(id) {
  return native("knowledge_load", { documentId: id, revision: null });
}
async function saved() {
  await expect(page.locator(".knowledge-header [role=status]")).toHaveText(
    "Сохранено",
  );
}
async function save() {
  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await saved();
}
async function dialogResponse(command, result) {
  await page.evaluate(
    ({ command, result }) => {
      window.__knowledgeDialogTrace ??= [];
      const original = window.fetch.bind(window);
      window.fetch = (input, options) => {
        const url = new URL(typeof input === "string" ? input : input.url);
        window.__knowledgeDialogTrace.push({
          transport: "fetch",
          command: decodeURIComponent(url.pathname.slice(1)),
        });
        if (
          url.hostname === "ipc.localhost" &&
          decodeURIComponent(url.pathname.slice(1)) === command
        )
          return Promise.resolve(
            new Response(JSON.stringify(result), {
              headers: {
                "Content-Type": "application/json",
                "Tauri-Response": "ok",
              },
            }),
          );
        return original(input, options);
      };
      // WebView2 can switch Tauri IPC to postMessage after a protocol failure.
      // Supply the same OS-dialog response on that transport as well.
      const webview = window.chrome?.webview;
      if (webview) {
        const originalPost = webview.postMessage.bind(webview);
        webview.postMessage = (payload) => {
          let message;
          try {
            message =
              typeof payload === "string" ? JSON.parse(payload) : payload;
          } catch {}
          window.__knowledgeDialogTrace.push({
            transport: "postMessage",
            command: message?.cmd ?? "unknown",
          });
          if (message?.cmd === command) {
            queueMicrotask(() =>
              window.__TAURI_INTERNALS__.runCallback(message.callback, result),
            );
            return;
          }
          return originalPost(payload);
        };
      }
    },
    { command, result },
  );
}
const fen = "rnbqkbnr/pp2pppp/2p5/3pP3/3P4/8/PPP2PPP/RNBQKBNR b KQkq - 0 3";
const tag = page.getByRole("combobox", { name: "Тег знания" });
try {
  await page.getByRole("button", { name: "Знания", exact: true }).click();
  await expect(page.getByText("Знаний нет", { exact: true })).toBeVisible();
  await page
    .getByRole("button", { name: "Создать знание", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Название знания", exact: true })
    .fill("Пешечная цепь");
  const initial = page.getByRole("textbox", { name: "Начальная FEN знания" });
  if (!(await initial.isVisible()))
    await page.locator(".knowledge-create-form summary").click();
  await initial.fill(fen);
  await page.getByRole("button", { name: "Главная", exact: true }).click();
  await page.getByRole("button", { name: "Знания", exact: true }).click();
  await expect(
    page.getByRole("textbox", { name: "Название знания", exact: true }),
  ).toHaveValue("Пешечная цепь");
  await expect(initial).toHaveValue(fen);
  await page.getByRole("button", { name: "Создать", exact: true }).click();
  await saved();
  const position = (await summaries()).find(
    (item) => item.title === "Пешечная цепь",
  );
  expect(position.kind).toBe("study");
  expect(position.classification).toBe("position");
  await tag.fill("Структура");
  await tag.press("Enter");
  await page
    .getByRole("textbox", { name: "Комментарий к позиции" })
    .fill("Цепь d4–e5. План: c3, Nf3 и Be2.");
  await save();
  await page.getByRole("button", { name: "Удалить тег «Структура»" }).click();
  await save();
  await tag.fill("стру");
  await expect(
    page.getByRole("option", { name: "Структура", exact: true }),
  ).toBeVisible();
  await tag.press("ArrowDown");
  await tag.press("Enter");
  await save();
  expect((await load(position.id)).metadata.tags).toEqual(["Структура"]);
  await page.screenshot({
    path: join(artifacts, "knowledge-position-light.png"),
    fullPage: true,
  });

  await page.getByRole("button", { name: "К знаниям", exact: true }).click();
  await dialogResponse("plugin:dialog|open", [
    resolve("tests/fixtures/knowledge-study.pgn"),
  ]);
  await page.getByRole("button", { name: "Добавить PGN", exact: true }).click();
  await saved();
  let theory = (await summaries()).find((item) => item.id !== position.id);
  expect(theory.classification).toBe("theory");
  expect(theory.chapters).toBe(2);
  await tag.fill("Стр");
  await page.getByRole("option", { name: "Структура", exact: true }).click();
  await save();
  await page.getByRole("tab", { name: "PGN / FEN", exact: true }).click();
  await page
    .getByRole("textbox", { name: "PGN Event", exact: true })
    .fill("Каро-Канн: планы");
  await save();
  await page
    .getByRole("button", { name: "Добавить главу", exact: true })
    .click();
  await page
    .getByRole("textbox", { name: "Название новой главы" })
    .fill("Ключевая позиция");
  await page.getByRole("textbox", { name: "FEN новой главы" }).fill(fen);
  await page
    .getByRole("button", { name: "Создать главу", exact: true })
    .click();
  await saved();
  let document = await load(theory.id);
  expect(document.chapters).toHaveLength(3);
  const added = document.chapters.find(
    (chapter) => chapter.title === "Ключевая позиция",
  );
  await page.locator(".knowledge-chapter-menu summary").click();
  await page.getByRole("button", { name: "Выше", exact: true }).click();
  await saved();
  await page.locator(".knowledge-chapter-menu summary").click();
  document = await load(theory.id);
  expect(document.chapters[1].id).toBe(added.id);
  // A legal alternate move created by real Chessground drag changes a position
  // chapter into a line without replacing the imported tree or its IDs.
  const board = page.locator(".knowledge-board-panel cg-board");
  const box = await board.boundingBox();
  const point = (square) => ({
    x: box.x + ((square.charCodeAt(0) - 97 + 0.5) * box.width) / 8,
    y: box.y + ((8 - Number(square[1]) + 0.5) * box.height) / 8,
  });
  const from = point("g8"),
    to = point("h6");
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 10 });
  await page.mouse.up();
  await expect
    .poll(
      async () =>
        (await load(theory.id)).chapters.find(
          (chapter) => chapter.id === added.id,
        ).root.children.length,
    )
    .toBe(1);
  const arrowTo = point("f5"),
    circle = point("e5");
  await page.mouse.move(to.x, to.y);
  await page.mouse.down({ button: "right" });
  await page.mouse.move(arrowTo.x, arrowTo.y, { steps: 8 });
  await page.mouse.up({ button: "right" });
  await page.mouse.move(circle.x, circle.y);
  await page.mouse.down({ button: "right" });
  await page.evaluate(
    () =>
      new Promise((resolve) =>
        requestAnimationFrame(() => requestAnimationFrame(resolve)),
      ),
  );
  await page.mouse.up({ button: "right" });
  await page.getByRole("tab", { name: "Комментарий", exact: true }).click();
  await page
    .getByRole("textbox", { name: "Комментарий к позиции" })
    .fill("Альтернативное развитие коня.");
  await page.getByRole("tab", { name: "Аннотации", exact: true }).click();
  await page.getByText("Номера аннотаций", { exact: true }).click();
  await page.getByRole("textbox", { name: "NAG выбранного хода" }).fill("5");
  await save();
  document = await load(theory.id);
  const moved = document.chapters.find((chapter) => chapter.id === added.id)
    .root.children[0];
  expect(moved.uci).toBe("g8h6");
  expect(moved.comments.join(" ")).toContain("Альтернативное");
  expect(moved.nags).toContain(5);
  expect(moved.shapes).toHaveLength(2);
  // Import's variations, comments, glyphs, shapes and edited headers survive.
  const source = document.chapters.find(
    (chapter) => chapter.title === "Advance",
  );
  expect(source.headers.Event).toBe("Каро-Канн: планы");
  const e5 =
    source.root.children[0].children[0].children[0].children[0].children[0];
  expect(e5.children.map((node) => node.uci)).toEqual(["c8f5", "c6c5"]);
  expect(e5.shapes).toHaveLength(2);
  await page.getByRole("tab", { name: "PGN / FEN", exact: true }).click();
  const output = join(artifacts, "exported-knowledge.pgn");
  await dialogResponse("plugin:dialog|save", output);
  await page.getByRole("button", { name: "Экспорт PGN", exact: true }).click();
  await page
    .getByRole("dialog", { name: "Экспорт PGN" })
    .getByRole("button", { name: "Скачать PGN" })
    .click();
  await expect
    .poll(async () => readFile(output, "utf8").catch(() => ""))
    .toContain("Каро-Канн: планы");
  const exported = await readFile(output, "utf8");
  expect(exported).toContain("ChessPushTags");
  expect(exported).toContain("[%cal Ge5e6]");
  expect(exported).toContain("$5");
  const roundtrip = await native("knowledge_import", {
    request: { title: "Roundtrip", side: "both", kind: "study", pgn: exported },
  });
  expect(roundtrip.chapters).toHaveLength(3);
  expect(roundtrip.metadata.tags).toEqual(["Структура"]);
  await native("knowledge_delete_study", {
    documentId: roundtrip.id,
    expectedRevision: roundtrip.revision,
  });
  await page.getByRole("button", { name: "Закрыть PGN" }).click();
  await page
    .getByRole("navigation", { name: "Главы знания" })
    .getByRole("button", { name: /Advance/ })
    .click();
  await page.locator(`[data-knowledge-node="${e5.id}"]`).click();
  await page
    .getByRole("button", { name: "Варианты после e5: c5", exact: true })
    .click();
  await page.getByRole("tab", { name: "Комментарий", exact: true }).click();
  await page.screenshot({
    path: join(artifacts, "knowledge-study-light.png"),
    fullPage: true,
  });
  await page.evaluate(() => (document.documentElement.dataset.theme = "dark"));
  await page.screenshot({
    path: join(artifacts, "knowledge-study-dark.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 900, height: 700 });
  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    )
    .toBe(true);
  await page.screenshot({
    path: join(artifacts, "knowledge-study-900.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 1440, height: 960 });
  await page.evaluate(() => (document.documentElement.dataset.theme = "light"));
  await page.getByRole("button", { name: "К знаниям", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Тип знания" })
    .selectOption("position");
  await expect(page.locator(".knowledge-library-row")).toHaveCount(1);
  await page.getByRole("combobox", { name: "Тип знания" }).selectOption("all");
  await page.getByRole("searchbox", { name: "Поиск знаний" }).fill("Структура");
  await expect(page.locator(".knowledge-library-row")).toHaveCount(2);
  await page.screenshot({
    path: join(artifacts, "knowledge-library-light.png"),
    fullPage: true,
  });
  await page.reload();
  await page.getByRole("button", { name: "Знания", exact: true }).click();
  await expect(page.locator(".knowledge-library-row")).toHaveCount(2);
  expect(await native("knowledge_tag_suggestions", { query: "сТр" })).toEqual([
    "Структура",
  ]);
  await page
    .getByRole("button", { name: "Удалить знание: Пешечная цепь" })
    .click();
  await page
    .getByRole("dialog", { name: "Удалить знание?" })
    .getByRole("button", { name: "Удалить", exact: true })
    .click();
  await expect(page.locator(".knowledge-library-row")).toHaveCount(1);
  expect(await native("knowledge_tag_suggestions", { query: "Стр" })).toEqual([
    "Структура",
  ]);
  expect(errors).toEqual([]);
  await writeFile(
    join(artifacts, "knowledge-results.json"),
    JSON.stringify(
      {
        pass: true,
        checks: [
          "native create and position classification",
          "persistent case-insensitive tag history after removal/reload/delete",
          "PGN file import",
          "chapters and reorder",
          "real board drag",
          "headers/comments/NAG/shapes/RAV roundtrip",
          "native file export",
          "catalog filters and delete",
          "light/dark and 900px layout",
        ],
        errors,
      },
      null,
      2,
    ),
  );
  console.log(`PASS knowledge native integration; artifacts: ${artifacts}`);
} catch (error) {
  await writeFile(
    join(artifacts, "dialog-trace.json"),
    JSON.stringify(
      await page.evaluate(() => window.__knowledgeDialogTrace ?? []),
    ),
  );
  await page
    .screenshot({
      path: join(artifacts, "knowledge-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  await writeFile(
    join(artifacts, "knowledge-failure.txt"),
    await page
      .locator("body")
      .innerText()
      .catch(() => ""),
  );
  throw error;
} finally {
  await browser.close();
}

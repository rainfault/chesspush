// Real Tauri/Rust/SQLite application in the runner's disposable profile.
// No IPC, chess, projection or application state is mocked by this test.
import { chromium, expect } from "@playwright/test";
import { mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";

if (!process.env.CHESSPUSH_TEST_DATA || !process.env.CHESSPUSH_TEST_CDP)
  throw new Error(
    "Run tools/check-repertoire-training.mjs with CHESSPUSH_TRAINING_TEST_SCRIPT=tests/knowledge-graph-native.mjs",
  );
const artifacts = resolve(process.env.CHESSPUSH_TRAINING_ARTIFACTS);
await mkdir(artifacts, { recursive: true });
const browser = await chromium.connectOverCDP(process.env.CHESSPUSH_TEST_CDP);
const page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(20000);
await page.setViewportSize({ width: 1440, height: 960 });
const errors = [];
page.on("pageerror", (error) => errors.push(error.message));
const results = {};

async function native(command, args = {}) {
  const envelope = JSON.parse(
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
  if (!envelope.ok)
    throw new Error(`${command}: ${JSON.stringify(envelope.error)}`);
  return envelope.value;
}
const query = (extra = {}) =>
  native("game_graph_query", {
    request: { filter: { variant: "standard" }, ...extra },
  });
const load = (id) =>
  native("knowledge_load", { documentId: id, revision: null });
const graph = page.getByRole("region", { name: "Граф партий", exact: true });
const editor = page.getByRole("region", {
  name: "Редактор знания",
  exact: true,
});
const line = (value) =>
  expect(graph).toHaveAttribute("data-projection-line", value.join(" "));
const ready = () => expect(graph).toHaveAttribute("aria-busy", "false");
const move = (uci) =>
  graph.locator(`.branch-node[data-level="1"][data-uci="${uci}"]`);
const atNode = async (document, chapter, node) => {
  await expect(editor).toBeVisible();
  await expect(editor.getByRole("heading", { level: 1 })).toHaveText(
    document.title,
  );
  await expect(
    editor.getByRole("combobox", {
      name: "Глава знания",
      exact: true,
      includeHidden: true,
    }),
  ).toHaveValue(chapter.id);
  await expect(
    editor.locator(`[data-knowledge-node="${node.id}"]`),
  ).toHaveAttribute("aria-current", "step");
  await expect(
    editor.getByRole("textbox", {
      name: "FEN выбранной позиции знания",
      exact: true,
      includeHidden: true,
    }),
  ).toHaveValue(node.fen);
};
async function onGraph(expectedLine) {
  await editor
    .getByRole("button", { name: "Открыть на графе", exact: true })
    .click();
  await expect(graph).toBeVisible();
  await line(expectedLine);
  await ready();
}
async function screenshot(name) {
  await page.screenshot({ path: join(artifacts, name), fullPage: true });
}
async function theme(value) {
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await page.getByRole("tab", { name: "Внешний вид", exact: true }).click();
  await page
    .getByRole("combobox", { name: "Тема", exact: true })
    .selectOption(value);
  await expect(page.locator("html")).toHaveAttribute("data-theme", value);
  await page
    .getByRole("button", { name: "Закрыть настройки", exact: true })
    .click();
}

try {
  await page.waitForFunction(
    () =>
      typeof window.__TAURI_INTERNALS__?.invoke === "function" &&
      !!document.querySelector(".app-header"),
  );
  expect(await native("knowledge_list")).toEqual([]);
  const open = await native("knowledge_import", {
    request: {
      title: "Открытая игра",
      side: "both",
      kind: "study",
      pgn: '[Event "Открытая игра"]\n[Result "*"]\n\n1. e4 {Контроль центра.} e5 2. Nf3 *',
    },
  });
  let sicilian = await native("knowledge_import", {
    request: {
      title: "Сицилианская защита",
      side: "both",
      kind: "study",
      pgn: '[Event "Сицилианская защита"]\n[Result "*"]\n\n1. e4 c5 {Асимметричная борьба за центр.} 2. Nf3 *',
    },
  });
  sicilian = await native("knowledge_apply", {
    documentId: sicilian.id,
    expectedRevision: sicilian.revision,
    operation: { type: "set_tags", tags: ["Дебют"] },
  });
  const e4 = open.chapters[0].root.children[0];
  const c5 = sicilian.chapters[0].root.children[0].children[0];
  const root = await query();
  const rootMove = root.continuations.find((edge) => edge.uci === "e2e4");
  expect(root.eligible_games).toBe(0);
  expect(root.indexed_games).toBe(0);
  expect(root.total_positions).toBe(0);
  expect(root.total_visits).toBe(0);
  expect(root.selected.games).toBe(0);
  expect(rootMove.games).toBe(0);
  expect(rootMove.visits).toBe(0);
  expect(rootMove.branch_games).toBe(0);
  expect(rootMove.score).toBeNull();
  expect(rootMove.prepared).toEqual([]);
  expect(rootMove.coverage).toBe("knowledge");
  expect(rootMove.knowledge_count).toBe(2);
  expect(
    rootMove.knowledge.map((reference) => reference.document_id).sort(),
  ).toEqual([open.id, sicilian.id].sort());
  results.zeroGamesProjection = {
    eligible: root.eligible_games,
    positions: root.total_positions,
    visits: root.total_visits,
    knowledge: rootMove.knowledge_count,
    score: rootMove.score,
  };

  await page.getByRole("button", { name: "Статистика", exact: true }).click();
  await page.getByRole("tab", { name: "Граф партий", exact: true }).click();
  await expect(graph).toBeVisible();
  await line([]);
  await ready();
  await expect(move("e2e4")).toBeVisible();
  await expect(move("e2e4")).toContainText("Знание");
  await expect(graph.locator(".branch-current")).toContainText("Партий: 0");
  await expect(
    graph.getByRole("checkbox", { name: "Знания", exact: true }),
  ).toBeChecked();
  await expect(
    graph.getByRole("button", { name: "Знания после e4: 2", exact: true }),
  ).toBeVisible();

  // A real Chessground click move follows a study line with no imported games.
  const board = graph.locator("cg-board");
  const box = await board.boundingBox();
  if (!box) throw new Error("Graph chessboard is missing");
  const square = (value) => ({
    x: box.x + ((value.charCodeAt(0) - 97 + 0.5) * box.width) / 8,
    y: box.y + ((8 - Number(value[1]) + 0.5) * box.height) / 8,
  });
  for (const value of ["e2", "e4"]) {
    const point = square(value);
    await page.mouse.click(point.x, point.y);
  }
  await line(["e2e4"]);
  await ready();
  await expect(move("c7c5")).toBeVisible();
  await expect(move("e7e5")).toBeVisible();
  await expect(
    graph.getByRole("region", { name: "Знания в позиции", exact: true }),
  ).toContainText("Дебют");
  await screenshot("knowledge-graph-light.png");

  // A single book marker opens its exact source occurrence, not chapter root.
  await graph
    .getByRole("button", { name: "Знания после c5: 1", exact: true })
    .click();
  await atNode(sicilian, sicilian.chapters[0], c5);
  await expect(
    editor.getByRole("textbox", { name: "Комментарий к позиции", exact: true }),
  ).toHaveValue("Асимметричная борьба за центр.");
  results.singleBook = {
    document: sicilian.id,
    chapter: sicilian.chapters[0].id,
    node: c5.id,
  };

  // A recovered draft at one occurrence must not override a new book address.
  const draftText = "Давление на d4. Продолжить развитие фигур.";
  await editor
    .getByRole("textbox", { name: "Комментарий к позиции", exact: true })
    .fill(draftText);
  await expect
    .poll(() =>
      page.evaluate(
        (id) =>
          JSON.parse(
            localStorage.getItem(`chesspush.knowledge.draft.${id}`) ?? "null",
          )?.comments,
        sicilian.id,
      ),
    )
    .toBe(draftText);
  await page.getByRole("button", { name: "Статистика", exact: true }).click();
  await line(["e2e4"]);
  await ready();
  await move("c7c5").click();
  await line(["e2e4", "c7c5"]);
  await ready();
  await graph
    .getByRole("button", { name: "Знания после Nf3: 1", exact: true })
    .click();
  const nf3 = c5.children[0];
  await atNode(sicilian, sicilian.chapters[0], nf3);
  await expect
    .poll(async () => {
      const saved = await load(sicilian.id);
      return saved.chapters[0].root.children[0].children[0].comments;
    })
    .toEqual([draftText]);
  results.draftThenBook = { savedDraftNode: c5.id, openedNode: nf3.id };
  await onGraph(["e2e4", "c7c5", "g1f3"]);
  await graph.getByRole("button", { name: "К началу", exact: true }).click();
  await line([]);
  await ready();

  // Multiple documents at one destination produce a chooser with exact links.
  await graph
    .getByRole("button", { name: "Знания после e4: 2", exact: true })
    .click();
  const choices = graph.getByRole("region", {
    name: "Знания в позиции",
    exact: true,
  });
  await expect(choices.getByRole("heading")).toContainText("После e4");
  await expect(
    choices.getByRole("button", { name: /Открытая игра/ }),
  ).toBeVisible();
  await choices.getByRole("button", { name: /Открытая игра/ }).click();
  await atNode(open, open.chapters[0], e4);
  results.multipleBooks = true;
  await onGraph(["e2e4"]);

  const atE4 = await query({ initial_fen: root.initial_fen, line: ["e2e4"] });
  const toggle = graph.getByRole("checkbox", { name: "Знания", exact: true });
  await toggle.uncheck();
  await ready();
  await expect(move("c7c5")).toHaveCount(0);
  await expect(move("e7e5")).toHaveCount(0);
  await expect(graph.locator(".branch-current")).toContainText("Партий: 0");
  const hidden = await query({
    initial_fen: root.initial_fen,
    line: ["e2e4"],
    include_knowledge: false,
  });
  expect(hidden.continuations).toEqual([]);
  expect(hidden.selected.knowledge_count).toBe(0);
  expect(hidden.selected.games).toBe(0);
  await expect
    .poll(
      async () =>
        (await native("game_graph_load_state", { accountId: null }))
          ?.include_knowledge,
    )
    .toBe(false);
  await toggle.check();
  await ready();
  await expect(move("c7c5")).toBeVisible();
  results.toggle = true;

  // Creating a note preserves both the selected FEN and original route context.
  await graph
    .getByRole("button", { name: "Создать знание", exact: true })
    .click();
  await expect(editor).toBeVisible();
  await expect(
    editor.getByRole("textbox", {
      name: "Начальная FEN знания",
      includeHidden: true,
    }),
  ).toHaveValue(atE4.selected_fen);
  await editor
    .getByRole("textbox", { name: "Название знания", exact: true })
    .fill("Инициатива после e4");
  const tagInput = editor.getByRole("combobox", { name: "Тег знания" });
  await tagInput.fill("Инициатива");
  await tagInput.press("Enter");
  await page.getByRole("button", { name: "Главная", exact: true }).click();
  await page.getByRole("button", { name: "Знания", exact: true }).click();
  await expect(
    editor.getByRole("textbox", { name: "Название знания", exact: true }),
  ).toHaveValue("Инициатива после e4");
  await expect(
    editor.getByRole("textbox", {
      name: "Начальная FEN знания",
      includeHidden: true,
    }),
  ).toHaveValue(atE4.selected_fen);
  await expect(
    editor.getByRole("list", { name: "Теги знания", exact: true }),
  ).toContainText("Инициатива");
  results.newFormTabPreserved = true;
  await editor.getByRole("button", { name: "Создать", exact: true }).click();
  await expect(editor.locator(".knowledge-header [role=status]")).toHaveText(
    "Сохранено",
  );
  const createdSummary = (await native("knowledge_list")).find(
    (item) => item.title === "Инициатива после e4",
  );
  expect(createdSummary.classification).toBe("position");
  expect(createdSummary.tags).toEqual(["Инициатива"]);
  const created = await load(createdSummary.id);
  expect(created.chapters[0].initial_fen).toBe(atE4.selected_fen);
  expect(created.chapters[0].root.fen).toBe(atE4.selected_fen);
  expect(created.chapters[0].links).toContainEqual({
    target: { type: "position", position_key: atE4.selected_key },
    provenance: "game_graph",
    initial_fen: root.initial_fen,
    path_uci: ["e2e4"],
  });
  results.created = {
    id: created.id,
    fen: created.chapters[0].root.fen,
    links: created.chapters[0].links,
  };
  await onGraph([]);
  await expect(
    graph.getByRole("region", { name: "Знания в позиции", exact: true }),
  ).toContainText("Инициатива после e4");

  // Arbitrary isolated FEN knowledge opens on the graph even without game roots.
  const isolatedFen = "8/8/8/3k4/8/4K3/8/8 w - - 12 37";
  const isolated = await native("knowledge_import", {
    request: {
      title: "Королевская оппозиция",
      side: "both",
      kind: "study",
      pgn: `[Event "Королевская оппозиция"]\n[SetUp "1"]\n[FEN "${isolatedFen}"]\n[Result "*"]\n\n*`,
    },
  });
  const isolatedRoot = isolated.chapters[0].root;
  const standalone = await query({ position_key: isolatedRoot.position_key });
  expect(standalone.selected_fen).toBe(isolatedFen);
  expect(standalone.initial_fen).toBe(isolatedFen);
  expect(standalone.line).toEqual([]);
  expect(standalone.selected.games).toBe(0);
  expect(standalone.selected.knowledge[0].classification).toBe("position");
  // Catalogue refreshes through its real active-page lifecycle.
  await page.getByRole("button", { name: "Знания", exact: true }).click();
  if (await editor.isVisible())
    await editor
      .getByRole("button", { name: "К знаниям", exact: true })
      .click();
  const row = page
    .locator(".knowledge-library-row")
    .filter({ hasText: "Королевская оппозиция" });
  await expect(row).toBeVisible();
  await row.getByRole("button", { name: /Показать в графе/ }).click();
  await expect(graph).toBeVisible();
  await line([]);
  await ready();
  await expect(
    graph.getByRole("region", { name: "Знания в позиции", exact: true }),
  ).toContainText("Королевская оппозиция");
  await expect(graph.locator(".branch-current")).toContainText("Партий: 0");
  results.isolatedFen = standalone.selected_fen;

  await theme("dark");
  await screenshot("knowledge-graph-isolated-dark.png");
  await page.setViewportSize({ width: 900, height: 700 });
  await expect
    .poll(() =>
      page.evaluate(() => document.documentElement.scrollWidth <= innerWidth),
    )
    .toBe(true);
  await screenshot("knowledge-graph-isolated-900.png");
  await page.setViewportSize({ width: 1440, height: 960 });
  await theme("light");
  const finalProjection = await query();
  expect(finalProjection.eligible_games).toBe(0);
  expect(finalProjection.total_positions).toBe(0);
  expect(finalProjection.total_visits).toBe(0);
  expect(errors).toEqual([]);
  await writeFile(
    join(artifacts, "knowledge-graph-results.json"),
    JSON.stringify({ pass: true, ...results }, null, 2),
  );
  console.log(
    `PASS native Knowledge graph: zero factual counts, study navigation, exact book links, toggle, FEN/context creation and isolated positions; artifacts ${artifacts}`,
  );
} catch (error) {
  await screenshot("knowledge-graph-failure.png").catch(() => {});
  await writeFile(
    join(artifacts, "knowledge-graph-results.json"),
    JSON.stringify(
      { pass: false, error: String(error), errors, ...results },
      null,
      2,
    ),
  ).catch(() => {});
  console.error(`Artifacts ${artifacts}`);
  throw error;
} finally {
  await browser.close();
}

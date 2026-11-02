// Real App, Rust/SQLite and Chessground. This uses only a disposable native
// profile and never replaces game-review, knowledge or training IPC responses.
import { chromium, expect as baseExpect } from "@playwright/test";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve, join, sep } from "node:path";

const expect = baseExpect.configure({ timeout: 20_000 });
const profile = process.env.CHESSPUSH_TEST_DATA;
const endpoint = process.env.CHESSPUSH_TEST_CDP;
if (!profile || !endpoint || !resolve(profile).startsWith(resolve(".qa") + sep))
  throw new Error("Run with the existing isolated native profile runner");
const artifacts = resolve(process.env.CHESSPUSH_TRAINING_ARTIFACTS);
await mkdir(artifacts, { recursive: true });
const fixture = process.env.CHESSPUSH_FIXTURE_EXECUTABLE;
const seeded = await promisify(execFile)(
  fixture ?? "cargo",
  fixture
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
console.log("Game-review native fixture ready; Cargo target lock released");
const browser = await chromium.connectOverCDP(endpoint);
const page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(20_000);
await page.setViewportSize({ width: 1440, height: 960 });
await page.emulateMedia({ reducedMotion: "reduce" });
const errors = [],
  checks = [];
page.on("pageerror", (error) => errors.push(error.message));
await page.waitForFunction(
  () =>
    window.__TAURI_INTERNALS__?.invoke && document.querySelector(".app-header"),
);
async function native(command, args = {}) {
  const result = JSON.parse(
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
  if (!result.ok)
    throw new Error(`${command}: ${JSON.stringify(result.error)}`);
  return result.data;
}
const nav = (name) =>
  page
    .getByRole("navigation", { name: "Разделы", exact: true })
    .getByRole("button", { name, exact: true })
    .click();
const editor = () =>
  page.getByRole("region", { name: "Редактор разбора партии", exact: true });
async function saved() {
  await expect(
    page.locator(".knowledge-header:visible [role=status]"),
  ).toHaveText("Сохранено");
}
async function save() {
  await page.getByRole("button", { name: "Сохранить", exact: true }).click();
  await saved();
}
const load = (documentId) =>
  native("knowledge_load", { documentId, revision: null });
const documents = () => native("knowledge_list");
const flatten = (root) => {
  const values = [],
    queue = [root];
  while (queue.length) {
    const node = queue.shift();
    values.push(node);
    queue.push(...node.children);
  }
  return values;
};
function snapshot(tables) {
  const db = new DatabaseSync(join(profile, "chesspush.sqlite3"), {
    readOnly: true,
  });
  db.exec("PRAGMA busy_timeout=5000; BEGIN");
  try {
    return Object.fromEntries(
      tables.map((table) => {
        const rows = db
          .prepare(`SELECT * FROM "${table}"`)
          .all()
          .map((row) =>
            JSON.stringify(row, (_, value) =>
              typeof value === "bigint" ? value.toString() : value,
            ),
          )
          .sort();
        return [
          table,
          {
            count: rows.length,
            hash: createHash("sha256")
              .update(JSON.stringify(rows))
              .digest("hex"),
          },
        ];
      }),
    );
  } finally {
    db.exec("COMMIT");
    db.close();
  }
}
async function play(
  uci,
  boardSelector = ".knowledge-board-panel cg-board",
  orientation = "white",
  drag = false,
) {
  const board = page.locator(boardSelector);
  await board.scrollIntoViewIfNeeded();
  const box = await board.boundingBox();
  const point = (square) => {
    const file = square.charCodeAt(0) - 97,
      rank = Number(square[1]) - 1;
    return {
      x:
        box.x +
        ((orientation === "white" ? file + 0.5 : 7.5 - file) * box.width) / 8,
      y:
        box.y +
        ((orientation === "white" ? 7.5 - rank : rank + 0.5) * box.height) / 8,
    };
  };
  const from = point(uci.slice(0, 2)),
    to = point(uci.slice(2, 4));
  if (drag) {
    await page.mouse.move(from.x, from.y);
    await page.mouse.down();
    await page.mouse.move(to.x, to.y, { steps: 10 });
    await page.mouse.up();
  } else {
    await page.mouse.click(from.x, from.y);
    await page.mouse.click(to.x, to.y);
  }
}
async function selectNode(id) {
  const button = page.locator(`[data-knowledge-node="${id}"]`);
  await button.click();
  await expect(button).toHaveAttribute("aria-current", "step");
  await expect(page.locator(".knowledge-board-panel:visible")).toHaveAttribute(
    "aria-busy",
    "false",
  );
}
async function shape() {
  const board = page.locator(".knowledge-board-panel cg-board");
  await board.scrollIntoViewIfNeeded();
  const box = await board.boundingBox();
  const point = (square) => ({
    x: box.x + ((square.charCodeAt(0) - 97 + 0.5) * box.width) / 8,
    y: box.y + ((8 - Number(square[1]) + 0.5) * box.height) / 8,
  });
  const from = point("c4"),
    to = point("d5"),
    circle = point("d4");
  await page.mouse.move(from.x, from.y);
  await page.mouse.down({ button: "right" });
  await page.mouse.move(to.x, to.y, { steps: 8 });
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
}
async function extract(scope, title) {
  await page
    .getByRole("button", { name: "Сохранить знание", exact: true })
    .click();
  const dialog = page.getByRole("dialog", {
    name: "Сохранить знание",
    exact: true,
  });
  await dialog
    .getByRole("textbox", { name: "Название сохраняемого знания" })
    .fill(title);
  await dialog
    .getByRole("combobox", { name: "Материал знания" })
    .selectOption(scope);
  await dialog
    .getByRole("button", { name: "Сохранить знание", exact: true })
    .click();
  await expect(
    page.getByRole("region", { name: "Редактор знания", exact: true }),
  ).toBeVisible();
  await saved();
  const summary = (await documents()).find((item) => item.title === title);
  expect(summary.kind).toBe("study");
  return { summary, document: await load(summary.id) };
}
async function theme(value) {
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  const settings = page.getByRole("dialog", { name: "Настройки", exact: true });
  await settings.getByRole("tab", { name: "Внешний вид", exact: true }).click();
  await settings
    .getByRole("combobox", { name: "Тема", exact: true })
    .selectOption(value);
  await settings
    .getByRole("button", { name: "Закрыть настройки", exact: true })
    .click();
  await expect(page.locator("html")).toHaveAttribute("data-theme", value);
}
async function reviewStatusGlyphs(reviewedGameId = null) {
  const rows = page
    .getByRole("table", { name: "Свои партии", exact: true })
    .locator("tbody tr");
  await expect(rows).toHaveCount(4);
  for (const row of await rows.all()) {
    const reviewed =
      (await row.getAttribute("data-game-id")) === reviewedGameId;
    const name = reviewed ? "Разобрана" : "Не разобрана";
    const glyph = reviewed ? "✓" : "—";
    const cell = row.getByRole("cell").last();
    await expect(cell).toHaveText(glyph);
    const status = cell.getByRole("img", { name, exact: true });
    await expect(status).toHaveText(glyph);
    await expect(status).toHaveAttribute("title", name);
  }
}
async function captureReviewList(width) {
  await page.setViewportSize({ width, height: width === 900 ? 700 : 960 });
  const table = page.getByRole("table", { name: "Свои партии", exact: true });
  await expect(table.getByRole("button").first()).toBeEnabled();
  await expect(
    page.getByText("Открывается партия…", { exact: true }),
  ).toHaveCount(0);
  await expect(page.locator(".review-library:visible")).toHaveAttribute(
    "aria-busy",
    "false",
  );
  const geometry = await table.evaluate((element) => {
    const bounds = (node) => {
      const rect = node.getBoundingClientRect();
      return { left: rect.left, right: rect.right, width: rect.width };
    };
    return {
      viewport: innerWidth,
      pageWidth: document.documentElement.scrollWidth,
      table: bounds(element),
      headers: [...element.querySelectorAll("th")].map((cell) => ({
        text: cell.textContent.trim(),
        ...bounds(cell),
        clientWidth: cell.clientWidth,
        scrollWidth: cell.scrollWidth,
      })),
      rows: [...element.querySelectorAll("tbody tr")].map((row) => ({
        gameId: row.getAttribute("data-game-id"),
        cells: [...row.querySelectorAll("td")].map((cell) => ({
          text: cell.textContent.trim(),
          ...bounds(cell),
          clientWidth: cell.clientWidth,
          scrollWidth: cell.scrollWidth,
          title: cell.getAttribute("title"),
          textOverflow: getComputedStyle(cell).textOverflow,
        })),
      })),
    };
  });
  await writeFile(
    join(artifacts, `game-review-list-layout-${width}.json`),
    JSON.stringify(geometry, null, 2),
  );
  await page.screenshot({
    path: join(artifacts, `game-review-list-${width}-light.png`),
    fullPage: true,
  });
  expect(geometry.headers.map((cell) => cell.text)).toEqual([
    "Дата",
    "Партия",
    "Результат",
    "Контроль",
    "Дебют",
    "Теги",
    "Анализ",
    "Разбор",
  ]);
  expect(geometry.pageWidth).toBeLessThanOrEqual(width);
  expect(geometry.table.left).toBeGreaterThanOrEqual(0);
  expect(geometry.table.right).toBeLessThanOrEqual(width);
  for (const header of geometry.headers) {
    expect(header.width, `${width}px ${header.text} column`).toBeGreaterThan(
      20,
    );
    expect(
      header.scrollWidth,
      `${width}px ${header.text} heading cropped`,
    ).toBeLessThanOrEqual(header.clientWidth);
  }
  for (const row of geometry.rows) {
    expect(row.cells).toHaveLength(8);
    const status = row.cells.at(-1);
    expect(status.text).toMatch(/^[✓—]$/);
    expect(status.scrollWidth).toBeLessThanOrEqual(status.clientWidth);
  }
}

try {
  const facts = snapshot([
    "games",
    "statistics_games",
    "statistics_account_games",
    "statistics_payloads",
  ]);
  const initial = await native("game_review_list", {
    request: { limit: 100, offset: 0 },
  });
  expect(initial.total).toBe(4);
  const source = initial.rows.find((row) => row.game_id.includes("stTr0001"));
  expect(source.own_color).toBe("white");
  await nav("Разбор партий");
  await expect(
    page.getByRole("table", { name: "Свои партии" }).locator("tbody tr"),
  ).toHaveCount(4);
  await reviewStatusGlyphs();
  await page
    .locator(`[data-game-id="${source.game_id}"]`)
    .getByRole("button")
    .click();
  await expect(editor()).toBeVisible();
  await saved();
  const engine = page.getByRole("region", {
    name: "Анализ позиции",
    exact: true,
  });
  await engine
    .getByRole("checkbox", { name: "Stockfish", exact: true })
    .check();
  await expect(engine.getByRole("alert")).toContainText("Stockfish");
  await engine
    .getByRole("checkbox", { name: "Stockfish", exact: true })
    .uncheck();
  await expect(engine.getByRole("alert")).toHaveCount(0);
  let review = await native("game_review_open", {
    gameId: source.game_id,
    accountId: source.account_id,
    sourceRevision: source.source_revision,
  });
  expect(review.document.kind).toBe("analysis");
  const originalDocumentId = review.document.id;
  const node2 = review.source_nodes.find((node) => node.ply === 2);
  await selectNode(node2.node_id);
  const tag = page.getByRole("combobox", { name: "Тег партии", exact: true });
  await tag.fill("План фианкетто");
  await tag.press("Enter");
  await page
    .getByRole("textbox", { name: "Комментарий к позиции" })
    .fill("Контроль центра. Сравнить g3 и c4.");
  // Reviewed is a committed action and must persist the currently typed tags/comment.
  const reviewed = page.getByRole("checkbox", {
    name: "Разобрана",
    exact: true,
  });
  await reviewed.click();
  await expect(reviewed).toBeChecked();
  await saved();
  let current = await load(originalDocumentId);
  expect(current.metadata.tags).toEqual(["План фианкетто"]);
  expect(
    await native("knowledge_tag_suggestions", { query: "план ф" }),
  ).toEqual(["План фианкетто"]);
  expect(
    flatten(current.chapters[0].root)
      .find((node) => node.id === node2.node_id)
      .comments.join(" "),
  ).toContain("Сравнить g3 и c4");
  expect(
    (
      await native("game_review_list", {
        request: { reviewed: true, tag: "План фианкетто", limit: 100 },
      })
    ).rows.map((row) => row.game_id),
  ).toEqual([source.game_id]);
  await play("c2c4", undefined, "white", true);
  await expect
    .poll(async () =>
      flatten((await load(originalDocumentId)).chapters[0].root).some(
        (node) => node.uci === "c2c4",
      ),
    )
    .toBe(true);
  await expect(page.locator(".knowledge-board-panel")).toHaveAttribute(
    "aria-busy",
    "false",
  );
  await shape();
  await page.getByRole("tab", { name: "Комментарий", exact: true }).click();
  await page
    .getByRole("textbox", { name: "Комментарий к позиции" })
    .fill("Кандидат c4: сравнить получающуюся структуру.");
  await page.getByRole("tab", { name: "Аннотации", exact: true }).click();
  await page
    .getByRole("button", { name: "!? — Интересный ход", exact: true })
    .click();
  await save();
  current = await load(originalDocumentId);
  const variation = flatten(current.chapters[0].root).find(
    (node) => node.uci === "c2c4",
  );
  expect(variation.nags).toContain(5);
  expect(variation.shapes).toHaveLength(2);
  expect(variation.comments.join(" ")).toContain("Кандидат c4");
  expect(snapshot(Object.keys(facts))).toEqual(facts);
  checks.push(
    "Actual own-game list, party tags, reviewed commit, real board variation/NAG/arrows/circle; original factual tables unchanged",
  );

  await selectNode(node2.node_id);
  await page.getByRole("tab", { name: "Комментарий", exact: true }).click();
  await page.screenshot({
    path: join(artifacts, "game-review-light.png"),
    fullPage: true,
  });
  await theme("dark");
  await page.screenshot({
    path: join(artifacts, "game-review-dark.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 900, height: 700 });
  await expect
    .poll(() => page.evaluate(() => document.documentElement.scrollWidth))
    .toBeLessThanOrEqual(900);
  const narrowLayout = await page.evaluate(() => {
    const headerControls = [
      ...document.querySelectorAll(
        ".knowledge-header button, .knowledge-header input, .knowledge-header summary",
      ),
    ]
      .filter((element) => element.getClientRects().length)
      .map((element) => {
        const rect = element.getBoundingClientRect();
        return {
          name:
            element.getAttribute("aria-label") ?? element.textContent.trim(),
          left: rect.left,
          right: rect.right,
        };
      });
    const notation = document.querySelector(".knowledge-notation");
    return {
      headerControls,
      clippedControls: headerControls.filter(
        (rect) => rect.left < -1 || rect.right > innerWidth + 1,
      ),
      notation: {
        clientHeight: notation.clientHeight,
        scrollHeight: notation.scrollHeight,
        overflowY: getComputedStyle(notation).overflowY,
      },
    };
  });
  expect(narrowLayout.clippedControls).toEqual([]);
  expect(narrowLayout.notation.clientHeight).toBeGreaterThan(100);
  expect(["auto", "scroll"]).toContain(narrowLayout.notation.overflowY);
  const board = page.locator(".knowledge-board-panel cg-board");
  await board.scrollIntoViewIfNeeded();
  await expect(board).toBeInViewport({ ratio: 1 });
  const boardControls = page.locator(".knowledge-board-controls");
  await boardControls.scrollIntoViewIfNeeded();
  await expect(boardControls).toBeInViewport({ ratio: 1 });
  await page.getByRole("tab", { name: "Комментарий", exact: true }).click();
  const comment = page.getByRole("textbox", { name: "Комментарий к позиции" });
  await comment.scrollIntoViewIfNeeded();
  await expect(comment).toBeInViewport({ ratio: 1 });
  await expect(comment).toHaveValue("Контроль центра. Сравнить g3 и c4.");
  const narrowTag = page.getByRole("combobox", {
    name: "Тег партии",
    exact: true,
  });
  await narrowTag.scrollIntoViewIfNeeded();
  await narrowTag.click();
  await expect(narrowTag).toBeFocused();
  await writeFile(
    join(artifacts, "game-review-layout-900.json"),
    JSON.stringify(narrowLayout, null, 2),
  );
  await page.screenshot({
    path: join(artifacts, "game-review-900.png"),
    fullPage: true,
  });
  await page.setViewportSize({ width: 1440, height: 960 });
  await theme("light");

  const branch = await extract("branch", "План после Nf6");
  const studyId = branch.document.id,
    chapterId = branch.document.chapters[0].id;
  expect(branch.summary.classification).toBe("theory");
  expect(branch.document.chapters[0].root.id).not.toBe(node2.node_id);
  expect(
    branch.document.chapters[0].root.children.map((node) => node.uci),
  ).toEqual(["g2g3", "c2c4"]);
  expect(branch.document.metadata.tags).toEqual(["План фианкетто"]);
  const occurrence = branch.document.chapters[0].links.find(
    (link) => link.target.type === "occurrence",
  );
  expect(occurrence.target).toEqual({
    type: "occurrence",
    game_id: source.game_id,
    revision: source.source_revision,
    ply: 2,
  });
  expect(occurrence.path_uci).toEqual(["g1f3", "g8f6"]);
  expect(
    branch.document.chapters[0].links.some(
      (link) =>
        link.target.type === "document_node" &&
        link.target.document_id === originalDocumentId &&
        link.target.node_id === node2.node_id,
    ),
  ).toBe(true);
  await page.locator(".knowledge-repertoire-menu > summary").click();
  await page
    .getByRole("combobox", { name: "Сторона репертуара знания" })
    .selectOption("white");
  await page
    .getByRole("button", { name: "Добавить в репертуар", exact: true })
    .click();
  await saved();
  expect((await load(studyId)).metadata.repertoire_attachment.active).toBe(
    true,
  );
  expect((await load(studyId)).kind).toBe("study");
  const combined = await native("knowledge_repertoire", { side: "white" });
  expect(combined.metadata.source_document_ids).toEqual([studyId]);
  expect(combined.chapters).toHaveLength(1);
  expect(
    flatten(combined.chapters[0].root).some(
      (node) => node.id === branch.document.chapters[0].root.id,
    ),
  ).toBe(true);
  expect(
    (await documents()).filter((item) => item.kind === "repertoire"),
  ).toHaveLength(0);
  await page.getByRole("button", { name: "Повторить", exact: true }).click();
  const training = page.getByRole("main", { name: "Тренировка репертуара" });
  await expect(training).toBeVisible();
  await expect(
    training.getByRole("combobox", { name: "Глава", exact: true }),
  ).toHaveValue(JSON.stringify([studyId, chapterId]));
  await training.getByRole("button", { name: "Начать", exact: true }).click();
  await expect(
    training.getByRole("progressbar", { name: "Прогресс повторения" }),
  ).toBeVisible();
  const scope = {
    side: "white",
    chapter_id: chapterId,
    source_document_id: studyId,
  };
  const trainingView = () =>
    native("training_start", {
      request: { scope, mode: "srs", restart: false, new_limit: 5 },
    });
  let session = await trainingView();
  expect(session.scope).toMatchObject(scope);
  const stableSessionId = session.id;
  for (let step = 0; step < 16; step++) {
    if (
      snapshot(["repertoire_training_srs"]).repertoire_training_srs.count > 0 ||
      session.finished
    )
      break;
    const identity = `${session.step}:${session.feedback}:${session.prompt?.position_key}`;
    if (session.feedback === "awaiting" || session.feedback === "retry") {
      const prepared = flatten((await load(studyId)).chapters[0].root).find(
        (node) =>
          node.position_key === session.prompt.position_key &&
          node.children.length,
      );
      expect(prepared).toBeTruthy();
      await play(prepared.children[0].uci, ".training-board cg-board", "white");
    } else {
      const forward = training.getByRole("button", {
        name: "Дальше",
        exact: true,
      });
      if (await forward.count())
        await forward.click({ timeout: 1000 }).catch(() => {});
    }
    await expect
      .poll(async () => {
        const value = await trainingView();
        return `${value.step}:${value.feedback}:${value.prompt?.position_key}`;
      })
      .not.toBe(identity);
    session = await trainingView();
  }
  expect(session.id).toBe(stableSessionId);
  expect(
    snapshot(["repertoire_training_srs"]).repertoire_training_srs.count,
  ).toBeGreaterThan(0);
  await nav("Репертуар");
  await expect(
    page.getByRole("main", { name: "Репертуар", exact: true }),
  ).toBeVisible();
  const history = snapshot([
    "repertoire_training_srs",
    "repertoire_training_reviews",
  ]);
  await page
    .getByRole("button", { name: "Отсоединить План после Nf6", exact: true })
    .click();
  await expect
    .poll(
      async () => (await load(studyId)).metadata.repertoire_attachment.active,
    )
    .toBe(false);
  await nav("Знания");
  await page.getByRole("button", { name: "К знаниям", exact: true }).click();
  await page
    .getByRole("button", { name: /План после Nf6 Теоретическая заметка/ })
    .click();
  await saved();
  await page.locator(".knowledge-repertoire-menu > summary").click();
  await page
    .getByRole("button", { name: "Добавить в репертуар", exact: true })
    .click();
  await saved();
  expect((await load(studyId)).metadata.repertoire_attachment.active).toBe(
    true,
  );
  expect(snapshot(Object.keys(history))).toEqual(history);
  checks.push(
    "Knowledge retains one canonical study ID while gaining repertoire role; direct chapter training creates real FSRS history, retained through detach/reattach",
  );

  await page
    .getByRole("button", { name: "Исходная партия", exact: true })
    .click();
  await expect(editor()).toBeVisible();
  await expect(
    page.locator(`[data-knowledge-node="${node2.node_id}"]`),
  ).toHaveAttribute("aria-current", "step");
  const c4Branch = page.getByRole("button", {
    name: "Варианты после Nf6: c4",
    exact: true,
  });
  await expect(c4Branch).toHaveAttribute("aria-expanded", "false");
  await c4Branch.click();
  await expect(c4Branch).toHaveAttribute("aria-expanded", "true");
  await selectNode(variation.id);
  const position = await extract("position", "Кандидат c4 — позиция");
  expect(position.summary.classification).toBe("position");
  expect(position.document.chapters[0].root.children).toHaveLength(0);
  const hypotheticalSource = position.document.chapters[0].links.find(
    (link) => link.target.type === "occurrence",
  );
  expect(hypotheticalSource.target.ply).toBe(2);
  expect(hypotheticalSource.path_uci).toEqual(["g1f3", "g8f6"]);
  await page
    .getByRole("button", { name: "Исходная партия", exact: true })
    .click();
  await expect(
    page.locator(`[data-knowledge-node="${node2.node_id}"]`),
  ).toHaveAttribute("aria-current", "step");
  const whole = await extract("game", "Полный разбор фианкетто");
  expect(whole.document.chapters[0].initial_fen).toBe(
    review.game.replay.initial_fen,
  );
  expect(
    flatten(whole.document.chapters[0].root).some(
      (node) =>
        node.uci === "c2c4" &&
        node.nags.includes(5) &&
        node.shapes.length === 2,
    ),
  ).toBe(true);
  await page
    .getByRole("button", { name: "Исходная партия", exact: true })
    .click();
  await expect(editor()).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Сохранить знание", exact: true }),
  ).toBeEnabled();
  await page.getByRole("button", { name: "К партиям", exact: true }).click();
  const list = page.getByRole("table", { name: "Свои партии", exact: true });
  await page
    .getByRole("combobox", { name: "Статус разбора", exact: true })
    .selectOption("true");
  await page
    .getByRole("combobox", { name: "Тег партии", exact: true })
    .selectOption("План фианкетто");
  await expect(list.locator("tbody tr")).toHaveCount(1);
  await expect(list.locator("tbody tr")).toHaveAttribute(
    "data-game-id",
    source.game_id,
  );
  await page.getByRole("searchbox", { name: "Поиск партий" }).fill("Opponent");
  await expect(list.locator("tbody tr")).toHaveCount(1);
  await page.getByRole("searchbox", { name: "Поиск партий" }).fill("");
  await page
    .getByRole("combobox", { name: "Статус разбора", exact: true })
    .selectOption("");
  await page
    .getByRole("combobox", { name: "Тег партии", exact: true })
    .selectOption("");
  await page
    .getByRole("combobox", { name: "Цвет в партии", exact: true })
    .selectOption("black");
  await expect(list.locator("tbody tr")).toHaveCount(1);
  await page
    .getByRole("combobox", { name: "Цвет в партии", exact: true })
    .selectOption("");
  await expect(list.locator("tbody tr")).toHaveCount(4);
  await reviewStatusGlyphs(source.game_id);
  await captureReviewList(1440);
  await captureReviewList(900);
  await page.setViewportSize({ width: 1440, height: 960 });
  checks.push(
    "Position/branch/whole-game extraction preserve annotations and typed provenance; hypothetical line links to real common prefix; source navigation and reviewed/tag/side/search filters; accessible status glyphs and every table column fit 1440/900px",
  );

  await nav("Статистика");
  await page.getByRole("tab", { name: "Граф партий", exact: true }).click();
  const graph = page.getByRole("region", { name: "Граф партий", exact: true });
  await expect(graph).toHaveAttribute("aria-busy", "false");
  await graph.getByRole("button", { name: "Партия", exact: true }).click();
  await expect(editor()).toBeVisible();
  await page.reload();
  await nav("Разбор партий");
  await page
    .locator(`[data-game-id="${source.game_id}"]`)
    .getByRole("button")
    .click();
  await expect(editor()).toBeVisible();
  await saved();
  const reopened = await native("game_review_open", {
    gameId: source.game_id,
    accountId: source.account_id,
    sourceRevision: source.source_revision,
  });
  expect(reopened.document.id).toBe(originalDocumentId);
  expect(reopened.reviewed).toBe(true);
  await expect(page.getByRole("list", { name: "Теги партии" })).toContainText(
    "План фианкетто",
  );
  expect(
    flatten(reopened.document.chapters[0].root).find(
      (node) => node.id === variation.id,
    ).shapes,
  ).toHaveLength(2);
  expect(snapshot(Object.keys(facts))).toEqual(facts);
  expect(errors).toEqual([]);
  checks.push(
    "Graph Party action opens the editable review workspace; all factual game/payload/membership rows remain byte-identical",
  );
  await writeFile(
    join(artifacts, "game-review-results.json"),
    JSON.stringify(
      {
        pass: true,
        checks,
        sourceGame: source.game_id,
        reviewDocument: originalDocumentId,
        sharedKnowledge: studyId,
        history,
        facts,
        errors,
      },
      null,
      2,
    ),
  );
  console.log(`PASS game-review native integration; artifacts: ${artifacts}`);
} catch (error) {
  await page
    .screenshot({
      path: join(artifacts, "game-review-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  await writeFile(
    join(artifacts, "game-review-failure.txt"),
    await page
      .locator("body")
      .innerText()
      .catch(() => ""),
  );
  throw error;
} finally {
  await browser.close();
}

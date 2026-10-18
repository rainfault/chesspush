import { expect } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const INITIAL_FEN = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const CUSTOM_FEN = "7k/P7/8/8/8/8/8/7K w - - 0 1";
const IMPORT_PGN = `[Event "Imported RAV"]
[Extra "preserved"]

1. e4 $1 {Idea [%eval 0.3] [%cal Ge2e4] [%csl Rd4]} e5
({Sicilian reply} 1... c5 2. Nf3 (2. Nc3) d6) 2. Nf3 Nc6 *

[Event "Imported FEN"]
[SetUp "1"]
[FEN "r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1"]

1. O-O (1. O-O-O) O-O-O *`;

function nodes(root) {
  const result = [],
    stack = [root];
  while (stack.length) {
    const current = stack.pop();
    result.push(current);
    stack.push(...current.children);
  }
  return result;
}
function semantic(node) {
  return {
    fen: node.fen,
    uci: node.uci,
    san: node.san,
    comments: node.comments,
    starting_comments: node.starting_comments ?? [],
    nags: node.nags,
    shapes: node.shapes,
    children: node.children.map(semantic),
  };
}
function resultSignature(report) {
  return {
    total: report.total_games,
    results: report.results.map(({ key, games, wins, draws, losses }) => ({
      key,
      games,
      wins,
      draws,
      losses,
    })),
  };
}

export async function runRepertoireChecks(page, invoke, phase, data) {
  const manifestPath = join(data, "repertoire-native.json");
  const report = () =>
    invoke("statistics_report", { filter: { variant: "standard" } });
  const before = resultSignature(await report());
  const editor = page.getByRole("dialog", {
    name: "Редактор репертуара",
    exact: true,
  });
  const loaded = (id) =>
    invoke("knowledge_load", { documentId: id, revision: null });
  const assertNoEditorError = async () =>
    expect(editor.getByRole("alert")).toHaveCount(0);

  if (phase === "persist") {
    const manifest = JSON.parse(await readFile(manifestPath, "utf8"));
    const document = await loaded(manifest.id);
    expect(document).toEqual(manifest.document);
    await page
      .getByLabel("Репертуар графа", { exact: true })
      .selectOption(manifest.id);
    await page
      .getByRole("button", { name: "Редактировать репертуар", exact: true })
      .click();
    await expect(editor).toBeVisible();
    await expect(
      editor.getByLabel("Название репертуара", { exact: true }),
    ).toHaveValue("Native repertoire");
    await expect(
      editor.getByLabel("Сторона репертуара", { exact: true }),
    ).toHaveValue("white");
    await expect(
      editor.getByLabel("Глава репертуара", { exact: true }).locator("option"),
    ).toHaveCount(4);
    await editor
      .getByLabel("Глава репертуара", { exact: true })
      .selectOption(manifest.firstChapterId);
    await editor.locator(`[data-knowledge-node="${manifest.d4Id}"]`).click();
    await expect(
      editor.getByLabel("Комментарий к позиции", { exact: true }),
    ).toHaveValue("Primary centre\n\nSaved through shared document API");
    await expect(
      editor.getByLabel("NAG выбранного хода", { exact: true }),
    ).toHaveValue("$1 $14");
    await assertNoEditorError();
    await editor.getByRole("button", { name: "В граф", exact: true }).click();
    await expect(editor).toBeHidden();
    expect(resultSignature(await report())).toEqual(before);
    return {
      document_id: manifest.id,
      revision: document.revision,
      persisted_chapters: document.chapters.length,
      stable_ids: true,
    };
  }

  await page
    .getByRole("button", { name: "Создать репертуар", exact: true })
    .click();
  await expect(editor).toBeVisible();
  await editor
    .getByLabel("Название репертуара", { exact: true })
    .fill("Native repertoire");
  await editor
    .getByLabel("Сторона репертуара", { exact: true })
    .selectOption("white");
  await editor
    .getByLabel("Начальная FEN репертуара", { exact: true })
    .fill(INITIAL_FEN);
  await editor.getByRole("button", { name: "Создать", exact: true }).click();
  await expect(
    editor.getByLabel("Глава репертуара", { exact: true }),
  ).toBeVisible();
  let document = (await invoke("knowledge_list")).find(
    (item) => item.title === "Native repertoire",
  );
  expect(document).toBeTruthy();
  const id = document.id;
  const revisionSaved = async (previous) => {
    try {
      await expect
        .poll(async () => (await loaded(id)).revision)
        .toBeGreaterThan(previous);
    } catch (failure) {
      const alert = await editor
        .getByRole("alert")
        .textContent()
        .catch(() => null);
      throw Error(alert || String(failure));
    }
    await expect(editor.getByRole("status")).toContainText("Сохранено");
    await assertNoEditorError();
    return loaded(id);
  };
  const square = async (name) => {
    const box = await editor
      .locator(".knowledge-board-panel > .board-frame > .cg-wrap")
      .boundingBox();
    if (!box) throw Error("Repertoire board unavailable");
    return {
      x: box.x + ((name.charCodeAt(0) - 97 + 0.5) * box.width) / 8,
      y: box.y + ((8 - Number(name[1]) + 0.5) * box.height) / 8,
    };
  };
  const play = async (from, to, promotion) => {
    const previous = (await loaded(id)).revision;
    // Wait for the authoritative backend navigation before sending real board input.
    await expect(editor.locator(".knowledge-board-panel")).toHaveAttribute(
      "aria-busy",
      "false",
    );
    const origin = await square(from),
      target = await square(to);
    await page.mouse.click(origin.x, origin.y);
    await page.mouse.click(target.x, target.y);
    if (promotion)
      await editor
        .getByRole("button", { name: promotion, exact: true })
        .click();
    return revisionSaved(previous);
  };

  document = await play("e2", "e4");
  const firstChapterId = document.chapters[0].id;
  const rootId = document.chapters[0].root.id;
  const e4Id = document.chapters[0].root.children[0].id;
  await play("e7", "e5");
  await play("g1", "f3");
  await editor
    .getByRole("button", { name: "Начало главы", exact: true })
    .click();
  document = await play("d2", "d4");
  const d4Id = document.chapters[0].root.children.find(
    (node) => node.uci === "d2d4",
  ).id;
  await play("d7", "d5");
  document = await play("c2", "c4");
  const c4 = nodes(document.chapters[0].root).find(
    (node) => node.uci === "c2c4",
  );
  expect(c4).toBeTruthy();
  let previous = document.revision;
  await editor
    .getByRole("button", { name: "Удалить ветвь", exact: true })
    .click();
  await editor
    .getByRole("button", { name: "Удалить ветвь и продолжения", exact: true })
    .click();
  document = await revisionSaved(previous);
  expect(
    nodes(document.chapters[0].root).some((node) => node.id === c4.id),
  ).toBe(false);
  expect(
    nodes(document.chapters[0].root).some((node) => node.id === e4Id),
  ).toBe(true);
  await editor.locator(`[data-knowledge-node="${d4Id}"]`).click();
  previous = document.revision;
  await editor
    .getByRole("button", { name: "Главный вариант", exact: true })
    .click();
  document = await revisionSaved(previous);
  expect(document.chapters[0].root.children[0].id).toBe(d4Id);
  expect(document.chapters[0].root.id).toBe(rootId);
  await editor
    .getByLabel("Комментарий к позиции", { exact: true })
    .fill("Primary centre\n\nSaved through shared document API");
  await editor
    .getByLabel("NAG выбранного хода", { exact: true })
    .fill("$1 $14");
  const arrowStart = await square("d4"),
    arrowEnd = await square("d5");
  await page.mouse.move(arrowStart.x, arrowStart.y);
  await page.mouse.down({ button: "right" });
  await page.mouse.move(arrowEnd.x, arrowEnd.y, { steps: 5 });
  await page.mouse.up({ button: "right" });
  previous = document.revision;
  await editor
    .getByRole("button", { name: "Сохранить аннотации", exact: true })
    .click();
  document = await revisionSaved(previous);
  const annotated = nodes(document.chapters[0].root).find(
    (node) => node.id === d4Id,
  );
  expect(annotated.comments).toEqual([
    "Primary centre",
    "Saved through shared document API",
  ]);
  expect(annotated.nags).toEqual([1, 14]);
  expect(annotated.shapes).toContainEqual({
    brush: "green",
    orig: "d4",
    dest: "d5",
  });

  await editor
    .getByRole("button", { name: "Добавить главу", exact: true })
    .click();
  await editor
    .getByLabel("Название новой главы", { exact: true })
    .fill("Promotion root");
  await editor.getByLabel("FEN новой главы", { exact: true }).fill(CUSTOM_FEN);
  previous = document.revision;
  await editor
    .getByRole("button", { name: "Создать главу", exact: true })
    .click();
  document = await revisionSaved(previous);
  expect(document.chapters).toHaveLength(2);
  document = await play("a7", "a8", "knight");
  expect(document.chapters[1].root.children[0].uci).toBe("a7a8n");

  await editor.getByRole("button", { name: "Импорт PGN", exact: true }).click();
  await editor.getByLabel("PGN для импорта", { exact: true }).fill(IMPORT_PGN);
  previous = document.revision;
  await editor
    .getByRole("button", { name: "Добавить главы из PGN", exact: true })
    .click();
  document = await revisionSaved(previous);
  expect(document.chapters).toHaveLength(4);
  expect(document.chapters[2].root.children[0].children).toHaveLength(2);
  expect(document.chapters[2].headers.Extra).toBe("preserved");
  expect(document.chapters[3].root.children).toHaveLength(2);
  await editor
    .getByRole("button", { name: "Экспорт PGN", exact: true })
    .click();
  const exported = await editor
    .getByLabel("Экспортированный PGN", { exact: true })
    .inputValue();
  expect(exported).toContain("(");
  expect(exported).toContain("[%cal");
  expect(exported).toContain("[%eval 0.3]");
  const roundtrip = await invoke("knowledge_import", {
    request: { title: "Roundtrip contract", side: "white", pgn: exported },
  });
  expect(roundtrip.chapters.map((chapter) => semantic(chapter.root))).toEqual(
    document.chapters.map((chapter) => semantic(chapter.root)),
  );
  await editor
    .getByRole("button", { name: "Закрыть PGN", exact: true })
    .click();
  await editor
    .getByRole("button", { name: "Закрыть редактор репертуара", exact: true })
    .click();
  await expect(editor).toBeHidden();
  document = await loaded(id);
  await writeFile(
    manifestPath,
    JSON.stringify({ id, firstChapterId, d4Id, document }, null, 2),
  );
  expect(resultSignature(await report())).toEqual(before);
  return {
    document_id: id,
    revision: document.revision,
    chapters: document.chapters.length,
    played_moves: 7,
    preserved_rav: true,
    comments_nags_arrows: true,
    pgn_roundtrip: true,
    factual_results_unchanged: true,
  };
}

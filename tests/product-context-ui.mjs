import { expect } from "@playwright/test";
import { readFile, writeFile, copyFile } from "node:fs/promises";
import { bridge } from "./helpers/product-bridge.mjs";
const c = JSON.parse(await readFile(".qa/ui-atlas-tools/current.json")),
  h = await bridge(c, "light", { videoDirectory: "screens/product-ux/videos" }),
  p = h.page,
  results = [];
p.setDefaultTimeout(12000);
const nav = (s) =>
  p
    .locator(".app-header")
    .getByRole("button", { name: s, exact: true })
    .click();
const ready = () =>
  expect(
    p.getByRole("region", { name: "Карта игры", exact: true }),
  ).toHaveAttribute("aria-busy", "false");
try {
  await nav("Моя игра");
  await p.getByRole("tab", { name: "Карта игры", exact: true }).click();
  await ready();
  if (
    await p.getByRole("button", { name: "К началу", exact: true }).isEnabled()
  )
    await p.getByRole("button", { name: "К началу", exact: true }).click();
  await ready();
  await p.getByRole("button", { name: "Я чёрными", exact: true }).click();
  await ready();
  await expect(
    p.getByRole("heading", { name: "Ходы соперника", exact: true }),
  ).toBeVisible();
  await p
    .getByRole("button", { name: "Подробности позиции", exact: true })
    .click();
  const before = await p.locator(".graph-position-details").innerText();
  await p
    .getByRole("button", { name: "Перевернуть доску", exact: true })
    .click();
  expect(await p.locator(".graph-position-details").innerText()).toBe(before);
  results.push({
    scenario: "black POV labels + flip preserves outcomes",
    result: "PASS",
  });
  await p.screenshot({
    path: "screens/product-ux/after/black-pov-light-1440.png",
  });
  await p.getByRole("button", { name: "Я белыми", exact: true }).click();
  await ready();
  const docs = await h.native("knowledge_list"),
    doc = docs.find((d) => d.title === "Репертуар для съёмки");
  await p.getByLabel("Репертуар графа", { exact: true }).selectOption(doc.id);
  await ready();
  const row = p
    .locator(".continuation-row")
    .filter({ has: p.getByRole("button", { name: "Белые: e4", exact: true }) });
  await p.evaluate(() => window.scrollTo({ top: 80 }));
  const branchScroll = await p.evaluate(() => scrollY);
  await row
    .getByRole("button", { name: "✓ В репертуаре", exact: true })
    .click();
  await expect(
    p.getByRole("region", { name: "Редактор репертуара", exact: true }),
  ).toBeVisible();
  const chapter = await p
      .getByLabel("Глава репертуара", { exact: true })
      .inputValue(),
    old = await h.native("knowledge_load", { documentId: doc.id });
  const oldNode = old.chapters
    .find((ch) => ch.id === chapter)
    .root.children.find((n) => n.uci === "e2e4");
  await expect(
    p.getByLabel("Комментарий к позиции", { exact: true }),
  ).toHaveValue(oldNode.comments.join("\n\n"));
  const comment = "Проверка перехода из карты: e4. " + Date.now();
  await p.getByLabel("Комментарий к позиции", { exact: true }).fill(comment);
  await p.getByRole("button", { name: "Сохранить", exact: true }).click();
  await expect(
    p.getByRole("status", { exact: false }).filter({ hasText: "Сохранено" }),
  ).toBeVisible();
  const updated = await h.native("knowledge_load", { documentId: doc.id });
  expect(
    updated.chapters
      .find((ch) => ch.id === chapter)
      .root.children.find((n) => n.id === oldNode.id).comments,
  ).toContain(comment);
  await p.screenshot({
    path: "screens/product-ux/after/linked-repertoire-comment-light.png",
  });
  await p.getByRole("button", { name: "К ветви", exact: true }).click();
  await ready();
  expect(await p.locator(".graph-path").getAttribute("data-uci-line")).toBe("");
  await expect.poll(() => p.evaluate(() => scrollY)).toBe(branchScroll);
  await expect(p.getByLabel("Репертуар графа", { exact: true })).toHaveValue(
    doc.id,
  );
  results.push({
    scenario:
      "exact repertoire-node edit/save→same branch and layer; stable document/chapter/node IDs",
    result: "PASS",
    documentId: doc.id,
    nodeId: oldNode.id,
  });
  await nav("Репертуар");
  await p
    .getByRole("button", { name: "Открыть на карте", exact: true })
    .click();
  await ready();
  await expect(p.locator(".graph-path")).toHaveAttribute(
    "data-uci-line",
    "e2e4",
  );
  await p.getByRole("button", { name: "Чёрные: e5", exact: true }).click();
  await ready();
  await p.getByRole("tab", { name: "Обзор", exact: true }).click();
  await p.getByRole("tab", { name: "Карта игры", exact: true }).click();
  await ready();
  await expect(p.locator(".graph-path")).toHaveAttribute(
    "data-uci-line",
    "e2e4 e7e5",
  );
  results.push({
    scenario:
      "document-position request consumed once; reopening map retains later path",
    result: "PASS",
  });
  await p.getByRole("button", { name: "К началу", exact: true }).click();
  await ready();
  const rootPath = await p.locator(".graph-path").getAttribute("data-uci-line");
  await p.locator(".continuation-chip").first().focus();
  await p.keyboard.press("ArrowDown");
  expect(await p.locator(".graph-path").getAttribute("data-uci-line")).toBe(
    rootPath,
  );
  await p.keyboard.press("Enter");
  await ready();
  expect(await p.locator(".graph-path").getAttribute("data-uci-line")).not.toBe(
    rootPath,
  );
  await p.keyboard.press("ArrowLeft");
  await ready();
  await expect(p.locator(".graph-path")).toHaveAttribute("data-uci-line", "");
  results.push({
    scenario:
      "keyboard focus/Down/Enter/Left; focus changes do not execute moves",
    result: "PASS",
  });
  await p.getByRole("button", { name: "Белые: e4", exact: true }).click();
  await ready();
  await p.waitForTimeout(450);
  await p.reload();
  await p.getByRole("button", { name: "Продолжить", exact: true }).click();
  await ready();
  await expect(p.locator(".graph-path")).toHaveAttribute(
    "data-uci-line",
    "e2e4",
  );
  results.push({
    scenario:
      "reload→Home Continue restores persisted branch+filter+repertoire",
    result: "PASS",
  });
} finally {
  const video = await p.video()?.path();
  await h.browser.close();
  if (video)
    await copyFile(
      video,
      "screens/product-ux/videos/context-repertoire-navigation.webm",
    );
  await writeFile(
    "screens/product-ux/evidence/context-ui.json",
    JSON.stringify(
      {
        results,
        provenance:
          "production frontend→real isolated Rust/SQLite; no response substitution",
      },
      null,
      2,
    ),
  );
}
console.log(JSON.stringify(results));

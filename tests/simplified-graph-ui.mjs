import { expect } from "@playwright/test";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { join } from "node:path";
import { bridge } from "./helpers/product-bridge.mjs";
const config = JSON.parse(
  await readFile(
    process.env.CHESSPUSH_GRAPH_CONFIG ?? ".qa/ui-atlas-tools/current.json",
    "utf8",
  ),
);
if (!config.data?.includes(".qa"))
  throw Error("An isolated QA profile is required");
const out = join(config.data, "simple-graph-ui");
await mkdir(out, { recursive: true });
const results = [],
  errors = [];
for (const theme of ["light", "dark"]) {
  const h = await bridge(config, theme),
    p = h.page;
  p.setDefaultTimeout(15000);
  p.on("pageerror", (e) => errors.push(e.message));
  const graph = p.getByRole("region", { name: "Граф партий", exact: true }),
    trail = graph.getByLabel("История ветки", { exact: true });
  const line = () => trail.getAttribute("data-uci-line");
  async function ready(expected) {
    await expect(graph).toHaveAttribute("aria-busy", "false");
    await expect
      .poll(async () => graph.getAttribute("data-projection-line"))
      .toBe(expected ?? (await line()));
  }
  const root = async () => {
    if (
      await graph
        .getByRole("button", { name: "К началу", exact: true })
        .isEnabled()
    )
      await graph
        .getByRole("button", { name: "К началу", exact: true })
        .click();
    await ready("");
  };
  try {
    await p
      .locator(".app-header")
      .getByRole("button", { name: "Статистика", exact: true })
      .click();
    await p.getByRole("tab", { name: "Граф партий", exact: true }).click();
    await ready();
    await p.getByRole("button", { name: "Белыми", exact: true }).click();
    await ready();
    await root();
    await expect(graph.locator(".graph-workspace > div")).toHaveCount(2);
    await expect(
      graph.getByRole("button", { name: "Общая карта", exact: true }),
    ).toHaveCount(0);
    const countBefore = await graph
      .locator(".branch-current .branch-counts")
      .innerText();
    const first = graph.locator(
      '.branch-node[data-level="1"][data-uci="e2e4"]',
    );
    await expect(first.locator("piece.white.pawn")).toHaveCount(1);
    await first.click();
    await ready("e2e4");
    await expect(graph.locator("cg-board piece.black.pawn")).toHaveCount(8);
    await expect(trail.locator(".history-node")).toHaveCount(2);
    await expect(
      graph.locator('.branch-node[data-level="1"][data-uci="e7e5"]'),
    ).toBeVisible();
    await graph
      .getByRole("button", { name: "Назад по ветви", exact: true })
      .click();
    await ready("");
    const reply = graph.locator(
      '.branch-node[data-level="2"][data-parent-uci="e2e4"][data-uci="e7e5"]',
    );
    await reply.click();
    await ready("e2e4 e7e5");
    await expect(trail.locator(".history-node")).toHaveCount(3);
    await trail.locator('[data-ply="1"]').click();
    await ready("e2e4");
    await graph.press("ArrowRight");
    await ready("e2e4 e7e5");
    await graph.press("ArrowLeft");
    await ready("e2e4");
    await root();
    const board = graph.locator(".graph-board .board-frame > .cg-wrap");
    const r = await board.boundingBox();
    await p.mouse.click(r.x + (r.width * 4.5) / 8, r.y + (r.height * 6.5) / 8);
    await p.mouse.click(r.x + (r.width * 4.5) / 8, r.y + (r.height * 4.5) / 8);
    await ready("e2e4");
    const beforeFlip = await graph.locator(".branch-current").innerText();
    await graph
      .getByRole("button", { name: "Перевернуть доску", exact: true })
      .click();
    expect(await graph.locator(".branch-current").innerText()).toBe(beforeFlip);
    await expect(board).toHaveClass(/orientation-black/);
    await graph
      .getByRole("button", { name: "Перевернуть доску", exact: true })
      .click();
    await root();
    expect(
      await graph.locator(".branch-current .branch-counts").innerText(),
    ).toBe(countBefore);
    results.push({
      theme,
      scenario:
        "two regions; SVG pieces; first/reply vertices; exact trail; keyboard replay; board e4; back; flip preserves statistics",
      result: "PASS",
    });
    if (theme === "light") {
      const path = ["e2e4", "e7e5", "g1f3", "d7d6"];
      for (let i = 0; i < path.length; i++) {
        await graph
          .locator('.branch-node[data-level="1"][data-uci="' + path[i] + '"]')
          .click();
        await ready(path.slice(0, i + 1).join(" "));
      }
      await expect(trail.locator(".history-node")).toHaveCount(5);
      await p.screenshot({
        path: join(out, "graph-history-four-plies-light.png"),
        fullPage: true,
      });
      await p.waitForTimeout(500);
      await p.reload();
      await p
        .locator(".app-header")
        .getByRole("button", { name: "Статистика", exact: true })
        .click();
      await p.getByRole("tab", { name: "Граф партий", exact: true }).click();
      await ready(path.join(" "));
      await expect(trail.locator(".history-node")).toHaveCount(5);
      results.push({
        scenario:
          "four real plies remain explicit history vertices and restore after frontend restart from persisted native graph state",
        result: "PASS",
      });
      await root();
    }
    if (theme === "light") {
      const docs = await h.native("knowledge_list"),
        original = await h.native("game_graph_query", {
          request: {
            filter: {
              account_id: "lichess:statistics_fixture",
              color: "white",
              variant: "standard",
            },
            line: [],
            all_paths: false,
          },
        });
      for (const [name, pgn] of [
        ["graph-fragment-a.pgn", "1. e4 c5 2. Nf3 *"],
        ["graph-fragment-b.pgn", "1. d4 d5 2. c4 *"],
      ]) {
        const file = join(out, name);
        await writeFile(file, pgn);
        if (!docs.some((d) => d.filename === name))
          await h.native("knowledge_attach_file", {
            path: file,
            side: "white",
          });
      }
      await expect(first.locator(".branch-prepared-mark")).toBeVisible();
      await expect(
        graph.locator(
          '.branch-node[data-level="1"][data-uci="d2d4"] .branch-prepared-mark',
        ),
      ).toBeVisible();
      const currentDocs = await h.native("knowledge_list"),
        attached = currentDocs
          .filter(
            (d) =>
              d.attachment &&
              d.attachment_active &&
              (d.side === "white" || d.side === "both"),
          )
          .map((d) => d.id);
      const layered = await h.native("game_graph_query", {
        request: {
          filter: {
            account_id: "lichess:statistics_fixture",
            color: "white",
            variant: "standard",
          },
          line: [],
          all_paths: false,
          repertoire_ids: attached,
        },
      });
      expect(layered.selected.games).toBe(original.selected.games);
      expect(layered.total_visits).toBe(original.total_visits);
      await graph
        .locator('.branch-node[data-level="1"][data-uci="d2d4"]')
        .click();
      await ready("d2d4");
      await expect(
        graph.locator(
          '.branch-node[data-level="1"][data-uci="d7d5"] .branch-prepared-mark',
        ),
      ).toBeVisible();
      await root();
      results.push({
        scenario:
          "attached-fragment union automatically marks both branches, prepared branch navigates, factual N/visits unchanged",
        result: "PASS",
      });
    }
    for (const [width, height] of [
      [1440, 960],
      [900, 700],
    ]) {
      await p.setViewportSize({ width, height });
      await p.waitForTimeout(250);
      const bounds = await graph.locator(".graph-workspace").evaluate((e) => {
        const [b, g] = e.children;
        return {
          board: b.getBoundingClientRect().toJSON(),
          graph: g.getBoundingClientRect().toJSON(),
          page: document.documentElement.scrollWidth,
          viewport: innerWidth,
          cols: getComputedStyle(e).gridTemplateColumns,
        };
      });
      expect(bounds.board.width).toBeGreaterThan(270);
      expect(bounds.graph.x).toBeGreaterThan(
        bounds.board.x + bounds.board.width,
      );
      expect(bounds.page).toBeLessThanOrEqual(bounds.viewport);
      const innerBounds = await graph
        .locator(".graph-scroll")
        .evaluate((e) => ({
          client: e.clientWidth,
          content: e.scrollWidth,
          right: e.getBoundingClientRect().right,
          nodeRights: [
            ...e.querySelectorAll('.branch-node[data-level="2"]'),
          ].map((n) => n.getBoundingClientRect().right),
        }));
      expect(innerBounds.content).toBeLessThanOrEqual(innerBounds.client + 1);
      for (const right of innerBounds.nodeRights)
        expect(right).toBeLessThanOrEqual(innerBounds.right);
      await p.screenshot({
        path: join(out, `graph-${theme}-${width}.png`),
        fullPage: true,
      });
      results.push({
        theme,
        viewport: { width, height },
        result: "PASS",
        bounds,
      });
    }
  } finally {
    await h.browser.close();
  }
}
for (const theme of ["light", "dark"]) {
  const h = await bridge(config, theme, { dpr: 1.25 }),
    p = h.page;
  try {
    await p.setViewportSize({ width: 1421, height: 954 });
    await p
      .locator(".app-header")
      .getByRole("button", { name: "Статистика", exact: true })
      .click();
    await p.getByRole("tab", { name: "Граф партий", exact: true }).click();
    const graph = p.getByRole("region", { name: "Граф партий", exact: true });
    await expect(graph).toHaveAttribute("aria-busy", "false");
    if (
      await graph
        .getByRole("button", { name: "К началу", exact: true })
        .isEnabled()
    )
      await graph
        .getByRole("button", { name: "К началу", exact: true })
        .click();
    await expect(graph).toHaveAttribute("aria-busy", "false");
    await expect
      .poll(() => graph.getAttribute("data-projection-line"))
      .toBe("");
    const size = await graph.locator(".graph-workspace").evaluate((e) => ({
      css: e.getBoundingClientRect().toJSON(),
      pageWidth: document.documentElement.scrollWidth,
      innerWidth,
      dpr: devicePixelRatio,
    }));
    expect(size.pageWidth).toBeLessThanOrEqual(size.innerWidth);
    await p.screenshot({
      path: join(out, "graph-" + theme + "-1776px-dpr125.png"),
      fullPage: true,
    });
    results.push({
      scenario: "125% DPI emulation",
      theme,
      cssViewport: { width: 1421, height: 954 },
      expectedPhysical: { width: 1776, height: 1193 },
      result: "PASS",
      size,
    });
  } finally {
    await h.browser.close();
  }
}
expect(errors).toEqual([]);
await writeFile(
  join(out, "results.json"),
  JSON.stringify({ results, errors }, null, 2),
);
console.log(JSON.stringify({ out, results, errors }, null, 2));

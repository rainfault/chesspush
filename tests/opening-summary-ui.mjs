// Opening statistics UI -> production renderer -> isolated native Rust/SQLite.
import { expect } from "@playwright/test";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
import { bridge } from "./helpers/product-bridge.mjs";
const config = JSON.parse(
  await readFile(".qa/ui-atlas-tools/current.json", "utf8"),
);
if (!resolve(config.data).startsWith(resolve(".qa") + "\\"))
  throw Error("Only isolated QA profiles");
const out = resolve("screens/openings-coverage");
await mkdir(out, { recursive: true });
const results = [],
  errors = [];
for (const theme of ["light", "dark"]) {
  const h = await bridge(config, theme),
    p = h.page;
  p.setDefaultTimeout(15000);
  p.on("pageerror", (e) => errors.push(e.message));
  let attached = [];
  try {
    await p.evaluate(() => {
      window.__openingCommands = [];
      const orig = window.__TAURI_INTERNALS__.invoke;
      window.__TAURI_INTERNALS__.invoke = (command, args) => {
        window.__openingCommands.push(command);
        return orig(command, args);
      };
    });
    const nav = (name) =>
      p
        .locator(".app-header")
        .getByRole("button", { name, exact: true })
        .click();
    await nav("Статистика");
    const stats = p.locator(".simple-statistics"),
      preview = stats.getByRole("complementary", { name: "Позиция дебюта" });
    await expect(stats.getByRole("tab")).toHaveCount(2);
    await expect(
      preview.getByRole("heading", { name: "Начальная позиция" }),
    ).toBeVisible();
    const summaries = {};
    for (const color of ["white", "black"]) {
      const name = color === "white" ? "белыми" : "чёрными";
      const summary = await h.native("statistics_openings", {
        accountId: null,
        color,
      });
      summaries[color] = summary;
      const table = stats.getByRole("table", {
        name: "Статистика дебютов " + name,
        exact: true,
      });
      await expect(table.locator("tbody tr")).toHaveCount(summary.rows.length);
      await expect(table.locator("tfoot td").nth(1)).toHaveText(
        String(summary.total_games),
      );
      for (const row of summary.rows) {
        const tr = table
          .getByRole("button", {
            name: row.name ?? "Дебют не определён",
            exact: true,
          })
          .locator("../..");
        await expect(tr).toHaveAttribute("data-color", color);
        for (const [index, key] of [
          [2, "games"],
          [3, "wins"],
          [4, "draws"],
          [5, "losses"],
        ])
          await expect(tr.locator("td").nth(index)).toHaveText(
            String(row[key]),
          );
        await tr.hover();
        await expect(preview).toHaveAttribute("data-color", color);
        await expect(preview.locator("h2")).toHaveText(
          row.name ?? "Дебют не определён",
        );
        if (row.preview_fen) {
          await expect
            .poll(
              async () =>
                (await preview.getAttribute("data-fen")).split(" ")[0],
            )
            .toBe(row.preview_fen.split(" ")[0]);
          await expect(preview.locator(".cg-wrap")).toHaveClass(
            new RegExp("orientation-" + color),
          );
        }
        await tr.getByRole("button").focus();
        await expect(preview.locator("h2")).toHaveText(
          row.name ?? "Дебют не определён",
        );
      }
      const header = table.locator("thead th").nth(2);
      await header.getByRole("button").click();
      await expect(header).toHaveAttribute("aria-sort", "ascending");
      const n = (
        await table.locator("tbody td:nth-child(3)").allTextContents()
      ).map(Number);
      expect(n).toEqual([...n].sort((a, b) => a - b));
      await header.getByRole("button").click();
      await expect(header).toHaveAttribute("aria-sort", "descending");
    }
    const shared = summaries.white.rows.find((w) =>
      summaries.black.rows.some((b) => w.eco === b.eco && w.name === b.name),
    );
    expect(shared).toBeTruthy();
    expect(
      summaries.white.rows.find((x) => x.name === shared.name).game_ids,
    ).not.toEqual(
      summaries.black.rows.find((x) => x.name === shared.name).game_ids,
    );
    results.push({
      theme,
      scenario:
        "Two simultaneous side-specific tables; same A05 has separate membership; native totals/WDL; sorting; hover and keyboard canonical preview",
      result: "PASS",
    });
    const beforeHover = await p.evaluate(
      () =>
        window.__openingCommands.filter((x) => x === "statistics_openings")
          .length,
    );
    await stats.getByRole("table").first().locator("tbody tr").first().hover();
    await stats.getByRole("table").last().locator("tbody tr").first().hover();
    expect(
      await p.evaluate(
        () =>
          window.__openingCommands.filter((x) => x === "statistics_openings")
            .length,
      ),
    ).toBe(beforeHover);
    expect(
      await p.evaluate(() =>
        window.__openingCommands.includes("statistics_report"),
      ),
    ).toBe(false);
    for (const size of [
      { width: 1440, height: 960 },
      { width: 900, height: 700 },
    ]) {
      await p.setViewportSize(size);
      await stats
        .getByRole("table")
        .first()
        .locator("tbody tr")
        .first()
        .hover();
      const geometry = await preview.evaluate((e) => ({
        x: e.getBoundingClientRect().x,
        right: e.getBoundingClientRect().right,
        board: e.querySelector(".board-frame").getBoundingClientRect().toJSON(),
        page: document.documentElement.scrollWidth,
        viewport: innerWidth,
      }));
      const tables = await stats.locator(".opening-tables").boundingBox();
      expect(geometry.x).toBeGreaterThan(tables.x + tables.width);
      expect(geometry.right).toBeLessThanOrEqual(size.width);
      expect(geometry.page).toBeLessThanOrEqual(size.width);
      expect(geometry.board.width).toBeGreaterThan(250);
      await p.waitForTimeout(300); // Chessground animation is 200 ms.
      await p.screenshot({
        path: join(out, theme + "-" + size.width + ".png"),
        fullPage: true,
      });
    }
    await p.setViewportSize({ width: 1440, height: 960 });
    results.push({
      theme,
      scenario:
        "Hover uses existing response; no full report; board remains on right at 1440 and 900, no page overflow",
      result: "PASS",
    });
    if (theme === "light") {
      const pgn = "1. Nf3 Nf6 2. g3 g6 3. Bg2 Bg7 *";
      const file = join(config.data, "qa-coverage-white.pgn");
      await writeFile(file, pgn);
      const doc = await h.native("knowledge_attach_file", {
        path: file,
        side: "white",
      });
      attached.push(doc.id);
      const findRow = async (side) =>
        (
          await h.native("statistics_openings", {
            accountId: null,
            color: side,
          })
        ).rows.find((r) => r.eco === shared.eco && r.name === shared.name);
      const white = await findRow("white"),
        black = await findRow("black");
      expect(white.coverage.percent).toBe(100);
      expect(black.coverage.percent).toBe(0);
      const whiteTable = stats.getByRole("table", {
        name: "Статистика дебютов белыми",
        exact: true,
      });
      const wr = whiteTable
        .getByRole("button", { name: shared.name, exact: true })
        .locator("../..");
      await expect(wr.locator(".opening-coverage > span").first()).toHaveText(
        "100%",
      );
      await h.native("statistics_preferences_save", {
        preferences: { opening_coverage_depth_moves: 1 },
      });
      const first = await findRow("white");
      expect(first.coverage.total_decisions).toBe(1);
      expect(first.coverage.depth_moves).toBe(1);
      await expect(wr.locator(".opening-coverage")).toHaveAttribute(
        "title",
        /до 1-го хода/,
      );
      await h.native("statistics_preferences_save", {
        preferences: { opening_coverage_depth_moves: 15 },
      });
      const detached = await h.native("knowledge_set_attachment", {
        documentId: doc.id,
        expectedRevision: doc.revision,
        active: false,
      });
      await expect(wr.locator(".opening-coverage > span").first()).toHaveText(
        "0%",
      );
      const active = await h.native("knowledge_set_attachment", {
        documentId: doc.id,
        expectedRevision: detached.revision,
        active: true,
      });
      await expect(wr.locator(".opening-coverage > span").first()).toHaveText(
        "100%",
      );
      await wr.hover();
      await p.waitForTimeout(300); // Chessground animation is 200 ms.
      await p.screenshot({
        path: join(out, "light-repertoire-100-percent.png"),
        fullPage: true,
      });
      await h.native("knowledge_delete_repertoire", {
        documentId: doc.id,
        expectedRevision: active.revision,
      });
      attached = [];
      await expect(wr.locator(".opening-coverage > span").first()).toHaveText(
        "0%",
      );
      expect(
        (
          await h.native("statistics_openings", {
            accountId: null,
            color: "white",
          })
        ).total_games,
      ).toBe(summaries.white.total_games);
      results.push({
        scenario:
          "Actual own-side PGN changes coverage 0→100; opposite side unchanged; depth native save updates denominator; detach/reattach/delete refreshes UI without changing WDL",
        result: "PASS",
      });
    }
    const row = summaries.black.rows[0];
    await stats
      .getByRole("table", { name: "Статистика дебютов чёрными", exact: true })
      .getByRole("button", { name: row.name, exact: true })
      .click();
    await expect(
      stats.getByRole("tab", { name: "Граф партий" }),
    ).toHaveAttribute("aria-selected", "true");
    const graph = stats.getByRole("region", {
      name: "Граф партий",
      exact: true,
    });
    await expect(graph).toHaveAttribute("aria-busy", "false");
    await expect(graph.locator(".branch-current .branch-counts")).toHaveText(
      "Партий: " + row.games,
    );
    await expect(
      stats.getByRole("button", { name: "Чёрными", exact: true }),
    ).toHaveAttribute("aria-pressed", "true");
    await expect(graph.locator(".graph-workspace > div")).toHaveCount(2);
    results.push({
      theme,
      scenario:
        "Click black row opens unchanged board+graph with exact black cohort",
      result: "PASS",
    });
  } finally {
    for (const id of attached) {
      const d = await h.native("knowledge_load", { documentId: id });
      await h.native("knowledge_delete_repertoire", {
        documentId: id,
        expectedRevision: d.revision,
      });
    }
    await h.browser.close();
  }
}
expect(errors).toEqual([]);
await writeFile(
  join(out, "results.json"),
  JSON.stringify(
    {
      results,
      errors,
      passed: true,
      backend: "native Rust/SQLite",
      fixtures: "isolated 4 observed games; same A05 different sides",
    },
    null,
    2,
  ),
);
console.log(JSON.stringify({ passed: results.length, errors }));

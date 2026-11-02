// Frontend-only layout/interaction check: real React App and Chessground with
// explicit synthetic IPC. No native profile, user account or database is opened.
import { chromium, expect as baseExpect } from "@playwright/test";
import { createServer } from "vite";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve, join } from "node:path";
const expect = baseExpect.configure({ timeout: 15_000 });
const output = resolve(".qa/statistics-layout");
await mkdir(output, { recursive: true });
const initialFen = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
const fens = {
  white: "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1",
  black: "rnbqkbnr/ppp1pppp/8/3p4/3P4/8/PPP1PPPP/RNBQKBNR w KQkq d6 0 2",
};
const accounts = [
  {
    id: "lichess:layoutfixture",
    external_id: "layoutfixture",
    username: "LayoutFixture",
    t0_ms: 1000,
    enabled: true,
    generation: 1,
  },
];
function fixture(color) {
  const rows = Array.from({ length: 127 }, (_, index) => {
    const games = 130 - index,
      wins = Math.floor((games * (index % 7)) / 7),
      draws = Math.floor((games - wins) / 3),
      losses = games - wins - draws;
    const coverage =
      index === 0 ? null : index === 1 ? 0 : index === 2 ? 100 : index % 101;
    return {
      color,
      eco:
        index === 0
          ? null
          : `${String.fromCharCode(65 + Math.floor(index / 100))}${String(index % 100).padStart(2, "0")}`,
      name:
        index === 0
          ? null
          : index === 1
            ? "Очень длинное название дебюта: основной вариант с подробным уточнением структуры пешек и необычным продолжением"
            : index === 126
              ? "Искомый уникальный дебют 126"
              : `Дебют ${String(index).padStart(3, "0")}`,
      games,
      wins,
      draws,
      losses,
      score: (100 * (wins + draws / 2)) / games,
      coverage: {
        covered_decisions: coverage ?? 0,
        total_decisions: coverage == null ? 0 : 100,
        percent: coverage,
        games,
        depth_moves: 15,
        version: "fixture",
        source: "synthetic",
        repertoire_documents: coverage == null ? 0 : 1,
      },
      game_ids: [`${color}-game-${index}`],
      preview_fen: fens[color],
      preview_last_move: color === "white" ? ["e2", "e4"] : ["d7", "d5"],
      preview_pgn: color === "white" ? "1. e4 *" : "1. d4 d5 *",
      preview_source: "synthetic",
    };
  });
  const wins = rows.reduce((n, row) => n + row.wins, 0),
    draws = rows.reduce((n, row) => n + row.draws, 0),
    losses = rows.reduce((n, row) => n + row.losses, 0);
  return {
    color,
    rows,
    accounts,
    selected_account_id: accounts[0].id,
    active_account_id: accounts[0].id,
    coverage_depth_moves: 15,
    coverage_version: "fixture",
    total_games: wins + draws + losses,
    wins,
    draws,
    losses,
    score: (100 * (wins + draws / 2)) / (wins + draws + losses),
  };
}
const summaries = { white: fixture("white"), black: fixture("black") };
const server = await createServer({
  server: { host: "127.0.0.1", port: 0, strictPort: false },
});
await server.listen();
const address = server.httpServer.address();
const browser = await chromium.launch({ channel: "msedge", headless: true });
const results = [],
  errors = [];
async function open(theme, mode = "dense") {
  const page = await browser.newPage({
    viewport: { width: 1440, height: 960 },
    reducedMotion: "reduce",
  });
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(
    ({ theme, summaries, initialFen, mode }) => {
      const callbacks = new Map(),
        listeners = new Map();
      let serial = 0;
      const calls = [],
        held = [];
      window.isTauri = true;
      window.__statisticsLayout = {
        calls,
        mode,
        release() {
          held.splice(0).forEach((resolve) => resolve());
        },
        dispatch(event = "statistics-updated") {
          for (const listener of listeners.values())
            if (listener.event === event)
              callbacks.get(listener.handler)?.({
                event,
                id: listener.id,
                payload: {},
              });
        },
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener(_event, id) {
          listeners.delete(id);
        },
      };
      window.__TAURI_INTERNALS__ = {
        transformCallback(callback) {
          const id = ++serial;
          callbacks.set(id, callback);
          return id;
        },
        async invoke(command, args = {}) {
          calls.push({ command, args });
          switch (command) {
            case "profile_restored_ui":
              return null;
            case "plugin:event|listen": {
              const id = ++serial;
              listeners.set(id, { ...args, id });
              return id;
            }
            case "plugin:event|unlisten":
              listeners.delete(args.eventId);
              return;
            case "get_theme":
              return theme;
            case "account_status":
              return {
                id: "layoutfixture",
                username: "LayoutFixture",
                status: "connected",
                rapid: 1800,
                blitz: 1750,
                hasToken: true,
                message: null,
              };
            case "knowledge_list":
            case "library_files":
            case "statistics_recent":
              return [];
            case "statistics_openings": {
              if (window.__statisticsLayout.mode === "loading")
                await new Promise((resolve) => held.push(resolve));
              if (window.__statisticsLayout.mode === "error")
                throw {
                  code: "fixture",
                  message: "Тестовая ошибка загрузки статистики",
                };
              const value = structuredClone(summaries[args.color]);
              if (
                ["empty", "disconnected"].includes(
                  window.__statisticsLayout.mode,
                )
              ) {
                Object.assign(value, {
                  rows: [],
                  total_games: 0,
                  wins: 0,
                  draws: 0,
                  losses: 0,
                  score: null,
                });
                if (window.__statisticsLayout.mode === "disconnected")
                  Object.assign(value, {
                    accounts: [],
                    active_account_id: null,
                    selected_account_id: null,
                  });
              }
              return value;
            }
            case "game_graph_load_state":
              return null;
            case "game_graph_save_state":
              return;
            case "game_graph_query": {
              const games = args.request.filter.game_ids?.length ?? 0;
              const selected = {
                id: "standard|fixture",
                fen: initialFen,
                games,
                visits: games,
                wins: 0,
                draws: 0,
                losses: games,
                score: 0,
                first_ms: null,
                last_ms: null,
                hidden_continuations: 0,
                example: null,
                knowledge: [],
                knowledge_count: 0,
              };
              return {
                root_key: selected.id,
                selected_key: selected.id,
                selected_fen: initialFen,
                initial_fen: initialFen,
                line: [],
                path_nodes: [],
                nodes: [selected],
                edges: [],
                continuations: [],
                previews: [],
                alternative_paths: 0,
                branch_games: games,
                all_paths: false,
                repertoire_ends_here: false,
                review_refs: [],
                review_analysed_games: 0,
                selected,
                hidden_nodes: 0,
                indexed_games: games,
                eligible_games: games,
                total_positions: 1,
                total_visits: games,
                index_revision: "fixture",
              };
            }
            default:
              throw Error(`Unexpected synthetic IPC: ${command}`);
          }
        },
      };
    },
    { theme, summaries, initialFen, mode },
  );
  await page.goto(`http://127.0.0.1:${address.port}`);
  await page
    .getByRole("navigation", { name: "Разделы" })
    .getByRole("button", { name: "Статистика", exact: true })
    .click();
  return page;
}
const section = (page, side) =>
  page.getByRole("region", { name: `Дебюты ${side}`, exact: true });
const table = (page, side) =>
  page.getByRole("table", { name: `Статистика дебютов ${side}`, exact: true });
const next = (page, side) =>
  page.getByRole("button", {
    name: `Следующая страница: ${side}`,
    exact: true,
  });
const previous = (page, side) =>
  page.getByRole("button", {
    name: `Предыдущая страница: ${side}`,
    exact: true,
  });
async function geometry(page) {
  return page.locator(".opening-explorer").evaluate((explorer) => {
    const rect = (element) => element.getBoundingClientRect().toJSON();
    const scrollables = [...explorer.querySelectorAll(".stats-table-wrap")].map(
      (element) => ({
        clientHeight: element.clientHeight,
        scrollHeight: element.scrollHeight,
        clientWidth: element.clientWidth,
        scrollWidth: element.scrollWidth,
      }),
    );
    return {
      width: innerWidth,
      height: innerHeight,
      pageWidth: document.documentElement.scrollWidth,
      pageHeight: document.documentElement.scrollHeight,
      explorer: rect(explorer),
      board: rect(explorer.querySelector(".board-frame")),
      tables: [...explorer.querySelectorAll(".opening-side-table")].map(rect),
      scrollables,
      firstRowCells: [...explorer.querySelector("tbody tr").children].map(
        (element) => ({
          width: element.getBoundingClientRect().width,
          clientWidth: element.clientWidth,
          scrollWidth: element.scrollWidth,
          buttonWidth:
            element.querySelector("button")?.getBoundingClientRect().width ??
            null,
        }),
      ),
      footerCells: [...explorer.querySelectorAll("tfoot td")].map(
        (element) => ({
          text: element.textContent,
          clientWidth: element.clientWidth,
          scrollWidth: element.scrollWidth,
        }),
      ),
    };
  });
}
try {
  if (process.env.CHESSPUSH_STATISTICS_BASELINE === "1") {
    const page = await open("light");
    await expect(table(page, "белыми").locator("tbody tr")).toHaveCount(127);
    for (const size of [
      { width: 1440, height: 960 },
      { width: 900, height: 700 },
    ]) {
      await page.setViewportSize(size);
      await page.screenshot({
        path: join(output, `baseline-light-${size.width}.png`),
        fullPage: false,
      });
      results.push({ baseline: true, geometry: await geometry(page) });
    }
    await page.close();
  } else {
    for (const theme of ["light", "dark"]) {
      const page = await open(theme);
      const white = table(page, "белыми"),
        black = table(page, "чёрными");
      await expect(white.locator("tbody tr").first()).toBeVisible();
      await expect(black.locator("tbody tr").first()).toBeVisible();
      for (const size of [
        { width: 1440, height: 960 },
        { width: 1366, height: 768 },
        { width: 900, height: 700 },
      ]) {
        await page.setViewportSize(size);
        await expect
          .poll(async () => (await geometry(page)).pageHeight)
          .toBeLessThanOrEqual(size.height + 1);
        const measured = await geometry(page);
        expect(measured.pageWidth).toBeLessThanOrEqual(size.width);
        expect(measured.board.width).toBeGreaterThanOrEqual(280);
        expect(measured.board.bottom).toBeLessThanOrEqual(size.height + 1);
        expect(measured.tables).toHaveLength(2);
        for (const bounds of measured.tables) {
          expect(bounds.y).toBeGreaterThanOrEqual(0);
          expect(bounds.bottom).toBeLessThanOrEqual(size.height + 1);
        }
        for (const scrollable of measured.scrollables) {
          expect(scrollable.scrollHeight).toBeLessThanOrEqual(
            scrollable.clientHeight + 1,
          );
          expect(scrollable.scrollWidth).toBeLessThanOrEqual(
            scrollable.clientWidth + 1,
          );
        }
        for (const cell of measured.footerCells)
          expect(cell.scrollWidth).toBeLessThanOrEqual(cell.clientWidth + 1);
        await white.locator("tbody tr").first().hover();
        await page.screenshot({
          path: join(output, `${theme}-${size.width}.png`),
          fullPage: true,
        });
        results.push({
          theme,
          scenario:
            "Dense127 rows per side fit viewport with both tables, totals, pagers and canonical board",
          geometry: measured,
        });
      }
      await page.setViewportSize({ width: 1440, height: 960 });
      // Let the ResizeObserver-driven page capacity settle before traversing it.
      await page.evaluate(
        () =>
          new Promise((resolve) =>
            requestAnimationFrame(() => requestAnimationFrame(resolve)),
          ),
      );
      const blackFirst = await black.locator("tbody tr").first().innerText();
      const seen = new Set();
      for (let pageNumber = 0; pageNumber < 128; pageNumber++) {
        for (const name of await white
          .locator("tbody .stats-link")
          .allTextContents())
          seen.add(name.trim());
        if (await next(page, "белыми").isDisabled()) break;
        await next(page, "белыми").click();
      }
      expect(seen.size).toBe(127);
      await expect(next(page, "белыми")).toBeDisabled();
      await expect(previous(page, "чёрными")).toBeDisabled();
      expect(await black.locator("tbody tr").first().innerText()).toBe(
        blackFirst,
      );
      await white.locator("thead th").nth(2).getByRole("button").click();
      await expect(previous(page, "белыми")).toBeDisabled();
      await expect(white.locator("thead th").nth(2)).toHaveAttribute(
        "aria-sort",
        "ascending",
      );
      let values = (
        await white.locator("tbody td:nth-child(3)").allTextContents()
      ).map(Number);
      expect(values).toEqual([...values].sort((a, b) => a - b));
      for (const column of [0, 1, 3, 4, 5, 6, 7]) {
        if (await next(page, "белыми").isEnabled())
          await next(page, "белыми").click();
        const header = white.locator("thead th").nth(column);
        await header.getByRole("button").click();
        await expect(previous(page, "белыми")).toBeDisabled();
        await expect(header).toHaveAttribute(
          "aria-sort",
          [0, 1].includes(column) ? "ascending" : "descending",
        );
      }
      const search = page.getByRole("searchbox", {
        name: "Поиск дебютов",
        exact: true,
      });
      await next(page, "чёрными").click();
      await expect(previous(page, "чёрными")).toBeEnabled();
      await search.fill("ИСКОМЫЙ УНИКАЛЬНЫЙ");
      await expect(black.locator("tbody tr")).toHaveCount(1);
      await search.fill("");
      await expect(previous(page, "чёрными")).toBeDisabled();
      await search.fill("ИСКОМЫЙ УНИКАЛЬНЫЙ");
      await expect(white.locator("tbody tr")).toHaveCount(1);
      await expect(black.locator("tbody tr")).toHaveCount(1);
      await expect(white.locator("tfoot td").nth(1)).toHaveText(
        String(summaries.white.total_games),
      );
      await expect(black.locator("tfoot td").nth(1)).toHaveText(
        String(summaries.black.total_games),
      );
      const before = await page.evaluate(
        () =>
          window.__statisticsLayout.calls.filter(
            (call) => call.command === "statistics_openings",
          ).length,
      );
      const preview = page.getByRole("complementary", {
        name: "Позиция дебюта",
        exact: true,
      });
      await white.locator("tbody tr").first().hover();
      await expect(preview).toHaveAttribute("data-color", "white");
      await expect
        .poll(
          async () => (await preview.getAttribute("data-fen")).split(" ")[0],
        )
        .toBe(fens.white.split(" ")[0]);
      await black.locator("tbody .stats-link").first().focus();
      await expect(preview).toHaveAttribute("data-color", "black");
      await expect
        .poll(
          async () => (await preview.getAttribute("data-fen")).split(" ")[0],
        )
        .toBe(fens.black.split(" ")[0]);
      expect(
        await page.evaluate(
          () =>
            window.__statisticsLayout.calls.filter(
              (call) => call.command === "statistics_openings",
            ).length,
        ),
      ).toBe(before);
      await search.fill("Не существующий дебют");
      await expect(white.locator("tbody .stats-link")).toHaveCount(0);
      await expect(black.locator("tbody .stats-link")).toHaveCount(0);
      await expect(previous(page, "белыми")).toBeDisabled();
      await expect(next(page, "белыми")).toBeDisabled();
      await search.fill("Очень длинное");
      await expect(white.locator("tbody .stats-link").first()).toHaveAttribute(
        "title",
        summaries.white.rows[1].name,
      );
      await expect(
        white.locator("tbody .opening-coverage > span").first(),
      ).toHaveText("0%");
      await search.fill("");
      // ECO sorting places unknown values last, independently of direction.
      await white.locator("thead th").nth(0).getByRole("button").click();
      await expect(white.locator("tbody tr").first()).not.toHaveAttribute(
        "data-eco",
        "",
      );
      while (await next(page, "белыми").isEnabled())
        await next(page, "белыми").click();
      await expect(white.locator("tbody tr").last()).toHaveAttribute(
        "data-eco",
        "",
      );
      await expect(
        white
          .locator("tbody tr")
          .last()
          .locator(".opening-coverage > span")
          .first(),
      ).toHaveText("—");
      await search.fill("Искомый уникальный");
      await black.locator("tbody .stats-link").first().click();
      await expect(
        page.getByRole("tab", { name: "Граф партий", exact: true }),
      ).toHaveAttribute("aria-selected", "true");
      await expect
        .poll(() =>
          page.evaluate(
            () =>
              window.__statisticsLayout.calls
                .filter((call) => call.command === "game_graph_query")
                .at(-1)?.args.request.filter,
          ),
        )
        .toMatchObject({ color: "black", game_ids: ["black-game-126"] });
      results.push({
        theme,
        scenario:
          "All127rows reachable; independent pages, all8sorts, common search, fullnames,0/nullcoverage, untouched totals, hover/focus noIPC and graph original membership",
        pass: true,
      });
      await page.close();
    }
    for (const mode of ["loading", "error", "empty", "disconnected"]) {
      const page = await open("light", mode);
      await page.setViewportSize({ width: 900, height: 700 });
      let captured = false;
      if (mode === "loading") {
        await expect(page.locator(".simple-statistics")).toHaveAttribute(
          "aria-busy",
          "true",
        );
        await expect(
          page.getByText("Загрузка дебютов…", { exact: true }),
        ).toBeVisible();
        await page.screenshot({
          path: join(output, `state-${mode}.png`),
          fullPage: true,
        });
        captured = true;
        await page.evaluate(() => {
          window.__statisticsLayout.mode = "dense";
          window.__statisticsLayout.release();
        });
        await expect(
          table(page, "белыми").locator("tbody tr").first(),
        ).toBeVisible();
      } else if (mode === "error") {
        await expect(page.getByRole("alert")).toContainText(
          "Тестовая ошибка загрузки статистики",
        );
        await page.screenshot({
          path: join(output, `state-${mode}.png`),
          fullPage: true,
        });
        captured = true;
        await page.evaluate(() => {
          window.__statisticsLayout.mode = "dense";
        });
        await page
          .getByRole("button", { name: "Повторить", exact: true })
          .click();
        await expect(
          table(page, "чёрными").locator("tbody tr").first(),
        ).toBeVisible();
      } else if (mode === "empty") {
        await expect(
          page.getByText("Нет завершённых партий белыми.", { exact: true }),
        ).toBeVisible();
        await expect(
          page.getByText("Нет завершённых партий чёрными.", { exact: true }),
        ).toBeVisible();
      } else
        await expect(
          page.getByRole("button", { name: "Подключить Lichess", exact: true }),
        ).toBeVisible();
      await expect
        .poll(() => page.evaluate(() => document.documentElement.scrollHeight))
        .toBeLessThanOrEqual(701);
      if (!captured)
        await page.screenshot({
          path: join(output, `state-${mode}.png`),
          fullPage: true,
        });
      results.push({ scenario: mode, pass: true });
      await page.close();
    }
    expect(errors).toEqual([]);
  }
  await writeFile(
    join(
      output,
      process.env.CHESSPUSH_STATISTICS_BASELINE === "1"
        ? "baseline.json"
        : "results.json",
    ),
    JSON.stringify(
      {
        method:
          "Frontend realReactApp/Chessground; explicit synthetic IPC only; no native profile",
        results,
        errors,
      },
      null,
      2,
    ),
  );
  console.log(`PASS statistics layout frontend; artifacts: ${output}`);
} catch (error) {
  for (const page of browser.contexts().flatMap((context) => context.pages())) {
    await page
      .screenshot({ path: join(output, "failure.png"), fullPage: true })
      .catch(() => {});
    await writeFile(
      join(output, "failure.txt"),
      await page
        .locator("body")
        .innerText()
        .catch(() => ""),
    );
  }
  throw error;
} finally {
  await browser.close();
  await server.close();
}

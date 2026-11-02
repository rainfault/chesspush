// Current Statistics UI against actual Rust/SQLite in a disposable profile.
// No IPC responses, chess positions or summary values are mocked.
import { chromium, expect as baseExpect } from "@playwright/test";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdir, writeFile } from "node:fs/promises";
import { resolve, join, sep } from "node:path";

const expect = baseExpect.configure({ timeout: 15_000 });
const profile = process.env.CHESSPUSH_TEST_DATA;
const endpoint = process.env.CHESSPUSH_TEST_CDP;
if (!profile || !endpoint || !resolve(profile).startsWith(resolve(".qa") + sep))
  throw new Error("Use the isolated native runner with a profile inside .qa");
const artifacts = resolve(process.env.CHESSPUSH_TRAINING_ARTIFACTS);
await mkdir(artifacts, { recursive: true });
const seeded = await promisify(execFile)(
  "cargo",
  [
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
const browser = await chromium.connectOverCDP(endpoint);
const page = browser.contexts()[0].pages()[0];
page.setDefaultTimeout(20_000);
await page.setViewportSize({ width: 1440, height: 960 });
const errors = [],
  checks = [];
page.on("pageerror", (error) => errors.push(error.message));
await page.waitForFunction(
  () =>
    window.__TAURI_INTERNALS__?.invoke && document.querySelector(".app-header"),
);
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
const summary = (color) =>
  native("statistics_openings", { accountId: null, color });
const sideLabel = (color) => (color === "white" ? "белыми" : "чёрными");
const table = (color) =>
  page.getByRole("table", {
    name: `Статистика дебютов ${sideLabel(color)}`,
    exact: true,
  });
const rowLocator = (row) =>
  table(row.color)
    .locator("tbody tr")
    .filter({
      has: page.getByRole("button", {
        name: row.name ?? "Дебют не определён",
        exact: true,
      }),
    });
const percent = (value) =>
  value == null
    ? "—"
    : `${value.toLocaleString("ru-RU", { maximumFractionDigits: 1 })}%`;
let attached;
try {
  const summaries = {
    white: await summary("white"),
    black: await summary("black"),
  };
  expect(summaries.white.total_games + summaries.black.total_games).toBe(4);
  await page.getByRole("button", { name: "Статистика", exact: true }).click();
  const preview = page.getByRole("complementary", {
    name: "Позиция дебюта",
    exact: true,
  });
  for (const color of ["white", "black"]) {
    const value = summaries[color],
      current = table(color);
    await expect(current).toBeVisible();
    const footer = current.locator("tfoot tr");
    await expect(footer.locator("td").nth(1)).toHaveText(
      String(value.total_games),
    );
    await expect(footer.locator("td").nth(2)).toHaveText(String(value.wins));
    await expect(footer.locator("td").nth(3)).toHaveText(String(value.draws));
    await expect(footer.locator("td").nth(4)).toHaveText(String(value.losses));
    await expect(footer.locator("td").nth(5)).toHaveText(percent(value.score));
    for (const row of value.rows) {
      const tr = rowLocator(row);
      await expect(tr).toHaveAttribute("data-color", color);
      await expect(tr.locator("td").nth(2)).toHaveText(String(row.games));
      await expect(tr.locator("td").nth(3)).toHaveText(String(row.wins));
      await expect(tr.locator("td").nth(4)).toHaveText(String(row.draws));
      await expect(tr.locator("td").nth(5)).toHaveText(String(row.losses));
      await expect(tr.locator("td").nth(6)).toContainText(percent(row.score));
      await expect(tr.locator(".opening-coverage > span").first()).toHaveText(
        percent(row.coverage.percent),
      );
      await tr.hover();
      await expect(preview).toHaveAttribute("data-color", color);
      await expect(preview.getByRole("heading", { level: 2 })).toHaveText(
        row.name ?? "Дебют не определён",
      );
      if (row.preview_fen) {
        await expect
          .poll(
            async () => (await preview.getAttribute("data-fen")).split(" ")[0],
          )
          .toBe(row.preview_fen.split(" ")[0]);
        await expect(preview.locator(".cg-wrap")).toHaveClass(
          new RegExp(`orientation-${color}`),
        );
      }
      await tr.getByRole("button").focus();
      await expect(preview.getByRole("heading", { level: 2 })).toHaveText(
        row.name ?? "Дебют не определён",
      );
    }
    const gamesHeader = current.locator("thead th").nth(2);
    await gamesHeader.getByRole("button").click();
    await expect(gamesHeader).toHaveAttribute("aria-sort", "ascending");
    const games = (
      await current.locator("tbody td:nth-child(3)").allTextContents()
    ).map(Number);
    expect(games).toEqual([...games].sort((a, b) => a - b));
    await gamesHeader.getByRole("button").click();
    await expect(gamesHeader).toHaveAttribute("aria-sort", "descending");
  }
  checks.push(
    "Two side-specific native summaries, exact WDL/totals/score/coverage, sorting, hover and keyboard canonical preview",
  );
  const target = summaries.white.rows.find((row) =>
    row.game_ids.includes("lichess:stTr0001"),
  );
  expect(target).toBeTruthy();
  expect(target.coverage.percent).toBe(0);
  const file = join(profile, "layout-coverage.pgn");
  await writeFile(file, "1. Nf3 Nf6 2. g3 g6 3. Bg2 Bg7 *");
  attached = await native("knowledge_attach_file", {
    path: file,
    side: "white",
  });
  const covered = (await summary("white")).rows.find(
    (row) => row.eco === target.eco && row.name === target.name,
  );
  expect(covered.coverage.percent).toBe(100);
  await expect(
    rowLocator(target).locator(".opening-coverage > span").first(),
  ).toHaveText("100%");
  expect((await summary("black")).total_games).toBe(
    summaries.black.total_games,
  );
  for (const row of (await summary("black")).rows)
    expect(row.coverage.percent).toBe(0);
  await native("knowledge_delete_repertoire", {
    documentId: attached.id,
    expectedRevision: attached.revision,
  });
  attached = undefined;
  await expect(
    rowLocator(target).locator(".opening-coverage > span").first(),
  ).toHaveText("0%");
  expect((await summary("white")).total_games).toBe(
    summaries.white.total_games,
  );
  checks.push(
    "Native repertoire coverage changes 0→100→0, independent opposite side, unchanged factual counts",
  );
  await page.screenshot({
    path: join(artifacts, "statistics-layout-native-light.png"),
    fullPage: true,
  });
  for (const selected of [summaries.white.rows[0], summaries.black.rows[0]]) {
    await rowLocator(selected).getByRole("button").click();
    await expect(
      page.getByRole("tab", { name: "Граф партий", exact: true }),
    ).toHaveAttribute("aria-selected", "true");
    const graph = page.getByRole("region", {
      name: "Граф партий",
      exact: true,
    });
    await expect(graph).toHaveAttribute("aria-busy", "false");
    await expect(graph.locator(".branch-current .branch-counts")).toHaveText(
      `Партий: ${selected.games}`,
    );
    await expect(
      page.locator(".simple-statistics-side").getByRole("button", {
        name: selected.color === "white" ? "Белыми" : "Чёрными",
        exact: true,
      }),
    ).toHaveAttribute("aria-pressed", "true");
    await page.getByRole("tab", { name: "Дебюты", exact: true }).click();
  }
  checks.push(
    "Opening actions preserve independent white/black graph membership and exact factual game counts",
  );
  expect(errors).toEqual([]);
  await writeFile(
    join(artifacts, "statistics-layout-native-results.json"),
    JSON.stringify({ pass: true, checks, errors }, null, 2),
  );
  console.log(`PASS current Statistics native UI; artifacts: ${artifacts}`);
} catch (error) {
  await page
    .screenshot({
      path: join(artifacts, "statistics-layout-native-failure.png"),
      fullPage: true,
    })
    .catch(() => {});
  await writeFile(
    join(artifacts, "statistics-layout-native-failure.txt"),
    await page
      .locator("body")
      .innerText()
      .catch(() => ""),
  );
  throw error;
} finally {
  if (attached)
    await native("knowledge_delete_repertoire", {
      documentId: attached.id,
      expectedRevision: attached.revision,
    }).catch(() => {});
  await browser.close();
}

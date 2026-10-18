// Production frontend -> actual isolated native SQLite. --reset is deliberately opt-in for QA.
import { expect } from "@playwright/test";
import { readFile, writeFile, mkdir } from "node:fs/promises";
import { resolve, join, sep } from "node:path";
import { createHash } from "node:crypto";
import { DatabaseSync } from "node:sqlite";
import { bridge } from "./helpers/product-bridge.mjs";
const config = JSON.parse(
  await readFile(".qa/ui-atlas-tools/current.json", "utf8"),
);
const profile = resolve(config.data);
if (!profile.startsWith(resolve(".qa") + sep))
  throw Error("Isolated .qa profile required");
const out = ".qa/statistics-settings-native";
await mkdir(out, { recursive: true });
const checks = [],
  errors = [];
function snapshot() {
  const db = new DatabaseSync(join(profile, "chesspush.sqlite3"), {
    readOnly: true,
  });
  try {
    const rows = {};
    for (const table of [
      "games",
      "statistics_games",
      "statistics_payloads",
      "statistics_analysis_jobs",
      "statistics_analysis_runs",
      "statistics_engine_observations",
      "statistics_move_evaluations",
      "statistics_engine_state",
      "game_graph_games",
      "game_graph_revisions",
      "game_occurrences",
      "knowledge_documents",
      "knowledge_document_versions",
      "knowledge_node_index",
      "statistics_quarantine_history",
      "statistics_rating_snapshots",
    ])
      rows[table] = db.prepare(`SELECT * FROM ${table} ORDER BY rowid`).all();
    return {
      primary: createHash("sha256").update(JSON.stringify(rows)).digest("hex"),
      counts: Object.fromEntries(
        Object.entries(rows).map(([key, values]) => [key, values.length]),
      ),
      membership: db
        .prepare(
          "SELECT * FROM statistics_account_games ORDER BY account_id,game_id",
        )
        .all(),
      accounts: db
        .prepare("SELECT * FROM statistics_accounts ORDER BY id")
        .all(),
      foreignKeys: Number(
        db.prepare("SELECT count(*) AS n FROM pragma_foreign_key_check").get()
          .n,
      ),
    };
  } finally {
    db.close();
  }
}
async function openSettings(page) {
  await page.getByRole("button", { name: "Настройки", exact: true }).click();
  await page
    .getByRole("dialog", { name: "Настройки", exact: true })
    .getByRole("tab", { name: "Статистика", exact: true })
    .click();
  await expect(
    page.getByLabel("Глубина покрытия дебютов, ходов", { exact: true }),
  ).toBeEnabled();
}
for (const theme of ["light", "dark"]) {
  const h = await bridge(config, theme),
    page = h.page;
  page.setDefaultTimeout(20000);
  page.on("pageerror", (error) => errors.push(error.message));
  const original = await h.native("statistics_preferences");
  const before = snapshot();
  try {
    await openSettings(page);
    const input = page.getByLabel("Глубина покрытия дебютов, ходов", {
      exact: true,
    });
    const save = page.getByRole("button", {
      name: "Сохранить глубину",
      exact: true,
    });
    await expect(input).toHaveValue(
      String(original.opening_coverage_depth_moves),
    );
    for (const value of ["0", "41", "1.5", ""]) {
      await input.fill(value);
      await expect(save).toBeDisabled();
      await expect(input).toHaveAttribute("aria-invalid", "true");
      await input.press("Enter");
    }
    expect(await h.native("statistics_preferences")).toEqual(original);
    const next = original.opening_coverage_depth_moves === 17 ? 18 : 17;
    await input.fill(String(next));
    await input.press("Enter");
    await expect(page.getByRole("status")).toContainText(
      "Глубина покрытия сохранена",
    );
    expect(await h.native("statistics_preferences")).toEqual({
      opening_coverage_depth_moves: next,
    });
    await page
      .getByRole("button", { name: "Закрыть настройки", exact: true })
      .click();
    await openSettings(page);
    await expect(input).toHaveValue(String(next));
    checks.push({
      scenario: `${theme}: real native preference validation, Enter, persistence across settings remount`,
      result: "PASS",
    });
    const selectedId = await page
      .getByLabel("Аккаунт статистики", { exact: true })
      .inputValue();
    const account = before.accounts.find(
      (account) => account.id === selectedId,
    );
    expect(account).toBeTruthy();
    await page
      .getByRole("button", { name: "Сбросить статистику", exact: true })
      .click();
    const confirmation = page.getByRole("alertdialog");
    await expect(confirmation).toHaveAccessibleName(
      `Сбросить статистику ${account.username}?`,
    );
    await expect(confirmation).toContainText(
      "Начать заново с текущего момента",
    );
    await expect(
      confirmation.getByRole("button", { name: "Отмена", exact: true }),
    ).toBeFocused();
    await confirmation
      .getByRole("button", { name: "Отмена", exact: true })
      .click();
    expect(snapshot()).toEqual(before);
    await page
      .getByRole("button", { name: "Сбросить статистику", exact: true })
      .click();
    // Explicit transport fault injection: actual native reset must not run on this branch.
    await page.evaluate(
      () => (window.__atlas.errors.statistics_reset = "Тестовая ошибка сброса"),
    );
    await page
      .getByRole("button", {
        name: "Сбросить статистику аккаунта",
        exact: true,
      })
      .click();
    await expect(page.getByRole("alert")).toContainText(
      "Тестовая ошибка сброса",
    );
    await expect(page.getByRole("status")).toHaveCount(0);
    expect(snapshot()).toEqual(before);
    await page.evaluate(() => delete window.__atlas.errors.statistics_reset);
    await page.screenshot({ path: out + `/${theme}-confirmation-error.png` });
    await page
      .getByRole("alertdialog")
      .getByRole("button", { name: "Отмена", exact: true })
      .click();
    checks.push({
      scenario: `${theme}: named default-Cancel confirmation and injected transport failure preserve all native data`,
      result: "PASS",
    });
  } finally {
    await h.native("statistics_preferences_save", { preferences: original });
    await h.browser.close();
  }
}
if (process.argv.includes("--reset")) {
  const h = await bridge(config, "light"),
    page = h.page;
  page.setDefaultTimeout(25000);
  page.on("pageerror", (error) => errors.push(error.message));
  try {
    const before = snapshot();
    const state = await h.native("statistics_state", {});
    const account =
      state.accounts.find(
        (account) => account.id === state.active_account_id,
      ) ?? state.accounts[0];
    if (!account?.id.toLowerCase().includes("fixture"))
      throw Error(
        "A synthetic fixture identity is required for reset acceptance",
      );
    expect(
      before.membership.filter((row) => row.account_id === account.id).length,
    ).toBeGreaterThan(0);
    await page
      .locator(".app-header")
      .getByRole("button", { name: "Статистика", exact: true })
      .click();
    await page.getByRole("tab", { name: "Граф партий", exact: true }).click();
    await expect(page.locator(".graph-workspace")).toBeVisible();
    const graphAccount = page.locator(
      ".simple-statistics-controls select[aria-label='Аккаунт статистики']",
    );
    if (await graphAccount.count()) await graphAccount.selectOption(account.id);
    await page
      .locator(".graph-history")
      .getByRole("button", { name: "Начало", exact: true })
      .click();
    const next = page.locator(".branch-node[data-level='1']").first();
    await expect(next).toBeVisible();
    await next.click();
    await expect(page.locator(".history-node")).toHaveCount(2);
    await openSettings(page);
    await page
      .getByLabel("Аккаунт статистики", { exact: true })
      .selectOption(account.id);
    await page
      .getByRole("button", { name: "Сбросить статистику", exact: true })
      .click();
    const started = Date.now();
    await page
      .getByRole("button", {
        name: "Сбросить статистику аккаунта",
        exact: true,
      })
      .click();
    await expect(page.getByRole("status")).toContainText(
      `Статистика ${account.username} сброшена`,
    );
    const after = snapshot();
    expect(after.primary).toBe(before.primary);
    expect(after.counts).toEqual(before.counts);
    expect(
      after.membership.filter((row) => row.account_id === account.id),
    ).toEqual([]);
    expect(
      after.membership.filter((row) => row.account_id !== account.id),
    ).toEqual(before.membership.filter((row) => row.account_id !== account.id));
    const updated = after.accounts.find((row) => row.id === account.id),
      previous = before.accounts.find((row) => row.id === account.id);
    expect(updated.t0_ms).toBeGreaterThanOrEqual(started);
    expect({ ...updated, t0_ms: previous.t0_ms }).toEqual(previous);
    expect(after.accounts.filter((row) => row.id !== account.id)).toEqual(
      before.accounts.filter((row) => row.id !== account.id),
    );
    expect(after.foreignKeys).toBe(0);
    await page
      .getByRole("button", { name: "Закрыть настройки", exact: true })
      .click();
    await expect(page.locator(".graph-workspace")).toBeVisible();
    await expect(page.locator(".history-node")).toHaveCount(1);
    await expect(
      page.getByRole("button", { name: "Назад по ветви", exact: true }),
    ).toBeDisabled();
    await page.getByRole("tab", { name: "Дебюты", exact: true }).click();
    await expect(page.locator(".opening-tables tbody tr")).toHaveCount(0);
    await h.native("statistics_sync");
    // Reload exercises durable cohort, observation boundary and fresh graph hydration.
    await page.reload();
    await page
      .locator(".app-header")
      .getByRole("button", { name: "Статистика", exact: true })
      .click();
    await expect(page.locator(".opening-tables tbody tr")).toHaveCount(0);
    const refreshed = await h.native("statistics_state", {
      accountId: account.id,
    });
    expect(refreshed.accounts.find((row) => row.id === account.id).t0_ms).toBe(
      updated.t0_ms,
    );
    expect(
      snapshot().membership.filter((row) => row.account_id === account.id),
    ).toEqual([]);
    await page.screenshot({ path: out + "/after-reset-empty-openings.png" });
    await writeFile(
      out + "/reset-state.json",
      JSON.stringify(
        {
          before,
          after,
          elapsedMs: Date.now() - started,
          accountId: account.id,
          t0Ms: updated.t0_ms,
        },
        null,
        2,
      ),
    );
    checks.push({
      scenario:
        "Actual native reset: explicit account, graph returns to root, empty white/black tables, T0 persisted across reload/sync; primary games/PGN/evaluations/documents/other accounts unchanged",
      result: "PASS",
    });
  } finally {
    await h.browser.close();
  }
}
expect(errors).toEqual([]);
await writeFile(
  out + "/results.json",
  JSON.stringify(
    {
      result: "PASS",
      resetExecuted: process.argv.includes("--reset"),
      checks,
      pageErrors: errors,
      boundary:
        "production frontend -> isolated native profile; only transport failure is injected",
    },
    null,
    2,
  ),
);
console.log(
  JSON.stringify({
    result: "PASS",
    checks: checks.length,
    resetExecuted: process.argv.includes("--reset"),
    pageErrors: errors.length,
  }),
);

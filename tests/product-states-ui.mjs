// Negative UI states are controlled at IPC; normal cache/data queries use isolated Rust/SQLite.
import { expect } from "@playwright/test";
import { readFile, mkdir, writeFile } from "node:fs/promises";
import { bridge } from "./helpers/product-bridge.mjs";
const config = process.env.CHESSPUSH_CDP
  ? { cdp: process.env.CHESSPUSH_CDP }
  : JSON.parse(await readFile(".qa/ui-atlas-tools/current.json"));
const h = await bridge(config, "light"),
  p = h.page;
const output = "screens/product-ux";
await mkdir(output + "/evidence", { recursive: true });
await mkdir(output + "/after", { recursive: true });
p.setDefaultTimeout(20000);
const errors = [],
  checks = [],
  shots = [];
p.on("pageerror", (error) => errors.push(String(error)));
const state = await h.native("statistics_state", { accountId: null });
const account = await h.native("account_status");
const accountId = state.active_account_id ?? state.accounts[0]?.id;
if (!accountId) throw Error("Expected an isolated account fixture");
const report = await h.native("statistics_report", {
  filter: { account_id: accountId, variant: "standard" },
});
if (!report.total_games) throw Error("Expected actual cached game fixture");
const sync = state.sync ?? {
  status: "idle",
  last_success_ms: Date.now() - 60000,
  last_reconciled_ms: null,
  error_message: null,
  error_code: null,
  retry_not_before_ms: null,
  pending_games: 0,
  pending_ranges: 0,
  quarantined_games: state.quarantine.length,
  boundary_uncertain: false,
  baseline_complete: true,
};
const connected = {
  ...account,
  status: "connected",
  message: null,
  hasToken: true,
};
const record = (
  name,
  method = "controlled IPC negative state + actual production frontend",
) => {
  checks.push({ name, result: "PASS", method });
  console.log("PASS", name);
};
const nav = (label) =>
  p
    .locator(".app-header")
    .getByRole("button", { name: label, exact: true })
    .click();
async function modes({
  accountValue = connected,
  stateValue = state,
  reportValue = null,
} = {}) {
  await p.evaluate(
    ({ accountValue, stateValue, reportValue }) => {
      const m = window.__atlas;
      m.errors = {};
      m.waits = {};
      m.statePatch = {};
      m.responses.account_status = accountValue;
      m.responses.statistics_state = stateValue;
      if (reportValue) m.responses.statistics_report = reportValue;
      else delete m.responses.statistics_report;
      window.__atlasDispatch("lichess-account-updated", accountValue);
      window.__atlasDispatch("statistics-updated", null);
    },
    { accountValue, stateValue, reportValue },
  );
}
async function shot(name) {
  // Let the emitted state event enter React, then capture its settled generation.
  await p.waitForTimeout(60);
  await expect(p.locator("main.statistics")).toHaveAttribute(
    "aria-busy",
    "false",
  );
  const path = `after/states-light-1440x960-${name}.png`;
  await p.screenshot({ path: output + "/" + path });
  shots.push({
    path,
    theme: "light",
    viewport: { width: 1440, height: 960 },
    dpr: 1,
    provenance:
      "production frontend→native isolated Rust/SQLite; specified negative states controlled at IPC",
  });
}
try {
  const empty = {
    ...report,
    total_games: 0,
    total_decisions: 0,
    analysed_decisions: 0,
    games: [],
    metrics: [],
    events: [],
  };
  await modes({
    accountValue: {
      id: null,
      status: "disconnected",
      username: null,
      rapid: null,
      blitz: null,
      message: null,
      hasToken: false,
    },
    stateValue: { ...state, accounts: [], active_account_id: null, sync: null },
    reportValue: empty,
  });
  await nav("Моя игра");
  const main = p.locator("main.statistics");
  await expect(
    main.getByRole("button", { name: "Подключить Lichess", exact: true }),
  ).toBeVisible();
  await expect(main.locator("table")).toHaveCount(0);
  await shot("no-account");
  await nav("Репертуар");
  await expect(
    p
      .locator(".repertoire-list")
      .getByRole("button", { name: "Создать", exact: true }),
  ).toBeEnabled();
  record(
    "No account: one relevant connect action, no dash tables; local repertoire remains available",
  );
  await modes({ reportValue: empty });
  await nav("Моя игра");
  await expect(main).toContainText("Новые завершённые партии появятся здесь");
  await expect(
    main.getByRole("button", { name: "Проверить новые партии", exact: true }),
  ).toBeVisible();
  await expect(main.locator("table")).toHaveCount(0);
  await shot("connected-empty");
  record(
    "Connected account with no accepted games: dedicated waiting state, no repeated tables or forced login",
  );
  await modes();
  await expect(main.locator(".practice-summary")).toContainText(
    `${report.total_games} партий`,
  );
  await main.locator("details.stats-extra > summary").click();
  await p.getByLabel("Рейтинг соперника от", { exact: true }).fill("9999");
  await expect(main).toContainText("Нет партий по этим условиям");
  await main.locator("details.stats-extra > summary").click();
  await expect(
    main
      .locator(".stats-empty")
      .getByRole("button", { name: "Сбросить фильтры", exact: true }),
  ).toBeVisible();
  await shot("empty-filter");
  await main
    .locator(".stats-empty")
    .getByRole("button", { name: "Сбросить фильтры", exact: true })
    .click();
  await expect(main.locator(".practice-summary")).toContainText(
    `${report.total_games} партий`,
  );
  record(
    "An actual native rating filter returns an empty cohort; reset restores the cached native games",
    "actual native statistics_report filter → production frontend",
  );
  const noAnalysis = {
    ...report,
    analysed_decisions: 0,
    events: [],
    games: report.games.map((game) => ({
      ...game,
      analysed: 0,
      accuracy: null,
      acpl: null,
      mean_loss: null,
    })),
  };
  await modes({ reportValue: noAnalysis });
  await expect(main.locator(".practice-analysis-prompt")).toContainText(
    "Для оценки решений нужен Stockfish",
  );
  await expect(
    main.locator(".practice-analysis-prompt").getByRole("button"),
  ).toBeEnabled();
  await expect(main.locator(".practice-review")).toContainText(
    "Моменты для разбора появятся после анализа партий",
  );
  await shot("no-analysis");
  record(
    "No engine analysis: factual results and games stay available, no invented errors, explicit analysis action",
  );
  const offline = {
    ...state,
    sync: {
      ...sync,
      status: "retry",
      error_code: "network",
      error_message: "Fixture offline: network unavailable",
      retry_not_before_ms: Date.now() + 60000,
    },
  };
  await modes({
    accountValue: {
      ...connected,
      status: "offline",
      message: "Соединение с Lichess недоступно",
    },
    stateValue: offline,
  });
  await expect(main.getByRole("alert")).toContainText(
    "Сохранённая практика доступна",
  );
  await expect(main.locator(".practice-summary")).toContainText(
    `${report.total_games} партий`,
  );
  await expect(
    main.getByRole("button", { name: "Повторить", exact: true }),
  ).toBeEnabled();
  await shot("offline-cached");
  record(
    "Offline sync: human-readable error, retry action and cached native practice remain available",
  );
  const limited = {
    ...state,
    sync: {
      ...sync,
      status: "backoff",
      error_code: "http_429",
      error_message: "HTTP 429 fixture rate limit",
      retry_not_before_ms: Date.now() + 120000,
    },
  };
  await modes({ stateValue: limited });
  await expect(main.getByRole("alert")).toContainText("Следующий повтор");
  await expect(main.locator(".practice-summary")).toContainText(
    `${report.total_games} партий`,
  );
  await expect(main).not.toContainText("HTTP 429 fixture rate limit");
  await shot("rate-limit-cached");
  record(
    "HTTP 429: cache stays visible, next retry is shown, raw server wording is not the primary message",
  );
  await modes({
    accountValue: {
      ...connected,
      status: "invalid_token",
      message: "Доступ к Lichess истёк. Подключите аккаунт заново.",
    },
    stateValue: state,
  });
  await expect(p.locator(".global-error")).toContainText(
    "Доступ к Lichess истёк",
  );
  await expect(main.locator(".practice-summary")).toContainText(
    `${report.total_games} партий`,
  );
  await shot("invalid-token-cached");
  await p
    .locator(".global-error")
    .getByRole("button", { name: "Проверить подключение", exact: true })
    .click();
  const settings = p.getByRole("dialog", { name: "Настройки", exact: true });
  await expect(settings).toContainText("Доступ к Lichess истёк");
  await expect(
    settings.getByRole("button", {
      name: "Подключить через браузер заново",
      exact: true,
    }),
  ).toBeEnabled();
  await expect(settings.getByLabel("Токен", { exact: true })).toBeHidden();
  await shot("invalid-token-action");
  await settings
    .getByRole("button", { name: "Закрыть настройки", exact: true })
    .click();
  record(
    "Expired access is visible beside cached data and leads to browser reconnect; manual token stays hidden",
  );
  await modes({ stateValue: limited });
  await p.getByRole("button", { name: "Настройки", exact: true }).click();
  await settings.getByRole("tab", { name: "Диагностика", exact: true }).click();
  await settings
    .getByRole("button", { name: "Открыть диагностику", exact: true })
    .click();
  const diagnostics = p.locator(".practice-diagnostics");
  await expect(
    diagnostics.getByRole("heading", {
      name: "Диагностика данных",
      exact: true,
    }),
  ).toBeVisible();
  const observation =
    state.accounts.find((value) => value.id === accountId) ?? state.accounts[0];
  const t0 = new Intl.DateTimeFormat("ru-RU", {
    day: "2-digit",
    month: "2-digit",
    year: "numeric",
    timeZone: "Europe/Moscow",
    hour: "2-digit",
    minute: "2-digit",
  }).format(observation.t0_ms);
  await expect(diagnostics).toContainText(t0);
  await expect(diagnostics).toContainText(
    "http_429: HTTP 429 fixture rate limit",
  );
  if (!state.quarantine.length)
    throw Error("Expected a real quarantined PGN fixture");
  await diagnostics
    .locator("details > summary")
    .filter({ hasText: "Отклонённые загрузки" })
    .click();
  const quarantine = state.quarantine[0];
  const row = diagnostics
    .getByRole("table", { name: "Отклонённые загрузки", exact: true })
    .locator("tbody tr")
    .filter({ hasText: quarantine.external_id });
  await expect(row).toContainText(quarantine.reason);
  await row.locator("summary").filter({ hasText: "Исходные данные" }).click();
  await expect(row.locator("pre")).toHaveText(quarantine.raw_json);
  await expect(
    row.getByRole("button", { name: "Загрузить повторно", exact: true }),
  ).toBeEnabled();
  await shot("diagnostics-quarantine-raw");
  record(
    "Settings→Diagnostics exposes exact native T0, sync code and real quarantined raw input without removing retry",
    "actual native T0/quarantine; 429 sync status controlled at IPC",
  );
  if (errors.length) throw Error(errors.join("\n"));
  await writeFile(
    output + "/evidence/states-ui.json",
    JSON.stringify(
      {
        result: "PASS",
        checks,
        shots,
        errors,
        fixture: {
          games: report.total_games,
          quarantine: state.quarantine.length,
        },
        method:
          "Production frontend via headless Edge IPC bridge to isolated native Rust/SQLite. Account absence/empty history/no analysis/offline/429/expired access controlled at IPC; rating-filter cohort and T0/quarantine are actual native data. No token submitted and no account modified.",
      },
      null,
      2,
    ),
  );
  console.log(
    JSON.stringify({
      checks: checks.length,
      shots: shots.length,
      result: "PASS",
    }),
  );
} catch (error) {
  await p.screenshot({ path: output + "/after/states-failure.png" });
  console.log((await p.locator("body").innerText()).slice(-4500));
  await writeFile(
    output + "/evidence/states-ui.json",
    JSON.stringify(
      { result: "FAIL", error: String(error), checks, shots, errors },
      null,
      2,
    ),
  );
  throw error;
} finally {
  await h.browser.close();
}

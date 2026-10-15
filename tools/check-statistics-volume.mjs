import { spawn, execFile } from "node:child_process";
import { mkdir, mkdtemp, realpath, stat, writeFile } from "node:fs/promises";
import { basename, isAbsolute, join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { DatabaseSync } from "node:sqlite";
import net from "node:net";

const run = promisify(execFile);
const root = await realpath(fileURLToPath(new URL("../", import.meta.url)));
if (process.argv.length !== 3)
  throw Error(
    "Usage: node tools/check-statistics-volume.mjs .qa/statistics-performance-<id>",
  );
const qa = await realpath(join(root, ".qa"));
const data = await realpath(process.argv[2]);
const within = relative(qa, data);
if (
  !within ||
  within === ".." ||
  within.startsWith(".." + sep) ||
  isAbsolute(within)
)
  throw Error(
    "Volume data must be a canonical child directory of the repository .qa",
  );
if (!/^statistics-performance-[a-z\d-]+$/i.test(basename(data)))
  throw Error(
    "Volume check requires an explicit statistics-performance fixture directory",
  );
const database = join(data, "chesspush.sqlite3");
if (!(await stat(database)).isFile())
  throw Error("Volume fixture database is missing");
const db = new DatabaseSync(database, { readOnly: true });
let fixture;
try {
  const account = db
    .prepare("SELECT id, username FROM statistics_accounts")
    .all();
  if (account.length !== 1 || account[0].username !== "BenchmarkFixture")
    throw Error("Only the isolated BenchmarkFixture account is accepted");
  const games = db
    .prepare("SELECT count(*) AS n FROM statistics_games")
    .get().n;
  const paused = db
    .prepare("SELECT paused FROM statistics_engine_state WHERE id=1")
    .get()?.paused;
  const analysed = db
    .prepare(
      "SELECT count(DISTINCT game_id) AS n FROM statistics_analysis_runs WHERE state='succeeded'",
    )
    .get().n;
  if (games !== 1001) throw Error(`Expected 1001 fixture games, got ${games}`);
  if (paused !== 1)
    throw Error("The persisted Stockfish worker must be paused before launch");
  if (analysed < 1001)
    throw Error(`Expected persisted evidence for 1001 games, got ${analysed}`);
  fixture = {
    games,
    analysed_games: analysed,
    paused: true,
    account_id: account[0].id,
  };
} finally {
  db.close();
}
const artifacts = await mkdtemp(join(data, "volume-run-"));
const webview = join(artifacts, "webview");
await mkdir(webview);
const delay = (ms) => new Promise((done) => setTimeout(done, ms));
const reservation = net.createServer();
await new Promise((done, fail) =>
  reservation.once("error", fail).listen(0, "127.0.0.1", done),
);
const port = reservation.address().port;
await new Promise((done) => reservation.close(done));
const env = {
  ...process.env,
  CHESSPUSH_TEST_DATA: data,
  CHESSPUSH_VOLUME_ARTIFACTS: artifacts,
  CHESSPUSH_VOLUME_ACCOUNT: fixture.account_id,
  CHESSPUSH_CDP: `http://127.0.0.1:${port}`,
  WEBVIEW2_USER_DATA_FOLDER: webview,
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
};
delete env.CHESSPUSH_TEST_API;
delete env.CHESSPUSH_STATS_PHASE;
delete env.CHESSPUSH_FLOW_PHASE;
let application, applicationClosed, checker, spawnError, cleanupPromise;
const alive = (child) =>
  !!child?.pid && child.exitCode === null && child.signalCode === null;
async function killOwned(child) {
  if (!alive(child)) return;
  if (process.platform === "win32")
    await run("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
      windowsHide: true,
    }).catch(() => {});
  else child.kill("SIGTERM");
}
function cleanup() {
  return (cleanupPromise ??= (async () => {
    await killOwned(checker);
    if (!alive(application)) return;
    if (process.platform === "win32") {
      await run(
        "powershell.exe",
        [
          "-NoProfile",
          "-Command",
          "$volumeProcess=Get-Process -Id " +
            application.pid +
            " -ErrorAction SilentlyContinue; if($volumeProcess){[void]$volumeProcess.CloseMainWindow()}",
        ],
        { windowsHide: true },
      ).catch(() => {});
    } else application.kill("SIGTERM");
    await Promise.race([applicationClosed, delay(30000)]);
    const forced = alive(application);
    await killOwned(application);
    await Promise.race([applicationClosed, delay(5000)]);
    if (forced) throw Error("Volume application did not shut down gracefully");
  })());
}
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => {
    void cleanup().finally(() => process.exit(signal === "SIGINT" ? 130 : 143));
  });
const started = performance.now();
try {
  application = spawn(
    join(
      root,
      "src-tauri",
      "target",
      "debug",
      process.platform === "win32" ? "chesspush.exe" : "chesspush",
    ),
    [],
    {
      cwd: root,
      env,
      windowsHide: true,
      stdio: ["ignore", "pipe", "pipe"],
    },
  );
  application.on("error", (error) => {
    spawnError = error;
  });
  applicationClosed = new Promise((done) => application.once("close", done));
  application.stdout.pipe(process.stdout);
  application.stderr.pipe(process.stderr);
  let ready = false;
  for (let attempt = 0; attempt < 200; attempt++) {
    if (spawnError) throw spawnError;
    if (!alive(application))
      throw Error("Volume application exited before CDP was ready");
    try {
      if (
        (
          await fetch(env.CHESSPUSH_CDP + "/json/version", {
            signal: AbortSignal.timeout(300),
          })
        ).ok
      ) {
        ready = true;
        break;
      }
    } catch {}
    await delay(150);
  }
  if (!ready) throw Error("Volume WebView2 CDP did not become ready");
  const pending = run(
    process.execPath,
    [join(root, "tests", "statistics-volume.mjs")],
    {
      cwd: root,
      env,
      windowsHide: true,
      timeout: 240000,
      maxBuffer: 4 * 1024 * 1024,
    },
  );
  checker = pending.child;
  const result = await pending;
  checker = undefined;
  process.stdout.write(result.stdout);
  process.stderr.write(result.stderr);
  let hostMemory = null;
  if (process.platform === "win32" && alive(application)) {
    const memory = await run(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        "Get-Process -Id " +
          application.pid +
          " | Select-Object WorkingSet64,PrivateMemorySize64 | ConvertTo-Json -Compress",
      ],
      { windowsHide: true },
    ).catch(() => null);
    if (memory) hostMemory = JSON.parse(memory.stdout);
  }
  await writeFile(
    join(artifacts, "volume-host.json"),
    JSON.stringify(
      {
        fixture,
        data,
        application_pid: application.pid,
        elapsed_ms: performance.now() - started,
        host_memory: hostMemory,
        memory_scope:
          "Owned Tauri host only; excludes WebView and Stockfish. Worker remained paused.",
      },
      null,
      2,
    ),
  );
  console.log("PASS isolated native volume check: " + artifacts);
} finally {
  await cleanup();
}

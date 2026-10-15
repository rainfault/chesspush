import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import { mkdir, mkdtemp, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import net from "node:net";
import { statisticsApiFixture } from "../tests/statistics-api-fixture.mjs";
const run = promisify(execFile),
  root = resolve(".");
const delay = (ms) => new Promise((ok) => setTimeout(ok, ms).unref());
await mkdir(".qa", { recursive: true });
const directory = await mkdtemp(join(root, ".qa", "statistics-flow-")),
  data = join(directory, "data");
await mkdir(data);
const fixture = await statisticsApiFixture();
let application, closed, checker, cleanupPromise;
const alive = (p) => p?.pid && p.exitCode === null && p.signalCode === null;
async function killOwned(p) {
  if (!alive(p)) return;
  await run("taskkill.exe", ["/PID", String(p.pid), "/T", "/F"], {
    windowsHide: true,
  }).catch(() => {});
}
function cleanup() {
  return (cleanupPromise ??= (async () => {
    await killOwned(checker);
    if (alive(application)) {
      await run(
        "powershell.exe",
        [
          "-NoProfile",
          "-Command",
          `$flowProcess=Get-Process -Id ${application.pid} -ErrorAction SilentlyContinue; if($flowProcess){[void]$flowProcess.CloseMainWindow()}`,
        ],
        { windowsHide: true },
      ).catch(() => {});
      await Promise.race([closed, delay(30000)]);
      const forced = alive(application);
      await killOwned(application);
      if (forced) throw Error("Native flow failed graceful shutdown");
    }
  })());
}
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => {
    void cleanup()
      .finally(() => fixture.close())
      .finally(() => process.exit(signal === "SIGINT" ? 130 : 143));
  });
try {
  for (const phase of ["connect", "catchup", "recover"]) {
    if (phase === "catchup") {
      fixture.publish(1);
      fixture.publish(2);
    }
    const reservation = net.createServer();
    await new Promise((ok, fail) =>
      reservation.once("error", fail).listen(0, "127.0.0.1", ok),
    );
    const port = reservation.address().port;
    await new Promise((ok) => reservation.close(ok));
    cleanupPromise = undefined;
    const env = {
      ...process.env,
      CHESSPUSH_TEST_DATA: data,
      CHESSPUSH_TEST_API: fixture.endpoint,
      CHESSPUSH_FLOW_PHASE: phase,
      CHESSPUSH_CDP: `http://127.0.0.1:${port}`,
      WEBVIEW2_USER_DATA_FOLDER: join(directory, "webview"),
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
      CHESSPUSH_STOCKFISH:
        process.env.CHESSPUSH_STOCKFISH ??
        join(
          root,
          ".qa",
          "stockfish",
          "release",
          "stockfish",
          "stockfish-windows-x86-64-universal.exe",
        ),
    };
    application = spawn(
      join(root, "src-tauri", "target", "debug", "chesspush.exe"),
      [],
      { cwd: root, env, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] },
    );
    let spawnError;
    application.on("error", (e) => {
      spawnError = e;
    });
    closed = new Promise((ok) => application.once("close", ok));
    application.stdout.pipe(process.stdout);
    application.stderr.pipe(process.stderr);
    try {
      let ready = false;
      for (let i = 0; i < 200; i++) {
        if (spawnError) throw spawnError;
        if (!alive(application)) throw Error("Native flow exited");
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
      if (!ready) throw Error("Native flow CDP unavailable");
      const pending = run(process.execPath, ["tests/statistics-flow.mjs"], {
        cwd: root,
        env,
        windowsHide: true,
        timeout: 240000,
        maxBuffer: 4 * 1024 * 1024,
      });
      checker = pending.child;
      const result = await pending;
      checker = undefined;
      process.stdout.write(result.stdout);
      process.stderr.write(result.stderr);
      const memory = await run(
        "powershell.exe",
        [
          "-NoProfile",
          "-Command",
          `Get-Process -Id ${application.pid} | Select-Object WorkingSet64,PrivateMemorySize64 | ConvertTo-Json -Compress`,
        ],
        { windowsHide: true },
      );
      await writeFile(join(data, `host-memory-${phase}.json`), memory.stdout);
    } finally {
      await cleanup();
    }
  }
  await writeFile(
    join(data, "api-requests.json"),
    JSON.stringify(
      { accountReads: fixture.accountReads, requests: fixture.requests },
      null,
      2,
    ),
  );
  if (
    fixture.accountReads < 2 ||
    !fixture.requests.some((r) => r.path.startsWith("/api/games/user/"))
  )
    throw Error("Real API transport not exercised");
  console.log(
    "PASS API → SQLite → GUI → real Stockfish → restart flow: " + directory,
  );
} finally {
  await cleanup();
  await fixture.close();
}

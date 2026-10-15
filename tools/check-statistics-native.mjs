import { spawn, execFile } from "node:child_process";
import { mkdir, mkdtemp } from "node:fs/promises";
import { resolve, join } from "node:path";
import { promisify } from "node:util";
import net from "node:net";
const run = promisify(execFile),
  root = resolve(".");
const delay = (ms) => new Promise((ok) => setTimeout(ok, ms));
let application, applicationClosed, checkProcess, spawnError, cleanupPromise;
function alive(child) {
  return !!child?.pid && child.exitCode === null && child.signalCode === null;
}
async function killOwned(child) {
  if (!alive(child)) return;
  if (process.platform === "win32") {
    await run("taskkill.exe", ["/PID", String(child.pid), "/T", "/F"], {
      windowsHide: true,
    }).catch(() => {});
  } else child.kill();
}
function cleanup() {
  if (cleanupPromise) return cleanupPromise;
  const child = checkProcess,
    app = application,
    closed = applicationClosed;
  checkProcess = undefined;
  cleanupPromise = (async () => {
    // These references come only from processes spawned by this harness.
    await killOwned(child);
    if (!alive(app)) return;
    if (process.platform === "win32") {
      await run(
        "powershell.exe",
        [
          "-NoProfile",
          "-Command",
          "$statsProcess=Get-Process -Id " +
            app.pid +
            " -ErrorAction SilentlyContinue; if($statsProcess){[void]$statsProcess.CloseMainWindow()}",
        ],
        { windowsHide: true },
      ).catch(() => {});
    } else app.kill("SIGTERM");
    await Promise.race([closed, delay(30000)]);
    const forced = alive(app);
    await killOwned(app);
    await Promise.race([closed, delay(5000)]);
    if (forced)
      throw new Error("Native application failed to shut down gracefully");
  })();
  return cleanupPromise;
}
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => {
    void cleanup().finally(() => process.exit(signal === "SIGINT" ? 130 : 143));
  });
async function checked(command, args, options) {
  const pending = run(command, args, options);
  checkProcess = pending.child;
  try {
    return await pending;
  } finally {
    if (checkProcess === pending.child) checkProcess = undefined;
  }
}
await mkdir(".qa", { recursive: true });
const directory = await mkdtemp(join(root, ".qa", "statistics-native-")),
  data = join(directory, "data");
await mkdir(data, { recursive: true });
const seeded = await checked(
  "cargo",
  [
    "run",
    "--manifest-path",
    "src-tauri/Cargo.toml",
    "--no-default-features",
    "--example",
    "statistics_fixture",
    "--",
    data,
  ],
  { cwd: root, windowsHide: true, timeout: 180000, maxBuffer: 4 * 1024 * 1024 },
);
process.stdout.write(seeded.stdout);
process.stderr.write(seeded.stderr);
const stockfish =
  process.env.CHESSPUSH_STOCKFISH ??
  join(
    root,
    ".qa",
    "stockfish",
    "release",
    "stockfish",
    "stockfish-windows-x86-64-universal.exe",
  );
for (const phase of ["first", "interrupt", "resume", "persist"]) {
  const reservation = net.createServer();
  await new Promise((ok, fail) =>
    reservation.once("error", fail).listen(0, "127.0.0.1", ok),
  );
  const port = reservation.address().port;
  await new Promise((ok) => reservation.close(ok));
  const env = {
    ...process.env,
    CHESSPUSH_TEST_DATA: data,
    WEBVIEW2_USER_DATA_FOLDER: join(directory, "webview"),
    WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: "--remote-debugging-port=" + port,
    CHESSPUSH_CDP: "http://127.0.0.1:" + port,
    CHESSPUSH_STOCKFISH: stockfish,
    CHESSPUSH_STATS_PHASE: phase,
  };
  cleanupPromise = undefined;
  spawnError = undefined;
  application = spawn(
    join(root, "src-tauri", "target", "debug", "chesspush.exe"),
    [],
    { cwd: root, env, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] },
  );
  application.on("error", (error) => {
    spawnError = error;
  });
  application.stdout.pipe(process.stdout);
  application.stderr.pipe(process.stderr);
  applicationClosed = new Promise((ok) => application.once("close", ok));
  try {
    let ready = false;
    for (let i = 0; i < 200; i++) {
      if (spawnError) throw spawnError;
      if (!alive(application))
        throw new Error("Application exited " + application.exitCode);
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
    if (!ready) throw new Error("Native WebView2 not ready");
    const result = await checked(
      process.execPath,
      ["tests/statistics-native.mjs"],
      {
        cwd: root,
        env,
        windowsHide: true,
        timeout: 240000,
        maxBuffer: 4 * 1024 * 1024,
      },
    );
    process.stdout.write(result.stdout);
    process.stderr.write(result.stderr);
  } finally {
    await cleanup();
  }
}
console.log(
  "PASS complete native Statistics QA; isolated artifacts: " + directory,
);

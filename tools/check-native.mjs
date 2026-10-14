import { spawn, execFile } from "node:child_process";
import { mkdir, mkdtemp } from "node:fs/promises";
import { resolve, join } from "node:path";
import { promisify } from "node:util";
const run = promisify(execFile);
const root = resolve(".");
await mkdir(".qa", { recursive: true });
const directory = await mkdtemp(join(root, ".qa", "native-"));
await run(process.execPath, ["tests/benchmark.mjs"], {
  cwd: root,
  windowsHide: true,
});
const server = await import("node:net");
const reservation = server.createServer();
await new Promise((ok, fail) =>
  reservation.once("error", fail).listen(0, "127.0.0.1", ok),
);
const port = reservation.address().port;
await new Promise((ok) => reservation.close(ok));
const endpoint = `http://127.0.0.1:${port}`;
const environment = {
  ...process.env,
  CHESSPUSH_TEST_DATA: join(directory, "data"),
  WEBVIEW2_USER_DATA_FOLDER: join(directory, "webview"),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
  CHESSPUSH_CDP: endpoint,
};
const executable = join(
  root,
  "src-tauri",
  "target",
  "debug",
  process.platform === "win32" ? "chesspush.exe" : "chesspush",
);
const application = spawn(executable, [], {
  cwd: root,
  env: environment,
  windowsHide: true,
  stdio: ["ignore", "pipe", "pipe"],
});
application.stdout.pipe(process.stdout);
application.stderr.pipe(process.stderr);
let spawnError;
application.on("error", (error) => {
  spawnError = error;
});
const closed = new Promise((resolve) => application.once("close", resolve));
let checkProcess;
let cleanupPromise;
function cleanup() {
  if (cleanupPromise) return cleanupPromise;
  cleanupPromise = (async () => {
    if (checkProcess?.exitCode === null) checkProcess.kill();
    if (application.exitCode !== null || !application.pid) return;
    if (process.platform === "win32") {
      await run(
        "powershell.exe",
        [
          "-NoProfile",
          "-Command",
          `$p=Get-Process -Id ${application.pid} -ErrorAction SilentlyContinue; if($p){[void]$p.CloseMainWindow()}`,
        ],
        { windowsHide: true },
      ).catch(() => {});
      await Promise.race([
        closed,
        new Promise((resolve) => setTimeout(resolve, 5000)),
      ]);
      if (application.exitCode === null)
        await run(
          "taskkill.exe",
          ["/PID", String(application.pid), "/T", "/F"],
          { windowsHide: true },
        ).catch(() => {});
    } else application.kill();
    await Promise.race([
      closed,
      new Promise((resolve) => setTimeout(resolve, 5000)),
    ]);
  })();
  return cleanupPromise;
}
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => {
    void cleanup().finally(() => process.exit(signal === "SIGINT" ? 130 : 143));
  });
try {
  let available = false;
  for (let attempt = 0; attempt < 150; attempt++) {
    if (spawnError) throw spawnError;
    if (application.exitCode !== null)
      throw new Error(`Test application exited: ${application.exitCode}`);
    try {
      const response = await fetch(endpoint + "/json/version", {
        signal: AbortSignal.timeout(400),
      });
      if (response.ok) {
        available = true;
        break;
      }
    } catch {}
    await new Promise((ok) => setTimeout(ok, 200));
  }
  if (!available) throw new Error("Test WebView2 did not become available");
  for (const script of ["tests/ui-smoke.mjs", "tests/native-smoke.mjs"]) {
    const pending = run(process.execPath, [script], {
      cwd: root,
      env: environment,
      windowsHide: true,
      timeout: 120000,
      maxBuffer: 1024 * 1024,
    });
    checkProcess = pending.child;
    const result = await pending;
    checkProcess = undefined;
    process.stdout.write(result.stdout);
    process.stderr.write(result.stderr);
  }
  console.log("Native checks passed; user database was not touched.");
} finally {
  await cleanup();
}

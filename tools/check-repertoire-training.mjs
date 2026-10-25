// Run the repertoire trainer against a fresh, isolated native profile.
// Build the debug application first: npm run tauri build -- --debug --no-bundle
import { spawn, execFile } from "node:child_process";
import { mkdir, mkdtemp } from "node:fs/promises";
import { resolve, join } from "node:path";
import { promisify } from "node:util";
import net from "node:net";

const run = promisify(execFile),
  root = resolve(".");
await mkdir(".qa", { recursive: true });
const directory = await mkdtemp(
  join(root, ".qa", "repertoire-training-native-"),
);
const data = join(directory, "data");
await mkdir(data);
const reservation = net.createServer();
await new Promise((ok, fail) =>
  reservation.once("error", fail).listen(0, "127.0.0.1", ok),
);
const port = reservation.address().port;
await new Promise((ok) => reservation.close(ok));
const env = {
  ...process.env,
  CHESSPUSH_TEST_DATA: data,
  CHESSPUSH_TEST_CDP: `http://127.0.0.1:${port}`,
  CHESSPUSH_TRAINING_ARTIFACTS: directory,
  WEBVIEW2_USER_DATA_FOLDER: join(directory, "webview"),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
};
const application = spawn(
  process.env.CHESSPUSH_TEST_EXECUTABLE ??
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
application.stdout.pipe(process.stdout);
application.stderr.pipe(process.stderr);
let spawnError, checkProcess, cleanupPromise;
application.on("error", (error) => {
  spawnError = error;
});
const closed = new Promise((ok) => application.once("close", ok));
const delay = (ms) => new Promise((ok) => setTimeout(ok, ms));
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
          `$trainingProcess=Get-Process -Id ${application.pid} -ErrorAction SilentlyContinue; if($trainingProcess){[void]$trainingProcess.CloseMainWindow()}`,
        ],
        { windowsHide: true },
      ).catch(() => {});
      await Promise.race([closed, delay(5000)]);
      if (application.exitCode === null)
        await run(
          "taskkill.exe",
          ["/PID", String(application.pid), "/T", "/F"],
          { windowsHide: true },
        ).catch(() => {});
    } else application.kill("SIGTERM");
    await Promise.race([closed, delay(5000)]);
  })();
  return cleanupPromise;
}
for (const signal of ["SIGINT", "SIGTERM"])
  process.once(signal, () => {
    void cleanup().finally(() => process.exit(signal === "SIGINT" ? 130 : 143));
  });
try {
  let ready = false;
  for (let i = 0; i < 200; i++) {
    if (spawnError) throw spawnError;
    if (application.exitCode !== null)
      throw new Error(`Native application exited: ${application.exitCode}`);
    try {
      if (
        (
          await fetch(env.CHESSPUSH_TEST_CDP + "/json/version", {
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
  if (!ready) throw new Error("Native WebView2 did not become available");
  const pending = run(
    process.execPath,
    [
      process.env.CHESSPUSH_TRAINING_TEST_SCRIPT ??
        "tests/repertoire-training-ui.mjs",
    ],
    {
      cwd: root,
      env,
      windowsHide: true,
      timeout: 240000,
      maxBuffer: 4 * 1024 * 1024,
    },
  );
  checkProcess = pending.child;
  checkProcess.stdout.pipe(process.stdout);
  checkProcess.stderr.pipe(process.stderr);
  await pending;
  checkProcess = undefined;
  console.log(`PASS isolated repertoire training UI; artifacts: ${directory}`);
} finally {
  await cleanup();
}

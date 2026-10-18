// Native volume harness: only a separately generated synthetic database is accepted.
import { DatabaseSync, backup } from "node:sqlite";
import { mkdir, mkdtemp } from "node:fs/promises";
import { resolve, join } from "node:path";
import { spawn, execFile } from "node:child_process";
import { promisify } from "node:util";
import net from "node:net";
const root = resolve("."),
  source = join(root, ".qa", "graph-volume.sqlite3"),
  run = promisify(execFile);
const db = new DatabaseSync(source, { readOnly: true });
const account = db.prepare("SELECT username FROM statistics_accounts").all();
if (account.length !== 1 || account[0].username !== "SyntheticGraph")
  throw Error("Only SyntheticGraph fixture accepted");
const directory = await mkdtemp(join(root, ".qa", "game-graph-volume-")),
  data = join(directory, "data");
await mkdir(data);
await backup(db, join(data, "chesspush.sqlite3"));
db.close();
const server = net.createServer();
await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
const port = server.address().port;
await new Promise((ok) => server.close(ok));
const env = {
  ...process.env,
  CHESSPUSH_TEST_DATA: data,
  CHESSPUSH_CDP: `http://127.0.0.1:${port}`,
  WEBVIEW2_USER_DATA_FOLDER: join(directory, "webview"),
  WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${port}`,
};
const app = spawn(
  join(root, "src-tauri", "target", "debug", "chesspush.exe"),
  [],
  { cwd: root, env, windowsHide: true, stdio: ["ignore", "pipe", "pipe"] },
);
let closed = false;
app.on("close", () => (closed = true));
try {
  for (let i = 0; i < 200; i++) {
    try {
      if ((await fetch(env.CHESSPUSH_CDP + "/json/version")).ok) break;
    } catch {}
    await new Promise((ok) => setTimeout(ok, 150));
  }
  const result = await run(process.execPath, ["tests/graph-volume.mjs"], {
    cwd: root,
    env,
    windowsHide: true,
    timeout: 240000,
    maxBuffer: 4 * 1024 * 1024,
  });
  process.stdout.write(result.stdout);
  process.stderr.write(result.stderr);
  if (process.env.CHESSPUSH_GRAPH_REPORT_CHECK === "1") {
    const report = await run(
      process.execPath,
      ["tests/graph-report-native.mjs"],
      {
        cwd: root,
        env,
        windowsHide: true,
        timeout: 240000,
        maxBuffer: 4 * 1024 * 1024,
      },
    );
    process.stdout.write(report.stdout);
    process.stderr.write(report.stderr);
  }
  if (process.env.CHESSPUSH_GRAPH_INSPECT === "1") {
    console.log(`INSPECT profile ${data}, PID ${app.pid}`);
    await new Promise((ok) => setTimeout(ok, 60000));
  }
} finally {
  if (!closed) {
    await run(
      "powershell.exe",
      [
        "-NoProfile",
        "-Command",
        `$graphOwned=Get-Process -Id ${app.pid} -ErrorAction SilentlyContinue; if($graphOwned){[void]$graphOwned.CloseMainWindow()}`,
      ],
      { windowsHide: true },
    ).catch(() => {});
    for (let i = 0; i < 60 && !closed; i++)
      await new Promise((ok) => setTimeout(ok, 250));
    if (!closed)
      await run("taskkill.exe", ["/PID", String(app.pid), "/T", "/F"], {
        windowsHide: true,
      });
  }
}
console.log("PASS native graph volume; " + directory);

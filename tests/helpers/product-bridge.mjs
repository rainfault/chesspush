import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { resolve, extname, join } from "node:path";
import { chromium } from "@playwright/test";
export async function bridge(config, theme, options = {}) {
  const nativeBrowser = await chromium.connectOverCDP(config.cdp),
    nativePage = nativeBrowser.contexts()[0].pages()[0];
  const native = async (command, args = {}) => {
    const wire = await nativePage.evaluate(
      async ({ command, args }) => {
        try {
          return JSON.stringify({
            ok: true,
            data: await (
              window.__atlasOriginalInvoke ??
              window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__)
            )(command, args),
          });
        } catch (error) {
          return JSON.stringify({ ok: false, error });
        }
      },
      { command, args },
    );
    const result = JSON.parse(wire);
    if (!result.ok) throw result.error;
    return result.data;
  };
  const root = resolve("dist");
  const mime = {
    ".js": "application/javascript",
    ".css": "text/css",
    ".html": "text/html",
    ".json": "application/json",
    ".svg": "image/svg+xml",
    ".ttf": "font/ttf",
    ".png": "image/png",
  };
  const server = createServer(async (req, res) => {
    try {
      const url = new URL(req.url, "http://127.0.0.1"),
        file = resolve(
          root,
          "." +
            decodeURIComponent(
              url.pathname === "/" ? "/index.html" : url.pathname,
            ),
        );
      if (!file.startsWith(root + "\\")) {
        res.writeHead(403);
        res.end();
        return;
      }
      const body = await readFile(file);
      res.setHeader(
        "Content-Type",
        mime[extname(file)] ?? "application/octet-stream",
      );
      res.end(body);
    } catch {
      res.writeHead(404);
      res.end();
    }
  });
  await new Promise((ok) => server.listen(0, "127.0.0.1", ok));
  const browser = await chromium.launch({ channel: "msedge", headless: true });
  const page = await browser.newPage({
    viewport: { width: 1440, height: 960 },
    deviceScaleFactor: options.dpr ?? 1,
    ...(options.videoDirectory
      ? {
          recordVideo: {
            dir: options.videoDirectory,
            size: { width: 1440, height: 960 },
          },
        }
      : {}),
  });
  await page.exposeFunction("__atlasNativeEnvelope", async (command, args) => {
    try {
      return JSON.stringify({ ok: true, data: await native(command, args) });
    } catch (error) {
      return JSON.stringify({ ok: false, error });
    }
  });
  await page.addInitScript(
    ({ theme }) => {
      const callbacks = new Map(),
        listeners = new Map();
      let serial = 1;
      window.isTauri = true;
      window.__atlas = {
        responses: {},
        errors: {},
        waits: {},
        statePatch: {},
        parserHold: false,
        reportMode: null,
      };
      window.__atlasDispatch = (event, payload) => {
        for (const listener of listeners.values())
          if (listener.event === event)
            callbacks.get(listener.handler)?.({
              event,
              id: listener.id,
              payload,
            });
      };
      window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
        unregisterListener(_event, id) {
          listeners.delete(id);
        },
      };
      window.__TAURI_INTERNALS__ = {
        transformCallback(callback, once = false) {
          const id = serial++;
          callbacks.set(
            id,
            once
              ? (value) => {
                  callbacks.delete(id);
                  callback(value);
                }
              : callback,
          );
          return id;
        },
        async invoke(command, args = {}) {
          const m = window.__atlas;
          if (command === "plugin:event|listen") {
            const id = serial++;
            listeners.set(id, { ...args, id });
            return id;
          }
          if (command === "plugin:event|unlisten") {
            listeners.delete(args.eventId);
            return;
          }
          if (command === "plugin:event|emit") {
            window.__atlasDispatch(args.event, args.payload);
            return;
          }
          if (command === "get_theme") return theme;
          if (m.waits[command])
            await new Promise((ok) => setTimeout(ok, m.waits[command]));
          if (m.errors[command])
            throw { code: "atlas_fixture", message: m.errors[command] };
          if (Object.hasOwn(m.responses, command))
            return structuredClone(m.responses[command]);
          if (command === "start_parse" && m.parserHold) {
            m.parserArgs = args;
            return;
          }
          if (command === "cancel_parse" && m.parserHold) return;
          const result = JSON.parse(
            await window.__atlasNativeEnvelope(command, args),
          );
          if (!result.ok) throw result.error;
          if (command === "statistics_state")
            return { ...result.data, ...structuredClone(m.statePatch) };
          return result.data;
        },
      };
    },
    { theme },
  );
  const binding = "__atlasForward" + Math.random().toString(36).slice(2);
  const forwarding = new Set();
  let closing = false,
    closePromise;
  await nativePage.exposeFunction(binding, (event, payload) => {
    if (closing || page.isClosed()) return;
    const pending = page
      .evaluate(
        ({ event, payload }) => {
          if (
            event === "lichess-account-updated" &&
            window.__atlas.responses.account_status
          )
            payload = window.__atlas.responses.account_status;
          window.__atlasDispatch?.(event, payload);
        },
        { event, payload },
      )
      .catch(() => {});
    forwarding.add(pending);
    void pending.finally(() => forwarding.delete(pending));
    return pending;
  });
  const subscriptions = [];
  for (const event of [
    "statistics-updated",
    "statistics-reset",
    "knowledge-updated",
    "parser-progress",
    "parser-finished",
    "lichess-account-updated",
  ]) {
    subscriptions.push(
      await nativePage.evaluate(
        async ({ event, binding }) => {
          const states = (window.__atlasBridgeForwarders ??= {});
          const state = (states[binding] ??= { active: true });
          const handler = window.__TAURI_INTERNALS__.transformCallback((e) => {
            if (!state.active || typeof window[binding] !== "function") return;
            // A queued event may outlive the CDP exposed function during cleanup.
            // Keep that bridge-only race from becoming an unhandled rejection.
            void Promise.resolve(window[binding](event, e.payload)).catch(
              () => {},
            );
          });
          const eventId = await (
            window.__atlasOriginalInvoke ??
            window.__TAURI_INTERNALS__.invoke.bind(window.__TAURI_INTERNALS__)
          )("plugin:event|listen", { event, target: { kind: "Any" }, handler });
          return { event, eventId };
        },
        { event, binding },
      ),
    );
  }
  await page.goto(`http://127.0.0.1:${server.address().port}`);
  await page.waitForFunction(() => document.querySelector(".app-header"));
  return {
    page,
    native,
    browser: {
      close() {
        if (closePromise) return closePromise;
        closing = true;
        closePromise = (async () => {
          const failures = [];
          // Disable callbacks in the native page before unlistening or detaching
          // CDP. The gate also covers already queued native events.
          await nativePage
            .evaluate(
              ({ binding, subscriptions }) => {
                const state = window.__atlasBridgeForwarders?.[binding];
                if (state) state.active = false;
              },
              { binding, subscriptions },
            )
            .catch((error) => failures.push(error));
          for (const { event, eventId } of subscriptions)
            await native("plugin:event|unlisten", { event, eventId }).catch(
              (error) => failures.push(error),
            );
          // Native acknowledgements precede local callback disposal: an event
          // already queued before unlisten can still arrive while we await IPC.
          await nativePage
            .evaluate(async (subscriptions) => {
              await new Promise((ok) => setTimeout(ok, 0));
              for (const { event, eventId } of subscriptions)
                window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener(
                  event,
                  eventId,
                );
            }, subscriptions)
            .catch((error) => failures.push(error));
          await Promise.allSettled([...forwarding]);
          await nativePage
            .evaluate((binding) => {
              delete window.__atlasBridgeForwarders?.[binding];
              delete window[binding];
            }, binding)
            .catch((error) => failures.push(error));
          await page.context().close();
          await browser.close();
          await nativeBrowser.close();
          await new Promise((ok) => server.close(ok));
          if (failures.length)
            throw new AggregateError(
              failures,
              "Native event bridge cleanup failed",
            );
        })();
        return closePromise;
      },
    },
  };
}

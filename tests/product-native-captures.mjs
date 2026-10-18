import { chromium, expect } from "@playwright/test";
import { readFile, writeFile } from "node:fs/promises";
const config = JSON.parse(await readFile(".qa/ui-atlas-tools/current.json")),
  b = await chromium.connectOverCDP(config.cdp),
  p = b.contexts()[0].pages()[0],
  shots = [];
p.setDefaultTimeout(12000);
const nav = (s) =>
    p
      .locator(".app-header")
      .getByRole("button", { name: s, exact: true })
      .click(),
  call = (cmd, args = {}) =>
    p.evaluate(
      ({ cmd, args }) => window.__TAURI_INTERNALS__.invoke(cmd, args),
      { cmd, args },
    );
const shot = async (theme, size, name) => {
  const path = `after/native-${theme}-${size.width}x${size.height}-${name}.png`;
  await p.screenshot({
    path: "screens/product-ux/" + path,
    animations: "disabled",
  });
  shots.push({
    path,
    theme,
    viewport: size,
    ...(await p.evaluate(() => ({
      dpr: devicePixelRatio,
      viewportWidth: innerWidth,
      viewportHeight: innerHeight,
    }))),
    provenance:
      "native Tauri/WebView2, final debug binary; isolated 4 played games/8 saved Stockfish runs",
  });
};
try {
  for (const theme of ["light", "dark"]) {
    await call("set_theme", { theme });
    await p.reload();
    await expect(p.locator(".app-header")).toBeVisible();
    for (const size of [
      { width: 1440, height: 960 },
      { width: 900, height: 700 },
      { width: 1920, height: 1080 },
    ]) {
      await p.setViewportSize(size);
      await nav("Главная");
      await shot(theme, size, "home");
      await nav("Моя игра");
      await p.getByRole("tab", { name: "Карта игры", exact: true }).click();
      await expect(p.locator(".continuation-chip").first()).toBeEnabled();
      if (
        await p
          .getByRole("button", { name: "К началу", exact: true })
          .isEnabled()
      )
        await p.getByRole("button", { name: "К началу", exact: true }).click();
      await expect(
        p.getByRole("region", { name: "Карта игры", exact: true }),
      ).toHaveAttribute("aria-busy", "false");
      await shot(theme, size, "map");
      await p.getByRole("tab", { name: "Обзор", exact: true }).click();
      await expect(p.locator(".practice-overview")).toBeVisible();
      await shot(theme, size, "overview");
      await nav("Репертуар");
      const back = p.getByRole("button", {
        name: "К репертуарам",
        exact: true,
      });
      if (await back.isVisible()) await back.click();
      await p
        .getByRole("main", { name: "Репертуар", exact: true })
        .getByRole("button")
        .filter({ hasText: "Репертуар для съёмки" })
        .click();
      await expect(
        p.getByLabel("Варианты репертуара", { exact: true }),
      ).toBeVisible();
      await shot(theme, size, "editor");
    }
  }
} finally {
  await b.close();
  await writeFile(
    "screens/product-ux/native-captures.json",
    JSON.stringify(shots, null, 2),
  );
}
console.log(JSON.stringify({ nativeShots: shots.length }));

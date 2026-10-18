import { chromium } from "@playwright/test";
import { writeFile, mkdir } from "node:fs/promises";
await mkdir(".qa", { recursive: true });
const browser = await chromium.launch({ channel: "msedge", headless: true });
try {
  const page = await browser.newPage({
    viewport: { width: 1100, height: 800 },
  });
  await page.goto("http://127.0.0.1:1420/tests/graph-renderers.html");
  await page.waitForFunction(
    () => typeof window.runRendererComparison === "function",
  );
  const results = [];
  for (const n of [100, 500, 1500]) {
    const rows = await page.evaluate((n) => window.runRendererComparison(n), n);
    results.push(...rows);
    console.log(JSON.stringify(rows));
  }
  await writeFile(
    ".qa/graph-renderers.json",
    JSON.stringify(
      {
        kind: "synthetic cyclic binary graph; Edge headless; 3-frame paint and 20 pan frames, not FPS guarantee",
        results,
      },
      null,
      2,
    ),
  );
} finally {
  await browser.close();
}

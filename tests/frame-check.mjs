import { chromium } from "@playwright/test";
const browser = await chromium.connectOverCDP(
  process.env.CHESSPUSH_CDP ?? "http://127.0.0.1:9223",
);
const page = browser.contexts()[0].pages()[0];
await page.getByRole("button", { name: "Базы", exact: true }).click();
await page.bringToFront();
const result = await page.evaluate(
  () =>
    new Promise((resolve) => {
      const timestamps = [];
      function frame(time) {
        timestamps.push(time);
        if (timestamps.length < 240) {
          requestAnimationFrame(frame);
          return;
        }
        const gaps = timestamps
          .slice(1)
          .map((v, i) => v - timestamps[i])
          .sort((a, b) => a - b);
        resolve({
          fps:
            (1000 * (timestamps.length - 1)) /
            (timestamps.at(-1) - timestamps[0]),
          median_ms: gaps[Math.floor(gaps.length / 2)],
          p95_ms: gaps[Math.floor(gaps.length * 0.95)],
          dpr: devicePixelRatio,
        });
      }
      requestAnimationFrame(frame);
    }),
);
console.log(JSON.stringify(result));
await browser.close();

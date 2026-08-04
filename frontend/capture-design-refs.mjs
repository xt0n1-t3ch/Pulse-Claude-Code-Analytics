// Captures the live Pulse dev UI to PNGs used as design-review / Image Gen references.
// Dev helper only: reads the running Vite dev server, writes into ../.design-refs/.
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";

const OUT = "../.design-refs/";
mkdirSync(OUT, { recursive: true });

const VIEWS = [
  ["Sessions", "sessions-current.png"],
  ["Context", "context-current.png"],
  ["Home", "dashboard-current.png"],
];

const browser = await chromium.launch();
const page = await browser.newPage({ viewport: { width: 1440, height: 1024 } });
await page.goto("http://localhost:1420", { waitUntil: "networkidle" });
await page.waitForTimeout(3000);

for (const [label, file] of VIEWS) {
  await page.getByRole("button", { name: label, exact: true }).click();
  await page.waitForTimeout(2000);
  await page.screenshot({ path: OUT + file });
  console.log("captured", file);
}

await browser.close();

import { defineConfig } from "@playwright/test";

const cdpEndpoint = process.env.PULSE_TAURI_CDP_URL;
if (!cdpEndpoint) {
  throw new Error("PULSE_TAURI_CDP_URL is required; start a repo-owned Pulse debug build first.");
}

export default defineConfig({
  testDir: "tests/e2e/tauri",
  outputDir: "../test-results/playwright-tauri",
  fullyParallel: false,
  forbidOnly: true,
  retries: 0,
  reporter: [["line"]],
  use: {
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
  },
});

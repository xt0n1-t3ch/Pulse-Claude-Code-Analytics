import { defineConfig, devices } from "@playwright/test";

const baseURL = "http://127.0.0.1:1420";

export default defineConfig({
  testDir: "tests/e2e/browser",
  outputDir: "../test-results/playwright",
  fullyParallel: false,
  forbidOnly: true,
  retries: 0,
  reporter: [["line"]],
  use: {
    baseURL,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
    ...devices["Desktop Chrome"],
  },
  webServer: {
    command: "bun run dev",
    url: baseURL,
    // Local visual QA keeps the authenticated bridge alive; CI still starts
    // an isolated server so a stale external process can never make it green.
    reuseExistingServer: !process.env.CI,
    stdout: "pipe",
    stderr: "pipe",
    timeout: 60_000,
  },
});

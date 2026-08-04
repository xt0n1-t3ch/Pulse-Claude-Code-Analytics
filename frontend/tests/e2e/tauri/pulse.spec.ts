import { chromium, expect, test } from "@playwright/test";

test("repo-owned Tauri runtime exposes the same Home contract", async () => {
  const endpoint = process.env.PULSE_TAURI_CDP_URL;
  if (!endpoint) throw new Error("PULSE_TAURI_CDP_URL is required");

  const browser = await chromium.connectOverCDP(endpoint);
  const context = browser.contexts()[0];
  const page = context?.pages()[0];
  if (!page) throw new Error("repo-owned Pulse WebView did not expose a page");

  await expect(page.getByRole("navigation", { name: "Primary navigation" })).toBeVisible();
  const notificationBox = await page.getByRole("button", { name: "Notifications" }).boundingBox();
  const themeBox = await page.getByRole("button", { name: "Toggle theme" }).boundingBox();
  expect(notificationBox).not.toBeNull();
  expect(themeBox).not.toBeNull();
  expect(notificationBox!.width).toBe(themeBox!.width);
  expect(notificationBox!.height).toBe(themeBox!.height);
  expect(notificationBox!.y).toBe(themeBox!.y);
  await expect(page.getByRole("heading", { name: "Live workspace" })).toBeVisible();
  await expect(page.getByText("No authenticated usage source", { exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Provider limits" })).toHaveCount(0);
  await expect(page.locator("[aria-label='Provider allowances']")).toHaveCount(0);
  await expect(page.locator("body")).not.toContainText(">=$");
});

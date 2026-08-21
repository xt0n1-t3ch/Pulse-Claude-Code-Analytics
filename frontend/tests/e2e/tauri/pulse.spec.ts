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

async function screenPosition(page: import("@playwright/test").Page): Promise<{ x: number; y: number }> {
  return page.evaluate(() => ({ x: window.screenX, y: window.screenY }));
}

async function dragFrom(
  page: import("@playwright/test").Page,
  locator: import("@playwright/test").Locator,
): Promise<void> {
  const box = await locator.boundingBox();
  if (!box) throw new Error("drag target is not visible");
  const x = box.x + box.width / 2;
  const y = box.y + box.height / 2;
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + 120, y + 40, { steps: 8 });
  await page.mouse.up();
}

test("frameless titlebar drag is native and excludes controls", async () => {
  const endpoint = process.env.PULSE_TAURI_CDP_URL;
  if (!endpoint) throw new Error("PULSE_TAURI_CDP_URL is required");

  const browser = await chromium.connectOverCDP(endpoint);
  const context = browser.contexts()[0];
  const page = context?.pages()[0];
  if (!page) throw new Error("repo-owned Pulse WebView did not expose a page");

  const header = page.locator("header.app-header");
  await expect(header).toBeVisible();
  const beforeDrag = await screenPosition(page);
  await dragFrom(page, header);
  await page.waitForTimeout(250);
  const afterDrag = await screenPosition(page);
  expect(afterDrag).not.toEqual(beforeDrag);

  for (const target of [
    page.locator(".app-nav button").first(),
    page.getByRole("button", { name: "Notifications" }),
    page.getByRole("button", { name: "Toggle theme" }),
  ]) {
    const beforeControl = await screenPosition(page);
    await dragFrom(page, target);
    await page.waitForTimeout(150);
    expect(await screenPosition(page)).toEqual(beforeControl);
  }
});

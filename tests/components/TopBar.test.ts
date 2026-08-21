import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const topBarSource = readFileSync(resolve(process.cwd(), "src/components/TopBar.svelte"), "utf8");
const notificationSource = readFileSync(resolve(process.cwd(), "src/components/NotificationCenter.svelte"), "utf8");

function capabilityPermissions(): unknown[] {
  const config = JSON.parse(readFileSync(resolve(process.cwd(), "../src-tauri/capabilities/default.json"), "utf8")) as {
    permissions: unknown[];
  };
  return config.permissions;
}

describe("TopBar frameless window behavior", () => {
  it("grants the native permission required by startDragging", () => {
    expect(capabilityPermissions()).toContain("core:window:allow-start-dragging");
  });

  it("uses one explicit drag handler with a control exclusion boundary", () => {
    expect(topBarSource).toContain('import { getCurrentWindow } from "@tauri-apps/api/window";');
    expect(topBarSource).toContain("startDragging()");
    expect(topBarSource).toContain("minimize()");
    expect(topBarSource).toContain("maximize()");
    expect(topBarSource).toContain("unmaximize()");
    expect(topBarSource).toContain("close()");
    expect(topBarSource).toContain('target.closest(".app-nav, .header-actions")');
    expect(topBarSource).toContain("event.detail === 2");
    expect(topBarSource).not.toContain("data-tauri-drag-region");
    expect(topBarSource).not.toContain("-webkit-app-region");
    expect(notificationSource).not.toContain("-webkit-app-region");
  });

  it("keeps every navigation and native control inside the non-draggable boundary", () => {
    expect(topBarSource).toContain("{item.label}");
    expect(topBarSource).toContain('class="app-nav"');
    expect(notificationSource).toContain('aria-label={unreadCount > 0 ? `Notifications');
    for (const label of ["Toggle theme", "Minimize", "Maximize", "Close"]) {
      expect(topBarSource).toContain(`aria-label="${label}`);
    }
    expect(topBarSource).toContain('class="header-actions"');
  });
});

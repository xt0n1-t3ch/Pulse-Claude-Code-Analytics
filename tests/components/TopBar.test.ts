import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render } from "@testing-library/svelte";
import { getCurrentWindow } from "@tauri-apps/api/window";
import TopBar from "@/components/TopBar.svelte";

const startDragging = vi.fn(async () => undefined);
const minimize = vi.fn(async () => undefined);
const maximize = vi.fn(async () => undefined);
const unmaximize = vi.fn(async () => undefined);
const isMaximized = vi.fn(async () => false);
const close = vi.fn(async () => undefined);

const appWindow = {
  startDragging,
  minimize,
  maximize,
  unmaximize,
  isMaximized,
  close,
};

describe("TopBar frameless window behavior", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getCurrentWindow).mockReturnValue(appWindow as never);
  });

  it("grants the native permission required by the drag consumer", () => {
    const capability = JSON.parse(
      readFileSync(resolve(process.cwd(), "../src-tauri/capabilities/default.json"), "utf8"),
    ) as { permissions: unknown[] };

    expect(capability.permissions).toContain("core:window:allow-start-dragging");
  });

  it("starts native dragging only from non-control titlebar space", async () => {
    const { container, getByRole } = render(TopBar, { onToggleTheme: vi.fn() });
    const header = container.querySelector("header.app-header");
    const brand = container.querySelector(".brand");
    expect(header).not.toBeNull();
    expect(brand).not.toBeNull();

    await fireEvent.mouseDown(brand!, { button: 0, detail: 1 });
    expect(startDragging).toHaveBeenCalledTimes(1);

    for (const control of [
      getByRole("button", { name: "Home" }),
      getByRole("button", { name: "Notifications" }),
      getByRole("button", { name: "Toggle theme" }),
      getByRole("button", { name: "Minimize" }),
      getByRole("button", { name: "Maximize" }),
      getByRole("button", { name: "Close" }),
    ]) {
      await fireEvent.mouseDown(control, { button: 0, detail: 1 });
    }
    await fireEvent.mouseDown(header!, { button: 2, detail: 1 });

    expect(startDragging).toHaveBeenCalledTimes(1);
  });

  it("toggles maximize on a titlebar double-click without starting a drag", async () => {
    const { container } = render(TopBar, { onToggleTheme: vi.fn() });
    const brand = container.querySelector(".brand");
    expect(brand).not.toBeNull();

    await fireEvent.mouseDown(brand!, { button: 0, detail: 2 });
    await Promise.resolve();

    expect(startDragging).not.toHaveBeenCalled();
    expect(isMaximized).toHaveBeenCalledOnce();
    expect(maximize).toHaveBeenCalledOnce();
    expect(unmaximize).not.toHaveBeenCalled();
  });
});

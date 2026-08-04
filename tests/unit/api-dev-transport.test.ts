import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

describe("browser development transport", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("posts commands to the same-origin real backend proxy without fixture data", async () => {
    const globalRecord = globalThis as Record<string, unknown>;
    const windowRecord = window as unknown as Record<string, unknown>;
    const savedGlobalInternals = globalRecord.__TAURI_INTERNALS__;
    const savedWindowInternals = windowRecord.__TAURI_INTERNALS__;
    delete globalRecord.__TAURI_INTERNALS__;
    delete windowRecord.__TAURI_INTERNALS__;
    vi.resetModules();

    const fetchMock = vi.fn(async () => new Response(JSON.stringify({
      version: "1.7.0",
      uptime_seconds: 1,
      discord_status: "Connected",
      discord_enabled: true,
    }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    }));
    vi.stubGlobal("fetch", fetchMock);

    const { getHealth } = await import("../../frontend/src/lib/api");
    await getHealth();

    expect(fetchMock).toHaveBeenCalledWith("/__pulse_api", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ command: "get_health", args: {} }),
    });

    globalRecord.__TAURI_INTERNALS__ = savedGlobalInternals;
    windowRecord.__TAURI_INTERNALS__ = savedWindowInternals;
  });
});

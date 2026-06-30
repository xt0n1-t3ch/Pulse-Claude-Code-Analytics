import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import type { AnalyticsSummary, HealthResponse } from "@/lib/api";

const summary: AnalyticsSummary = {
  total_sessions: 42,
  total_cost: 100,
  total_tokens: 5_000_000,
  total_cache_read: 2_000_000,
  total_cache_write: 200_000,
  avg_duration_secs: 1200,
  avg_tokens_per_session: 120_000,
  avg_cost_per_session: 2.38,
  top_project: "pulse",
  top_model: "Claude Opus 4.8",
  days_tracked: 30,
};

const getDbSize = vi.fn(async () => 5 * 1024 * 1024);
const getAnalyticsSummary = vi.fn(async () => summary);
const getPlanInfo = vi.fn(async () => ({ provider: "claude", plan_key: "", plan_name: "Max 20x", detected: true }));
const setPlanOverride = vi.fn(async () => undefined);
const clearHistory = vi.fn(async () => 7);
const exportAllData = vi.fn(async () => ({ ok: true }));

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getDbSize: () => getDbSize(),
    getAnalyticsSummary: () => getAnalyticsSummary(),
    getPlanInfo: () => getPlanInfo(),
    setPlanOverride: (plan: string) => setPlanOverride(plan),
    clearHistory: () => clearHistory(),
    exportAllData: () => exportAllData(),
  };
});

const healthFixture: HealthResponse = {
  version: "0.1.0",
  uptime_seconds: 120,
  discord_status: "Connected",
  discord_enabled: true,
};

describe("Settings.svelte", () => {
  beforeEach(async () => {
    getDbSize.mockClear();
    getAnalyticsSummary.mockClear();
    clearHistory.mockClear();
    const { health, rateLimits, planInfo } = await import("@/lib/stores");
    health.set(healthFixture);
    rateLimits.set(null);
    planInfo.set({ provider: "claude", plan_key: "", plan_name: "Max 20x", detected: true });
  });

  it("mounts and shows the identity masthead plus configuration controls", async () => {
    const Settings = (await import("@/views/Settings.svelte")).default;
    const { container, getByText } = render(Settings, {
      props: { onToggleTheme: () => {}, currentTheme: "dark" },
    });
    await tick();

    expect(getByText("Settings")).toBeTruthy();
    expect(getByText("Data Sources")).toBeTruthy();
    expect(getByText("Data Management")).toBeTruthy();
    expect(container.querySelectorAll(".rail-ctrl").length).toBe(3);
  });

  it("reflects a manual plan override on the select instead of reverting to auto", async () => {
    const { planInfo } = await import("@/lib/stores");
    planInfo.set({ provider: "claude", plan_key: "max_20x", plan_name: "Max 20x", detected: false });
    const Settings = (await import("@/views/Settings.svelte")).default;
    const { container } = render(Settings, {
      props: { onToggleTheme: () => {}, currentTheme: "dark" },
    });
    await tick();

    const planSelect = container.querySelector('[aria-label="Plan override"]');
    expect(planSelect).toBeTruthy();
    expect(planSelect?.textContent).toContain("Max 20x");
    expect(planSelect?.textContent).not.toContain("Auto-detect");
  });

  it("loads the database size and session total from the api layer", async () => {
    const Settings = (await import("@/views/Settings.svelte")).default;
    const { getByText } = render(Settings, {
      props: { onToggleTheme: () => {}, currentTheme: "dark" },
    });
    await tick();

    await waitFor(() => expect(getDbSize).toHaveBeenCalled());
    await waitFor(() => expect(getByText("5.0 MB")).toBeTruthy());
    expect(getByText("42")).toBeTruthy();
  });

  it("requires a confirm step before clearing history", async () => {
    const Settings = (await import("@/views/Settings.svelte")).default;
    const { getByText } = render(Settings, {
      props: { onToggleTheme: () => {}, currentTheme: "dark" },
    });
    await tick();

    await fireEvent.click(getByText("Clear history"));
    expect(clearHistory).not.toHaveBeenCalled();

    await fireEvent.click(getByText("Confirm clear"));
    await waitFor(() => expect(clearHistory).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(getByText("Cleared 7 sessions")).toBeTruthy());
  });

  it("toggles the theme through the appearance control", async () => {
    const onToggleTheme = vi.fn();
    const Settings = (await import("@/views/Settings.svelte")).default;
    const { getByText } = render(Settings, {
      props: { onToggleTheme, currentTheme: "dark" },
    });
    await tick();

    await fireEvent.click(getByText("Light"));
    expect(onToggleTheme).toHaveBeenCalledTimes(1);
  });
});

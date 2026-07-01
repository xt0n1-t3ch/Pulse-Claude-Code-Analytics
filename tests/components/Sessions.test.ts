import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import type { SessionInfo, HistoricalSession, AnalyticsSummary } from "@/lib/api";

function makeSession(id: string, project: string, cost: number): SessionInfo {
  return {
    session_id: id,
    session_name: null,
    project,
    model: "Claude Opus 4.8",
    model_id: "claude-opus-4-8",
    provider: "claude",
    context_window: "200K",
    cost,
    tokens: 120_000,
    input_tokens: 40_000,
    output_tokens: 20_000,
    cache_write_tokens: 10_000,
    cache_read_tokens: 50_000,
    branch: "main",
    activity: "Editing",
    activity_target: "stores.ts",
    effort: "High",
    effort_explicit: true,
    is_idle: false,
    started_at: "2026-05-28T10:00:00Z",
    duration_secs: 600,
    has_thinking: true,
    workflow_label: null,
    subagent_count: 0,
    subagents: [],
    tokens_per_sec: 42,
    input_cost: cost * 0.3,
    output_cost: cost * 0.4,
    cache_write_cost: cost * 0.2,
    cache_read_cost: cost * 0.1,
    speed: "standard",
    fast: false,
    service_tier: null,
    app_name: null,
  };
}

function hist(id: string, project: string, cost: number): HistoricalSession {
  return {
    id,
    session_name: null,
    project,
    model: "Claude Opus 4.8",
    model_id: "claude-opus-4-8",
    context_window: "200K",
    branch: "main",
    effort: "High",
    started_at: "2026-05-20T10:00:00Z",
    ended_at: "2026-05-20T10:30:00Z",
    duration_secs: 1800,
    total_cost: cost,
    input_tokens: 50_000,
    output_tokens: 20_000,
    cache_write_tokens: 10_000,
    cache_read_tokens: 100_000,
    total_tokens: 180_000,
    input_cost: cost * 0.3,
    output_cost: cost * 0.4,
    cache_write_cost: cost * 0.2,
    cache_read_cost: cost * 0.1,
    has_thinking: false,
    workflow_label: null,
    subagent_count: 1,
    is_active: false,
  };
}

const summary: AnalyticsSummary = {
  total_sessions: 5,
  total_cost: 25,
  total_tokens: 2_000_000,
  total_cache_read: 1_000_000,
  total_cache_write: 100_000,
  avg_duration_secs: 1200,
  avg_tokens_per_session: 400_000,
  avg_cost_per_session: 5,
  top_project: "pulse",
  top_model: "Claude Opus 4.8",
  days_tracked: 30,
};

const getAnalyticsSummary = vi.fn(async () => summary);
const getTopSessions = vi.fn(async () => [] as HistoricalSession[]);
const getSessionHistory = vi.fn(async () => [hist("h1", "pulse", 6), hist("h2", "other", 4)]);
const getSessionHistoryFiltered = vi.fn(async () => [] as HistoricalSession[]);
const searchSessions = vi.fn(async () => [] as HistoricalSession[]);

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getAnalyticsSummary: () => getAnalyticsSummary(),
    getTopSessions: () => getTopSessions(),
    getSessionHistory: () => getSessionHistory(),
    getSessionHistoryFiltered: () => getSessionHistoryFiltered(),
    searchSessions: () => searchSessions(),
  };
});

describe("Sessions.svelte", () => {
  beforeEach(() => {
    getAnalyticsSummary.mockClear();
    getTopSessions.mockClear();
    getSessionHistory.mockClear();
  });

  it("mounts, shows the KPI tiles, and lists live session rows", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("s1", "pulse", 3), makeSession("s2", "other", 2)]);

    const Sessions = (await import("@/views/Sessions.svelte")).default;
    const { container, getByText } = render(Sessions);
    await tick();

    const labels = [...container.querySelectorAll(".stat-label")].map((e) => e.textContent?.trim());
    expect(labels).toEqual(["Total Tokens", "Total Cost", "Avg Duration", "Avg Cost/Session"]);

    await waitFor(() => {
      expect(container.querySelectorAll(".session-list .session-card").length).toBe(2);
    });
    expect(getByText("2 active")).toBeTruthy();
  });

  it("loads the history table from the api layer", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([]);

    const Sessions = (await import("@/views/Sessions.svelte")).default;
    const { container, getByText } = render(Sessions);
    await tick();

    await waitFor(() => expect(getSessionHistory).toHaveBeenCalled());
    await waitFor(() => {
      expect(container.querySelectorAll(".ht-row").length).toBe(2);
    });
    expect(getByText("Session History")).toBeTruthy();
  });

  it("renders the most-costly sessions sorted by cost without mutating state", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([]);
    getTopSessions.mockResolvedValueOnce([
      hist("low", "alpha", 2),
      hist("high", "beta", 9),
      hist("mid", "gamma", 5),
    ]);

    const Sessions = (await import("@/views/Sessions.svelte")).default;
    const { container } = render(Sessions);
    await tick();

    await waitFor(() => {
      expect(container.querySelectorAll(".top-row").length).toBe(3);
    });
    const projects = [...container.querySelectorAll(".top-row .project")].map((e) => e.textContent?.trim());
    expect(projects).toEqual(["beta", "gamma", "alpha"]);
  });
});

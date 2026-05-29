import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import type {
  MetricsResponse,
  AnalyticsSummary,
  HistoricalSession,
  CostForecast,
  HourlyActivity,
  DailyStat,
  ProjectStat,
} from "@/lib/api";

const metricsFixture: MetricsResponse = {
  total_cost: 12.5,
  input_tokens: 400_000,
  pure_input_tokens: 300_000,
  output_tokens: 120_000,
  cache_write_tokens: 80_000,
  cache_read_tokens: 600_000,
  total_tokens: 1_100_000,
  session_count: 4,
  input_cost: 4,
  output_cost: 5,
  cache_write_cost: 2,
  cache_read_cost: 1.5,
  cache_hit_ratio: 66,
  models: [
    { model: "Claude Opus 4.8", sessions: 3, cost: 10, tokens: 900_000 },
    { model: "Claude Sonnet 4.6", sessions: 1, cost: 2.5, tokens: 200_000 },
  ],
};

const summary: AnalyticsSummary = {
  total_sessions: 4,
  total_cost: 12.5,
  total_tokens: 1_100_000,
  total_cache_read: 600_000,
  total_cache_write: 80_000,
  avg_duration_secs: 900,
  avg_tokens_per_session: 275_000,
  avg_cost_per_session: 3.125,
  top_project: "pulse",
  top_model: "Claude Opus 4.8",
  days_tracked: 14,
};

function hist(id: string, project: string, cost: number): HistoricalSession {
  return {
    id,
    session_name: null,
    project,
    model: "Claude Opus 4.8",
    model_id: "claude-opus-4-8",
    context_window: "200K",
    branch: null,
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
    subagent_count: 0,
    is_active: false,
  };
}

const forecast: CostForecast = {
  spent_this_month: 30,
  days_elapsed: 10,
  days_in_month: 31,
  projected_monthly: 93,
  daily_average: 3,
};

const hourly: HourlyActivity[] = [
  { hour: 9, session_count: 3, total_cost: 5 },
  { hour: 14, session_count: 2, total_cost: 4 },
];

const daily: DailyStat[] = [
  { date: "2026-05-19", project: "pulse", model: "Claude Opus 4.8", session_count: 2, total_cost: 6, total_tokens: 500_000, input_tokens: 100_000, output_tokens: 50_000, cache_write_tokens: 40_000, cache_read_tokens: 310_000 },
  { date: "2026-05-20", project: "pulse", model: "Claude Opus 4.8", session_count: 2, total_cost: 6.5, total_tokens: 600_000, input_tokens: 200_000, output_tokens: 70_000, cache_write_tokens: 40_000, cache_read_tokens: 290_000 },
];

const projects: ProjectStat[] = [
  { project: "pulse", session_count: 3, total_cost: 10, total_tokens: 900_000, avg_session_cost: 3.33, avg_duration_secs: 1200, cache_read_tokens: 500_000, cache_write_tokens: 60_000, top_model: "Claude Opus 4.8" },
  { project: "other", session_count: 1, total_cost: 2.5, total_tokens: 200_000, avg_session_cost: 2.5, avg_duration_secs: 600, cache_read_tokens: 100_000, cache_write_tokens: 20_000, top_model: "Claude Sonnet 4.6" },
];

const getAnalyticsSummary = vi.fn(async () => summary);
const getSessionHistory = vi.fn(async () => [hist("h1", "pulse", 6), hist("h2", "other", 4)]);
const getCostForecast = vi.fn(async () => forecast);
const getHourlyActivity = vi.fn(async () => hourly);
const getDailyStats = vi.fn(async () => daily);
const getProjectStats = vi.fn(async () => projects);

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getAnalyticsSummary: () => getAnalyticsSummary(),
    getSessionHistory: () => getSessionHistory(),
    getCostForecast: () => getCostForecast(),
    getHourlyActivity: () => getHourlyActivity(),
    getDailyStats: () => getDailyStats(),
    getProjectStats: () => getProjectStats(),
  };
});

describe("Dashboard.svelte", () => {
  beforeEach(async () => {
    const { metrics, sessions, planInfo, rateLimits } = await import("@/lib/stores");
    metrics.set(metricsFixture);
    sessions.set([]);
    planInfo.set({ provider: "claude", plan_name: "Max 20x ($200/mo)", detected: true });
    rateLimits.set({
      provider: "claude",
      five_hour_pct: 40,
      five_hour_resets: "2026-05-28T18:00:00Z",
      five_hour_label: "Current session",
      five_hour_window_minutes: 300,
      seven_day_pct: 55,
      seven_day_resets: "2026-06-01T00:00:00Z",
      seven_day_label: "Weekly",
      seven_day_window_minutes: null,
      sonnet_pct: 12,
      sonnet_resets: "2026-06-01T00:00:00Z",
      extra_enabled: true,
      extra_limit: 50,
      extra_used: 7.5,
      extra_pct: 15,
      source: "Anthropic usage API",
    });
  });

  it("mounts and shows the four KPI tiles with metric values", async () => {
    const Dashboard = (await import("@/views/Dashboard.svelte")).default;
    const { container } = render(Dashboard);
    await tick();

    const labels = [...container.querySelectorAll(".stats-row .stat-label")].map((e) => e.textContent?.trim());
    expect(labels).toEqual(["Total Cost", "Total Tokens", "Sessions", "Avg Duration"]);
    const values = [...container.querySelectorAll(".stats-row .stat-value")].map((e) => e.textContent?.trim());
    expect(values[0]).toBe("$12.50");
    expect(values[1]).toBe("1.1M");
    expect(values[2]).toBe("4");
  });

  it("renders the cost breakdown that reconciles to the estimated total", async () => {
    const Dashboard = (await import("@/views/Dashboard.svelte")).default;
    const { container, getByText } = render(Dashboard);
    await tick();

    await waitFor(() => expect(getByText("Cost Breakdown")).toBeTruthy());
    const total = metricsFixture.input_cost + metricsFixture.output_cost + metricsFixture.cache_write_cost + metricsFixture.cache_read_cost;
    const totalRow = container.querySelector(".bd-row.total .bd-val");
    expect(totalRow?.textContent?.trim()).toBe("$" + total.toFixed(2));
  });

  it("shows the plan usage limits and model distribution from the store + history", async () => {
    const Dashboard = (await import("@/views/Dashboard.svelte")).default;
    const { container, getByText } = render(Dashboard);
    await tick();

    await waitFor(() => expect(getByText(/Plan Usage Limits/)).toBeTruthy());
    expect(getByText("Model Distribution")).toBeTruthy();
    const modelNames = [...container.querySelectorAll(".model-list .model-name")].map((e) => e.textContent?.trim());
    expect(modelNames).toContain("Claude Opus 4.8");
    expect(modelNames).toContain("Claude Sonnet 4.6");
  });
});

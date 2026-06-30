import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import type { HistoricalSession, CostForecast, BudgetStatus } from "@/lib/api";

vi.mock("@/components/Chart.svelte", async () => ({
  default: (await import("../fixtures/ChartStub.svelte")).default,
}));

function hist(id: string, project: string, parts: { input: number; output: number; cacheW: number; cacheR: number }): HistoricalSession {
  const total = parts.input + parts.output + parts.cacheW + parts.cacheR;
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
    total_cost: total,
    input_tokens: 50_000,
    output_tokens: 20_000,
    cache_write_tokens: 10_000,
    cache_read_tokens: 100_000,
    total_tokens: 180_000,
    input_cost: parts.input,
    output_cost: parts.output,
    cache_write_cost: parts.cacheW,
    cache_read_cost: parts.cacheR,
    has_thinking: false,
    subagent_count: 0,
    is_active: false,
  };
}

const histList = [
  hist("h1", "pulse", { input: 3, output: 4, cacheW: 2, cacheR: 1 }),
  hist("h2", "other", { input: 1.5, output: 2, cacheW: 1, cacheR: 0.5 }),
];

const forecast: CostForecast = {
  spent_this_month: 30,
  days_elapsed: 10,
  days_in_month: 31,
  projected_monthly: 93,
  daily_average: 3,
};

const budget: BudgetStatus = {
  monthly_budget: 100,
  alert_threshold_pct: 80,
  spent_this_month: 30,
  pct_used: 30,
  projected_monthly: 93,
  over_budget: false,
};

const getSessionHistory = vi.fn(async () => histList);
const getCostForecast = vi.fn(async () => forecast);
const getBudgetStatus = vi.fn(async () => budget);
const setBudget = vi.fn(async () => undefined);

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getSessionHistory: () => getSessionHistory(),
    getCostForecast: () => getCostForecast(),
    getBudgetStatus: () => getBudgetStatus(),
    setBudget: () => setBudget(),
  };
});

describe("Costs.svelte", () => {
  beforeEach(async () => {
    getSessionHistory.mockClear();
    getCostForecast.mockClear();
    getBudgetStatus.mockClear();
    const { sessions } = await import("@/lib/stores");
    sessions.set([]);
  });

  it("mounts and shows the four cost KPI tiles", async () => {
    const Costs = (await import("@/views/Costs.svelte")).default;
    const { container } = render(Costs);
    await tick();

    await waitFor(() => expect(getSessionHistory).toHaveBeenCalled());
    const labels = [...container.querySelectorAll(".stat-label")].map((e) => e.textContent?.trim());
    expect(labels).toEqual(["Total Spent (30d)", "Avg / Session", "Cost / 1M Tokens", "Cache Savings"]);
  });

  it("renders a Cost by Type breakdown whose legend reconciles to the per-component total", async () => {
    const Costs = (await import("@/views/Costs.svelte")).default;
    const { container, getByText } = render(Costs);
    await tick();

    await waitFor(() => expect(container.querySelector(".cost-type-bar")).not.toBeNull());

    const inputCost = histList.reduce((s, h) => s + h.input_cost, 0);
    const outputCost = histList.reduce((s, h) => s + h.output_cost, 0);
    const cacheWCost = histList.reduce((s, h) => s + h.cache_write_cost, 0);
    const cacheRCost = histList.reduce((s, h) => s + h.cache_read_cost, 0);

    const vals = [...container.querySelectorAll(".cost-type-legend .ct-val")].map((e) => e.textContent?.trim());
    expect(vals).toEqual([
      "$" + inputCost.toFixed(2),
      "$" + outputCost.toFixed(2),
      "$" + cacheWCost.toFixed(2),
      "$" + cacheRCost.toFixed(2),
    ]);

    const legendTotal = inputCost + outputCost + cacheWCost + cacheRCost;
    const rowsTotal = histList.reduce((s, h) => s + h.total_cost, 0);
    expect(legendTotal).toBeCloseTo(rowsTotal, 5);
    expect(getByText("Session Details")).toBeTruthy();
  });

  it("shows budget tracking from the budget status fixture", async () => {
    const Costs = (await import("@/views/Costs.svelte")).default;
    const { getByText } = render(Costs);
    await tick();

    await waitFor(() => expect(getByText("Budget Tracking")).toBeTruthy());
    expect(getByText("Change Budget")).toBeTruthy();
    expect(getByText("$30.00 / $100.00")).toBeTruthy();
  });
});

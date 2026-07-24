import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import type { HistoricalSession, CostForecast, BudgetStatus, CostTotals } from "@/lib/api";

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
    workflow_label: null,
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

/** Window-wide aggregate matching `histList`, since the KPIs read from here
 *  rather than summing the visible page. */
const totals: CostTotals = {
  days: 30,
  sessions: histList.length,
  total_cost: histList.reduce((s, h) => s + h.total_cost, 0),
  input_cost: histList.reduce((s, h) => s + h.input_cost, 0),
  output_cost: histList.reduce((s, h) => s + h.output_cost, 0),
  cache_write_cost: histList.reduce((s, h) => s + h.cache_write_cost, 0),
  cache_read_cost: histList.reduce((s, h) => s + h.cache_read_cost, 0),
  total_tokens: histList.reduce((s, h) => s + h.total_tokens, 0),
  cache_read_tokens: histList.reduce((s, h) => s + h.cache_read_tokens, 0),
  pure_input_tokens: 40_000,
  by_model: [{ label: "Claude Opus 4.8", cost: histList.reduce((s, h) => s + h.total_cost, 0), sessions: 2 }],
  by_project: [
    { label: "pulse", cost: 10, sessions: 1 },
    { label: "other", cost: 5, sessions: 1 },
  ],
};
const getCostTotals = vi.fn(async () => totals);

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getSessionHistory: () => getSessionHistory(),
    getCostForecast: () => getCostForecast(),
    getCostTotals: () => getCostTotals(),
    getBudgetStatus: () => getBudgetStatus(),
    setBudget: () => setBudget(),
  };
});

describe("Costs.svelte", () => {
  beforeEach(async () => {
    getSessionHistory.mockClear();
    getCostForecast.mockClear();
    getCostTotals.mockClear();
    getBudgetStatus.mockClear();
    const { sessions } = await import("@/lib/stores");
    sessions.set([]);
  });

  it("leads with the budget cockpit and supporting inline figures", async () => {
    const Costs = (await import("@/views/Costs.svelte")).default;
    const { container } = render(Costs);
    await tick();

    await waitFor(() => expect(getSessionHistory).toHaveBeenCalled());
    // The gauge is the hero, not a row of equal-weight tiles.
    await waitFor(() => expect(container.querySelector(".cockpit")).not.toBeNull());

    const labels = [...container.querySelectorAll(".is-label")].map((e) => e.textContent?.trim());
    expect(labels).toEqual([
      "Avg / session",
      "Cost / 1M tokens",
      "Cache savings",
      "Total spent (30d)",
    ]);
  });

  it("reads KPI totals from the window aggregate, not the visible page", async () => {
    const Costs = (await import("@/views/Costs.svelte")).default;
    const { container } = render(Costs);
    await tick();

    await waitFor(() => expect(getCostTotals).toHaveBeenCalled());
    await waitFor(() => expect(container.querySelector(".is-value")).not.toBeNull());

    const values = [...container.querySelectorAll(".is-value")].map((e) => e.textContent?.trim());
    // Total spent mirrors the aggregate exactly.
    expect(values[3]).toBe("$" + totals.total_cost.toFixed(2));
    // Average divides by the aggregate session count.
    expect(values[0]).toBe("$" + (totals.total_cost / totals.sessions).toFixed(2));
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

  it("plots spend, projection and cap on the cockpit gauge", async () => {
    const Costs = (await import("@/views/Costs.svelte")).default;
    const { container, findByText } = render(Costs);
    await tick();

    await waitFor(() => expect(container.querySelector(".ck-track")).not.toBeNull());
    // $30 spent, $93 projected, $100 cap: healthy, and the cap tick is drawn.
    expect(container.querySelector(".ck-figure")?.textContent).toContain("30.00");
    expect(container.querySelector(".ck-cap")).not.toBeNull();
    expect(await findByText(/under the .* cap/)).toBeTruthy();
  });
});

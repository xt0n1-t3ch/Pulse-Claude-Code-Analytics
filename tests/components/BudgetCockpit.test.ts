import { describe, it, expect } from "vitest";
import { render } from "@testing-library/svelte";
import BudgetCockpit from "@/components/BudgetCockpit.svelte";
import type { BudgetStatus, CostForecast } from "@/lib/api";

function forecast(overrides: Partial<CostForecast> = {}): CostForecast {
  return {
    spent_this_month: 7371.35,
    days_elapsed: 24,
    days_in_month: 31,
    projected_monthly: 9521.32,
    daily_average: 307.14,
    ...overrides,
  };
}

function budget(monthly: number): BudgetStatus {
  return {
    monthly_budget: monthly,
    alert_threshold_pct: 80,
    spent_this_month: 7371.35,
    pct_used: (7371.35 / monthly) * 100,
    projected_monthly: 9521.32,
    over_budget: 9521.32 > monthly,
  };
}

const noop = (): void => undefined;

describe("BudgetCockpit.svelte", () => {
  it("leads with month-to-date spend as the headline figure", () => {
    const { container } = render(BudgetCockpit, {
      props: { forecast: forecast(), budget: null, onSetBudget: noop },
    });

    expect(container.querySelector(".ck-figure")?.textContent).toContain("7371.35");
  });

  it("offers to set a cap when none exists, and hides the budget tick", () => {
    const { container, getByText } = render(BudgetCockpit, {
      props: { forecast: forecast(), budget: null, onSetBudget: noop },
    });

    expect(getByText("Set a cap")).toBeTruthy();
    expect(container.querySelector(".ck-cap")).toBeNull();
  });

  it("reads healthy when the projection lands under the cap", () => {
    const { container, getByText } = render(BudgetCockpit, {
      props: { forecast: forecast(), budget: budget(12000), onSetBudget: noop },
    });

    expect(container.querySelector(".cockpit.warn")).toBeNull();
    expect(container.querySelector(".cockpit.over")).toBeNull();
    expect(getByText(/under the .* cap/)).toBeTruthy();
  });

  /** Spend still inside the cap but heading past it is the case the whole
   *  screen exists to surface, so it must be visually distinct. */
  it("warns when spend is under the cap but projected to overshoot", () => {
    const { container, getByText } = render(BudgetCockpit, {
      props: { forecast: forecast(), budget: budget(8000), onSetBudget: noop },
    });

    expect(container.querySelector(".cockpit.warn")).not.toBeNull();
    expect(container.querySelector(".cockpit.over")).toBeNull();
    expect(getByText(/On course to overshoot/)).toBeTruthy();
  });

  it("escalates to over-budget once spend already exceeds the cap", () => {
    const { container, getByText } = render(BudgetCockpit, {
      props: { forecast: forecast(), budget: budget(5000), onSetBudget: noop },
    });

    expect(container.querySelector(".cockpit.over")).not.toBeNull();
    expect(getByText(/Already .* over the/)).toBeTruthy();
  });

  /** The gauge is only readable if spend, projection and cap share a scale
   *  and stay in order along the track. */
  it("places spend, projection and cap in ascending order on one track", () => {
    const { container } = render(BudgetCockpit, {
      props: { forecast: forecast(), budget: budget(12000), onSetBudget: noop },
    });

    const width = (sel: string) =>
      parseFloat((container.querySelector(sel) as HTMLElement).style.width);
    const spent = width(".ck-spent");
    const projected = width(".ck-projected");
    const capLeft = parseFloat((container.querySelector(".ck-cap") as HTMLElement).style.left);

    expect(spent).toBeLessThan(projected);
    expect(projected).toBeLessThan(capLeft);
    expect(capLeft).toBeLessThanOrEqual(100);
  });

  it("states plainly when nothing has been spent this month", () => {
    const { getByText } = render(BudgetCockpit, {
      props: {
        forecast: forecast({ spent_this_month: 0, days_elapsed: 0, projected_monthly: 0 }),
        budget: null,
        onSetBudget: noop,
      },
    });

    expect(getByText("No spend recorded this month yet.")).toBeTruthy();
  });

  it("uses no hardcoded colours", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    const source = readFileSync(
      resolve(process.cwd(), "src/components/BudgetCockpit.svelte"),
      "utf8",
    );
    const css = source.match(/<style>([\s\S]*)<\/style>/)?.[1] ?? "";
    expect(css.length).toBeGreaterThan(0);
    expect(css.match(/#[0-9a-fA-F]{3,8}\b/g)).toBeNull();
    expect(css.match(/rgba?\(/g)).toBeNull();
  });
});

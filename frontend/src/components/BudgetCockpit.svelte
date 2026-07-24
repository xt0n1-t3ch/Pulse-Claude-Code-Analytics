<script lang="ts">
  /**
   * Budget-first hero for Cost Analysis.
   *
   * The screen's real question is "am I going to overshoot this month?", so a
   * single gauge answers it directly instead of four equal-weight tiles that
   * leave the reader to do the comparison. Three marks share one track:
   * spend so far (filled), the projected month-end total (ghosted), and the
   * budget cap (tick). Their relative positions are the answer.
   *
   * Deliberately distinct from the Reports timeline: that view plots history
   * over time, this one plots a single month against a target.
   */
  import { fmtCost } from "../lib/utils";
  import type { BudgetStatus, CostForecast } from "../lib/api";

  let {
    forecast = null,
    budget = null,
    onSetBudget,
  }: {
    forecast: CostForecast | null;
    budget: BudgetStatus | null;
    onSetBudget: () => void;
  } = $props();

  let spent = $derived(forecast?.spent_this_month ?? 0);
  let projected = $derived(forecast?.projected_monthly ?? 0);
  let cap = $derived(budget?.monthly_budget ?? 0);
  let hasCap = $derived(cap > 0);

  /**
   * Track ceiling. With a cap, leave headroom so the tick is never flush
   * against the right edge; without one, the projection defines the scale.
   */
  let ceiling = $derived(
    Math.max(spent, projected, hasCap ? cap * 1.15 : 0, 1),
  );

  function pct(value: number): number {
    return Math.max(0, Math.min(100, (value / ceiling) * 100));
  }

  let spentPct = $derived(pct(spent));
  let projectedPct = $derived(pct(projected));
  let capPct = $derived(hasCap ? pct(cap) : 0);

  /** Over cap is danger; on course to cross it is caution. */
  let status = $derived.by((): "none" | "ok" | "warn" | "over" => {
    if (!hasCap) return "none";
    if (spent > cap) return "over";
    if (projected > cap) return "warn";
    return "ok";
  });

  let overshoot = $derived(Math.max(0, projected - cap));
  let headroom = $derived(Math.max(0, cap - projected));
  let capShare = $derived(hasCap && cap > 0 ? (projected / cap) * 100 : 0);

  /** One plain sentence, so the gauge does not need decoding. */
  let verdict = $derived.by(() => {
    if (!forecast || forecast.days_elapsed === 0) {
      return "No spend recorded this month yet.";
    }
    if (!hasCap) {
      return `Averaging ${fmtCost(forecast.daily_average)}/day — on course for ${fmtCost(projected)} by month end.`;
    }
    if (status === "over") {
      return `Already ${fmtCost(spent - cap)} over the ${fmtCost(cap)} cap with ${forecast.days_in_month - forecast.days_elapsed} days left.`;
    }
    if (status === "warn") {
      return `On course to overshoot the ${fmtCost(cap)} cap by ${fmtCost(overshoot)}.`;
    }
    return `On course for ${fmtCost(projected)}, ${fmtCost(headroom)} under the ${fmtCost(cap)} cap.`;
  });
</script>

<section class="cockpit" class:warn={status === "warn"} class:over={status === "over"}>
  <header class="ck-head">
    <div class="ck-primary">
      <span class="ck-label">Spent this month</span>
      <span class="ck-figure">{fmtCost(spent)}</span>
    </div>

    <div class="ck-marks">
      <div class="ck-mark">
        <span class="ck-mark-label">Projected</span>
        <span class="ck-mark-value">{fmtCost(projected)}</span>
      </div>
      <div class="ck-mark">
        <span class="ck-mark-label">Monthly budget</span>
        {#if hasCap}
          <button type="button" class="ck-mark-value ck-cap-btn" onclick={onSetBudget}>
            {fmtCost(cap)}
          </button>
        {:else}
          <button type="button" class="ck-set-btn" onclick={onSetBudget}>Set a cap</button>
        {/if}
      </div>
    </div>
  </header>

  <div
    class="ck-track"
    role="meter"
    aria-label="Month-to-date spend against budget"
    aria-valuemin="0"
    aria-valuemax={ceiling}
    aria-valuenow={spent}
  >
    <!-- Projection sits behind the actual fill so the gap between them reads
         as "still to come" rather than as a separate quantity. -->
    <div class="ck-projected" style="width:{projectedPct}%"></div>
    <div class="ck-spent" style="width:{spentPct}%"></div>
    {#if hasCap}
      <div class="ck-cap" style="left:{capPct}%">
        <span class="ck-cap-tick"></span>
      </div>
    {/if}
  </div>

  <p class="ck-verdict">{verdict}</p>

  {#if forecast && forecast.days_in_month > 0}
    <p class="ck-period">
      Day {forecast.days_elapsed} of {forecast.days_in_month}
      {#if hasCap}· {capShare.toFixed(0)}% of cap projected{/if}
    </p>
  {/if}
</section>

<style>
  .cockpit {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 4px 0 22px;
  }

  .ck-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 32px;
    flex-wrap: wrap;
  }

  .ck-primary { display: flex; flex-direction: column; gap: 6px; }
  .ck-label {
    font-size: var(--fs-xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: var(--letter-wider);
    color: var(--text-muted);
  }
  .ck-figure {
    font-family: var(--font-mono);
    font-size: var(--fs-display);
    font-weight: 700;
    line-height: 1;
    letter-spacing: var(--letter-tighter);
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
  }

  .ck-marks { display: flex; gap: 32px; padding-bottom: 4px; }
  .ck-mark { display: flex; flex-direction: column; gap: 4px; align-items: flex-end; }
  .ck-mark-label {
    font-size: var(--fs-xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: var(--letter-wider);
    color: var(--text-muted);
  }
  .ck-mark-value {
    font-family: var(--font-mono);
    font-size: var(--fs-lg);
    font-weight: 600;
    color: var(--text-secondary);
    font-variant-numeric: tabular-nums;
  }
  .ck-cap-btn,
  .ck-set-btn {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-family: var(--font-mono);
    transition: color 0.15s var(--ease);
  }
  .ck-cap-btn:hover { color: var(--text-primary); }
  .ck-set-btn {
    font-family: var(--font-sans);
    font-size: var(--fs-base);
    font-weight: 600;
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 3px;
  }

  .ck-track {
    position: relative;
    height: 12px;
    border-radius: var(--radius-full);
    background: var(--bg-elevated);
    overflow: visible;
  }
  .ck-projected,
  .ck-spent {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: var(--radius-full);
    transition: width 0.35s var(--ease-out);
  }
  /* Ghosted: the part of the month that has not happened yet. */
  .ck-projected { background: var(--accent-dim); }
  .ck-spent { background: var(--success); }
  .warn .ck-spent { background: var(--warning); }
  .over .ck-spent { background: var(--danger); }

  .ck-cap {
    position: absolute;
    top: -5px;
    bottom: -5px;
    width: 0;
  }
  .ck-cap-tick {
    position: absolute;
    inset: 0;
    width: 2px;
    background: var(--text-primary);
    border-radius: 1px;
  }

  .ck-verdict {
    font-size: var(--fs-md);
    line-height: var(--lh-snug);
    color: var(--text-primary);
    max-width: 70ch;
  }
  .warn .ck-verdict { color: var(--warning); }
  .over .ck-verdict { color: var(--danger); }

  .ck-period {
    font-size: var(--fs-sm);
    color: var(--text-muted);
  }

  @media (max-width: 720px) {
    .ck-head { align-items: flex-start; }
    .ck-marks { gap: 20px; }
  }
</style>

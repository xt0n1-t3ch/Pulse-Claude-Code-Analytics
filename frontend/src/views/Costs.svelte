<script lang="ts">
  import { onMount } from "svelte";
  import StatCard from "../components/StatCard.svelte";
  import Chart from "../components/Chart.svelte";
  import { sessions, metrics } from "../lib/stores";
  import { fmtCost, fmtTokens, fmtPct } from "../lib/utils";
  import { getSessionHistory, getCostForecast, getCostTotals, getBudgetStatus, setBudget } from "../lib/api";
  import type { HistoricalSession, CostForecast, CostTotals, BudgetStatus } from "../lib/api";
  import type { ChartConfiguration, Chart as ChartType } from "chart.js/auto";
  import ExportModal from "../components/ExportModal.svelte";
  import type { ExportColumn } from "../lib/export";
  import BudgetCockpit from "../components/BudgetCockpit.svelte";

  let showExport = $state(false);
  let forecast = $state<CostForecast | null>(null);
  /** Window-wide aggregates. The session table is a capped page, so KPIs read
   *  from here instead of summing the visible rows. */
  let totals = $state<CostTotals | null>(null);
  let budgetStatus = $state<BudgetStatus | null>(null);
  let editingBudget = $state(false);
  let budgetInput = $state("");

  const costColumns: ExportColumn[] = [
    { key: "project", label: "Project", enabled: true },
    { key: "model", label: "Model", enabled: true },
    { key: "branch", label: "Branch", enabled: false },
    { key: "input_tokens", label: "Input Tokens", enabled: true },
    { key: "output_tokens", label: "Output Tokens", enabled: true },
    { key: "cache_write_tokens", label: "Cache Write", enabled: true },
    { key: "cache_read_tokens", label: "Cache Read", enabled: true },
    { key: "tokens", label: "Total Tokens", enabled: true },
    { key: "input_cost", label: "Input Cost", enabled: true },
    { key: "output_cost", label: "Output Cost", enabled: true },
    { key: "cache_write_cost", label: "Cache Write Cost", enabled: true },
    { key: "cache_read_cost", label: "Cache Read Cost", enabled: true },
    { key: "cost", label: "Total Cost", enabled: true },
  ];

  let histSessions = $state<HistoricalSession[]>([]);

  async function loadData(): Promise<void> {
    [histSessions, totals, forecast, budgetStatus] = await Promise.all([
      getSessionHistory(30, undefined, 200),
      getCostTotals(30),
      getCostForecast(),
      getBudgetStatus(),
    ]);
  }

  async function saveBudget(): Promise<void> {
    const val = parseFloat(budgetInput);
    if (!isNaN(val) && val >= 0) {
      await setBudget(val);
      budgetStatus = await getBudgetStatus();
    }
    editingBudget = false;
  }

  onMount(() => { loadData(); });

  let projectFilter = $state("");

  let allSessions = $derived.by(() => {
    const live = $sessions.map((s) => ({
      id: s.session_id, project: s.project, model: s.model, branch: s.branch,
      cost: s.cost, tokens: s.tokens, input_tokens: s.input_tokens, output_tokens: s.output_tokens,
      cache_write_tokens: s.cache_write_tokens, cache_read_tokens: s.cache_read_tokens,
      input_cost: s.input_cost, output_cost: s.output_cost,
      cache_write_cost: s.cache_write_cost, cache_read_cost: s.cache_read_cost,
      is_active: true,
    }));
    const hist = histSessions
      .filter((h) => !live.some((l) => l.id === h.id))
      .map((h) => ({
        id: h.id, project: h.project, model: h.model, branch: h.branch,
        cost: h.total_cost, tokens: h.total_tokens, input_tokens: h.input_tokens, output_tokens: h.output_tokens,
        cache_write_tokens: h.cache_write_tokens, cache_read_tokens: h.cache_read_tokens,
        input_cost: h.input_cost, output_cost: h.output_cost,
        cache_write_cost: h.cache_write_cost, cache_read_cost: h.cache_read_cost,
        is_active: false,
      }));
    return [...live, ...hist];
  });

  let projects = $derived([...new Set(allSessions.map((s) => s.project))].sort());
  let filtered = $derived(projectFilter ? allSessions.filter((s) => s.project === projectFilter) : allSessions);
  let costExportRows = $derived([...filtered].sort((a, b) => b.cost - a.cost).map((s) => ({ ...s } as Record<string, unknown>)));

  /** True when no project filter is applied, so the window-wide totals apply.
   *  Filtering to one project has to fall back to summing the loaded rows. */
  let unfiltered = $derived(projectFilter === "");
  let totalCost = $derived(
    unfiltered && totals ? totals.total_cost : filtered.reduce((sum, s) => sum + s.cost, 0),
  );
  let sessionCount = $derived(unfiltered && totals ? totals.sessions : filtered.length);
  let avgCost = $derived(sessionCount ? totalCost / sessionCount : 0);
  let maxCost = $derived(filtered.reduce((m, s) => Math.max(m, s.cost), 0));
  // Per 1M tokens: at real usage the per-1K figure rounds to $0.00, so 1M is the
  // meaningful unit (e.g. $0.67 / 1M rather than $0.00 / 1K).
  let costPerMToken = $derived.by(() => {
    const tot = unfiltered && totals
      ? totals.total_tokens
      : filtered.reduce((s, x) => s + x.tokens, 0);
    return tot > 0 ? (totalCost / tot) * 1_000_000 : 0;
  });

  let totalInputCost = $derived(
    unfiltered && totals ? totals.input_cost : filtered.reduce((s, x) => s + x.input_cost, 0),
  );
  let totalOutputCost = $derived(
    unfiltered && totals ? totals.output_cost : filtered.reduce((s, x) => s + x.output_cost, 0),
  );
  let totalCacheWCost = $derived(
    unfiltered && totals
      ? totals.cache_write_cost
      : filtered.reduce((s, x) => s + x.cache_write_cost, 0),
  );
  let totalCacheRCost = $derived(
    unfiltered && totals
      ? totals.cache_read_cost
      : filtered.reduce((s, x) => s + x.cache_read_cost, 0),
  );
  let costTotal = $derived(totalInputCost + totalOutputCost + totalCacheWCost + totalCacheRCost);

  let cacheSavings = $derived.by(() => {
    // Savings = what those cached tokens would have cost at the full input
    // rate, minus what they actually cost as cache reads.
    //
    // The rate must be derived from the same population as the token counts.
    // Mixing a window-wide token total with a rate computed from the visible
    // page produced a wildly inflated figure, so both sides come from `totals`
    // when it is available.
    if (unfiltered && totals) {
      // Rate and token counts both come from the window aggregate, so the two
      // sides of the multiplication are always the same population.
      if (totals.pure_input_tokens <= 0 || totals.input_cost <= 0) return 0;
      const rate = totals.input_cost / totals.pure_input_tokens;
      return Math.max(0, totals.cache_read_tokens * rate - totals.cache_read_cost);
    }
    const cacheReadTokens = filtered.reduce((s, x) => s + x.cache_read_tokens, 0);
    const pureInput = filtered.reduce((s, x) => s + Math.max(0, x.input_tokens - x.cache_write_tokens - x.cache_read_tokens), 0);
    const inputCostRate = pureInput > 0 && totalInputCost > 0 ? totalInputCost / pureInput : 5 / 1_000_000;
    return Math.max(0, cacheReadTokens * inputCostRate - totalCacheRCost);
  });

  let costByProject = $derived.by(() => {
    // Window-wide when unfiltered, so the bars reconcile with Total Spent.
    if (unfiltered && totals) {
      return totals.by_project.map((p) => [p.label, p.cost] as [string, number]);
    }
    const map: Record<string, number> = {};
    filtered.forEach((s) => (map[s.project] = (map[s.project] || 0) + s.cost));
    return Object.entries(map).sort((a, b) => b[1] - a[1]);
  });

  let modelCosts = $derived.by(() => {
    if (unfiltered && totals) {
      return totals.by_model.map((m) => [m.label, m.cost] as [string, number]);
    }
    const map: Record<string, number> = {};
    filtered.forEach((s) => (map[s.model] = (map[s.model] || 0) + s.cost));
    return Object.entries(map).sort((a, b) => b[1] - a[1]);
  });

  let costChartConfig: ChartConfiguration = {
    type: "bar",
    data: { labels: [], datasets: [{ data: [], backgroundColor: "#f5f5f5", borderRadius: 6, maxBarThickness: 40 }] },
    options: {
      responsive: true, maintainAspectRatio: false, indexAxis: "y",
      scales: { x: { grid: { color: "rgba(255,255,255,0.06)" }, ticks: { callback: (v: any) => "$" + Number(v).toFixed(2), maxTicksLimit: 6 } }, y: { grid: { display: false } } },
      plugins: { legend: { display: false }, tooltip: { callbacks: { label: (c: any) => fmtCost(c.raw as number) } } },
    },
  };

  function updateCostChart(chart: ChartType): void {
    chart.data.labels = costByProject.map((e) => e[0]);
    chart.data.datasets[0].data = costByProject.map((e) => e[1]);
  }
</script>

<div class="costs-view">
  <div class="view-header">
    <h2 class="view-title">Cost Analysis</h2>
    <div class="filters">
      <select bind:value={projectFilter}>
        <option value="">All Projects</option>
        {#each projects as p}<option value={p}>{p}</option>{/each}
      </select>
    </div>
  </div>

  <BudgetCockpit
    {forecast}
    budget={budgetStatus}
    onSetBudget={() => {
      editingBudget = true;
      budgetInput = String(budgetStatus?.monthly_budget || 200);
    }}
  />

  {#if editingBudget}
    <div class="budget-edit">
      <input type="number" min="0" step="10" bind:value={budgetInput} placeholder="Monthly budget ($)" class="budget-input" />
      <button class="budget-save-btn" onclick={saveBudget}>Save</button>
      <button class="budget-cancel-btn" onclick={() => editingBudget = false}>Cancel</button>
    </div>
  {/if}

  <!-- Supporting figures: spacing and rules only, no boxes competing with the
       cockpit above. -->
  <div class="inline-stats">
    <div class="is-item">
      <span class="is-label">Avg / session</span>
      <span class="is-value">{fmtCost(avgCost)}</span>
      <span class="is-meta">{sessionCount} {sessionCount === 1 ? "session" : "sessions"}</span>
    </div>
    <div class="is-item">
      <span class="is-label">Cost / 1M tokens</span>
      <span class="is-value">{fmtCost(costPerMToken)}</span>
      <span class="is-meta">blended rate</span>
    </div>
    <div class="is-item">
      <span class="is-label">Cache savings</span>
      <span class="is-value">{fmtCost(cacheSavings)}</span>
      <span class="is-meta">vs uncached input</span>
    </div>
    <div class="is-item">
      <span class="is-label">Total spent (30d)</span>
      <span class="is-value">{fmtCost(totalCost)}</span>
      <span class="is-meta">window total</span>
    </div>
  </div>

  <div class="charts-row">
    <section class="pane">
      <h3 class="pane-title">Cost by Type</h3>
      {#if costTotal > 0}
        <div class="cost-type-bar">
          <div class="cost-seg input" style="width:{(totalInputCost / costTotal) * 100}%"></div>
          <div class="cost-seg output" style="width:{(totalOutputCost / costTotal) * 100}%"></div>
          <div class="cost-seg cache-w" style="width:{(totalCacheWCost / costTotal) * 100}%"></div>
          <div class="cost-seg cache-r" style="width:{(totalCacheRCost / costTotal) * 100}%"></div>
        </div>
        <div class="cost-type-legend">
          <div class="ct-row"><span class="dot input"></span><span class="ct-label">Input</span><span class="ct-val">{fmtCost(totalInputCost)}</span></div>
          <div class="ct-row"><span class="dot output"></span><span class="ct-label">Output</span><span class="ct-val">{fmtCost(totalOutputCost)}</span></div>
          <div class="ct-row"><span class="dot cache-w"></span><span class="ct-label">Cache Write</span><span class="ct-val">{fmtCost(totalCacheWCost)}</span></div>
          <div class="ct-row"><span class="dot cache-r"></span><span class="ct-label">Cache Read</span><span class="ct-val">{fmtCost(totalCacheRCost)}</span></div>
        </div>
      {:else}
        <div class="empty-hint">No cost data yet</div>
      {/if}
    </section>

    {#if modelCosts.length > 0}
      <section class="pane">
        <h3 class="pane-title">Cost per Model</h3>
        <div class="model-cost-list">
          {#each modelCosts as [model, cost]}
            <div class="mc-row">
              <span class="mc-name">{model}</span>
              <div class="mc-bar-track">
                <div class="mc-bar-fill" style="width:{modelCosts[0][1] > 0 ? (cost / modelCosts[0][1]) * 100 : 0}%"></div>
              </div>
              <span class="mc-val">{fmtCost(cost)}</span>
            </div>
          {/each}
        </div>
      </section>
    {/if}
  </div>

  {#if costByProject.length > 0}
    <div class="card">
      <h3 class="card-title">Cost by Project</h3>
      <div
        class="chart-container"
        style="height: {Math.max(140, Math.min(360, 44 + costByProject.length * 44))}px"
      >
        <Chart config={costChartConfig} updateData={updateCostChart} />
      </div>
    </div>
  {/if}

  <div class="card">
    <div class="card-title-row">
      <h3 class="card-title">Session Details</h3>
      {#if filtered.length > 0}
        <button class="export-btn" onclick={() => showExport = true}>Export</button>
      {/if}
    </div>
    <div class="detail-table">
      <div class="dt-header">
        <span class="dt-col status"></span>
        <span class="dt-col project">Project</span>
        <span class="dt-col">Input</span>
        <span class="dt-col">Output</span>
        <span class="dt-col">Cache W</span>
        <span class="dt-col">Cache R</span>
        <span class="dt-col">Tokens</span>
        <span class="dt-col cost">Cost</span>
      </div>
      {#each [...filtered].sort((a, b) => b.cost - a.cost) as s (s.id)}
        <div class="dt-row">
          <span class="dt-col status"><span class="status-dot" class:active={s.is_active}></span></span>
          <span class="dt-col project">{s.project}{s.branch ? " · " + s.branch : ""}</span>
          <span class="dt-col">{fmtTokens(s.input_tokens)}</span>
          <span class="dt-col">{fmtTokens(s.output_tokens)}</span>
          <span class="dt-col">{fmtTokens(s.cache_write_tokens)}</span>
          <span class="dt-col">{fmtTokens(s.cache_read_tokens)}</span>
          <span class="dt-col">{fmtTokens(s.tokens)}</span>
          <span class="dt-col cost">{fmtCost(s.cost)}</span>
        </div>
      {:else}
        <div class="dt-empty">No session data yet</div>
      {/each}
    </div>
  </div>
</div>

<ExportModal
  open={showExport}
  title="Export Cost Data"
  defaultFilename="pulse-costs"
  columns={costColumns}
  rows={costExportRows}
  onclose={() => showExport = false}
/>

<style>
  .costs-view { display: flex; flex-direction: column; gap: 16px; }
  .view-header { display: flex; align-items: center; gap: 12px; }
  .view-title { font-size: 20px; font-weight: 700; }
  .filters { margin-left: auto; }
  /* Supporting figures read as one row of text, separated by rules rather
     than four boxes competing with the cockpit gauge above them. */
  .inline-stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    padding: 18px 0;
    border-top: 1px solid var(--border);
    border-bottom: 1px solid var(--border);
  }
  .is-item {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 0 20px;
    border-left: 1px solid var(--border);
  }
  .is-item:first-child { padding-left: 0; border-left: none; }
  .is-label {
    font-size: var(--fs-xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: var(--letter-wider);
    color: var(--text-muted);
  }
  .is-value {
    font-family: var(--font-mono);
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: var(--letter-tight);
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
  }
  .is-meta { font-size: var(--fs-xs); color: var(--text-muted); }
  @media (max-width: 900px) {
    .inline-stats { grid-template-columns: repeat(2, 1fr); row-gap: 18px; }
    .is-item:nth-child(3) { padding-left: 0; border-left: none; }
  }

  .charts-row { display: grid; grid-template-columns: 1fr 1fr; gap: 40px; }

  /* Panes sit on the page surface; a hairline divides the pair instead of
     wrapping each half in its own card. */
  .pane { display: flex; flex-direction: column; }
  .charts-row .pane + .pane {
    padding-left: 40px;
    border-left: 1px solid var(--border);
  }
  .pane-title {
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    margin-bottom: 16px;
  }

  .card { background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 20px; }
  .card-title { font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--accent); margin-bottom: 16px; display: flex; align-items: center; gap: 8px; }
  .card-title::before { content: ""; width: 3px; height: 14px; background: var(--accent); border-radius: 2px; }

  .cost-type-bar { display: flex; height: 12px; border-radius: 99px; overflow: hidden; background: var(--bg-elevated); margin-bottom: 14px; }
  .cost-seg { height: 100%; transition: width 0.4s var(--ease); }
  .cost-seg.input { background: var(--info); }
  .cost-seg.output { background: #7cb9e8; }
  .cost-seg.cache-w { background: #77dd77; }
  .cost-seg.cache-r { background: #c3b1e1; }

  .cost-type-legend { display: flex; flex-direction: column; gap: 6px; }
  .ct-row { display: flex; align-items: center; gap: 8px; font-size: 12px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .dot.input { background: var(--info); }
  .dot.output { background: #7cb9e8; }
  .dot.cache-w { background: #77dd77; }
  .dot.cache-r { background: #c3b1e1; }
  .ct-label { flex: 1; color: var(--text-secondary); }
  .ct-val { font-weight: 700; color: var(--text-primary); font-variant-numeric: tabular-nums; }

  .model-cost-list { display: flex; flex-direction: column; gap: 8px; }
  .mc-row { display: flex; align-items: center; gap: 10px; font-size: 12px; }
  .mc-name { min-width: 120px; font-weight: 600; font-size: 13px; }
  .mc-bar-track { flex: 1; height: 8px; background: var(--bg-elevated); border-radius: 99px; overflow: hidden; }
  .mc-bar-fill { height: 100%; background: var(--accent); border-radius: 99px; transition: width 0.3s var(--ease); }
  .mc-val { min-width: 60px; text-align: right; font-weight: 700; color: var(--accent); font-variant-numeric: tabular-nums; }

  .chart-container { height: 250px; min-height: 140px; }

  .card-title-row { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
  .card-title-row .card-title { margin-bottom: 0; }
  .export-btn { font-size: 11px; font-weight: 600; color: var(--text-secondary); background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 4px 12px; cursor: pointer; transition: all 0.15s ease; }
  .export-btn:hover { color: var(--accent); border-color: var(--accent); background: var(--accent-dim); }

  .detail-table { font-size: 12px; max-height: 400px; overflow-y: auto; --dt-cols: 24px 2fr 80px 80px 80px 80px 80px 80px; }
  .dt-header { display: grid; grid-template-columns: var(--dt-cols); gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--border); font-weight: 700; color: var(--text-muted); text-transform: uppercase; font-size: 10px; letter-spacing: 0.05em; position: sticky; top: 0; background: var(--bg-card); z-index: 1; }
  .dt-row { display: grid; grid-template-columns: var(--dt-cols); gap: 8px; padding: 8px 12px; border-radius: var(--radius-sm); transition: background 0.15s var(--ease); }
  .dt-row:hover { background: var(--bg-elevated); }
  .dt-col { text-align: right; font-variant-numeric: tabular-nums; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .dt-col.project { text-align: left; font-weight: 500; color: var(--text-primary); }
  .dt-col.cost { font-weight: 700; color: var(--accent); }
  .dt-col.status { text-align: center; }
  .status-dot { display: inline-block; width: 6px; height: 6px; border-radius: 50%; background: var(--text-muted); }
  .status-dot.active { background: var(--success); box-shadow: 0 0 4px var(--success-glow); }
  .dt-empty { text-align: center; padding: 20px; color: var(--text-muted); }

  .empty-hint { text-align: center; padding: 20px; color: var(--text-muted); font-size: 12px; }

  /* Forecast and budget rendering now lives in BudgetCockpit.svelte; only the
     inline budget editor remains in this view. */
  .budget-edit { display: flex; gap: 8px; align-items: center; margin-top: 10px; padding-top: 10px; border-top: 1px solid var(--border); }
  .budget-input { flex: 1; padding: 7px 12px; font: inherit; font-size: 12px; background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-primary); outline: none; transition: border-color 0.15s ease; }
  .budget-input:focus { border-color: var(--accent); }
  .budget-save-btn { font-size: 11px; font-weight: 700; letter-spacing: 0.04em; color: var(--accent-fg); background: var(--accent); border: 1px solid var(--accent); border-radius: var(--radius-sm); padding: 7px 16px; cursor: pointer; transition: opacity 0.15s ease, transform 0.15s ease; }
  .budget-save-btn:hover { opacity: 0.9; transform: translateY(-1px); }
  .budget-cancel-btn { font-size: 11px; font-weight: 500; color: var(--text-secondary); background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 7px 14px; cursor: pointer; transition: color 0.15s ease, border-color 0.15s ease; }
  .budget-cancel-btn:hover { color: var(--text-primary); border-color: var(--border-hover); }
</style>

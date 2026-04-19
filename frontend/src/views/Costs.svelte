<script lang="ts">
  import { onMount } from "svelte";
  import StatCard from "../components/StatCard.svelte";
  import Chart from "../components/Chart.svelte";
  import { sessions, metrics } from "../lib/stores";
  import { fmtCost, fmtTokens, fmtPct } from "../lib/utils";
  import { getSessionHistory, getCostForecast, getBudgetStatus, setBudget } from "../lib/api";
  import type { HistoricalSession, CostForecast, BudgetStatus } from "../lib/api";
  import type { ChartConfiguration, Chart as ChartType } from "chart.js/auto";
  import ExportModal from "../components/ExportModal.svelte";
  import type { ExportColumn } from "../lib/export";

  let showExport = $state(false);
  let forecast = $state<CostForecast | null>(null);
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
    [histSessions, forecast, budgetStatus] = await Promise.all([
      getSessionHistory(30, undefined, 200),
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

  let totalCost = $derived(filtered.reduce((sum, s) => sum + s.cost, 0));
  let avgCost = $derived(filtered.length ? totalCost / filtered.length : 0);
  let maxCost = $derived(filtered.reduce((m, s) => Math.max(m, s.cost), 0));
  let costPerKToken = $derived.by(() => {
    const tot = filtered.reduce((s, x) => s + x.tokens, 0);
    return tot > 0 ? (totalCost / tot) * 1000 : 0;
  });

  let totalInputCost = $derived(filtered.reduce((s, x) => s + x.input_cost, 0));
  let totalOutputCost = $derived(filtered.reduce((s, x) => s + x.output_cost, 0));
  let totalCacheWCost = $derived(filtered.reduce((s, x) => s + x.cache_write_cost, 0));
  let totalCacheRCost = $derived(filtered.reduce((s, x) => s + x.cache_read_cost, 0));
  let costTotal = $derived(totalInputCost + totalOutputCost + totalCacheWCost + totalCacheRCost);

  let cacheSavings = $derived.by(() => {
    const cacheReadTokens = filtered.reduce((s, x) => s + x.cache_read_tokens, 0);
    const pureInput = filtered.reduce((s, x) => s + Math.max(0, x.input_tokens - x.cache_write_tokens - x.cache_read_tokens), 0);
    const inputCostRate = pureInput > 0 && totalInputCost > 0 ? totalInputCost / pureInput : 5 / 1_000_000;
    return Math.max(0, cacheReadTokens * inputCostRate - totalCacheRCost);
  });

  let costByProject = $derived.by(() => {
    const map: Record<string, number> = {};
    filtered.forEach((s) => (map[s.project] = (map[s.project] || 0) + s.cost));
    return Object.entries(map).sort((a, b) => b[1] - a[1]);
  });

  let modelCosts = $derived.by(() => {
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

  <div class="stats-row">
    <StatCard label="Total Spent" value={fmtCost(totalCost)} />
    <StatCard label="Avg / Session" value={fmtCost(avgCost)} />
    <StatCard label="Cost / 1K Tokens" value={fmtCost(costPerKToken)} />
    <StatCard label="Cache Savings" value={fmtCost(cacheSavings)} />
  </div>

  <div class="budget-forecast-row">
    {#if forecast && forecast.spent_this_month > 0}
      <div class="card">
        <h3 class="card-title">Monthly Forecast</h3>
        <div class="forecast-grid">
          <div class="forecast-item">
            <span class="fg-label">Spent This Month</span>
            <span class="fg-value">{fmtCost(forecast.spent_this_month)}</span>
          </div>
          <div class="forecast-item">
            <span class="fg-label">Daily Average</span>
            <span class="fg-value">{fmtCost(forecast.daily_average)}</span>
          </div>
          <div class="forecast-item">
            <span class="fg-label">Projected Total</span>
            <span class="fg-value accent">{fmtCost(forecast.projected_monthly)}</span>
          </div>
          <div class="forecast-item">
            <span class="fg-label">Progress</span>
            <span class="fg-value">{forecast.days_elapsed}/{forecast.days_in_month} days</span>
          </div>
        </div>
      </div>
    {/if}

    <div class="card">
      <h3 class="card-title">Budget Tracking</h3>
      {#if budgetStatus && budgetStatus.monthly_budget > 0}
        <div class="budget-info">
          <div class="budget-header">
            <span class="budget-amount">{fmtCost(budgetStatus.spent_this_month)} / {fmtCost(budgetStatus.monthly_budget)}</span>
            <span class="budget-pct" class:over={budgetStatus.pct_used > 100}>{fmtPct(budgetStatus.pct_used)}</span>
          </div>
          <div class="budget-bar-track">
            <div class="budget-bar-fill" class:warning={budgetStatus.pct_used > budgetStatus.alert_threshold_pct} class:danger={budgetStatus.pct_used > 100} style="width:{Math.min(budgetStatus.pct_used, 100)}%"></div>
          </div>
          {#if budgetStatus.over_budget}
            <div class="budget-warning">Projected to exceed budget by {fmtCost(budgetStatus.projected_monthly - budgetStatus.monthly_budget)}</div>
          {/if}
          <button class="budget-edit-btn" onclick={() => { editingBudget = true; budgetInput = String(budgetStatus?.monthly_budget ?? 0); }}>Change Budget</button>
        </div>
      {:else}
        <div class="budget-empty">
          <span>No monthly budget set</span>
          <button class="budget-set-btn" onclick={() => { editingBudget = true; budgetInput = "200"; }}>Set Budget</button>
        </div>
      {/if}
      {#if editingBudget}
        <div class="budget-edit">
          <input type="number" min="0" step="10" bind:value={budgetInput} placeholder="Monthly budget ($)" class="budget-input" />
          <button class="budget-save-btn" onclick={saveBudget}>Save</button>
          <button class="budget-cancel-btn" onclick={() => editingBudget = false}>Cancel</button>
        </div>
      {/if}
    </div>
  </div>

  <div class="charts-row">
    <div class="card">
      <h3 class="card-title">Cost by Type</h3>
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
    </div>

    {#if modelCosts.length > 0}
      <div class="card">
        <h3 class="card-title">Cost per Model</h3>
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
      </div>
    {/if}
  </div>

  {#if costByProject.length > 0}
    <div class="card">
      <h3 class="card-title">Cost by Project</h3>
      <div class="chart-container">
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
  .stats-row { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; }
  .charts-row { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }

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

  .chart-container { height: 250px; }

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

  .budget-forecast-row { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .forecast-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
  .forecast-item { display: flex; flex-direction: column; gap: 3px; }
  .fg-label { font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); }
  .fg-value { font-size: 18px; font-weight: 700; color: var(--text-primary); font-variant-numeric: tabular-nums; }
  .fg-value.accent { color: var(--accent); }

  .budget-info { display: flex; flex-direction: column; gap: 8px; }
  .budget-header { display: flex; justify-content: space-between; align-items: center; }
  .budget-amount { font-size: 14px; font-weight: 600; color: var(--text-primary); }
  .budget-pct { font-size: 14px; font-weight: 700; color: var(--success); font-variant-numeric: tabular-nums; }
  .budget-pct.over { color: var(--danger); }
  .budget-bar-track { height: 8px; background: var(--bg-elevated); border-radius: 99px; overflow: hidden; }
  .budget-bar-fill { height: 100%; background: var(--success); border-radius: 99px; transition: width 0.4s var(--ease); }
  .budget-bar-fill.warning { background: var(--warning); }
  .budget-bar-fill.danger { background: var(--danger); }
  .budget-warning { font-size: 11px; color: var(--danger); font-weight: 600; }
  .budget-edit-btn { font-size: 11px; color: var(--text-muted); background: none; border: none; cursor: pointer; text-decoration: underline; padding: 0; align-self: flex-start; }
  .budget-empty { display: flex; align-items: center; justify-content: space-between; font-size: 13px; color: var(--text-muted); }
  .budget-set-btn { font-size: 11px; font-weight: 600; letter-spacing: 0.04em; color: var(--accent-fg); background: var(--accent); border: 1px solid var(--accent); border-radius: var(--radius-sm); padding: 6px 14px; cursor: pointer; transition: opacity 0.15s ease, transform 0.15s ease; }
  .budget-set-btn:hover { opacity: 0.9; transform: translateY(-1px); }
  .budget-edit { display: flex; gap: 8px; align-items: center; margin-top: 10px; padding-top: 10px; border-top: 1px solid var(--border); }
  .budget-input { flex: 1; padding: 7px 12px; font: inherit; font-size: 12px; background: var(--bg-input); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-primary); outline: none; transition: border-color 0.15s ease; }
  .budget-input:focus { border-color: var(--accent); }
  .budget-save-btn { font-size: 11px; font-weight: 700; letter-spacing: 0.04em; color: var(--accent-fg); background: var(--accent); border: 1px solid var(--accent); border-radius: var(--radius-sm); padding: 7px 16px; cursor: pointer; transition: opacity 0.15s ease, transform 0.15s ease; }
  .budget-save-btn:hover { opacity: 0.9; transform: translateY(-1px); }
  .budget-cancel-btn { font-size: 11px; font-weight: 500; color: var(--text-secondary); background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 7px 14px; cursor: pointer; transition: color 0.15s ease, border-color 0.15s ease; }
  .budget-cancel-btn:hover { color: var(--text-primary); border-color: var(--border-hover); }
</style>

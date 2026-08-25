<script lang="ts">
  import { onMount } from "svelte";
  import Chart from "../components/Chart.svelte";
  import { sessions, selectedAnalyticsProviderScope } from "../lib/stores";
  import { providerMatchesAnalyticsScope } from "../lib/access";
  import { fmtCost, fmtExactCost, fmtTokens, fmtPct, monetaryValueLabel } from "../lib/utils";
  import {
    getCostsBundle,
    getCostTotals,
    getBudgetStatus,
    setBudget,
  } from "../lib/api";
  import type {
    HistoricalSession,
    CostBasis,
    CostForecast,
    CostTotals,
    BudgetStatus,
    DailyStat,
  } from "../lib/api";
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
  let dailyUsage = $state<DailyStat[]>([]);
  let editingBudget = $state(false);
  let budgetInput = $state("");
  let loading = $state(true);
  let hasLoaded = $state(false);
  let loadError = $state<string | null>(null);

  function costSourceLabel(source: string): string {
    const normalized = source.trim().toLowerCase();
    if (normalized === "provider_billed" || normalized === "provider-billed") return "Provider billing";
    if (normalized.includes("codex")) return "Codex API-equivalent rates";
    if (normalized.includes("anthropic")) return "Anthropic API-equivalent rates";
    if (normalized.includes("pricing")) return "Versioned pricing";
    if (normalized === "legacy-calculated") return "Migrated calculated history";
    return source.replaceAll("_", " ");
  }

  function themeColor(token: string): string {
    return getComputedStyle(document.documentElement).getPropertyValue(token).trim();
  }

  const costColumns: ExportColumn[] = [
    { key: "project", label: "Project", enabled: true },
    { key: "model", label: "Model", enabled: true },
    { key: "branch", label: "Branch", enabled: false },
    { key: "input_tokens", label: "Input Tokens", enabled: true },
    { key: "output_tokens", label: "Output Tokens", enabled: true },
    { key: "cache_write_tokens", label: "Cache Write", enabled: true },
    { key: "cache_read_tokens", label: "Cache Read", enabled: true },
    { key: "tokens", label: "Total Tokens", enabled: true },
    { key: "input_cost", label: "Input Monetary Value (USD)", enabled: true },
    { key: "output_cost", label: "Output Monetary Value (USD)", enabled: true },
    { key: "cache_write_cost", label: "Cache Write Monetary Value (USD)", enabled: true },
    { key: "cache_read_cost", label: "Cache Read Monetary Value (USD)", enabled: true },
    { key: "cost", label: "Known Monetary Value (USD)", enabled: true },
    { key: "cost_basis", label: "Monetary Value Basis", enabled: true },
    { key: "cost_source", label: "Monetary Value Source", enabled: true },
  ];

  let histSessions = $state<HistoricalSession[]>([]);
  const DETAIL_PAGE_SIZE = 50;
  let visibleDetailLimit = $state(DETAIL_PAGE_SIZE);
  let loadGeneration = 0;
  let totalsGeneration = 0;
  let lastTotalsKey = "";
  let lastTotalsScope = "";
  // Request bookkeeping only; making it reactive would let effects subscribe
  // to their own in-flight toggle and schedule redundant refresh passes.
  let bundleRefreshing = false;

  function totalsScopeKey(
    provider: typeof $selectedAnalyticsProviderScope,
    project: string,
  ): string {
    return `${provider}:${project}`;
  }

  async function loadData(): Promise<void> {
    const generation = ++loadGeneration;
    const provider = $selectedAnalyticsProviderScope;
    const project = projectFilter;
    const scopeKey = totalsScopeKey(provider, project);
    const requestKey = `${scopeKey}:${monetaryFingerprint}`;
    if (hasLoaded && scopeKey !== lastTotalsScope) {
      histSessions = [];
      forecast = null;
      budgetStatus = null;
      totals = null;
      dailyUsage = [];
    }
    bundleRefreshing = true;
    loading = !hasLoaded;
    loadError = null;
    showExport = false;
    try {
      const bundle = await getCostsBundle(project || undefined, provider, requestKey);
      if (
        generation !== loadGeneration
        || provider !== $selectedAnalyticsProviderScope
        || project !== projectFilter
      ) {
        return;
      }
      histSessions = bundle.history;
      visibleDetailLimit = DETAIL_PAGE_SIZE;
      forecast = bundle.forecast;
      budgetStatus = bundle.budget;
      totals = bundle.totals;
      dailyUsage = bundle.daily_usage;
      lastTotalsKey = requestKey;
      lastTotalsScope = scopeKey;
      hasLoaded = true;
    } catch (error) {
      if (generation !== loadGeneration) return;
      loadError = error instanceof Error && error.message
        ? `Cost data unavailable. ${error.message}`
        : "Cost data unavailable. Pulse could not load your usage ledger.";
    } finally {
      if (generation === loadGeneration) {
        loading = false;
        bundleRefreshing = false;
      }
    }
  }

  /**
   * Reloads only the window aggregate.
   *
   * Two things invalidate it while the view is open: switching the project
   * filter, and a live session accruing cost. Without this the KPIs would keep
   * describing whatever the first `onMount` fetch saw, and a filtered project
   * would fall back to summing the capped table page.
   */
  async function refreshTotals(
    project: string,
    provider: typeof $selectedAnalyticsProviderScope,
  ): Promise<void> {
    const generation = ++totalsGeneration;
    const scopeKey = totalsScopeKey(provider, project);
    const requestKey = `${scopeKey}:${monetaryFingerprint}`;
    if (project === projectFilter && provider === $selectedAnalyticsProviderScope) {
      loadError = null;
    }
    try {
      const next = await getCostTotals(30, project || undefined, provider, requestKey);
      // Ignore a response that lost the race against a newer filter selection.
      if (
        generation === totalsGeneration
        && project === projectFilter
        && provider === $selectedAnalyticsProviderScope
      ) {
        totals = next;
        lastTotalsKey = requestKey;
        lastTotalsScope = scopeKey;
      }
    } catch (error) {
      if (
        generation === totalsGeneration
        && project === projectFilter
        && provider === $selectedAnalyticsProviderScope
      ) {
        loadError = error instanceof Error && error.message
          ? `Cost data unavailable. ${error.message}`
          : "Cost data unavailable. Pulse could not load your usage ledger.";
      }
    }
  }

  async function saveBudget(): Promise<void> {
    const val = parseFloat(budgetInput);
    if (!isNaN(val) && val >= 0) {
      await setBudget(val);
      budgetStatus = await getBudgetStatus($selectedAnalyticsProviderScope);
    }
    editingBudget = false;
  }

  let projectFilter = $state("");
  let monetaryFingerprint = $derived.by(() =>
    $sessions
      .filter((session) =>
        providerMatchesAnalyticsScope(session.provider, $selectedAnalyticsProviderScope),
      )
      .map((session) => [
        session.session_id,
        session.cost_available === true ? session.cost : "unavailable",
        session.cost_basis ?? "unavailable",
        session.input_tokens,
        session.output_tokens,
        session.cache_write_tokens,
        session.cache_read_tokens,
      ].join(":"))
      .sort()
      .join("|"),
  );

  onMount(() => { loadData(); });
  let previousProviderScope: string | undefined;
  $effect(() => {
    const provider = $selectedAnalyticsProviderScope;
    if (previousProviderScope !== undefined && provider !== previousProviderScope) {
      void loadData();
    }
    previousProviderScope = provider;
  });

  // Live session deltas and filter changes both invalidate the aggregate.
  $effect(() => {
    const project = projectFilter;
    const provider = $selectedAnalyticsProviderScope;
    const scopeKey = totalsScopeKey(provider, project);
    const key = `${scopeKey}:${monetaryFingerprint}`;
    if (hasLoaded && scopeKey !== lastTotalsScope) {
      // A value from another project/provider cannot be shown under the new
      // filter. Live deltas inside the same scope still retain the last
      // verified aggregate while the refresh runs.
      totals = null;
    }
    if (hasLoaded && !bundleRefreshing && key !== lastTotalsKey) {
      void refreshTotals(project, provider);
    }
  });

  let allSessions = $derived.by(() => {
    const live = $sessions
      .filter((session) =>
        providerMatchesAnalyticsScope(session.provider, $selectedAnalyticsProviderScope),
      )
      .map((s) => ({
      id: s.session_id, project: s.project, model: s.model, branch: s.branch,
      cost: s.cost_available === true ? s.cost : null,
      cost_basis: s.cost_basis ?? "unavailable",
      cost_source: s.cost_available === true ? "live_session" : "unknown",
      tokens: s.tokens, input_tokens: s.input_tokens, output_tokens: s.output_tokens,
      cache_write_tokens: s.cache_write_tokens, cache_read_tokens: s.cache_read_tokens,
      input_cost: s.input_cost, output_cost: s.output_cost,
      cache_write_cost: s.cache_write_cost, cache_read_cost: s.cache_read_cost,
      is_active: true,
    }));
    const consumedHistory = new Set<number>();
    for (const liveSession of live) {
      const exactIndex = histSessions.findIndex(
        (history, index) => !consumedHistory.has(index) && history.id === liveSession.id,
      );
      if (exactIndex >= 0) {
        consumedHistory.add(exactIndex);
        continue;
      }
      const structuralMatches = histSessions
        .map((history, index) => ({ history, index }))
        .filter(({ history, index }) =>
          !consumedHistory.has(index)
          && history.is_active
          && history.project === liveSession.project
          && history.model === liveSession.model
          && (history.branch ?? "") === (liveSession.branch ?? ""),
        );
      // A single active structural match is the database mirror of this live
      // session. Multiple matches are ambiguous and must remain visible rather
      // than silently discarding a potentially distinct session.
      if (structuralMatches.length === 1) {
        consumedHistory.add(structuralMatches[0].index);
      }
    }
    const hist = histSessions
      .filter((_, index) => !consumedHistory.has(index))
      .map((h) => ({
        id: h.id, project: h.project, model: h.model, branch: h.branch,
        cost: h.known_cost, cost_basis: h.cost_basis, cost_source: h.cost_source,
        tokens: h.total_tokens, input_tokens: h.input_tokens, output_tokens: h.output_tokens,
        cache_write_tokens: h.cache_write_tokens, cache_read_tokens: h.cache_read_tokens,
        input_cost: h.input_cost, output_cost: h.output_cost,
        cache_write_cost: h.cache_write_cost, cache_read_cost: h.cache_read_cost,
        is_active: false,
      }));
    return [...live, ...hist];
  });

  let projects = $derived([...new Set(allSessions.map((s) => s.project))].sort());
  let filtered = $derived(projectFilter ? allSessions.filter((s) => s.project === projectFilter) : allSessions);
  let costExportRows = $derived(
    [...filtered]
      .sort((a, b) => (b.cost ?? -1) - (a.cost ?? -1))
      .map((s) => ({ ...s } as Record<string, unknown>)),
  );
  let sortedFiltered = $derived(
    [...filtered].sort((a, b) => (b.cost ?? -1) - (a.cost ?? -1)),
  );
  let visibleCostRows = $derived(sortedFiltered.slice(0, visibleDetailLimit));

  /** The aggregate is fetched for the active filter, so it always describes
   *  the same population the KPIs claim. Summing `filtered` is only a fallback
   *  for the first paint, before the aggregate arrives. */
  let hasTotals = $derived(totals !== null && totals.days > 0);
  let costAvailable = $derived(
    totals !== null
      && totals.cost_basis !== "unavailable"
      && totals.priced_sessions > 0,
  );
  let totalCost = $derived(
    hasTotals && totals ? totals.total_cost : filtered.reduce((sum, s) => sum + (s.cost ?? 0), 0),
  );
  let sessionCount = $derived(hasTotals && totals ? totals.sessions : filtered.length);
  let pricedSessionCount = $derived(
    hasTotals && totals
      ? totals.priced_sessions
      : filtered.filter((session) => session.cost !== null).length,
  );
  let avgCost = $derived(pricedSessionCount ? totalCost / pricedSessionCount : 0);
  let derivedRatesAvailable = $derived(
    totals
      ? totals.cost_basis === "exact"
      : filtered.every((session) => session.cost !== null),
  );
  // Per 1M tokens: at real usage the per-1K figure rounds to $0.00, so 1M is the
  // meaningful unit (e.g. $0.67 / 1M rather than $0.00 / 1K).
  let costPerMToken = $derived.by(() => {
    const tot = hasTotals && totals
      ? totals.total_tokens
      : filtered.reduce((s, x) => s + x.tokens, 0);
    return tot > 0 ? (totalCost / tot) * 1_000_000 : 0;
  });

  let usageTotalTokens = $derived(
    hasTotals && totals
      ? totals.total_tokens
      : filtered.reduce((sum, session) => sum + session.tokens, 0),
  );
  let usageInputTokens = $derived(
    hasTotals && totals
      ? totals.input_tokens
      : filtered.reduce((sum, session) => sum + session.input_tokens, 0),
  );
  let usageOutputTokens = $derived(
    hasTotals && totals
      ? totals.output_tokens
      : filtered.reduce((sum, session) => sum + session.output_tokens, 0),
  );
  let usageCacheWriteTokens = $derived(
    hasTotals && totals
      ? totals.cache_write_tokens
      : filtered.reduce((sum, session) => sum + session.cache_write_tokens, 0),
  );
  let usageCacheReadTokens = $derived(
    hasTotals && totals
      ? totals.cache_read_tokens
      : filtered.reduce((sum, session) => sum + session.cache_read_tokens, 0),
  );
  let cacheReusePct = $derived(
    usageTotalTokens > 0 ? (usageCacheReadTokens / usageTotalTokens) * 100 : 0,
  );
  let usageTokenMix = $derived([
    {
      label: "Input",
      value: Math.max(0, usageInputTokens - usageCacheWriteTokens - usageCacheReadTokens),
      className: "input",
    },
    { label: "Output", value: usageOutputTokens, className: "output" },
    { label: "Cache write", value: usageCacheWriteTokens, className: "cache-w" },
    { label: "Cache read", value: usageCacheReadTokens, className: "cache-r" },
  ]);
  let tokenTrend = $derived.by(() => {
    const byDate = new Map<string, number>();
    for (const day of dailyUsage) {
      if (projectFilter && day.project !== projectFilter) continue;
      byDate.set(day.date, (byDate.get(day.date) ?? 0) + day.total_tokens);
    }
    return [...byDate.entries()]
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([date, tokens]) => ({ date, tokens }));
  });
  let tokenTrendMax = $derived(Math.max(0, ...tokenTrend.map((point) => point.tokens)));

  function costCoverageLabel(basis: CostBasis): string {
    if (basis === "exact") return "Complete cost coverage";
    if (basis === "partial") return "Partial cost coverage";
    if (basis === "estimated") return "API-equivalent estimate";
    return "Cost not reported by provider";
  }

  let totalInputCost = $derived(
    hasTotals && totals ? totals.input_cost : filtered.reduce((s, x) => s + x.input_cost, 0),
  );
  let totalOutputCost = $derived(
    hasTotals && totals ? totals.output_cost : filtered.reduce((s, x) => s + x.output_cost, 0),
  );
  let totalCacheWCost = $derived(
    hasTotals && totals
      ? totals.cache_write_cost
      : filtered.reduce((s, x) => s + x.cache_write_cost, 0),
  );
  let totalCacheRCost = $derived(
    hasTotals && totals
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
    if (hasTotals && totals) {
      // Rate and token counts both come from the window aggregate, so the two
      // sides of the multiplication are always the same population.
      if (totals.pure_input_tokens <= 0 || totals.input_cost <= 0) return 0;
      const rate = totals.input_cost / totals.pure_input_tokens;
      return Math.max(0, totals.cache_read_tokens * rate - totals.cache_read_cost);
    }
    const cacheReadTokens = filtered.reduce((s, x) => s + x.cache_read_tokens, 0);
    const pureInput = filtered.reduce((s, x) => s + Math.max(0, x.input_tokens - x.cache_write_tokens - x.cache_read_tokens), 0);
    if (pureInput <= 0 || totalInputCost <= 0) return 0;
    const inputCostRate = totalInputCost / pureInput;
    return Math.max(0, cacheReadTokens * inputCostRate - totalCacheRCost);
  });

  let costByProject = $derived.by(() => {
    if (!costAvailable) return [];
    // Window-wide when unfiltered, so the bars reconcile with the value total.
    if (hasTotals && totals) {
      return totals.by_project.map((p) => [p.label, p.cost] as [string, number]);
    }
    const map: Record<string, number> = {};
    filtered.forEach((s) => (map[s.project] = (map[s.project] || 0) + (s.cost ?? 0)));
    return Object.entries(map).sort((a, b) => b[1] - a[1]);
  });

  let modelCosts = $derived.by(() => {
    if (!costAvailable) return [];
    if (hasTotals && totals) {
      return totals.by_model.map((m) => [m.label, m.cost] as [string, number]);
    }
    const map: Record<string, number> = {};
    filtered.forEach((s) => (map[s.model] = (map[s.model] || 0) + (s.cost ?? 0)));
    return Object.entries(map).sort((a, b) => b[1] - a[1]);
  });

  let costChartConfig: ChartConfiguration = {
    type: "bar",
    data: { labels: [], datasets: [{
      data: [],
      backgroundColor: () => themeColor("--chart-1"),
      hoverBackgroundColor: () => themeColor("--accent"),
      borderRadius: 6,
      borderSkipped: false,
      maxBarThickness: 34,
      barPercentage: 0.86,
      categoryPercentage: 0.82,
    }] },
    options: {
      responsive: true, maintainAspectRatio: false, indexAxis: "y",
      layout: { padding: { right: 10 } },
      animation: { duration: 320 },
      scales: {
        x: {
          border: { display: false },
          grid: { color: () => themeColor("--divider"), drawTicks: false },
          ticks: { callback: (v: any) => "$" + Number(v).toFixed(2), maxTicksLimit: 6, padding: 6 },
        },
        y: { border: { display: false }, grid: { display: false }, ticks: { padding: 6 } },
      },
      plugins: {
        legend: { display: false },
        tooltip: {
          padding: 10,
          displayColors: false,
          callbacks: { label: (c: any) => fmtCost(c.raw as number) },
        },
      },
    },
  };

  function updateCostChart(chart: ChartType): void {
    chart.data.labels = costByProject.map((e) => e[0]);
    chart.data.datasets[0].data = costByProject.map((e) => e[1]);
  }
</script>

<div class="costs-view app-view">
  <div class="view-header">
    <div>
      <h1 class="view-title">Usage &amp; cost</h1>
      <p class="view-subtitle">Provider-billed spend and API-equivalent value, kept as separate facts.</p>
    </div>
    <div class="filters">
      <!-- Explicit handler rather than `bind:value`: the selection drives a
           backend refetch, so the assignment needs to be visible at the seam
           the aggregate depends on. -->
      <select
        aria-label="Filter costs by project"
        value={projectFilter}
        onchange={(event) => {
          projectFilter = event.currentTarget.value;
          visibleDetailLimit = DETAIL_PAGE_SIZE;
        }}
      >
        <option value="">All Projects</option>
        {#each projects as p}<option value={p}>{p}</option>{/each}
      </select>
    </div>
  </div>

  {#if loading}
    <section class="load-state" aria-live="polite">
      <strong>Loading cost ledger</strong>
      <span>Adding up the current month and previous 30 days.</span>
    </section>
  {:else if loadError && !hasLoaded}
    <section class="load-state error" role="alert">
      <strong>Cost data unavailable</strong>
      <span>{loadError.replace(/^Cost data unavailable\.\s*/, "")}</span>
      <button type="button" onclick={loadData}>Retry</button>
    </section>
  {:else}
    {#if loadError}
      <section class="load-state error" role="status">
        <strong>Showing the last verified ledger</strong>
        <span>{loadError.replace(/^Cost data unavailable\.\s*/, "")}</span>
        <button type="button" onclick={loadData}>Retry</button>
      </section>
    {/if}
    <section class="value-ledger" aria-label="Subscription value ledger">
      <header class="ledger-head">
        <div>
          <span class="ledger-eyebrow">Last 30 days · provenance-aware value</span>
          <h2>Known monetary value by provenance</h2>
          <p>Provider-billed amounts and API-equivalent estimates stay distinct. Every value comes from observed usage; unavailable billing is never guessed.</p>
        </div>
        <span
          class="coverage-pill"
          class:partial={totals?.cost_basis === "partial"}
          class:estimated={totals?.cost_basis === "estimated"}
          class:unavailable={totals?.cost_basis === "unavailable"}
        >
          {costCoverageLabel(totals?.cost_basis ?? "unavailable")}
        </span>
      </header>

      <div class="ledger-metrics">
        <div class="ledger-metric">
          <span class="ledger-label">Provider-billed · 30d</span>
          <strong class="ledger-value">
            {fmtExactCost(totals?.billed_spend_usd ?? 0, totals?.billed_spend_usd !== null && totals?.billed_spend_usd !== undefined)}
          </strong>
          <small>{totals?.billed_sessions ?? 0} provider readbacks</small>
        </div>
        <div class="ledger-metric">
          <span class="ledger-label">API-equivalent · 30d</span>
          <strong class="ledger-value">
            {fmtExactCost(totals?.api_equivalent_usd ?? 0, totals?.api_equivalent_usd !== null && totals?.api_equivalent_usd !== undefined)}
          </strong>
          <small>{totals?.api_equivalent_sessions ?? 0} priced from published rates</small>
        </div>
        <div class="ledger-metric">
          <span class="ledger-label">Total tokens</span>
          <strong class="ledger-value">{fmtTokens(usageTotalTokens)}</strong>
          <small>Across the selected scope</small>
        </div>
        <div class="ledger-metric">
          <span class="ledger-label">Sessions</span>
          <strong class="ledger-value">{sessionCount}</strong>
          <small>{projectFilter || "All projects"}</small>
        </div>
        <div class="ledger-metric">
          <span class="ledger-label">Cache reuse</span>
          <strong class="ledger-value">{fmtPct(cacheReusePct)}</strong>
          <small>{fmtTokens(usageCacheReadTokens)} cache-read tokens</small>
        </div>
        <div class="ledger-metric">
          <span class="ledger-label">Cost coverage</span>
          <strong class="ledger-value">{pricedSessionCount} / {sessionCount}</strong>
          <small>
            {#if !costAvailable}
              Not reported
            {:else if totals?.cost_basis === "estimated"}
              {fmtCost(totalCost)} estimated
            {:else}
              {fmtCost(totalCost)} {monetaryValueLabel(totals?.cost_sources ?? []).toLowerCase()}{totals?.cost_basis === "partial" ? " lower bound" : ""}
            {/if}
          </small>
        </div>
      </div>

      <div class="usage-ledger-grid">
        <section class="usage-token-mix">
          <div class="ledger-section-head">
            <h3>Token mix</h3>
            <span>{fmtTokens(usageTotalTokens)} observed</span>
          </div>
          <div class="token-mix-bar" role="img" aria-label="Token mix">
            {#each usageTokenMix as item}
              <span
                class={item.className}
                style="width:{usageTotalTokens > 0 ? (item.value / usageTotalTokens) * 100 : 0}%"
              ></span>
            {/each}
          </div>
          <div class="token-mix-legend">
            {#each usageTokenMix as item}
              <div>
                <span class="mix-dot {item.className}"></span>
                <small>{item.label}</small>
                <strong>{fmtTokens(item.value)}</strong>
              </div>
            {/each}
          </div>
        </section>

        <section class="token-trend">
          <div class="ledger-section-head">
            <h3>Token trend</h3>
            <span>Provider scope · 30d</span>
          </div>
          {#if tokenTrend.length > 0}
            <div class="trend-bars" aria-label="Daily token trend">
              {#each tokenTrend as point}
                <span
                  class="trend-bar"
                  style="height:{tokenTrendMax > 0 ? Math.max(4, (point.tokens / tokenTrendMax) * 100) : 0}%"
                  title={`${point.date} · ${fmtTokens(point.tokens)} tokens`}
                ></span>
              {/each}
            </div>
          {:else}
            <div class="trend-empty">Daily token trend appears once history builds up.</div>
          {/if}
        </section>
      </div>
    </section>

    {#if costAvailable}
      <BudgetCockpit
        {forecast}
        budget={budgetStatus}
        onSetBudget={() => {
          editingBudget = true;
          budgetInput = budgetStatus?.monthly_budget
            ? String(budgetStatus.monthly_budget)
            : "";
        }}
      />

      {#if totals && totals.cost_basis === "partial"}
        <section class="coverage-strip" aria-live="polite">
          <div>
            <strong>Coverage details</strong>
            <span>
              {totals.priced_sessions} of {totals.sessions} sessions have a known monetary value; this is a lower bound and excludes unpriced sessions.
            </span>
          </div>
          {#if totals.cost_sources.length > 0}
            <span class="coverage-source">{totals.cost_sources.map(costSourceLabel).join(" · ")}</span>
          {/if}
        </section>
      {/if}

      {#if forecast?.priced_sessions && (forecast.billed_spend_usd ?? 0) === 0 && (forecast.api_equivalent_usd ?? 0) === 0 && totalCost > 0}
        <p class="window-context">
          No month-to-date monetary value is available. The previous 30 days include
          {fmtCost(totalCost)} from earlier sessions with known provenance.
        </p>
      {/if}

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
      <span class="is-label">Value / session</span>
      <span class="is-value">{costAvailable ? fmtCost(avgCost) : "—"}</span>
       <span class="is-meta">
         {costAvailable
           ? `${pricedSessionCount}/${sessionCount} priced`
           : "cost not reported"}
       </span>
    </div>
    <div class="is-item">
      <span class="is-label">Value / 1M tokens</span>
      <span class="is-value">{costAvailable && derivedRatesAvailable ? fmtCost(costPerMToken) : "—"}</span>
      <span class="is-meta">{costAvailable && derivedRatesAvailable ? "blended rate" : "requires complete coverage"}</span>
    </div>
    <div class="is-item">
      <span class="is-label">Cache savings</span>
      <span class="is-value">{costAvailable && derivedRatesAvailable ? fmtCost(cacheSavings) : "—"}</span>
      <span class="is-meta">{costAvailable && derivedRatesAvailable ? "vs uncached input" : "requires complete coverage"}</span>
    </div>
    <div class="is-item">
      <span class="is-label">{monetaryValueLabel(totals?.cost_sources ?? [])} (30d)</span>
      <span class="is-value">{costAvailable ? fmtCost(totalCost) : "—"}</span>
      <span class="is-meta">{costAvailable ? (totals?.cost_basis === "partial" ? "known lower bound" : "window total") : "monetary value unavailable"}</span>
    </div>
      </div>

      <div class="charts-row">
    <section class="pane">
      <h2 class="pane-title">Monetary value by type</h2>
      {#if costAvailable && costTotal > 0}
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
        <div class="empty-hint">
          {costAvailable ? "No monetary value recorded in this window" : "Monetary value unavailable for this window"}
        </div>
      {/if}
    </section>

    {#if modelCosts.length > 0}
      <section class="pane">
        <h2 class="pane-title">Monetary value per model</h2>
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
        <div class="card surface-matte">
          <h2 class="card-title">Monetary value by project</h2>
          <div
            class="chart-container"
            style="height: {Math.max(140, Math.min(360, 44 + costByProject.length * 44))}px"
          >
            <Chart config={costChartConfig} updateData={updateCostChart} />
          </div>
        </div>
      {/if}
    {:else}
      <section class="cost-boundary" aria-live="polite">
        <div>
          <strong>Cost not reported by provider</strong>
          <span>Pulse keeps the usage ledger complete and does not guess subscription spend.</span>
        </div>
        <span>{pricedSessionCount} of {sessionCount} sessions priced</span>
      </section>
    {/if}

    <div class="card surface-matte">
    <div class="card-title-row">
      <h2 class="card-title">Session details</h2>
      {#if filtered.length > 0}
        <button class="export-btn" onclick={() => showExport = true}>Export</button>
      {/if}
    </div>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="detail-table" role="region" tabindex="0" aria-label="Scrollable session cost details">
      <div class="dt-header">
        <span class="dt-col status"></span>
        <span class="dt-col project">Project</span>
        <span class="dt-col">Input</span>
        <span class="dt-col">Output</span>
        <span class="dt-col">Cache W</span>
        <span class="dt-col">Cache R</span>
        <span class="dt-col">Tokens</span>
        <span class="dt-col cost">Monetary value</span>
      </div>
      {#each visibleCostRows as s (s.id)}
        <div class="dt-row">
          <span class="dt-col status"><span class="status-dot" class:active={s.is_active}></span></span>
          <span class="dt-col project">{s.project}{s.branch ? " · " + s.branch : ""}</span>
          <span class="dt-col">{fmtTokens(s.input_tokens)}</span>
          <span class="dt-col">{fmtTokens(s.output_tokens)}</span>
          <span class="dt-col">{fmtTokens(s.cache_write_tokens)}</span>
          <span class="dt-col">{fmtTokens(s.cache_read_tokens)}</span>
          <span class="dt-col">{fmtTokens(s.tokens)}</span>
          <span class="dt-col cost" class:unavailable={s.cost === null}>
            {s.cost === null ? "Not reported" : fmtCost(s.cost)}
          </span>
        </div>
      {:else}
        <div class="dt-empty">No session data yet</div>
      {/each}
    </div>
    {#if sortedFiltered.length > visibleCostRows.length}
      <button
        class="show-more"
        type="button"
        onclick={() => (visibleDetailLimit += DETAIL_PAGE_SIZE)}
      >
        Show {Math.min(DETAIL_PAGE_SIZE, sortedFiltered.length - visibleCostRows.length)} more
      </button>
    {/if}
    </div>
  {/if}
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
  .costs-view { display: flex; flex-direction: column; gap: var(--page-gap); }
  .view-header { display: flex; align-items: flex-end; gap: 20px; flex-wrap: wrap; }
  .view-title { font-size: 20px; font-weight: 700; }
  .view-subtitle { margin-top: 3px; color: var(--text-muted); font-size: var(--fs-xs); }
  .filters { margin-left: auto; }
  .value-ledger {
    display: grid;
    gap: 22px;
    padding: 22px;
    /* Matte hero surface only. The old info-tinted gradient fill broke the
       "matte is the only app-panel fill" contract and read as a different
       product from the rest of the app. */
    position: relative;
    background:
      var(--panel-sheen-strong),
      var(--surface-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--elev-2);
  }
  .value-ledger::before {
    content: "";
    position: absolute;
    inset: 0 0 auto 0;
    height: 1px;
    background: var(--panel-edge);
    border-radius: var(--radius-lg) var(--radius-lg) 0 0;
    pointer-events: none;
  }
  .ledger-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 24px;
  }
  .ledger-head > div { min-width: 0; }
  .ledger-eyebrow,
  .ledger-label {
    color: var(--text-muted);
    font-size: var(--fs-xs);
    font-weight: 700;
    letter-spacing: var(--letter-wider);
    text-transform: uppercase;
  }
  .ledger-head h2 {
    margin-top: 6px;
    font-size: clamp(20px, 2.4vw, 28px);
    letter-spacing: var(--letter-tight);
  }
  .ledger-head p {
    max-width: 680px;
    margin-top: 7px;
    color: var(--text-muted);
    font-size: var(--fs-sm);
    line-height: var(--lh-relaxed);
  }
  .coverage-pill {
    flex: 0 0 auto;
    padding: 6px 10px;
    color: var(--success);
    background: var(--success-dim);
    border: 1px solid color-mix(in srgb, var(--success) 35%, var(--border));
    border-radius: var(--radius-full);
    font-size: var(--fs-xs);
    font-weight: 650;
    white-space: nowrap;
  }
  .coverage-pill.partial {
    color: var(--warning);
    background: var(--warning-dim);
    border-color: color-mix(in srgb, var(--warning) 35%, var(--border));
  }
  .coverage-pill.estimated {
    color: var(--info);
    background: var(--info-dim);
    border-color: color-mix(in srgb, var(--info) 35%, var(--border));
  }
  .coverage-pill.unavailable {
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border-color: var(--border);
  }
  .ledger-metrics {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    border-block: 1px solid var(--divider);
  }
  .ledger-metric {
    min-width: 0;
    display: grid;
    gap: 5px;
    padding: 16px 18px;
    border-left: 1px solid var(--divider);
  }
  .ledger-metric:first-child { padding-left: 0; border-left: none; }
  .ledger-metric:nth-child(4) { padding-left: 0; border-left: none; border-top: 1px solid var(--divider); }
  .ledger-metric:nth-child(n+5) { border-top: 1px solid var(--divider); }
  .ledger-value {
    overflow: hidden;
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: clamp(22px, 2.5vw, 31px);
    line-height: 1.05;
    letter-spacing: var(--letter-tight);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ledger-metric small { color: var(--text-muted); font-size: var(--fs-xs); }
  .usage-ledger-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.18fr) minmax(280px, 0.82fr);
    gap: 28px;
  }
  .usage-token-mix,
  .token-trend {
    min-width: 0;
    display: grid;
    align-content: start;
    gap: 13px;
  }
  .usage-ledger-grid > section + section {
    padding-left: 28px;
    border-left: 1px solid var(--divider);
  }
  .ledger-section-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
  }
  .ledger-section-head h3 {
    color: var(--text-primary);
    font-size: var(--fs-sm);
    font-weight: 700;
  }
  .ledger-section-head span {
    overflow: hidden;
    color: var(--text-muted);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .token-mix-bar {
    width: 100%;
    height: 10px;
    display: flex;
    overflow: hidden;
    background: var(--bg-elevated);
    border-radius: var(--radius-full);
  }
  .token-mix-bar > span { min-width: 0; height: 100%; }
  .token-mix-bar > .input,
  .mix-dot.input { background: var(--info); }
  .token-mix-bar > .output,
  .mix-dot.output { background: var(--token-output); }
  .token-mix-bar > .cache-w,
  .mix-dot.cache-w { background: var(--token-cache-write); }
  .token-mix-bar > .cache-r,
  .mix-dot.cache-r { background: var(--token-cache-read); }
  .token-mix-legend {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 9px 20px;
  }
  .token-mix-legend > div {
    min-width: 0;
    display: grid;
    grid-template-columns: 7px minmax(0, 1fr) auto;
    align-items: center;
    gap: 7px;
  }
  .mix-dot { width: 7px; height: 7px; border-radius: 50%; }
  .token-mix-legend small {
    overflow: hidden;
    color: var(--text-muted);
    font-size: var(--fs-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .token-mix-legend strong {
    color: var(--text-secondary);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
  }
  .trend-bars {
    height: 74px;
    display: flex;
    align-items: flex-end;
    gap: 3px;
    padding-top: 4px;
    border-bottom: 1px solid var(--divider);
  }
  .trend-bar {
    min-width: 2px;
    flex: 1 1 0;
    background: color-mix(in srgb, var(--info) 72%, var(--text-primary));
    border-radius: 2px 2px 0 0;
  }
  .trend-empty {
    min-height: 74px;
    display: grid;
    place-items: center;
    padding: 12px;
    color: var(--text-muted);
    font-size: var(--fs-xs);
    text-align: center;
    border: 1px dashed var(--border);
    border-radius: var(--radius-sm);
  }
  .cost-boundary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 14px 16px;
    color: var(--text-muted);
    background: color-mix(in srgb, var(--bg-elevated) 72%, transparent);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }
  .cost-boundary > div { display: grid; gap: 4px; }
  .cost-boundary strong { color: var(--text-primary); font-size: var(--fs-sm); }
  .cost-boundary span { font-size: var(--fs-xs); line-height: 1.45; }
  .cost-boundary > span {
    flex: 0 0 auto;
    font-family: var(--font-mono);
    white-space: nowrap;
  }
  .load-state {
    min-height: 170px;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 8px;
    padding: 28px;
    color: var(--text-muted);
    text-align: center;
    border-block: 1px solid var(--border);
  }
  .load-state strong { color: var(--text-primary); font-size: var(--fs-lg); }
  .load-state span { max-width: 540px; font-size: var(--fs-sm); line-height: 1.55; }
  .load-state.error strong { color: var(--danger); }
  .load-state button {
    margin-top: 6px;
    padding: 7px 14px;
    color: var(--text-primary);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .window-context {
    margin-top: -4px;
    padding: 10px 0;
    color: var(--text-muted);
    font-size: var(--fs-xs);
    border-bottom: 1px solid var(--divider);
  }
  .coverage-strip {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 12px 0;
    color: var(--text-secondary);
    border-block: 1px solid color-mix(in srgb, var(--warning) 40%, var(--border));
  }
  .coverage-strip > div { display: flex; align-items: baseline; gap: 10px; min-width: 0; }
  .coverage-strip strong { flex: 0 0 auto; color: var(--warning); font-size: var(--fs-sm); }
  .coverage-strip span { color: var(--text-muted); font-size: var(--fs-xs); line-height: 1.45; }
  .coverage-source { flex: 0 0 auto; font-family: var(--font-mono); text-align: right; }
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
    .ledger-metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .ledger-metric:nth-child(3),
    .ledger-metric:nth-child(5) { padding-left: 0; border-left: none; }
    .ledger-metric:nth-child(n+3) { border-top: 1px solid var(--divider); }
    .usage-ledger-grid { grid-template-columns: 1fr; }
    .usage-ledger-grid > section + section { padding: 20px 0 0; border-left: none; border-top: 1px solid var(--divider); }
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
  .cost-seg.output { background: var(--token-output); }
  .cost-seg.cache-w { background: var(--token-cache-write); }
  .cost-seg.cache-r { background: var(--token-cache-read); }

  .cost-type-legend { display: flex; flex-direction: column; gap: 6px; }
  .ct-row { display: flex; align-items: center; gap: 8px; font-size: 12px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .dot.input { background: var(--info); }
  .dot.output { background: var(--token-output); }
  .dot.cache-w { background: var(--token-cache-write); }
  .dot.cache-r { background: var(--token-cache-read); }
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
  .show-more { display: block; margin: 12px auto 0; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg-elevated); color: var(--text-secondary); padding: 7px 16px; font-size: 11px; font-weight: 600; cursor: pointer; }
  .show-more:hover { color: var(--accent); border-color: var(--accent); }

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

  .card { min-width: 0; }
  .detail-table { overflow-x: auto; overscroll-behavior-inline: contain; }
  .dt-header, .dt-row { min-width: 760px; }

  @media (max-width: 1050px) {
    .charts-row { grid-template-columns: 1fr; }
    .charts-row .pane + .pane { padding-left: 0; border-left: none; }
  }

  @media (max-width: 800px) {
    .view-header { align-items: stretch; flex-direction: column; gap: 10px; }
    .filters { width: 100%; margin-left: 0; }
    .filters select { width: 100%; }
  }

  @media (max-width: 620px) {
    .card { padding: 14px; }
    .budget-edit { flex-wrap: wrap; }
  }
</style>

<script lang="ts">
  import { onMount } from "svelte";
  import StatCard from "../components/StatCard.svelte";
  import SessionCard from "../components/SessionCard.svelte";
  import { sessions, selectedAnalyticsProviderScope } from "../lib/stores";
  import { providerMatchesAnalyticsScope } from "../lib/access";
  import { fmtTokens, fmtCost, fmtExactCost, fmtDuration, fmtTps, classifyActivity, fmtClock, monetaryValueLabel } from "../lib/utils";
  import { getSessionHistory, getAnalyticsSummary, searchSessions, getSessionHistoryFiltered } from "../lib/api";
  import type { HistoricalSession, AnalyticsSummary } from "../lib/api";
  import { fly } from "svelte/transition";
  import ExportModal from "../components/ExportModal.svelte";
  import type { ExportColumn } from "../lib/export";

  let showExport = $state(false);
  let expandedId = $state<string | null>(null);
  let compareMode = $state(false);
  let compareIds = $state<Set<string>>(new Set());

  const historyColumns: ExportColumn[] = [
    { key: "project", label: "Project", enabled: true },
    { key: "model", label: "Model", enabled: true },
    { key: "context_window", label: "Context", enabled: true },
    { key: "branch", label: "Branch", enabled: true },
    { key: "total_tokens", label: "Tokens", enabled: true },
    { key: "input_tokens", label: "Input Tokens", enabled: true },
    { key: "output_tokens", label: "Output Tokens", enabled: true },
    { key: "cache_write_tokens", label: "Cache Write", enabled: true },
    { key: "cache_read_tokens", label: "Cache Read", enabled: true },
    { key: "duration_secs", label: "Duration (s)", enabled: true },
    { key: "total_cost", label: "Known Monetary Value (USD)", enabled: true },
    { key: "cost_basis", label: "Monetary Value Basis", enabled: true },
    { key: "cost_source", label: "Monetary Value Source", enabled: true },
    { key: "started_at", label: "Started", enabled: true },
    { key: "ended_at", label: "Ended", enabled: false },
    { key: "effort", label: "Effort", enabled: false },
    { key: "is_active", label: "Active", enabled: false },
  ];

  let sortBy = $state("cost");
  let projectFilter = $state("");

  let liveSessions = $derived(
    $sessions.filter(
      (session) =>
        !session.is_idle
        && providerMatchesAnalyticsScope(session.provider, $selectedAnalyticsProviderScope),
    ),
  );

  let filtered = $derived.by(() => {
    let list = projectFilter
      ? liveSessions.filter((s) => s.project === projectFilter)
      : liveSessions;
    return [...list].sort((a, b) => {
      if (sortBy === "cost") {
        const left = a.cost_available === true ? a.cost : -1;
        const right = b.cost_available === true ? b.cost : -1;
        return right - left;
      }
      if (sortBy === "tokens") return b.tokens - a.tokens;
      if (sortBy === "duration") return b.duration_secs - a.duration_secs;
      if (sortBy === "tps") return b.tokens_per_sec - a.tokens_per_sec;
      return a.project.localeCompare(b.project);
    });
  });

  let totalTokens = $derived(filtered.reduce((s, x) => s + x.tokens, 0));
  let totalCost = $derived(filtered.reduce((s, x) => s + (x.cost_available === true ? x.cost : 0), 0));
  let totalCostAvailable = $derived(filtered.length === 0 || filtered.every((session) => session.cost_available === true));
  let avgTps = $derived(filtered.length ? filtered.reduce((s, x) => s + x.tokens_per_sec, 0) / filtered.length : 0);
  let totalInput = $derived(filtered.reduce((s, x) => s + Math.max(0, x.input_tokens - x.cache_write_tokens - x.cache_read_tokens), 0));
  let totalOutput = $derived(filtered.reduce((s, x) => s + x.output_tokens, 0));
  let totalCacheW = $derived(filtered.reduce((s, x) => s + x.cache_write_tokens, 0));
  let totalCacheR = $derived(filtered.reduce((s, x) => s + x.cache_read_tokens, 0));
  let totalAll = $derived(totalInput + totalOutput + totalCacheW + totalCacheR);

  let activityCounts = $derived.by(() => {
    const counts: Record<string, number> = {};
    filtered.forEach((s) => {
      const a = classifyActivity(s.activity);
      counts[a] = (counts[a] || 0) + 1;
    });
    return Object.entries(counts).sort((a, b) => b[1] - a[1]);
  });

  let modelCounts = $derived.by(() => {
    const counts: Record<string, number> = {};
    filtered.forEach((s) => { counts[s.model] = (counts[s.model] || 0) + 1; });
    return Object.entries(counts).sort((a, b) => b[1] - a[1]);
  });

  let history = $state<HistoricalSession[]>([]);
  let knownProjects = $state<string[]>([]);
  let summary = $state<AnalyticsSummary | null>(null);
  let historyLoading = $state(true);
  let historyError = $state<string | null>(null);
  let historyRequest = 0;
  const HISTORY_PAGE_SIZE = 50;
  let visibleHistoryLimit = $state(HISTORY_PAGE_SIZE);
  let historyDays = $state(7);
  let searchQuery = $state("");
  // granular filters
  let fromDate = $state("");
  let toDate = $state("");
  let minCost = $state<number | null>(null);
  let modelFilter = $state("");

  let exportRows = $derived(
    history.map((h) => ({
      ...h,
      total_cost: h.known_cost,
    } as Record<string, unknown>)),
  );
  let projects = $derived(
    [...new Set([...liveSessions.map((s) => s.project), ...knownProjects])].sort(),
  );

  let compareList = $derived(history.filter((h) => compareIds.has(h.id)));
  let visibleHistory = $derived(history.slice(0, visibleHistoryLimit));

  async function loadHistory(): Promise<void> {
    const request = ++historyRequest;
    const provider = $selectedAnalyticsProviderScope;
    historyLoading = true;
    historyError = null;
    showExport = false;
    compareIds = new Set();
    try {
      const useAdvanced = fromDate || toDate || minCost !== null || modelFilter;
      const [nextSummary, nextHistory] = await Promise.all([
        getAnalyticsSummary(provider),
        useAdvanced
          ? getSessionHistoryFiltered({
              // Compose the selected date-range window with the advanced
              // filters: without an explicit From, fall back to the window
              // dropdown (Today/7/30/90/365) so e.g. "Last 7 days" + a Model
              // filter still honors the 7-day window instead of searching all
              // time. An explicit From always wins.
              from_iso: fromDate
                ? new Date(fromDate).toISOString()
                : historyDays > 0
                  ? new Date(Date.now() - historyDays * 86_400_000).toISOString()
                  : null,
              to_iso: toDate ? new Date(toDate + "T23:59:59").toISOString() : null,
              project: projectFilter || null,
              model: modelFilter || null,
              min_cost: minCost,
              limit: 500,
              provider,
            })
          : getSessionHistory(historyDays, projectFilter || undefined, 200, provider),
      ]);
      if (request !== historyRequest) return;
      summary = nextSummary;
      history = nextHistory;
      visibleHistoryLimit = HISTORY_PAGE_SIZE;
      knownProjects = [...new Set(nextHistory.map((session) => session.project))].sort();
    } catch (error) {
      if (request !== historyRequest) return;
      historyError = error instanceof Error && error.message
        ? `Session history unavailable. ${error.message}`
        : "Session history unavailable. Pulse could not load your saved sessions.";
    } finally {
      if (request === historyRequest) historyLoading = false;
    }
  }

  function resetFilters(): void {
    fromDate = "";
    toDate = "";
    minCost = null;
    modelFilter = "";
    loadHistory();
  }

  async function doSearch(): Promise<void> {
    if (!searchQuery.trim()) return loadHistory();
    const request = ++historyRequest;
    const provider = $selectedAnalyticsProviderScope;
    historyLoading = true;
    historyError = null;
    showExport = false;
    try {
      const result = await searchSessions(searchQuery, 100, provider);
      if (request === historyRequest && provider === $selectedAnalyticsProviderScope) {
        history = result;
        visibleHistoryLimit = HISTORY_PAGE_SIZE;
      }
    } catch (error) {
      if (request === historyRequest) {
        historyError = error instanceof Error && error.message
          ? `Session search unavailable. ${error.message}`
          : "Session search unavailable. Pulse could not complete the search.";
      }
    } finally {
      if (request === historyRequest) historyLoading = false;
    }
  }

  function toggleExpand(id: string): void {
    expandedId = expandedId === id ? null : id;
  }

  function toggleCompare(id: string): void {
    const next = new Set(compareIds);
    if (next.has(id)) next.delete(id);
    else if (next.size < 3) next.add(id);
    compareIds = next;
  }

  onMount(() => { loadHistory(); });
  let previousProviderScope: string | undefined;
  $effect(() => {
    const provider = $selectedAnalyticsProviderScope;
    if (previousProviderScope !== undefined && provider !== previousProviderScope) {
      expandedId = null;
      compareIds = new Set();
      knownProjects = [];
      void (searchQuery.trim() ? doSearch() : loadHistory());
    }
    previousProviderScope = provider;
  });
</script>

<div class="sessions-view app-view">
  <div class="view-header">
    <div class="title-line">
      <h2 class="view-title">Sessions</h2>
      <span class="view-sub">{filtered.length} active</span>
    </div>
    <div class="filters">
      <select
        value={projectFilter}
        onchange={(event) => {
          projectFilter = event.currentTarget.value;
          void (searchQuery.trim() ? doSearch() : loadHistory());
        }}
      >
        <option value="">All Projects</option>
        {#each projects as p}<option value={p}>{p}</option>{/each}
      </select>
      <select bind:value={sortBy}>
        <option value="cost">Sort: Monetary value</option>
        <option value="tokens">Sort: Tokens</option>
        <option value="duration">Sort: Duration</option>
        <option value="tps">Sort: Throughput</option>
        <option value="project">Sort: Project</option>
      </select>
    </div>
  </div>

  <div class="stats-row metric-strip">
    <StatCard label="Active sessions" value={String(filtered.length)} />
    <StatCard label="Live tokens" value={filtered.length > 0 ? fmtTokens(totalTokens) : "—"} />
    <StatCard label="Live monetary value" value={filtered.length > 0 ? fmtExactCost(totalCost, totalCostAvailable) : "—"} />
    <StatCard label="Avg throughput" value={filtered.length > 0 ? fmtTps(avgTps) : "—"} />
  </div>

  <div class="session-list">
    {#if filtered.length === 0}
      <div class="empty-state state-panel">
        <div class="empty-text">No live sessions match these filters</div>
        <div class="empty-sub">Change the project or sort controls, or use the history ledger below.</div>
      </div>
    {:else}
      {#each filtered as session (session.session_id)}
        <div in:fly={{ y: 12, duration: 200 }}>
          <SessionCard {session} />
        </div>
      {/each}
    {/if}
  </div>

  <div class="card surface-matte">
    <div class="card-title-row">
      <h3 class="card-title">Session history</h3>
      <div class="title-actions">
        <button class="action-btn" class:active={compareMode} onclick={() => { compareMode = !compareMode; compareIds = new Set(); }}>
          {compareMode ? "Exit Compare" : "Compare"}
        </button>
        {#if history.length > 0}
          <button class="action-btn" onclick={() => showExport = true}>Export CSV</button>
        {/if}
      </div>
    </div>
    <div class="history-controls">
      <div class="history-filters">
        <select bind:value={historyDays} onchange={() => loadHistory()}>
          <option value={1}>Today</option>
          <option value={7}>Last 7 days</option>
          <option value={30}>Last 30 days</option>
          <option value={90}>Last 90 days</option>
          <option value={365}>Last year</option>
        </select>
        <div class="search-box">
          <input type="text" placeholder="Search sessions (BM25)..." bind:value={searchQuery} onkeydown={(e) => e.key === "Enter" && doSearch()} />
        </div>
      </div>
      <div class="history-filters advanced">
        <label class="flt">
          <span class="flt-lbl">From</span>
          <input type="date" bind:value={fromDate} onchange={() => loadHistory()} />
        </label>
        <label class="flt">
          <span class="flt-lbl">To</span>
          <input type="date" bind:value={toDate} onchange={() => loadHistory()} />
        </label>
        <label class="flt">
          <span class="flt-lbl">Min $</span>
          <input type="number" min="0" step="0.01" placeholder="0.00" bind:value={minCost} onchange={() => loadHistory()} />
        </label>
        <label class="flt">
          <span class="flt-lbl">Model</span>
          <input type="text" placeholder="opus / sonnet" bind:value={modelFilter} onchange={() => loadHistory()} />
        </label>
        {#if fromDate || toDate || minCost !== null || modelFilter}
          <button class="btn btn-ghost" onclick={resetFilters}>Reset</button>
        {/if}
      </div>
      {#if summary}
        <div class="history-summary">
          <span>All time: <strong>{summary.total_sessions}</strong> sessions</span>
          <span>
            {monetaryValueLabel(summary.cost_sources)}:
            <strong>
              {summary.cost_basis === "unavailable"
                ? "Unavailable"
                : summary.cost_basis === "partial"
                  ? `${fmtCost(summary.total_cost)} lower bound`
                  : fmtCost(summary.total_cost)}
            </strong>
          </span>
          <span>Tokens: <strong>{fmtTokens(summary.total_tokens)}</strong></span>
          <span>Top: <strong>{summary.top_project}</strong></span>
          <span><strong>{summary.days_tracked}</strong> days tracked</span>
        </div>
      {/if}
    </div>

    {#if historyError}
      <section class="history-state error" role="alert">
        <strong>{history.length > 0 ? "Showing the last verified history" : historyError.split(".")[0]}</strong>
        <span>{historyError.split(".").slice(1).join(".").trim()}</span>
        <button type="button" onclick={loadHistory}>Retry</button>
      </section>
    {:else if historyLoading}
      <section class="history-state" role="status">
        <strong>{history.length > 0 ? "Refreshing session history" : "Loading session history"}</strong>
        <span>Reading the selected provider and time window.</span>
      </section>
    {/if}

    {#if compareMode && compareList.length >= 2}
      <div class="compare-panel">
        <h4 class="compare-title">Comparison · {compareList.length} sessions</h4>
        <div class="compare-grid" style="--compare-cols:{compareList.length}">
          <div class="compare-label"></div>
          {#each compareList as c}<div class="compare-head">{c.project}</div>{/each}
          <div class="compare-label">Model</div>
          {#each compareList as c}<div class="compare-cell">{c.model}</div>{/each}
          <div class="compare-label">Tokens</div>
          {#each compareList as c}<div class="compare-cell">{fmtTokens(c.total_tokens)}</div>{/each}
          <div class="compare-label">Monetary value</div>
          {#each compareList as c}
            <div class="compare-cell accent">
              {c.known_cost === null
                ? "Unavailable"
                : c.cost_basis === "partial"
                  ? `${fmtCost(c.known_cost)} known`
                  : c.cost_basis === "estimated"
                    ? `${fmtCost(c.known_cost)} estimated`
                  : fmtCost(c.known_cost)}
            </div>
          {/each}
          <div class="compare-label">Duration</div>
          {#each compareList as c}<div class="compare-cell">{c.duration_secs > 0 ? fmtDuration(c.duration_secs) : "—"}</div>{/each}
          <div class="compare-label">Cache Hit</div>
          {#each compareList as c}
            {@const total = c.cache_read_tokens + Math.max(0, c.input_tokens - c.cache_write_tokens - c.cache_read_tokens)}
            <div class="compare-cell">{total > 0 ? ((c.cache_read_tokens / total) * 100).toFixed(0) + "%" : "—"}</div>
          {/each}
          <div class="compare-label">Effort</div>
          {#each compareList as c}<div class="compare-cell">{c.effort}</div>{/each}
        </div>
      </div>
    {/if}

    {#if history.length > 0 || (!historyLoading && !historyError)}
    <div class="history-table" class:refreshing={historyLoading && history.length > 0}>
      <div class="ht-header">
        {#if compareMode}<span class="ht-col check"></span>{/if}
        <span class="ht-col status"></span>
        <span class="ht-col project">Project</span>
        <span class="ht-col model">Model</span>
        <span class="ht-col">Tokens</span>
        <span class="ht-col">Duration</span>
        <span class="ht-col cost">Monetary value</span>
        <span class="ht-col date">Date</span>
      </div>
      {#each visibleHistory as h (h.id)}
        <div class="ht-row-wrap">
          <div
            class="ht-row"
            class:active={h.is_active}
            class:expanded={expandedId === h.id}
            role="button"
            tabindex="0"
            aria-expanded={expandedId === h.id}
            onclick={() => toggleExpand(h.id)}
            onkeydown={(e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleExpand(h.id); } }}
          >
            {#if compareMode}
              <span class="ht-col check">
                <input type="checkbox" checked={compareIds.has(h.id)} onclick={(e) => { e.stopPropagation(); toggleCompare(h.id); }} disabled={!compareIds.has(h.id) && compareIds.size >= 3} />
              </span>
            {/if}
            <span class="ht-col status"><span class="status-dot" class:active={h.is_active}></span></span>
            <span class="ht-col project">{h.project}{h.branch ? " · " + h.branch : ""}{#if h.session_name}<span class="session-name">{h.session_name}</span>{/if}</span>
            <span class="ht-col model">{h.model} <small class="ctx-badge">{h.context_window}</small></span>
            <span class="ht-col">{fmtTokens(h.total_tokens)}</span>
            <span class="ht-col">{h.duration_secs > 0 ? fmtDuration(h.duration_secs) : "—"}</span>
            <span class="ht-col cost" class:unavailable={h.known_cost === null}>
              {h.known_cost === null
                ? "Unavailable"
                : h.cost_basis === "partial"
                  ? `${fmtCost(h.known_cost)} known`
                  : h.cost_basis === "estimated"
                    ? `${fmtCost(h.known_cost)} estimated`
                  : fmtCost(h.known_cost)}
            </span>
            <span class="ht-col date">{h.started_at?.slice(0, 10) ?? "—"}</span>
          </div>
          {#if expandedId === h.id}
            <div class="ht-detail" transition:fly={{ y: -8, duration: 150 }}>
              <div class="detail-grid">
                <div class="detail-section">
                  <span class="detail-label">Token Breakdown</span>
                  <div class="detail-row"><span>Input</span><span>{fmtTokens(Math.max(0, h.input_tokens - h.cache_write_tokens - h.cache_read_tokens))}</span></div>
                  <div class="detail-row"><span>Output</span><span>{fmtTokens(h.output_tokens)}</span></div>
                  <div class="detail-row"><span>Cache Write</span><span>{fmtTokens(h.cache_write_tokens)}</span></div>
                  <div class="detail-row"><span>Cache Read</span><span>{fmtTokens(h.cache_read_tokens)}</span></div>
                </div>
                <div class="detail-section">
                  <span class="detail-label">Monetary Value Breakdown</span>
                  {#if h.known_cost === null}
                    <p class="detail-unavailable">The provider did not return enough billing inputs for this session.</p>
                  {:else}
                    {#if h.cost_basis === "partial"}
                      <p class="detail-unavailable">Known subtotal; this session has incomplete cost coverage.</p>
                    {:else if h.cost_basis === "estimated" || monetaryValueLabel([h.cost_source]) === "API-equivalent value"}
                      <p class="detail-unavailable">API-equivalent estimate reconstructed from session tokens and model pricing.</p>
                    {:else if monetaryValueLabel([h.cost_source]) === "Provider-billed spend"}
                      <p class="detail-unavailable">Provider-reported billing for this session.</p>
                    {/if}
                    <div class="detail-row"><span>Input</span><span>{fmtCost(h.input_cost)}</span></div>
                    <div class="detail-row"><span>Output</span><span>{fmtCost(h.output_cost)}</span></div>
                    <div class="detail-row"><span>Cache Write</span><span>{fmtCost(h.cache_write_cost)}</span></div>
                    <div class="detail-row"><span>Cache Read</span><span>{fmtCost(h.cache_read_cost)}</span></div>
                  {/if}
                </div>
                <div class="detail-section">
                  <span class="detail-label">Details</span>
                  <div class="detail-row"><span>Effort</span><span>{h.effort}</span></div>
                  <div class="detail-row"><span>Thinking</span><span>{h.has_thinking ? "Yes" : "No"}</span></div>
                  <div class="detail-row"><span>Subagents</span><span>{h.subagent_count}</span></div>
                  <div class="detail-row"><span>Context</span><span>{h.context_window}</span></div>
                </div>
              </div>
            </div>
          {/if}
        </div>
      {:else}
        <div class="ht-empty">No sessions match the selected history filters. All-time totals above remain unchanged.</div>
      {/each}
    </div>
    {#if history.length > visibleHistory.length}
      <button
        class="show-more"
        type="button"
        onclick={() => (visibleHistoryLimit += HISTORY_PAGE_SIZE)}
      >
        Show {Math.min(HISTORY_PAGE_SIZE, history.length - visibleHistory.length)} more
      </button>
    {/if}
    {/if}
  </div>
</div>

<ExportModal
  open={showExport}
  title="Export session history"
  defaultFilename="pulse-sessions"
  columns={historyColumns}
  rows={exportRows}
  onclose={() => showExport = false}
/>

<style>
  .sessions-view { display: flex; flex-direction: column; gap: var(--page-gap); }
  .view-header { display: flex; align-items: flex-end; gap: 20px; flex-wrap: wrap; }
  .title-line { display: flex; align-items: center; gap: 10px; }
  .view-title { font-size: 20px; font-weight: 700; }
  .view-sub { font-size: 11px; color: var(--text-muted); border: 1px solid var(--border); padding: 3px 9px; border-radius: 99px; font-family: var(--font-mono); }
  .filters { margin-left: auto; display: flex; gap: 8px; }
  .stats-row { grid-template-columns: repeat(4, 1fr); }

  .card { position: relative; background: var(--surface-panel); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 20px; overflow: hidden; }
  .card-title-row { display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px; }
  .card-title-row .card-title { margin-bottom: 0; }
  .card-title { font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--accent); margin-bottom: 14px; }
  .title-actions { display: flex; gap: 6px; }
  .action-btn { font-size: 11px; font-weight: 600; color: var(--text-secondary); background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 4px 12px; cursor: pointer; transition: all 0.15s ease; }
  .action-btn:hover { color: var(--accent); border-color: var(--accent); background: var(--accent-dim); }
  .action-btn.active { color: var(--accent); border-color: var(--accent); background: var(--accent-dim); }

  .session-list { display: flex; flex-direction: column; gap: 8px; }
  .empty-state { text-align: center; padding: 34px 24px; }
  .empty-text { font-size: 14px; font-weight: 600; color: var(--text-primary); }
  .empty-sub { margin-top: 6px; font-size: 12px; color: var(--text-muted); }

  .compare-panel { margin-bottom: 16px; padding: 16px; background: var(--bg-primary); border: 1px solid var(--accent-dim); border-radius: var(--radius-md); }
  .compare-title { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--accent); margin-bottom: 12px; }
  .compare-grid { display: grid; grid-template-columns: 80px repeat(var(--compare-cols), 1fr); gap: 6px 12px; font-size: 12px; }
  .compare-label { font-weight: 600; color: var(--text-muted); padding: 4px 0; }
  .compare-head { font-weight: 700; color: var(--text-primary); padding: 4px 0; border-bottom: 1px solid var(--border); }
  .compare-cell { color: var(--text-secondary); padding: 4px 0; font-variant-numeric: tabular-nums; }
  .compare-cell.accent { color: var(--accent); font-weight: 700; }

  .history-controls { margin-bottom: 16px; display: flex; flex-direction: column; gap: 10px; }
  .history-state {
    min-height: 96px;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 6px;
    margin-bottom: 14px;
    color: var(--text-muted);
    text-align: center;
    border-block: 1px solid var(--divider);
  }
  .history-state strong { color: var(--text-primary); font-size: 12px; }
  .history-state span { font-size: 10px; }
  .history-state.error strong { color: var(--danger); }
  .history-state button {
    margin-top: 4px;
    padding: 5px 11px;
    color: var(--text-primary);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .history-filters { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .history-filters.advanced { gap: 10px; padding: 10px 12px; background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--radius-md); }
  .flt { display: flex; flex-direction: column; gap: 3px; font-size: 11px; }
  .flt-lbl { font-size: 9px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.08em; color: var(--text-muted); }
  .flt input { padding: 5px 8px; font-size: 11px; width: 110px; }
  .flt input[type="number"] { width: 80px; }
  .search-box { flex: 1; position: relative; }
  .search-box input { width: 100%; padding: 8px 14px 8px 32px; font: inherit; font-size: 12px; background: var(--bg-primary); border: 1px solid var(--border); border-radius: var(--radius-md); color: var(--text-primary); outline: none; transition: border-color 0.15s ease, box-shadow 0.15s ease; }
  .search-box input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-dim); }
  .search-box::before { content: ""; position: absolute; left: 12px; top: 50%; transform: translateY(-50%); width: 12px; height: 12px; border: 1.5px solid var(--text-muted); border-radius: 50%; pointer-events: none; }
  .search-box::after { content: ""; position: absolute; left: 22px; top: 60%; width: 4px; height: 1.5px; background: var(--text-muted); transform: rotate(45deg); pointer-events: none; }
  .history-summary { display: flex; gap: 16px; font-size: 11px; color: var(--text-muted); padding: 10px 14px; background: var(--bg-primary); border-radius: var(--radius-md); border: 1px solid var(--border); }
  .history-summary strong { color: var(--text-primary); }
  .history-summary span { display: flex; align-items: center; gap: 4px; }

  .history-table { font-size: 12px; max-height: 500px; overflow-y: auto; --ht-cols: 24px 2fr 1.5fr 90px 80px 80px 80px; }
  .history-table.refreshing { opacity: 0.72; pointer-events: none; }
  .ht-header { display: grid; grid-template-columns: var(--ht-cols); gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border); font-weight: 700; color: var(--text-muted); text-transform: uppercase; font-size: 9px; letter-spacing: 0.08em; position: sticky; top: 0; background: var(--bg-card); z-index: 1; }
  .ht-row-wrap { border-bottom: 1px solid var(--border); }
  .ht-row { display: grid; grid-template-columns: var(--ht-cols); gap: 8px; padding: 10px 14px; transition: background 0.15s var(--ease); cursor: pointer; }
  .ht-row:hover { background: var(--bg-card-hover); }
  .ht-row.active { background: var(--success-dim); border-left: 2px solid var(--success); padding-left: 12px; }
  .ht-row.expanded { background: var(--bg-elevated); }
  .ht-col { text-align: right; font-variant-numeric: tabular-nums; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ht-col.project { text-align: left; font-weight: 600; color: var(--text-primary); }
  .ht-col.model { text-align: left; }
  .ht-col.cost { font-weight: 700; color: var(--accent); }
  .ht-col.cost.unavailable { color: var(--text-muted); font-weight: 500; }
  .ht-col.date { color: var(--text-muted); font-size: 11px; }
  .ht-col.status { text-align: center; }
  .ht-col.check { text-align: center; display: flex; align-items: center; justify-content: center; }
  .session-name { display: block; font-size: 10px; font-weight: 400; color: var(--text-muted); margin-top: 3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 280px; }
  .status-dot { display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: var(--text-muted); }
  .status-dot.active { background: var(--success); box-shadow: 0 0 6px var(--success-glow); }
  .ctx-badge { font-size: 8px; font-weight: 700; color: var(--accent); background: var(--accent-dim); padding: 2px 5px; border-radius: 3px; margin-left: 4px; letter-spacing: 0.02em; }
  .ht-empty { text-align: center; padding: 40px; color: var(--text-muted); font-size: 12px; }

  .ht-detail { padding: 12px 14px 16px; background: var(--bg-elevated); border-top: 1px solid var(--border); }
  .detail-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; }
  .detail-section { display: flex; flex-direction: column; gap: 4px; }
  .detail-label { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--accent); margin-bottom: 4px; }
  .detail-row { display: flex; justify-content: space-between; font-size: 11px; color: var(--text-secondary); padding: 2px 0; }
  .detail-row span:last-child { font-weight: 600; color: var(--text-primary); font-variant-numeric: tabular-nums; }
  .detail-unavailable { max-width: 34ch; color: var(--text-muted); font-size: 11px; line-height: 1.5; }
  .show-more { align-self: center; margin: 12px auto 0; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bg-elevated); color: var(--text-secondary); padding: 7px 16px; font-size: 11px; font-weight: 600; cursor: pointer; }
  .show-more:hover { color: var(--accent); border-color: var(--accent); }

  .card { min-width: 0; }
  .history-table, .compare-panel { overflow-x: auto; overscroll-behavior-inline: contain; }
  .ht-header, .ht-row, .ht-detail { min-width: 720px; }

  @media (max-width: 1050px) {
    .stats-row { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .detail-grid { grid-template-columns: 1fr 1fr; }
  }

  @media (max-width: 800px) {
    .view-header { align-items: stretch; flex-direction: column; gap: 10px; }
    .filters { width: 100%; margin-left: 0; }
    .filters select { min-width: 0; flex: 1; }
  }

  @media (max-width: 620px) {
    .stats-row, .detail-grid { grid-template-columns: 1fr; }
    .history-summary { flex-wrap: wrap; gap: 8px 12px; }
    .history-filters.advanced { align-items: stretch; }
    .flt input, .flt input[type="number"] { width: 100%; }
    .filters { flex-direction: column; }
  }
</style>

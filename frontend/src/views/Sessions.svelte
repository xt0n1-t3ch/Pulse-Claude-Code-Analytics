<script lang="ts">
  import { onMount } from "svelte";
  import StatCard from "../components/StatCard.svelte";
  import SessionCard from "../components/SessionCard.svelte";
  import { sessions } from "../lib/stores";
  import { fmtTokens, fmtCost, fmtDuration, fmtTps, classifyActivity, fmtClock } from "../lib/utils";
  import { getSessionHistory, getAnalyticsSummary, searchSessions, getTopSessions, getSessionHistoryFiltered } from "../lib/api";
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
    { key: "total_cost", label: "Cost", enabled: true },
    { key: "started_at", label: "Started", enabled: true },
    { key: "ended_at", label: "Ended", enabled: false },
    { key: "effort", label: "Effort", enabled: false },
    { key: "is_active", label: "Active", enabled: false },
  ];

  let sortBy = $state("cost");
  let projectFilter = $state("");

  let projects = $derived([...new Set($sessions.map((s) => s.project))].sort());

  let filtered = $derived.by(() => {
    let list = projectFilter ? $sessions.filter((s) => s.project === projectFilter) : $sessions;
    return [...list].sort((a, b) => {
      if (sortBy === "cost") return b.cost - a.cost;
      if (sortBy === "tokens") return b.tokens - a.tokens;
      if (sortBy === "duration") return b.duration_secs - a.duration_secs;
      if (sortBy === "tps") return b.tokens_per_sec - a.tokens_per_sec;
      return a.project.localeCompare(b.project);
    });
  });

  let totalTokens = $derived(filtered.reduce((s, x) => s + x.tokens, 0));
  let totalCost = $derived(filtered.reduce((s, x) => s + x.cost, 0));
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
  let topSessions = $state<HistoricalSession[]>([]);
  let summary = $state<AnalyticsSummary | null>(null);
  let historyDays = $state(7);
  let searchQuery = $state("");
  // granular filters
  let fromDate = $state("");
  let toDate = $state("");
  let minCost = $state<number | null>(null);
  let modelFilter = $state("");

  let exportRows = $derived(history.map((h) => ({ ...h } as Record<string, unknown>)));

  let compareList = $derived(history.filter((h) => compareIds.has(h.id)));

  async function loadHistory(): Promise<void> {
    const useAdvanced = fromDate || toDate || minCost !== null || modelFilter;
    const [summaryRes, top] = await Promise.all([getAnalyticsSummary(), getTopSessions(10, 30)]);
    summary = summaryRes;
    topSessions = top;

    if (useAdvanced) {
      history = await getSessionHistoryFiltered({
        from_iso: fromDate ? new Date(fromDate).toISOString() : null,
        to_iso: toDate ? new Date(toDate + "T23:59:59").toISOString() : null,
        project: projectFilter || null,
        model: modelFilter || null,
        min_cost: minCost,
        limit: 500,
      });
    } else {
      history = await getSessionHistory(historyDays, projectFilter || undefined, 200);
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
    if (searchQuery.trim()) {
      history = await searchSessions(searchQuery, 100);
    } else {
      await loadHistory();
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
</script>

<div class="sessions-view">
  <div class="view-header">
    <h2 class="view-title">Sessions</h2>
    <span class="view-sub">{filtered.length} active</span>
    <div class="filters">
      <select bind:value={projectFilter}>
        <option value="">All Projects</option>
        {#each projects as p}<option value={p}>{p}</option>{/each}
      </select>
      <select bind:value={sortBy}>
        <option value="cost">Sort: Cost</option>
        <option value="tokens">Sort: Tokens</option>
        <option value="duration">Sort: Duration</option>
        <option value="tps">Sort: Throughput</option>
        <option value="project">Sort: Project</option>
      </select>
    </div>
  </div>

  <div class="stats-row">
    <StatCard label="Total Tokens" value={fmtTokens(totalTokens || (summary?.total_tokens ?? 0))} />
    <StatCard label="Total Cost" value={fmtCost(totalCost || (summary?.total_cost ?? 0))} />
    <StatCard label="Avg Duration" value={summary ? fmtDuration(summary.avg_duration_secs) : "—"} />
    <StatCard label="Avg Cost/Session" value={summary ? fmtCost(summary.avg_cost_per_session) : "—"} />
  </div>

  <div class="session-list">
    {#if filtered.length === 0}
      <div class="empty-state">
        <div class="empty-icon">✳</div>
        <div class="empty-text">No sessions match filters</div>
      </div>
    {:else}
      {#each filtered as session (session.session_id)}
        <div in:fly={{ y: 12, duration: 200 }}>
          <SessionCard {session} />
        </div>
      {/each}
    {/if}
  </div>

  {#if topSessions.length > 0}
    <div class="card">
      <h3 class="card-title">Most Costly Sessions (30 days)</h3>
      <div class="top-table">
        <div class="top-header">
          <span class="top-col rank">#</span>
          <span class="top-col project">Project</span>
          <span class="top-col model">Model</span>
          <span class="top-col">Tokens</span>
          <span class="top-col">Duration</span>
          <span class="top-col cost">Cost</span>
        </div>
        {#each topSessions.sort((a, b) => b.total_cost - a.total_cost).slice(0, 10) as h, i (h.id)}
          <div class="top-row">
            <span class="top-col rank">{i + 1}</span>
            <span class="top-col project">{h.project}</span>
            <span class="top-col model">{h.model}</span>
            <span class="top-col">{fmtTokens(h.total_tokens)}</span>
            <span class="top-col">{h.duration_secs > 0 ? fmtDuration(h.duration_secs) : "—"}</span>
            <span class="top-col cost">{fmtCost(h.total_cost)}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <div class="card">
    <div class="card-title-row">
      <h3 class="card-title">Session History</h3>
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
          <span>Cost: <strong>{fmtCost(summary.total_cost)}</strong></span>
          <span>Tokens: <strong>{fmtTokens(summary.total_tokens)}</strong></span>
          <span>Top: <strong>{summary.top_project}</strong></span>
          <span><strong>{summary.days_tracked}</strong> days tracked</span>
        </div>
      {/if}
    </div>

    {#if compareMode && compareList.length >= 2}
      <div class="compare-panel">
        <h4 class="compare-title">Comparison ({compareList.length} sessions)</h4>
        <div class="compare-grid" style="--compare-cols:{compareList.length}">
          <div class="compare-label"></div>
          {#each compareList as c}<div class="compare-head">{c.project}</div>{/each}
          <div class="compare-label">Model</div>
          {#each compareList as c}<div class="compare-cell">{c.model}</div>{/each}
          <div class="compare-label">Tokens</div>
          {#each compareList as c}<div class="compare-cell">{fmtTokens(c.total_tokens)}</div>{/each}
          <div class="compare-label">Cost</div>
          {#each compareList as c}<div class="compare-cell accent">{fmtCost(c.total_cost)}</div>{/each}
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

    <div class="history-table">
      <div class="ht-header">
        {#if compareMode}<span class="ht-col check"></span>{/if}
        <span class="ht-col status"></span>
        <span class="ht-col project">Project</span>
        <span class="ht-col model">Model</span>
        <span class="ht-col">Tokens</span>
        <span class="ht-col">Duration</span>
        <span class="ht-col cost">Cost</span>
        <span class="ht-col date">Date</span>
      </div>
      {#each history as h (h.id)}
        <div class="ht-row-wrap">
          <div class="ht-row" class:active={h.is_active} class:expanded={expandedId === h.id} onclick={() => toggleExpand(h.id)}>
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
            <span class="ht-col cost">{fmtCost(h.total_cost)}</span>
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
                  <span class="detail-label">Cost Breakdown</span>
                  <div class="detail-row"><span>Input</span><span>{fmtCost(h.input_cost)}</span></div>
                  <div class="detail-row"><span>Output</span><span>{fmtCost(h.output_cost)}</span></div>
                  <div class="detail-row"><span>Cache Write</span><span>{fmtCost(h.cache_write_cost)}</span></div>
                  <div class="detail-row"><span>Cache Read</span><span>{fmtCost(h.cache_read_cost)}</span></div>
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
        <div class="ht-empty">No historical sessions yet. Data persists across app restarts.</div>
      {/each}
    </div>
  </div>
</div>

<ExportModal
  open={showExport}
  title="Export Session History"
  defaultFilename="pulse-sessions"
  columns={historyColumns}
  rows={exportRows}
  onclose={() => showExport = false}
/>

<style>
  .sessions-view { display: flex; flex-direction: column; gap: 16px; }
  .view-header { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
  .view-title { font-size: 20px; font-weight: 700; }
  .view-sub { font-size: 12px; color: var(--text-muted); background: var(--bg-elevated); padding: 3px 10px; border-radius: 99px; }
  .filters { margin-left: auto; display: flex; gap: 8px; }
  .stats-row { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; }
  .analytics-row { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }

  .card { background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 20px; }
  .card.mini { padding: 14px 16px; }
  .card-title-row { display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px; }
  .card-title-row .card-title { margin-bottom: 0; }
  .card-title { font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--accent); margin-bottom: 14px; }
  .title-actions { display: flex; gap: 6px; }
  .action-btn { font-size: 11px; font-weight: 600; color: var(--text-secondary); background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--radius-sm); padding: 4px 12px; cursor: pointer; transition: all 0.15s ease; }
  .action-btn:hover { color: var(--accent); border-color: var(--accent); background: var(--accent-dim); }
  .action-btn.active { color: var(--accent); border-color: var(--accent); background: var(--accent-dim); }

  .mini-title { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--text-muted); margin-bottom: 8px; }
  .mini-row { display: flex; justify-content: space-between; padding: 4px 0; font-size: 12px; }
  .mini-label { color: var(--text-secondary); display: flex; align-items: center; gap: 6px; }
  .mini-val { font-weight: 700; color: var(--text-primary); font-variant-numeric: tabular-nums; }

  .activity-dot-sm { width: 6px; height: 6px; border-radius: 50%; }
  .activity-dot-sm.thinking { background: var(--success); }
  .activity-dot-sm.editing { background: var(--accent); }
  .activity-dot-sm.reading { background: #7cb9e8; }
  .activity-dot-sm.running { background: var(--warning); }
  .activity-dot-sm.idle { background: var(--text-muted); }
  .activity-dot-sm.waiting { background: var(--text-muted); }

  .mega-bar { display: flex; height: 12px; border-radius: 99px; overflow: hidden; background: var(--bg-elevated); margin-bottom: 10px; }
  .mega-seg { height: 100%; transition: width 0.4s var(--ease); }
  .mega-seg.input { background: var(--info); }
  .mega-seg.output { background: #7cb9e8; }
  .mega-seg.cache-w { background: #77dd77; }
  .mega-seg.cache-r { background: #c3b1e1; }
  .mega-legend { display: flex; flex-wrap: wrap; gap: 14px; font-size: 12px; color: var(--text-secondary); }
  .mega-legend span { display: flex; align-items: center; gap: 5px; }
  .dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .dot.input { background: var(--info); }
  .dot.output { background: #7cb9e8; }
  .dot.cache-w { background: #77dd77; }
  .dot.cache-r { background: #c3b1e1; }

  .session-list { display: flex; flex-direction: column; gap: 8px; }
  .empty-state { text-align: center; padding: 40px; }
  .empty-icon { font-size: 28px; color: var(--accent); margin-bottom: 8px; }
  .empty-text { font-size: 14px; color: var(--text-secondary); }

  .timeline { position: relative; padding-left: 24px; display: flex; flex-direction: column; gap: 2px; }
  .timeline::before { content: ""; position: absolute; left: 7px; top: 4px; bottom: 4px; width: 2px; background: var(--border); border-radius: 1px; }
  .timeline-item { position: relative; display: flex; gap: 12px; padding: 10px 14px; border-radius: var(--radius-md); transition: background 0.15s var(--ease); }
  .timeline-item:hover { background: var(--bg-card); }
  .timeline-dot { position: absolute; left: -20px; top: 16px; width: 10px; height: 10px; border-radius: 50%; background: var(--accent); border: 2px solid var(--bg-primary); box-shadow: 0 0 0 2px var(--accent-dim); }
  .timeline-content { flex: 1; }
  .timeline-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 4px; }
  .timeline-project { font-weight: 600; font-size: 13px; }
  .timeline-time { font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .timeline-meta { font-size: 12px; color: var(--text-secondary); }

  .top-table { font-size: 12px; --top-cols: 30px 2fr 1.5fr 90px 80px 80px; }
  .top-header { display: grid; grid-template-columns: var(--top-cols); gap: 8px; padding: 8px 10px; border-bottom: 1px solid var(--border); font-weight: 700; color: var(--text-muted); text-transform: uppercase; font-size: 10px; letter-spacing: 0.05em; }
  .top-row { display: grid; grid-template-columns: var(--top-cols); gap: 8px; padding: 8px 10px; border-radius: var(--radius-sm); transition: background 0.15s var(--ease); }
  .top-row:hover { background: var(--bg-elevated); }
  .top-col { text-align: right; font-variant-numeric: tabular-nums; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .top-col.rank { text-align: center; font-weight: 700; color: var(--text-muted); }
  .top-col.project { text-align: left; font-weight: 500; color: var(--text-primary); }
  .top-col.model { text-align: left; }
  .top-col.cost { font-weight: 700; color: var(--accent); }

  .compare-panel { margin-bottom: 16px; padding: 16px; background: var(--bg-primary); border: 1px solid var(--accent-dim); border-radius: var(--radius-md); }
  .compare-title { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--accent); margin-bottom: 12px; }
  .compare-grid { display: grid; grid-template-columns: 80px repeat(var(--compare-cols), 1fr); gap: 6px 12px; font-size: 12px; }
  .compare-label { font-weight: 600; color: var(--text-muted); padding: 4px 0; }
  .compare-head { font-weight: 700; color: var(--text-primary); padding: 4px 0; border-bottom: 1px solid var(--border); }
  .compare-cell { color: var(--text-secondary); padding: 4px 0; font-variant-numeric: tabular-nums; }
  .compare-cell.accent { color: var(--accent); font-weight: 700; }

  .history-controls { margin-bottom: 16px; display: flex; flex-direction: column; gap: 10px; }
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
  .ht-header { display: grid; grid-template-columns: var(--ht-cols); gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border); font-weight: 700; color: var(--text-muted); text-transform: uppercase; font-size: 9px; letter-spacing: 0.08em; position: sticky; top: 0; background: var(--bg-card); z-index: 1; }
  .ht-row-wrap { border-bottom: 1px solid rgba(255,255,255,0.02); }
  .ht-row { display: grid; grid-template-columns: var(--ht-cols); gap: 8px; padding: 10px 14px; transition: background 0.15s var(--ease); cursor: pointer; }
  .ht-row:hover { background: rgba(255,255,255,0.02); }
  .ht-row.active { background: rgba(217, 119, 87, 0.04); border-left: 2px solid var(--success); padding-left: 12px; }
  .ht-row.expanded { background: var(--bg-elevated); }
  .ht-col { text-align: right; font-variant-numeric: tabular-nums; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ht-col.project { text-align: left; font-weight: 600; color: var(--text-primary); }
  .ht-col.model { text-align: left; }
  .ht-col.cost { font-weight: 700; color: var(--accent); }
  .ht-col.date { color: var(--text-muted); font-size: 11px; }
  .ht-col.status { text-align: center; }
  .ht-col.check { text-align: center; display: flex; align-items: center; justify-content: center; }
  .session-name { display: block; font-size: 10px; font-weight: 400; color: var(--text-muted); margin-top: 3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 280px; }
  .status-dot { display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: rgba(255,255,255,0.1); }
  .status-dot.active { background: var(--success); box-shadow: 0 0 6px var(--success-glow); }
  .ctx-badge { font-size: 8px; font-weight: 700; color: var(--accent); background: var(--accent-dim); padding: 2px 5px; border-radius: 3px; margin-left: 4px; letter-spacing: 0.02em; }
  .ht-empty { text-align: center; padding: 40px; color: var(--text-muted); font-size: 12px; }

  .ht-detail { padding: 12px 14px 16px; background: var(--bg-elevated); border-top: 1px solid var(--border); }
  .detail-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; }
  .detail-section { display: flex; flex-direction: column; gap: 4px; }
  .detail-label { font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--accent); margin-bottom: 4px; }
  .detail-row { display: flex; justify-content: space-between; font-size: 11px; color: var(--text-secondary); padding: 2px 0; }
  .detail-row span:last-child { font-weight: 600; color: var(--text-primary); font-variant-numeric: tabular-nums; }
</style>

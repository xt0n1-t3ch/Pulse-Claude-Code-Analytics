<script lang="ts">
  import Heatmap from "../components/Heatmap.svelte";
  import AllowanceRail from "../components/AllowanceRail.svelte";
  import {
    backendConnection,
    health,
    sessions,
    selectedAccessRoutes,
    selectedAnalyticsProviderScope,
  } from "../lib/stores";
  import { providerMatchesAnalyticsScope } from "../lib/access";
  import { fmtTokens, fmtCost, fmtExactCost, fmtDuration, fmtPct, fmtTps } from "../lib/utils";
  import {
    getAnalyticsSummary, getSessionHistory, getCostForecast,
    getHourlyActivity,
  } from "../lib/api";
  import type { AnalyticsSummary, HistoricalSession, CostForecast, HourlyActivity } from "../lib/api";

  let summary = $state<AnalyticsSummary | null>(null);
  let histSessions = $state<HistoricalSession[]>([]);
  let forecast = $state<CostForecast | null>(null);
  let hourlyData = $state<HourlyActivity[]>([]);
  let selectedFocusId = $state<string | null>(null);
  let analyticsLoading = $state(true);
  let analyticsError = $state<string | null>(null);

  let analyticsRequest = 0;
  async function refresh(provider: typeof $selectedAnalyticsProviderScope): Promise<void> {
    const request = ++analyticsRequest;
    analyticsLoading = true;
    analyticsError = null;
    summary = null;
    histSessions = [];
    forecast = null;
    hourlyData = [];
    try {
      const [nextSummary, nextSessions, nextForecast, nextHourlyData] = await Promise.all([
        getAnalyticsSummary(provider),
        getSessionHistory(30, undefined, 50, provider),
        getCostForecast(provider),
        getHourlyActivity(30, provider),
      ]);
      if (request !== analyticsRequest) return;
      summary = nextSummary;
      histSessions = nextSessions;
      forecast = nextForecast;
      hourlyData = nextHourlyData;
    } catch (error) {
      if (request !== analyticsRequest) return;
      summary = null;
      histSessions = [];
      forecast = null;
      hourlyData = [];
      analyticsError = error instanceof Error ? error.message : "Historical analytics could not be loaded.";
    } finally {
      if (request === analyticsRequest) analyticsLoading = false;
    }
  }

  $effect(() => {
    void refresh($selectedAnalyticsProviderScope);
  });

  let scopedSessions = $derived(
    $sessions.filter((session) =>
      providerMatchesAnalyticsScope(session.provider, $selectedAnalyticsProviderScope),
    ),
  );
  let hasSessions = $derived(scopedSessions.length > 0);
  let totalCost = $derived(
    hasSessions
      ? scopedSessions.reduce(
          (sum, session) => sum + (session.cost_available === true ? session.cost : 0),
          0,
        )
      : (summary?.total_cost ?? 0),
  );
  let totalCostAvailable = $derived(
    hasSessions
      ? scopedSessions.every((session) => session.cost_available === true)
      : Boolean(summary && summary.priced_sessions > 0 && summary.cost_basis !== "unavailable"),
  );
  let totalTokens = $derived(
    hasSessions
      ? scopedSessions.reduce((sum, session) => sum + session.tokens, 0)
      : (summary?.total_tokens ?? 0),
  );
  let sessionCount = $derived(hasSessions ? scopedSessions.length : (summary?.total_sessions ?? 0));

  let totalInput = $derived(
    scopedSessions.reduce(
      (sum, session) =>
        sum + Math.max(0, session.input_tokens - session.cache_write_tokens - session.cache_read_tokens),
      0,
    ),
  );
  let totalCacheR = $derived(
    scopedSessions.reduce((sum, session) => sum + session.cache_read_tokens, 0),
  );

  let histInput = $derived(histSessions.reduce((s, h) => s + Math.max(0, h.input_tokens - h.cache_write_tokens - h.cache_read_tokens), 0));
  let histCacheR = $derived(histSessions.reduce((s, h) => s + h.cache_read_tokens, 0));

  let showInput = $derived(hasSessions ? totalInput : histInput);
  let showCacheR = $derived(hasSessions ? totalCacheR : histCacheR);
  let showCacheHit = $derived(showCacheR + showInput > 0 ? showCacheR / (showCacheR + showInput) * 100 : 0);

  let modelGroups = $derived.by(() => {
    if (hasSessions) {
      const liveMap: Record<string, { sessions: number; cost: number; tokens: number }> = {};
      scopedSessions.forEach((session) => {
        const entry = liveMap[session.model] ?? { sessions: 0, cost: 0, tokens: 0 };
        entry.sessions++;
        entry.cost += session.cost_available === true ? session.cost : 0;
        entry.tokens += session.tokens;
        liveMap[session.model] = entry;
      });
      return Object.entries(liveMap)
        .map(([model, value]) => ({ model, ...value }))
        .sort((a, b) => b.sessions - a.sessions || b.tokens - a.tokens);
    }
    const map: Record<string, { sessions: number; cost: number; tokens: number }> = {};
    histSessions.forEach((h) => {
      const e = map[h.model] ?? { sessions: 0, cost: 0, tokens: 0 };
      e.sessions++;
      e.cost += h.known_cost ?? 0;
      e.tokens += h.total_tokens;
      map[h.model] = e;
    });
    return Object.entries(map)
      .map(([model, v]) => ({ model, ...v }))
      .sort((a, b) => b.sessions - a.sessions || b.tokens - a.tokens);
  });

  let cacheGrade = $derived.by(() => {
    // No token data yet — show a neutral mark instead of a red "F", which would
    // read as bad performance rather than "nothing measured".
    if (showCacheR + showInput <= 0) return { letter: "—", color: "var(--text-muted)" };
    const ratio = showCacheHit;
    if (ratio >= 80) return { letter: "A", color: "var(--success)" };
    if (ratio >= 65) return { letter: "B", color: "var(--token-cache-write)" };
    if (ratio >= 50) return { letter: "C", color: "var(--warning)" };
    if (ratio >= 30) return { letter: "D", color: "var(--warning)" };
    return { letter: "F", color: "var(--danger)" };
  });

  let topModel = $derived.by(() => {
    if (!modelGroups.length) return null;
    const total = modelGroups.reduce((s, m) => s + m.sessions, 0);
    const top = modelGroups[0];
    const pct = total > 0 ? (top.sessions / total) * 100 : 0;
    return { name: top.model, pct, sessions: top.sessions };
  });
  let hasOperationalSummary = $derived(
    showCacheR + showInput > 0
      || Boolean(forecast && forecast.spent_this_month > 0)
      || Boolean(topModel && topModel.pct > 60)
      || hourlyData.length > 0,
  );

  let liveInstances = $derived(scopedSessions.filter((session) => !session.is_idle));
  let visibleInstances = $derived(liveInstances.length > 0 ? liveInstances : scopedSessions);
  $effect(() => {
    const instances = visibleInstances;
    if (!instances.some((session) => session.session_id === selectedFocusId)) {
      const nextFocusId = instances[0]?.session_id ?? null;
      if (selectedFocusId !== nextFocusId) selectedFocusId = nextFocusId;
    }
  });

  /** Live state wins; history is presentation fallback only and never replaces
   * the backend presence/session contract. */
  let focusSession = $derived(
    visibleInstances.find((session) => session.session_id === selectedFocusId)
      ?? visibleInstances[0]
      ?? null,
  );
  let focusHistory = $derived(histSessions[0] ?? null);
  let focusHistoryFallback = $derived(
    focusSession ? null : focusHistory,
  );
  let focusName = $derived(
    focusSession?.session_name
      ?? focusSession?.project
      ?? focusHistoryFallback?.session_name
      ?? focusHistoryFallback?.project
      ?? "No active session",
  );
  let focusProject = $derived(focusSession?.project ?? focusHistoryFallback?.project ?? "Waiting for session");
  let focusModel = $derived(focusSession?.model ?? focusHistoryFallback?.model ?? "No live model");
  let focusBranch = $derived(focusSession?.branch ?? focusHistoryFallback?.branch ?? "—");
  let focusDuration = $derived(focusSession?.duration_secs ?? focusHistoryFallback?.duration_secs ?? 0);
  let focusCost = $derived(
    focusSession?.cost_available === true
      ? focusSession.cost
      : focusHistoryFallback
        ? focusHistoryFallback.known_cost ?? 0
        : totalCost,
  );
  let focusCostAvailable = $derived(
    focusSession
      ? focusSession.cost_available === true
      : focusHistoryFallback
        ? focusHistoryFallback.known_cost !== null
        : totalCostAvailable,
  );
  let focusCostBasis = $derived(
    focusSession
      ? focusSession.cost_basis
      : focusHistoryFallback
        ? focusHistoryFallback.cost_basis
        : summary?.cost_basis ?? "unavailable",
  );
  let focusCostNote = $derived.by(() => {
    if (focusCostBasis === "partial") {
      const priced = summary?.priced_sessions ?? 0;
      const sessions = summary?.total_sessions ?? 0;
      return priced > 0 && sessions > priced
        ? `Known subtotal · ${priced}/${sessions} sessions priced`
        : "Known subtotal · incomplete provider coverage";
    }
    if (focusCostBasis === "exact") return "Exact total";
    return "Exact total not reported";
  });
  let focusTokens = $derived(
    focusSession?.tokens
      ?? focusHistoryFallback?.total_tokens
      ?? totalTokens,
  );
  let focusContextPct = $derived.by(() => {
    const used = focusSession?.context_used_tokens ?? 0;
    const window = focusSession?.context_window_tokens ?? 0;
    return window > 0 ? Math.min(100, (used / window) * 100) : 0;
  });
  let focusContextUsed = $derived(focusSession?.context_used_tokens ?? 0);
  let focusContextWindow = $derived(focusSession?.context_window_tokens ?? 0);
  let focusContextRemaining = $derived(Math.max(0, focusContextWindow - focusContextUsed));
  let focusIsLive = $derived(Boolean(focusSession && !focusSession.is_idle));
  let focusPureInput = $derived(focusSession
    ? Math.max(0, focusSession.input_tokens - focusSession.cache_write_tokens - focusSession.cache_read_tokens)
    : 0
  );
  let focusTokenTotal = $derived.by(() => focusSession
    ? focusPureInput + focusSession.output_tokens + focusSession.cache_write_tokens + focusSession.cache_read_tokens
    : 0
  );
  let focusTokenMix = $derived(focusSession ? [
    { label: "Input", value: focusPureInput, color: "var(--info)" },
    { label: "Output", value: focusSession.output_tokens, color: "var(--token-output)" },
    { label: "Cache write", value: focusSession.cache_write_tokens, color: "var(--token-cache-write)" },
    { label: "Cache read", value: focusSession.cache_read_tokens, color: "var(--token-cache-read)" },
  ] : []);
  let burnRate = $derived(focusDuration > 0 ? focusCost / (focusDuration / 3600) : 0);
  let hasAllowanceRoutes = $derived($selectedAccessRoutes.length > 0);
</script>

<div class="dashboard app-view" data-dashboard-layout="direction-two">
  <div class="home-grid" class:without-allowances={!hasAllowanceRoutes}>
    {#if hasAllowanceRoutes}
      <AllowanceRail />
    {/if}
    <section class="work-now">
      <header class="work-now-head">
        <div>
          <h2>Live workspace</h2>
          <p>Your current session — context, spend, and throughput.</p>
        </div>
        {#if $backendConnection !== "live"}
          <div class="home-status" aria-label="Connection status">
            <span class="status-chip" class:warn={$backendConnection === "disconnected"}>
              <i></i>
              {$backendConnection === "disconnected" ? "Reconnecting…" : "Connecting…"}
            </span>
          </div>
        {/if}
      </header>

      {#if analyticsError}
        <div class="analytics-alert" role="alert">
          <div>
            <strong>Historical analytics unavailable</strong>
            <span>{analyticsError}</span>
          </div>
          <button
            type="button"
            onclick={() => void refresh($selectedAnalyticsProviderScope)}
            disabled={analyticsLoading}
          >
            {analyticsLoading ? "Retrying…" : "Retry"}
          </button>
        </div>
      {/if}

  {#if liveInstances.length > 1}
    <section class="instance-tray" aria-label="Active sessions">
      <div class="instance-tray-head">
        <span class="instance-count">{liveInstances.length} active sessions</span>
      </div>
      <div class="instance-grid" role="tablist" aria-label="Active sessions">
        {#each liveInstances as session (session.session_id)}
          {@const instanceUsed = session.context_used_tokens ?? 0}
          {@const instanceWindow = session.context_window_tokens ?? 0}
          {@const instancePct = instanceWindow > 0
            ? Math.min(100, (instanceUsed / instanceWindow) * 100)
            : 0}
          <button
            class="instance-tab"
            class:selected={session.session_id === selectedFocusId}
            role="tab"
            aria-selected={session.session_id === selectedFocusId}
            aria-label={`${session.project}, ${session.model}`}
            data-session-instance
            onclick={() => (selectedFocusId = session.session_id)}
          >
            <span class="instance-main">
              <strong>{session.session_name ?? session.project}</strong>
              <span>{session.activity}</span>
            </span>
            <span class="instance-meta">
              <span>{session.model}</span>
              <b>{fmtTokens(instanceUsed)} / {fmtTokens(instanceWindow)}</b>
            </span>
            <span class="instance-meter"><i style={`width:${instancePct}%`}></i></span>
          </button>
        {/each}
      </div>
    </section>
  {/if}

  <div class="signal-grid">
    <section class="focus-panel" data-session-focus>
      <div class="focus-head">
        <div>
          <div class="view-kicker">
            <span class="focus-dot" class:live={focusIsLive}></span>
            {focusIsLive ? "Live session" : focusSession ? "Recent session" : "Latest session"}
          </div>
          <h1>{focusName}</h1>
          <div class="focus-meta">
            <span>{focusProject}</span>
            <span>{focusBranch}</span>
            <span>{focusModel}</span>
            <span>{focusDuration > 0 ? fmtDuration(focusDuration) : "Waiting"}</span>
          </div>
        </div>
        <span class="focus-state" class:live={focusIsLive}>
          {focusIsLive ? "Running" : focusSession ? "Idle" : "History"}
        </span>
      </div>

      <div class="focus-values">
        <div>
          <span class="focus-label">Current cost</span>
          <strong>{fmtExactCost(focusCost, focusCostAvailable)}</strong>
          {#if focusCostBasis !== "exact"}
            <span class="focus-note">{focusCostNote}</span>
          {/if}
        </div>
        <div>
          <span class="focus-label">Burn rate</span>
          <strong>{focusCostAvailable && burnRate > 0 ? `${fmtCost(burnRate)}/hr` : "—"}</strong>
        </div>
        <div>
          <span class="focus-label">Cumulative tokens</span>
          <strong>{fmtTokens(focusTokens)}</strong>
        </div>
      </div>

      <div class="focus-chart" aria-label="Session token composition">
        {#if focusSession && focusTokenTotal > 0}
          <div class="focus-chart-head">
            <strong>{focusIsLive ? "Cumulative token mix" : "Recent token mix"}</strong>
            <span>{fmtTokens(focusTokenTotal)} tokens this session</span>
          </div>
          <div class="mix-track" aria-hidden="true">
            {#each focusTokenMix as item}
              <span style={`width:${(item.value / focusTokenTotal) * 100}%;background:${item.color}`}></span>
            {/each}
          </div>
          <div class="mix-legend">
            {#each focusTokenMix as item}
              <div><i style={`background:${item.color}`}></i><span>{item.label}</span><strong>{fmtTokens(item.value)}</strong></div>
            {/each}
          </div>
        {:else}
          <div class="focus-empty">
            <strong>No live counters yet</strong>
            <span>The token breakdown appears here once a session starts reporting.</span>
          </div>
        {/if}
      </div>
    </section>

    <aside class="telemetry-ledger" data-telemetry-ledger>
      <div class="ledger-head">
        <div>
          <span class="view-kicker">This session</span>
          <h2>Session status</h2>
        </div>
        <span class="ledger-date">Today</span>
      </div>

      {#if focusSession}
        <section class="ledger-section">
          <div class="ledger-row">
            <span>Context Window</span>
            <strong>{focusContextWindow > 0 ? `${fmtTokens(focusContextUsed)} / ${fmtTokens(focusContextWindow)}` : "No reading"}</strong>
          </div>
          <div class="ledger-track">
            <span
              class:warn={focusContextPct >= 70 && focusContextPct < 85}
              class:danger={focusContextPct >= 85}
              style={`width:${focusContextPct}%`}
            ></span>
          </div>
          <span class="ledger-note">{focusContextWindow > 0 ? `${fmtPct(focusContextPct)} used · ${fmtTokens(focusContextRemaining)} available` : "Window unavailable"}</span>
        </section>

        <section class="ledger-section">
          <div class="ledger-row"><span>Current activity</span><strong>{focusSession.activity}</strong></div>
          <span class="ledger-note">{focusSession.activity_target ?? "No target reported"}</span>
        </section>

        <section class="ledger-section split">
          <div>
            <span class="ledger-note">Reasoning effort</span>
            <strong>{focusSession.effort}</strong>
          </div>
          <div>
            <span class="ledger-note">Output speed</span>
            <strong>{focusSession.tokens_per_sec ? fmtTps(focusSession.tokens_per_sec) : "—"}</strong>
          </div>
        </section>
      {:else}
        <div class="ledger-empty">
          <strong>No live session</strong>
          <span>Context Window, activity, and throughput appear here once a session starts.</span>
        </div>
      {/if}

      <section class="ledger-section">
        <div class="ledger-row">
          <span>Discord presence</span>
          <strong class:connected={$health?.discord_status === "Connected"}>{$health?.discord_status ?? "Detecting"}</strong>
        </div>
        <span class="ledger-note">Rich Presence connection</span>
      </section>
    </aside>
  </div>

  {#if hasOperationalSummary}
  <section class="glance-section">
    <div class="glance-head">
      <span class="view-kicker">At a glance</span>
    </div>
    <div class="insight-row">
    {#if showCacheR + showInput > 0}
    <div class="insight-card">
      <div class="cache-grade" style="color:{cacheGrade.color}">
        <span class="grade-letter">{cacheGrade.letter}</span>
        <div class="grade-info">
          <span class="grade-title">Cache health</span>
          <span class="grade-ratio">{fmtPct(showCacheHit)} hit ratio</span>
        </div>
      </div>
    </div>
    {/if}

    {#if forecast && forecast.spent_this_month > 0}
      <div class="insight-card">
        <div class="forecast-info">
          <span class="forecast-label">Monthly projection</span>
          <span class="forecast-value">{fmtCost(forecast.projected_monthly)}</span>
          <span class="forecast-meta">
            {fmtCost(forecast.spent_this_month)} spent
            ({forecast.days_elapsed}/{forecast.days_in_month} days)
          </span>
        </div>
      </div>
    {/if}

    {#if topModel && topModel.pct > 60}
      <div class="insight-card">
        <div class="routing-info">
          <span class="routing-label">Model focus</span>
          <span class="routing-value">{fmtPct(topModel.pct)} {topModel.name}</span>
          <span class="routing-meta">{topModel.sessions} of {sessionCount} sessions</span>
        </div>
      </div>
    {/if}

    {#if hourlyData.length > 0}
      <div class="insight-card heatmap-card">
        <span class="heatmap-title">Activity by hour</span>
        <Heatmap data={hourlyData} />
      </div>
    {/if}
    </div>
  </section>
  {/if}

    </section>
  </div>
</div>

<style>
  .dashboard {
    min-height: 100%;
    display: flex;
    flex-direction: column;
  }
  .home-status { display: flex; align-items: center; justify-content: flex-end; gap: 7px; }
  .status-chip {
    min-height: 28px;
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 5px 9px;
    color: var(--text-muted);
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-full);
    font-size: 9px;
    white-space: nowrap;
  }
  .status-chip i { width: 6px; height: 6px; background: var(--text-placeholder); border-radius: 50%; }
  .status-chip.warn { color: var(--warning); border-color: color-mix(in srgb, var(--warning) 30%, var(--border)); }
  .status-chip.warn i { background: var(--warning); }
  .home-grid {
    flex: 1;
    display: grid;
    grid-template-columns: clamp(280px, 23vw, 360px) minmax(0, 1fr);
    gap: 0;
    align-items: stretch;
    min-height: 100%;
    /* One unified surface: the grid itself is the card. Its two columns
       (Provider limits + Live workspace) share this matte panel and are separated by
       a single interior divider, never two floating cards. */
    position: relative;
    background: var(--panel-sheen), var(--surface-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: var(--elev-1);
    overflow: hidden;
  }
  .home-grid::before {
    content: "";
    position: absolute;
    inset: 0 0 auto 0;
    height: 1px;
    background: var(--panel-edge);
    z-index: 1;
    pointer-events: none;
  }
  /* An unproved quota source must not reserve a permanent empty column. Session
   * telemetry remains useful and expands into the reclaimed workspace. */
  .home-grid.without-allowances { grid-template-columns: minmax(0, 1fr); }
  .work-now {
    min-width: 0;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    /* Column of the shared home-grid card, divided from Provider limits by one
       interior hairline instead of its own border/shadow. */
    border-left: 1px solid var(--divider);
  }
  .home-grid.without-allowances .work-now { border-left: 0; }
  .work-now-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 18px;
    padding: 20px 22px 17px;
  }
  .work-now-head h2 { font-size: 19px; letter-spacing: -0.03em; }
  .work-now-head p { margin-top: 5px; color: var(--text-muted); font-size: 11px; }
  .analytics-alert {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    margin: 0 14px 2px;
    padding: 11px 12px;
    color: var(--text-secondary);
    background: color-mix(in srgb, var(--danger) 7%, var(--surface-panel-soft));
    border: 1px solid color-mix(in srgb, var(--danger) 24%, var(--divider));
    border-radius: var(--radius-md);
  }
  .analytics-alert > div { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .analytics-alert strong { color: var(--text-primary); font-size: var(--fs-sm); }
  .analytics-alert span { overflow: hidden; color: var(--text-muted); font-size: var(--fs-xs); text-overflow: ellipsis; white-space: nowrap; }
  .analytics-alert button {
    min-height: 30px;
    flex: 0 0 auto;
    padding: 5px 11px;
    color: var(--text-primary);
    background: var(--bg-card);
    border: 1px solid var(--border-hover);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font: 650 var(--fs-xs) var(--font-sans);
  }
  .analytics-alert button:disabled { cursor: wait; opacity: 0.6; }
  .instance-tray {
    margin: 0 14px 2px;
    overflow: hidden;
    background: var(--surface-panel-soft);
    border: 1px solid var(--divider);
    border-radius: var(--radius-md);
  }
  .instance-tray-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 9px 12px 7px;
    border-bottom: 1px solid var(--divider);
  }
  .instance-count {
    color: var(--text-secondary);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: var(--letter-wider);
    text-transform: uppercase;
  }
  .instance-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); }
  .instance-tab {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    padding: 11px 12px 12px;
    color: var(--text-secondary);
    background: transparent;
    border: 0;
    border-right: 1px solid var(--divider);
    cursor: pointer;
    font: inherit;
    text-align: left;
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
  }
  .instance-tab:last-child { border-right: 0; }
  .instance-tab::after { content: ""; position: absolute; inset: auto 12px 0; height: 2px; background: transparent; }
  .instance-tab:hover { background: var(--surface-panel-soft); color: var(--text-primary); }
  .instance-tab.selected { color: var(--text-primary); background: var(--bg-card); }
  .instance-tab.selected::after { background: var(--provider-accent); }
  .instance-main, .instance-meta { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; min-width: 0; }
  .instance-main strong { overflow: hidden; color: inherit; font-size: 12px; font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  .instance-main > span { flex-shrink: 0; overflow: hidden; max-width: 44%; color: var(--text-muted); font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
  .instance-meta { color: var(--text-muted); font: 500 10px var(--font-mono); }
  .instance-meta > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .instance-meta b { flex-shrink: 0; color: var(--text-secondary); font-weight: 600; }
  .instance-meter { height: 2px; overflow: hidden; background: var(--meter-track); }
  .instance-meter i { display: block; height: 100%; background: var(--info); }
  .signal-grid {
    display: grid;
    grid-template-columns: minmax(0, 2fr) minmax(250px, 0.78fr);
    align-items: stretch;
    margin-top: 2px;
  }
  .focus-panel, .telemetry-ledger { min-width: 0; padding: 22px; }
  .focus-panel { display: flex; flex-direction: column; gap: 20px; }
  .focus-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 20px; }
  .focus-head h1 { margin-top: 7px; font-size: clamp(26px, 2.65vw, 38px); font-weight: 620; line-height: 1.05; letter-spacing: var(--letter-tighter); }
  .focus-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--text-placeholder); }
  .focus-dot.live { background: var(--success); box-shadow: 0 0 0 3px var(--success-dim); }
  .focus-meta { display: flex; flex-wrap: wrap; gap: 8px 16px; margin-top: 12px; color: var(--text-muted); font-size: var(--fs-sm); }
  .focus-meta span:not(:first-child)::before { content: "·"; margin-right: 16px; color: var(--border-hover); }
  .focus-state { max-width: 180px; flex: 0 0 auto; padding: 5px 10px; overflow: hidden; color: var(--text-muted); background: var(--surface-panel-soft); border: 1px solid var(--border); border-radius: var(--radius-full); font-size: var(--fs-xs); text-overflow: ellipsis; white-space: nowrap; }
  .focus-state.live { color: var(--success); background: var(--success-dim); border-color: color-mix(in srgb, var(--success) 30%, transparent); }
  .focus-values {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    padding: 4px 0;
    background: var(--surface-panel-soft);
    border: 1px solid var(--divider);
    border-radius: var(--radius-md);
  }
  .focus-values > div { display: flex; flex-direction: column; gap: 4px; padding: 13px 16px; border-right: 1px solid var(--divider); }
  .focus-values > div:last-child { border-right: 0; }
  .focus-label { color: var(--text-muted); font-size: var(--fs-xs); font-weight: 700; letter-spacing: var(--letter-wider); text-transform: uppercase; }
  .focus-values strong { color: var(--text-primary); font-size: clamp(19px, 2vw, 27px); font-variant-numeric: tabular-nums; letter-spacing: var(--letter-tight); }
  .focus-note { color: var(--text-muted); font-size: 11px; }
  .focus-chart { display: flex; flex-direction: column; gap: 12px; min-height: 82px; overflow: hidden; }
  .focus-chart-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; }
  .focus-chart-head strong { font-size: 12px; font-weight: 650; }
  .focus-chart-head span { color: var(--text-muted); font: 500 10px var(--font-mono); text-align: right; }
  .mix-track { display: flex; height: 7px; overflow: hidden; background: var(--meter-track); border-radius: var(--radius-full); }
  .mix-track span { min-width: 0; height: 100%; }
  .mix-legend { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 8px 14px; }
  .mix-legend > div { display: grid; grid-template-columns: auto 1fr; align-items: center; gap: 2px 7px; min-width: 0; }
  .mix-legend i { grid-row: 1 / span 2; width: 6px; height: 6px; border-radius: 50%; }
  .mix-legend span { overflow: hidden; color: var(--text-muted); font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
  .mix-legend strong { color: var(--text-secondary); font: 600 11px var(--font-mono); }

  .focus-empty { display: flex; flex-direction: column; align-items: flex-start; justify-content: center; gap: 5px; min-height: 76px; color: var(--text-muted); text-align: left; }
  .focus-empty strong { color: var(--text-secondary); font-size: var(--fs-sm); }
  .focus-empty span { font-size: var(--fs-xs); }

  .telemetry-ledger {
    display: flex;
    flex-direction: column;
    align-self: stretch;
    background: color-mix(in srgb, var(--bg-elevated) 52%, transparent);
    border-left: 1px solid var(--divider);
  }
  .ledger-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 12px; padding-bottom: 16px; border-bottom: 1px solid var(--divider); }
  .ledger-head h2 { margin-top: 4px; font-size: var(--fs-lg); font-weight: 650; letter-spacing: var(--letter-tight); }
  .ledger-date { color: var(--text-muted); font-family: var(--font-mono); font-size: var(--fs-xs); }
  .ledger-section { display: flex; flex-direction: column; gap: 8px; padding: 16px 0; border-bottom: 1px solid var(--divider); }
  .ledger-section:last-child { border-bottom: 0; padding-bottom: 0; }
  .ledger-empty { display: flex; flex-direction: column; gap: 6px; padding: 26px 0; color: var(--text-muted); font-size: var(--fs-xs); line-height: 1.5; }
  .ledger-empty strong { color: var(--text-secondary); font-size: var(--fs-sm); }
  .ledger-row { display: flex; align-items: baseline; justify-content: space-between; gap: 14px; }
  .ledger-row { color: var(--text-secondary); font-size: var(--fs-sm); }
  .ledger-row strong { max-width: 58%; overflow: hidden; color: var(--text-primary); font-size: var(--fs-sm); text-align: right; text-overflow: ellipsis; white-space: nowrap; }
  .ledger-track { height: 7px; overflow: hidden; background: var(--bg-elevated); border-radius: var(--radius-full); }
  .ledger-track span { display: block; height: 100%; min-width: 0; background: var(--success); border-radius: inherit; transition: width 0.35s var(--ease-out); }
  .ledger-track span.warn { background: var(--warning); }
  .ledger-track span.danger { background: var(--danger); }
  .ledger-note { color: var(--text-muted); font-size: var(--fs-xs); }
  .ledger-section.split { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; }
  .ledger-section.split > div { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .ledger-section.split > div + div { padding-left: 16px; border-left: 1px solid var(--divider); }
  .ledger-section.split strong { overflow: hidden; color: var(--text-secondary); font-size: var(--fs-sm); text-overflow: ellipsis; white-space: nowrap; }
  .ledger-row strong.connected { color: var(--success); }

  .glance-section {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin: 0 14px 14px;
    padding: 14px 16px 15px;
    background: var(--surface-panel-soft);
    border: 1px solid var(--divider);
    border-radius: var(--radius-md);
  }
  .glance-head { display: flex; align-items: baseline; justify-content: space-between; gap: 18px; }
  .insight-row { display: grid; grid-template-columns: repeat(auto-fit, minmax(175px, 1fr)); }
  .insight-card { min-width: 0; padding: 4px 16px; display: flex; flex-direction: column; border-left: 1px solid var(--divider); }
  .insight-card:first-child { padding-left: 0; border-left: 0; }
  .heatmap-card { min-width: 260px; }
  .heatmap-title { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--accent); margin-bottom: 10px; }

  .cache-grade { display: flex; align-items: center; gap: 14px; }
  .grade-letter { font-size: 36px; font-weight: 900; line-height: 1; }
  .grade-info { display: flex; flex-direction: column; gap: 2px; }
  .grade-title { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); }
  .grade-ratio { font-size: 13px; font-weight: 600; color: var(--text-primary); }

  .forecast-info, .routing-info { display: flex; flex-direction: column; gap: 3px; }
  .forecast-label, .routing-label { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); }
  .forecast-value { font-size: 22px; font-weight: 800; color: var(--accent); font-variant-numeric: tabular-nums; }
  .forecast-meta, .routing-meta { font-size: 11px; color: var(--text-muted); }
  .routing-value { font-size: 14px; font-weight: 700; color: var(--text-primary); }

  /* Give each live instance enough room before the rest of the dashboard collapses. */
  @media (max-width: 1180px) {
    .instance-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .instance-tab:nth-child(2n) { border-right: 0; }
    .instance-tab:nth-child(n + 3) { border-top: 1px solid var(--divider); }
  }

  @media (max-width: 920px) {
    .home-grid { grid-template-columns: 1fr; }
    .signal-grid { grid-template-columns: 1fr; }
    .telemetry-ledger { border-top: 1px solid var(--divider); border-left: 0; }
  }

  @media (max-width: 800px) {
    .insight-row { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .heatmap-card { min-width: 0; }
  }

  @media (max-width: 620px) {
    .work-now-head { align-items: flex-start; flex-direction: column; }
    .analytics-alert { align-items: flex-start; flex-direction: column; }
    .home-status { justify-content: flex-start; }
    .instance-grid { grid-template-columns: 1fr; }
    .instance-tab { border-right: 0; border-top: 1px solid var(--divider); }
    .instance-tab:first-child { border-top: 0; }
    .focus-panel, .telemetry-ledger { padding: 17px; }
    .focus-head { flex-direction: column; }
    .focus-values { grid-template-columns: 1fr; }
    .focus-values > div { padding: 12px 14px; border-right: 0; border-bottom: 1px solid var(--divider); }
    .focus-values > div:last-child { border-bottom: 0; }
    .focus-meta span:not(:first-child)::before { display: none; }
    .focus-chart-head { align-items: flex-start; flex-direction: column; }
    .focus-chart-head span { text-align: left; }
    .mix-legend { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .insight-row { grid-template-columns: 1fr; }
    .insight-card { padding: 10px 0; border-top: 1px solid var(--divider); border-left: 0; }
    .insight-card:first-child { border-top: 0; }
  }
</style>

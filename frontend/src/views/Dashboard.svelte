<script lang="ts">
  import { onMount } from "svelte";
  import StatCard from "../components/StatCard.svelte";
  import ProgressBar from "../components/ProgressBar.svelte";
  import Sparkline from "../components/Sparkline.svelte";
  import Heatmap from "../components/Heatmap.svelte";
  import { health, metrics, sessions, rateLimits, planInfo } from "../lib/stores";
  import { providerProfile } from "../lib/provider";
  import { fmtTokens, fmtCost, fmtDuration, fmtPct, fmtTps, formatResetDateTime } from "../lib/utils";
  import {
    getAnalyticsSummary, getSessionHistory, getCostForecast,
    getHourlyActivity, getDailyStats, getProjectStats, refreshUsage,
  } from "../lib/api";
  import { addToast } from "../lib/stores";
  import type { AnalyticsSummary, HistoricalSession, CostForecast, HourlyActivity, DailyStat, ProjectStat } from "../lib/api";

  let summary = $state<AnalyticsSummary | null>(null);
  let histSessions = $state<HistoricalSession[]>([]);
  let forecast = $state<CostForecast | null>(null);
  let hourlyData = $state<HourlyActivity[]>([]);
  let dailyStats = $state<DailyStat[]>([]);
  let projectStats = $state<ProjectStat[]>([]);
  let refreshing = $state(false);
  let selectedFocusId = $state<string | null>(null);

  function isFreshObservation(value: string | null): boolean {
    if (!value) return false;
    const age = Date.now() - new Date(value).getTime();
    return Number.isFinite(age) && age >= 0 && age <= 2 * 60 * 1000;
  }

  function windowLabel(minutes: number): string {
    if (minutes === 300) return "5h";
    if (minutes === 1440) return "24h";
    if (minutes === 10080) return "7d";
    if (minutes > 0 && minutes % 1440 === 0) return `${minutes / 1440}d`;
    if (minutes > 0 && minutes % 60 === 0) return `${minutes / 60}h`;
    return `${minutes}m`;
  }

  function limitLabel(minutes: number): string {
    if (minutes <= 0) return "Account limit";
    if (minutes === 300) return "5h Limit";
    if (minutes === 10080) return "Weekly Limit";
    return `${windowLabel(minutes)} Limit`;
  }

  function limitContext(name: string | null, id: string | null, kind: string): string {
    if (kind === "global") return "All models";
    if (name) return name;
    if (id) return id.replace(/^codex_/, "").replaceAll("_", "-");
    if (kind === "model") return "Model-specific";
    return "Account usage";
  }

  /**
   * Humanises the provenance string the backend observed. The backend already
   * emits a real label for the network path ("OAuth · Max"); these are the
   * remaining raw keys for the local fallbacks, which previously leaked as
   * bare lowercase tokens.
   */
  const SOURCE_LABELS: Record<string, string> = {
    jsonl: "Local transcripts",
    statusline: "Statusline",
    session: "Live session telemetry",
    cached: "Cached reading",
  };

  function sourceLabel(source: string): string {
    if (source.startsWith("Codex account API")) return "Codex account quota · live";
    if (source.startsWith("Codex JSONL")) return "Codex local telemetry";
    return SOURCE_LABELS[source.trim().toLowerCase()] ?? source;
  }

  function creditsDisplay(balance: string | null, unlimited: boolean): string {
    if (unlimited) return "Unlimited";
    if (balance == null) return "Unavailable";
    const numeric = Number(balance);
    return Number.isFinite(numeric) ? numeric.toLocaleString() : balance;
  }

  async function handleRefresh(): Promise<void> {
    if (refreshing) return;
    refreshing = true;
    try {
      await refreshUsage();
      addToast(
        $providerProfile.id === "claude"
          ? "Refreshing Claude usage from Anthropic..."
          : "Refreshing Codex account quota...",
        "info",
        2500,
      );
      setTimeout(() => { refreshing = false; }, 5500);
    } catch (err) {
      addToast(`Refresh failed: ${String(err)}`, "danger", 3500);
      refreshing = false;
    }
  }

  async function refresh(): Promise<void> {
    [summary, histSessions, forecast, hourlyData, dailyStats, projectStats] = await Promise.all([
      getAnalyticsSummary(),
      getSessionHistory(30, undefined, 50),
      getCostForecast(),
      getHourlyActivity(30),
      getDailyStats(14),
      getProjectStats(30),
    ]);
  }

  onMount(() => { void refresh(); });

  let hasSessions = $derived(($metrics?.session_count ?? 0) > 0);
  let totalCost = $derived(hasSessions ? $metrics!.total_cost : (summary?.total_cost ?? 0));
  let totalTokens = $derived(hasSessions ? $metrics!.total_tokens : (summary?.total_tokens ?? 0));
  let sessionCount = $derived(hasSessions ? $metrics!.session_count : (summary?.total_sessions ?? 0));

  let avgTps = $derived.by(() => {
    if (!$sessions.length) return 0;
    return $sessions.reduce((sum, s) => sum + s.tokens_per_sec, 0) / $sessions.length;
  });

  let totalInput = $derived($metrics?.pure_input_tokens ?? 0);
  let totalOutput = $derived($metrics?.output_tokens ?? 0);
  let totalCacheW = $derived($metrics?.cache_write_tokens ?? 0);
  let totalCacheR = $derived($metrics?.cache_read_tokens ?? 0);
  let tokenTotal = $derived(totalInput + totalOutput + totalCacheW + totalCacheR);

  let histInput = $derived(histSessions.reduce((s, h) => s + Math.max(0, h.input_tokens - h.cache_write_tokens - h.cache_read_tokens), 0));
  let histOutput = $derived(histSessions.reduce((s, h) => s + h.output_tokens, 0));
  let histCacheW = $derived(histSessions.reduce((s, h) => s + h.cache_write_tokens, 0));
  let histCacheR = $derived(histSessions.reduce((s, h) => s + h.cache_read_tokens, 0));
  let histTokenTotal = $derived(histInput + histOutput + histCacheW + histCacheR);

  let showInput = $derived(hasSessions ? totalInput : histInput);
  let showOutput = $derived(hasSessions ? totalOutput : histOutput);
  let showCacheW = $derived(hasSessions ? totalCacheW : histCacheW);
  let showCacheR = $derived(hasSessions ? totalCacheR : histCacheR);
  let showTokenTotal = $derived(hasSessions ? tokenTotal : histTokenTotal);

  let showInputCost = $derived(hasSessions ? ($metrics?.input_cost ?? 0) : histSessions.reduce((s, h) => s + h.input_cost, 0));
  let showOutputCost = $derived(hasSessions ? ($metrics?.output_cost ?? 0) : histSessions.reduce((s, h) => s + h.output_cost, 0));
  let showCacheWCost = $derived(hasSessions ? ($metrics?.cache_write_cost ?? 0) : histSessions.reduce((s, h) => s + h.cache_write_cost, 0));
  let showCacheRCost = $derived(hasSessions ? ($metrics?.cache_read_cost ?? 0) : histSessions.reduce((s, h) => s + h.cache_read_cost, 0));
  let showCostTotal = $derived(showInputCost + showOutputCost + showCacheWCost + showCacheRCost);
  let showCacheHit = $derived(hasSessions ? ($metrics?.cache_hit_ratio ?? 0) : (showCacheR + showInput > 0 ? showCacheR / (showCacheR + showInput) * 100 : 0));

  let modelGroups = $derived.by(() => {
    if (hasSessions && $metrics?.models.length) return $metrics.models;
    const map: Record<string, { sessions: number; cost: number; tokens: number }> = {};
    histSessions.forEach((h) => {
      const e = map[h.model] ?? { sessions: 0, cost: 0, tokens: 0 };
      e.sessions++;
      e.cost += h.total_cost;
      e.tokens += h.total_tokens;
      map[h.model] = e;
    });
    return Object.entries(map).map(([model, v]) => ({ model, ...v })).sort((a, b) => b.cost - a.cost);
  });

  let dailyCostTrend = $derived(dailyStats
    .reduce<Record<string, number>>((acc, d) => { acc[d.date] = (acc[d.date] ?? 0) + d.total_cost; return acc; }, {})
  );
  let sparkCost = $derived(Object.entries(dailyCostTrend).sort(([a], [b]) => a.localeCompare(b)).map(([_, v]) => v));
  let sparkTokens = $derived.by(() => {
    const agg = dailyStats.reduce<Record<string, number>>((acc, d) => { acc[d.date] = (acc[d.date] ?? 0) + d.total_tokens; return acc; }, {});
    return Object.entries(agg).sort(([a], [b]) => a.localeCompare(b)).map(([_, v]) => v);
  });
  let sparkSessions = $derived.by(() => {
    const agg = dailyStats.reduce<Record<string, number>>((acc, d) => { acc[d.date] = (acc[d.date] ?? 0) + d.session_count; return acc; }, {});
    return Object.entries(agg).sort(([a], [b]) => a.localeCompare(b)).map(([_, v]) => v);
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

  let liveInstances = $derived($sessions.filter((session) => !session.is_idle));
  let visibleInstances = $derived(liveInstances.length > 0 ? liveInstances : $sessions);
  $effect(() => {
    const instances = visibleInstances;
    if (!instances.some((session) => session.session_id === selectedFocusId)) {
      selectedFocusId = instances[0]?.session_id ?? null;
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
  let focusName = $derived(
    focusSession?.session_name
      ?? focusSession?.project
      ?? focusHistory?.session_name
      ?? focusHistory?.project
      ?? "No active session",
  );
  let focusProject = $derived(focusSession?.project ?? focusHistory?.project ?? "Waiting for telemetry");
  let focusModel = $derived(focusSession?.model ?? focusHistory?.model ?? $providerProfile.productName);
  let focusBranch = $derived(focusSession?.branch ?? focusHistory?.branch ?? "—");
  let focusDuration = $derived(focusSession?.duration_secs ?? focusHistory?.duration_secs ?? 0);
  let focusCost = $derived(focusSession?.cost ?? focusHistory?.total_cost ?? totalCost);
  let focusTokens = $derived(focusSession?.tokens ?? focusHistory?.total_tokens ?? totalTokens);
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
</script>

<div class="dashboard" data-dashboard-layout="signal-ledger">
  {#if liveInstances.length > 1}
    <section class="instance-tray surface-matte" aria-label="Live instances">
      <div class="instance-tray-head">
        <span class="instance-count">{liveInstances.length} live instances</span>
        <span class="instance-sync"><span></span>Backend live</span>
      </div>
      <div class="instance-grid" role="tablist" aria-label="Live session instances">
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
    <section class="focus-panel surface-panel" data-session-focus>
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
          <strong>{fmtCost(focusCost)}</strong>
        </div>
        <div>
          <span class="focus-label">Burn rate</span>
          <strong>{burnRate > 0 ? `${fmtCost(burnRate)}/hr` : "—"}</strong>
        </div>
        <div>
          <span class="focus-label">Session tokens</span>
          <strong>{fmtTokens(focusTokens)}</strong>
        </div>
      </div>

      <div class="focus-chart" aria-label="Session token composition">
        {#if focusSession && focusTokenTotal > 0}
          <div class="focus-chart-head">
            <strong>{focusIsLive ? "Live token mix" : "Recent token mix"}</strong>
            <span>{fmtTokens(focusTokenTotal)} observed · backend session counters</span>
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
            <strong>Waiting for live counters</strong>
            <span>Pulse will render the provider-reported session mix here.</span>
          </div>
        {/if}
      </div>
    </section>

    <aside class="telemetry-ledger" data-telemetry-ledger>
      <div class="ledger-head">
        <div>
          <span class="view-kicker">Current session</span>
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
        <span class="ledger-note">Backend IPC status</span>
      </section>
    </aside>
  </div>

  <div class="stats-row metric-strip">
    <StatCard label="Total Cost (Live)" value={fmtCost(totalCost)}>
      {#snippet extra()}<Sparkline data={sparkCost} color="var(--accent)" />{/snippet}
    </StatCard>
    <StatCard label="Total Tokens" value={fmtTokens(totalTokens)}>
      {#snippet extra()}<Sparkline data={sparkTokens} color="var(--info)" />{/snippet}
    </StatCard>
    <StatCard label="Sessions" value={String(sessionCount)}>
      {#snippet extra()}<Sparkline data={sparkSessions} color="var(--success)" />{/snippet}
    </StatCard>
    <StatCard label="Avg Duration" value={summary ? fmtDuration(summary.avg_duration_secs) : "—"}>
      {#snippet extra()}
        {#if summary && summary.avg_cost_per_session > 0}
          <span class="stat-sub">{fmtCost(summary.avg_cost_per_session)}/session</span>
        {/if}
      {/snippet}
    </StatCard>
  </div>

  <div class="insight-row">
    <div class="card insight-card">
      <div class="cache-grade" style="color:{cacheGrade.color}">
        <span class="grade-letter">{cacheGrade.letter}</span>
        <div class="grade-info">
          <span class="grade-title">Cache Health</span>
          <span class="grade-ratio">{fmtPct(showCacheHit)} hit ratio</span>
        </div>
      </div>
    </div>

    {#if forecast && forecast.spent_this_month > 0}
      <div class="card insight-card">
        <div class="forecast-info">
          <span class="forecast-label">Monthly Projection</span>
          <span class="forecast-value">{fmtCost(forecast.projected_monthly)}</span>
          <span class="forecast-meta">
            {fmtCost(forecast.spent_this_month)} spent
            ({forecast.days_elapsed}/{forecast.days_in_month} days)
          </span>
        </div>
      </div>
    {/if}

    {#if topModel && topModel.pct > 60}
      <div class="card insight-card">
        <div class="routing-info">
          <span class="routing-label">Model Focus</span>
          <span class="routing-value">{fmtPct(topModel.pct)} {topModel.name}</span>
          <span class="routing-meta">{topModel.sessions} of {sessionCount} sessions</span>
        </div>
      </div>
    {/if}

    {#if hourlyData.length > 0}
      <div class="card insight-card heatmap-card">
        <span class="heatmap-title">Activity by Hour</span>
        <Heatmap data={hourlyData} />
      </div>
    {/if}
  </div>

  <div class="charts-row">
    <div class="card surface-matte quota-card">
      <div class="usage-header">
        <div>
          <h3 class="card-title">Account quota</h3>
          <span class="card-context">{$planInfo?.plan_name ?? $providerProfile.productName}</span>
        </div>
        <button
          class="refresh-btn"
          class:spinning={refreshing}
          onclick={handleRefresh}
          title={$providerProfile.id === "claude" ? "Refresh usage from Anthropic API" : "Refresh Codex telemetry"}
        >
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M20.49 9A9 9 0 005.64 5.64L1 10m22 4l-4.64 4.36A9 9 0 013.51 15"/></svg>
        </button>
      </div>
      {#if $rateLimits?.usage && ($rateLimits.usage.scopes.length > 0 || $rateLimits.usage.credits)}
        <div class="quota-list">
          {#each $rateLimits.usage.scopes as scope (`${scope.kind}:${scope.id ?? scope.name ?? "default"}`)}
            {#each scope.windows as window (`${window.window_minutes}:${window.resets_at ?? "none"}`)}
              <section class="quota-row">
                <ProgressBar
                  label={limitLabel(window.window_minutes)}
                  sublabel={limitContext(scope.name, scope.id, scope.kind)}
                  pct={window.used_percent}
                  remainingPct={window.remaining_percent}
                  meta={window.resets_at ? `Resets ${formatResetDateTime(window.resets_at)}` : "Reset time unavailable"}
                />
              </section>
            {/each}
          {/each}
          {#if $rateLimits.usage.credits}
            <section class="credits-row">
              <div class="credits-copy">
                <span class="credits-title">Credits Available</span>
                <span class="credits-meta">{#if $rateLimits.usage.credits.unlimited}No metered balance{:else if $rateLimits.usage.credits.has_credits}Available beyond plan limits{:else}Explicit account balance{/if}</span>
              </div>
              <strong class="credits-value">{creditsDisplay($rateLimits.usage.credits.balance, $rateLimits.usage.credits.unlimited)}</strong>
            </section>
          {/if}
        </div>
        <div class="usage-footer">
          <span class="source-dot" class:fresh={isFreshObservation($rateLimits.usage.observed_at)} aria-hidden="true"></span>
          <span>{sourceLabel($rateLimits.usage.source)}</span>
          {#if $rateLimits.usage.observed_at}
            <span class="source-separator" aria-hidden="true">·</span>
            <span>{isFreshObservation($rateLimits.usage.observed_at) ? "Live" : "Last observed"} {new Date($rateLimits.usage.observed_at).toLocaleString()}</span>
          {/if}
        </div>
      {:else}
        <div class="empty-hint">{$rateLimits?.source ?? "Waiting for usage data..."}</div>
      {/if}
      {#if $rateLimits && $providerProfile.supportsExtraUsage}
        <div class="extra-usage">
          <div class="extra-header">
            <span class="extra-title">Extra usage</span>
            <span class="extra-badge" class:on={$rateLimits.extra_enabled}>
              <span class="extra-dot"></span>
              {$rateLimits.extra_enabled ? "On" : "Off"}
            </span>
          </div>
          {#if $rateLimits.extra_used != null || $rateLimits.extra_limit != null}
            <div class="extra-grid">
              {#if $rateLimits.extra_used != null}
                <div class="extra-cell">
                  <span class="extra-cell-label">Spent</span>
                  <span class="extra-cell-val">{fmtCost($rateLimits.extra_used)}</span>
                  {#if $rateLimits.extra_pct != null}
                    <span class="extra-cell-meta">{fmtPct($rateLimits.extra_pct)} used</span>
                  {/if}
                </div>
              {/if}
              {#if $rateLimits.extra_limit != null}
                <div class="extra-cell">
                  <span class="extra-cell-label">Monthly cap</span>
                  <span class="extra-cell-val">{fmtCost($rateLimits.extra_limit)}</span>
                  <span class="extra-cell-meta">Spend limit</span>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="card surface-matte breakdown-card">
      <div class="section-headline">
        <div><h3 class="card-title">Cost Breakdown</h3><span class="card-context">Observed session ledger</span></div>
        {#if showCostTotal > 0}<strong>{fmtCost(showCostTotal)}</strong>{/if}
      </div>
      {#if showCostTotal > 0}
        <div class="breakdown-table">
          <div class="bd-row"><span class="bd-dot" style="background:var(--info)"></span><span class="bd-label">Input</span><span class="bd-val">{fmtCost(showInputCost)}</span></div>
          <div class="bd-row"><span class="bd-dot" style="background:var(--token-output)"></span><span class="bd-label">Output</span><span class="bd-val">{fmtCost(showOutputCost)}</span></div>
          <div class="bd-row"><span class="bd-dot" style="background:var(--token-cache-write)"></span><span class="bd-label">Cache Write</span><span class="bd-val">{fmtCost(showCacheWCost)}</span></div>
          <div class="bd-row"><span class="bd-dot" style="background:var(--token-cache-read)"></span><span class="bd-label">Cache Read</span><span class="bd-val">{fmtCost(showCacheRCost)}</span></div>
          <div class="bd-divider"></div>
          <div class="bd-row total"><span class="bd-dot" style="background:transparent"></span><span class="bd-label">Estimated Total</span><span class="bd-val">{fmtCost(showCostTotal)}</span></div>
        </div>
        <div class="bd-metrics">
          <span>Cache Hit Ratio: <strong>{fmtPct(showCacheHit)}</strong></span>
          {#if avgTps > 0}
            <span>Output Speed: <strong>{fmtTps(avgTps)}</strong></span>
          {/if}
          {#if !hasSessions}
            <span class="bd-source">From historical data</span>
          {/if}
        </div>
      {:else}
        <div class="empty-hint">No cost data yet</div>
      {/if}
    </div>
  </div>

  <div class="charts-row">
    <div class="card surface-matte">
      <div class="section-headline">
        <div><h3 class="card-title">Token Consumption</h3><span class="card-context">Input, output, and cache paths</span></div>
        {#if showTokenTotal > 0}<strong>{fmtTokens(showTokenTotal)}</strong>{/if}
      </div>
      {#if showTokenTotal > 0}
        <div class="consumption-grid">
          <div class="cons-row">
            <span class="cons-label"><span class="cons-dot" style="background:var(--info)"></span>Input</span>
            <div class="cons-bar-track"><div class="cons-bar-fill" style="width:{(showInput / showTokenTotal) * 100}%; background:var(--info)"></div></div>
            <span class="cons-val">{fmtTokens(showInput)}</span>
          </div>
          <div class="cons-row">
            <span class="cons-label"><span class="cons-dot" style="background:var(--token-output)"></span>Output</span>
            <div class="cons-bar-track"><div class="cons-bar-fill" style="width:{(showOutput / showTokenTotal) * 100}%; background:var(--token-output)"></div></div>
            <span class="cons-val">{fmtTokens(showOutput)}</span>
          </div>
          <div class="cons-row">
            <span class="cons-label"><span class="cons-dot" style="background:var(--token-cache-write)"></span>Cache Write</span>
            <div class="cons-bar-track"><div class="cons-bar-fill" style="width:{(showCacheW / showTokenTotal) * 100}%; background:var(--token-cache-write)"></div></div>
            <span class="cons-val">{fmtTokens(showCacheW)}</span>
          </div>
          <div class="cons-row">
            <span class="cons-label"><span class="cons-dot" style="background:var(--token-cache-read)"></span>Cache Read</span>
            <div class="cons-bar-track"><div class="cons-bar-fill" style="width:{(showCacheR / showTokenTotal) * 100}%; background:var(--token-cache-read)"></div></div>
            <span class="cons-val">{fmtTokens(showCacheR)}</span>
          </div>
        </div>
        <div class="cons-total">Total: {fmtTokens(showTokenTotal)}{#if !hasSessions} <small>(historical)</small>{/if}</div>
      {:else}
        <div class="empty-hint">No token data yet</div>
      {/if}
    </div>

    <div class="card surface-matte">
      <div class="section-headline"><div><h3 class="card-title">Model Distribution</h3><span class="card-context">Sessions and attributed cost</span></div></div>
      <div class="model-list">
        {#if modelGroups.length}
          {#each modelGroups as m}
            <div class="model-row">
              <div class="model-info">
                <span class="model-name">{m.model}</span>
                <span class="model-meta">{m.sessions} session{m.sessions !== 1 ? "s" : ""} · {fmtTokens(m.tokens)}</span>
              </div>
              <span class="model-cost">{fmtCost(m.cost)}</span>
            </div>
          {/each}
        {:else}
          <div class="empty-hint">No model data yet</div>
        {/if}
      </div>
    </div>
  </div>

  {#if projectStats.length > 1}
    <div class="card surface-matte data-card">
      <div class="section-headline"><div><h3 class="card-title">Projects</h3><span class="card-context">30-day durable ledger · {projectStats.length} projects</span></div></div>
      <div class="project-table">
        <div class="pt-header">
          <span class="pt-col name">Project</span>
          <span class="pt-col">Sessions</span>
          <span class="pt-col">Tokens</span>
          <span class="pt-col">Avg Cost</span>
          <span class="pt-col cost">Total Cost</span>
        </div>
        {#each projectStats.slice(0, 10) as p}
          <div class="pt-row">
            <span class="pt-col name">{p.project}</span>
            <span class="pt-col">{p.session_count}</span>
            <span class="pt-col">{fmtTokens(p.total_tokens)}</span>
            <span class="pt-col">{fmtCost(p.avg_session_cost)}</span>
            <span class="pt-col cost">{fmtCost(p.total_cost)}</span>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if $sessions.length === 0}
    <div class="card surface-matte data-card">
      <div class="section-headline">
        <div>
          <h3 class="card-title">Recent Sessions</h3>
          <span class="card-context">Durable analytics history</span>
        </div>
      </div>
      <div class="session-list">
        {#if histSessions.length > 0}
        <div class="recent-hint">No live sessions detected — showing recent history</div>
        <div class="recent-table">
          <div class="rt-header">
            <span class="rt-col project">Project</span>
            <span class="rt-col model">Model</span>
            <span class="rt-col">Tokens</span>
            <span class="rt-col">Duration</span>
            <span class="rt-col cost">Cost</span>
          </div>
          {#each histSessions.slice(0, 5) as h (h.id)}
            <div class="rt-row">
              <span class="rt-col project">{h.project}</span>
              <span class="rt-col model">{h.model}</span>
              <span class="rt-col">{fmtTokens(h.total_tokens)}</span>
              <span class="rt-col">{h.duration_secs > 0 ? fmtDuration(h.duration_secs) : "—"}</span>
              <span class="rt-col cost">{fmtCost(h.total_cost)}</span>
            </div>
          {/each}
        </div>
        {:else}
        <div class="empty-state">
          <div class="empty-icon">
            <svg width="28" height="28" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9" opacity="0.35"/><path d="M12 8v4l2.5 2.5"/></svg>
          </div>
          <div class="empty-text">No sessions yet</div>
          <div class="empty-sub">Start a {$providerProfile.productName} session to see data</div>
        </div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .dashboard { display: flex; flex-direction: column; gap: 16px; max-width: var(--content-max); margin: 0 auto; }
  .instance-tray { padding: 0; overflow: hidden; }
  .instance-tray-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--divider);
  }
  .instance-count { color: var(--text-primary); font-size: 12px; font-weight: 650; }
  .instance-sync { display: inline-flex; align-items: center; gap: 6px; color: var(--text-muted); font: 600 10px var(--font-mono); }
  .instance-sync > span { width: 6px; height: 6px; border-radius: 50%; background: var(--success); box-shadow: 0 0 0 3px var(--success-dim); }
  .instance-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); }
  .instance-tab {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
    padding: 12px 14px 13px;
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
  .instance-tab::after { content: ""; position: absolute; inset: auto 14px 0; height: 2px; background: transparent; }
  .instance-tab:hover { background: var(--surface-panel-soft); color: var(--text-primary); }
  .instance-tab.selected { color: var(--text-primary); }
  .instance-tab.selected::after { background: var(--info); }
  .instance-main, .instance-meta { display: flex; align-items: baseline; justify-content: space-between; gap: 10px; min-width: 0; }
  .instance-main strong { overflow: hidden; color: inherit; font-size: 12px; font-weight: 650; text-overflow: ellipsis; white-space: nowrap; }
  .instance-main > span { flex-shrink: 0; overflow: hidden; max-width: 44%; color: var(--text-muted); font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
  .instance-meta { color: var(--text-muted); font: 500 10px var(--font-mono); }
  .instance-meta > span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .instance-meta b { flex-shrink: 0; color: var(--text-secondary); font-weight: 600; }
  .instance-meter { height: 2px; overflow: hidden; background: var(--meter-track); }
  .instance-meter i { display: block; height: 100%; background: var(--info); }
  .signal-grid { display: grid; grid-template-columns: minmax(0, 2fr) minmax(280px, 0.82fr); gap: 18px; align-items: start; }
  .focus-panel, .telemetry-ledger { min-width: 0; padding: 20px; }
  .focus-panel { display: flex; flex-direction: column; gap: 18px; }
  .focus-head { display: flex; align-items: flex-start; justify-content: space-between; gap: 20px; }
  .focus-head h1 { margin-top: 7px; font-size: clamp(24px, 2.5vw, 34px); font-weight: 600; line-height: 1.05; letter-spacing: var(--letter-tighter); }
  .focus-dot { width: 7px; height: 7px; border-radius: 50%; background: var(--text-placeholder); }
  .focus-dot.live { background: var(--success); box-shadow: 0 0 0 3px var(--success-dim); }
  .focus-meta { display: flex; flex-wrap: wrap; gap: 8px 16px; margin-top: 12px; color: var(--text-muted); font-size: var(--fs-sm); }
  .focus-meta span:not(:first-child)::before { content: "·"; margin-right: 16px; color: var(--border-hover); }
  .focus-state { max-width: 180px; padding: 5px 10px; overflow: hidden; color: var(--text-muted); background: var(--surface-panel-soft); border: 1px solid var(--border); border-radius: var(--radius-full); font-size: var(--fs-xs); text-overflow: ellipsis; white-space: nowrap; }
  .focus-state.live { color: var(--success); background: var(--success-dim); border-color: color-mix(in srgb, var(--success) 30%, transparent); }
  .focus-values { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); border-block: 1px solid var(--divider); }
  .focus-values > div { display: flex; flex-direction: column; gap: 4px; padding: 14px 18px; border-right: 1px solid var(--divider); }
  .focus-values > div:first-child { padding-left: 0; }
  .focus-values > div:last-child { border-right: 0; }
  .focus-label { color: var(--text-muted); font-size: var(--fs-xs); font-weight: 700; letter-spacing: var(--letter-wider); text-transform: uppercase; }
  .focus-values strong { color: var(--text-primary); font-size: clamp(19px, 2vw, 27px); font-variant-numeric: tabular-nums; letter-spacing: var(--letter-tight); }
  .focus-chart { display: flex; flex-direction: column; gap: 12px; min-height: 96px; overflow: hidden; }
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

  .focus-empty { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 5px; min-height: 96px; color: var(--text-muted); text-align: center; }
  .focus-empty strong { color: var(--text-secondary); font-size: var(--fs-sm); }
  .focus-empty span { font-size: var(--fs-xs); }

  .telemetry-ledger { display: flex; flex-direction: column; align-self: start; padding: 4px 0 4px 20px; border-left: 1px solid var(--divider); }
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

  .stats-row { display: grid; grid-template-columns: repeat(4, 1fr); gap: 0; }
  .charts-row { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }

  .stat-sub { font-size: 10px; color: var(--text-muted); font-weight: 500; }

  .insight-row { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; }
  .insight-card { padding: 16px; display: flex; flex-direction: column; }
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

  .card { background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 20px; transition: border-color 0.2s var(--ease); }
  .card:hover { border-color: var(--border-hover); }
  .card-title { font-size: 12px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.06em; color: var(--accent); margin-bottom: 16px; display: flex; align-items: center; gap: 8px; }
  .card-title::before { content: ""; width: 3px; height: 14px; background: var(--accent); border-radius: 2px; }
  .card-context { display: block; margin-top: 4px; color: var(--text-muted); font-size: 10px; }
  .section-headline { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; margin-bottom: 18px; }
  .section-headline .card-title { margin-bottom: 0; }
  .section-headline > strong { color: var(--text-primary); font-size: 22px; font-variant-numeric: tabular-nums; letter-spacing: var(--letter-tight); }

  .usage-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 16px; }
  .usage-header .card-title { margin-bottom: 0; }
  .refresh-btn {
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-md);
    color: var(--text-muted);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    cursor: pointer;
    transition: all 0.15s var(--ease);
  }
  .refresh-btn:hover {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-dim);
  }
  .refresh-btn.spinning svg {
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin { to { transform: rotate(360deg); } }
  .usage-footer {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 14px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
    font-size: 10px;
    color: var(--text-muted);
    text-align: center;
    letter-spacing: 0.01em;
  }
  .source-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--warning); box-shadow: 0 0 0 3px var(--warning-dim); }
  .source-dot.fresh { background: var(--success); box-shadow: 0 0 0 3px var(--success-dim); }
  .source-separator { color: var(--border-hover); }
  .quota-list {
    display: flex;
    flex-direction: column;
  }
  .quota-row {
    min-width: 0;
    padding: 8px 2px 13px;
    border-bottom: 1px solid var(--border);
  }
  .quota-row:first-child { padding-top: 0; }
  .credits-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 14px 2px 5px;
  }
  .credits-copy { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .credits-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
  .credits-value {
    flex-shrink: 0;
    font-size: clamp(22px, 3vw, 28px);
    line-height: 1;
    letter-spacing: var(--letter-tighter);
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
  }
  .credits-meta { color: var(--text-muted); font-size: var(--fs-sm); }

  .extra-usage {
    margin-top: 16px;
    padding: 14px;
    background: var(--bg-elevated);
    border-radius: var(--radius-md);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .extra-header { display: flex; justify-content: space-between; align-items: center; }
  .extra-title {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .extra-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    background: var(--bg-card);
    border: 1px solid var(--border);
    padding: 3px 9px;
    border-radius: 99px;
  }
  .extra-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--text-muted); }
  .extra-badge.on { color: var(--success); background: var(--success-dim); border-color: transparent; }
  .extra-badge.on .extra-dot { background: var(--success); box-shadow: 0 0 0 3px var(--success-glow); }

  .extra-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  .extra-cell { display: flex; flex-direction: column; gap: 2px; }
  .extra-cell-label {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .extra-cell-val {
    font-size: 16px;
    font-weight: 700;
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
  }
  .extra-cell-meta { font-size: 11px; color: var(--text-muted); }

  .breakdown-table { display: flex; flex-direction: column; gap: 8px; }
  .bd-row { display: flex; align-items: center; gap: 10px; font-size: 13px; }
  .bd-row.total { font-weight: 700; padding-top: 4px; }
  .bd-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .bd-label { flex: 1; color: var(--text-secondary); }
  .bd-row.total .bd-label { color: var(--text-primary); }
  .bd-val { font-weight: 600; color: var(--text-primary); font-variant-numeric: tabular-nums; min-width: 60px; text-align: right; }
  .bd-divider { height: 1px; background: var(--border); margin: 4px 0; }
  .bd-metrics { display: flex; gap: 20px; margin-top: 14px; padding-top: 12px; border-top: 1px solid var(--border); font-size: 12px; color: var(--text-muted); }
  .bd-metrics strong { color: var(--text-primary); }
  .bd-source { font-style: italic; color: var(--text-muted); }

  .consumption-grid { display: flex; flex-direction: column; gap: 12px; }
  .cons-row { display: flex; align-items: center; gap: 10px; }
  .cons-label { display: flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 500; color: var(--text-secondary); min-width: 90px; }
  .cons-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .cons-bar-track { flex: 1; height: 10px; background: var(--bg-elevated); border-radius: 99px; overflow: hidden; }
  .cons-bar-fill { height: 100%; border-radius: 99px; transition: width 0.5s var(--ease); }
  .cons-val { font-size: 12px; font-weight: 700; color: var(--text-primary); min-width: 55px; text-align: right; font-variant-numeric: tabular-nums; }
  .cons-total { margin-top: 10px; font-size: 12px; color: var(--text-muted); text-align: right; font-weight: 600; }

  .model-list { display: flex; flex-direction: column; gap: 4px; }
  .model-row { display: flex; align-items: center; justify-content: space-between; padding: 10px 12px; border-radius: var(--radius-sm); transition: background 0.15s var(--ease); }
  .model-row:hover { background: var(--bg-elevated); }
  .model-info { display: flex; flex-direction: column; gap: 2px; }
  .model-name { font-weight: 600; font-size: 13px; }
  .model-meta { font-size: 11px; color: var(--text-muted); }
  .model-cost { font-weight: 700; font-size: 14px; color: var(--accent); font-variant-numeric: tabular-nums; }

  .project-table { font-size: 12px; --pt-cols: 2fr 80px 90px 90px 90px; }
  .pt-header { display: grid; grid-template-columns: var(--pt-cols); gap: 8px; padding: 8px 10px; border-bottom: 1px solid var(--border); font-weight: 700; color: var(--text-muted); text-transform: uppercase; font-size: 10px; letter-spacing: 0.05em; }
  .pt-row { display: grid; grid-template-columns: var(--pt-cols); gap: 8px; padding: 8px 10px; border-radius: var(--radius-sm); transition: background 0.15s var(--ease); }
  .pt-row:hover { background: var(--bg-elevated); }
  .pt-col { text-align: right; font-variant-numeric: tabular-nums; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .pt-col.name { text-align: left; font-weight: 500; color: var(--text-primary); }
  .pt-col.cost { font-weight: 700; color: var(--accent); }

  .session-list { display: flex; flex-direction: column; gap: 8px; max-height: 400px; overflow-y: auto; }
  .empty-state { text-align: center; padding: 44px 20px; display: flex; flex-direction: column; align-items: center; gap: 6px; }
  .empty-icon {
    width: 48px; height: 48px;
    display: flex; align-items: center; justify-content: center;
    color: var(--text-muted);
    background: var(--bg-elevated);
    border-radius: 50%;
    margin-bottom: 6px;
  }
  .empty-text { font-size: 14px; font-weight: 600; color: var(--text-secondary); }
  .empty-sub { font-size: 12px; color: var(--text-muted); }
  .empty-hint { text-align: center; padding: 20px; color: var(--text-muted); font-size: 12px; }

  .recent-hint { font-size: 11px; color: var(--text-muted); margin-bottom: 12px; font-style: italic; }
  .recent-table { font-size: 12px; --rt-cols: 2fr 1.5fr 90px 80px 80px; }
  .rt-header { display: grid; grid-template-columns: var(--rt-cols); gap: 8px; padding: 8px 10px; border-bottom: 1px solid var(--border); font-weight: 700; color: var(--text-muted); text-transform: uppercase; font-size: 10px; letter-spacing: 0.05em; }
  .rt-row { display: grid; grid-template-columns: var(--rt-cols); gap: 8px; padding: 8px 10px; border-radius: var(--radius-sm); transition: background 0.15s var(--ease); }
  .rt-row:hover { background: var(--bg-elevated); }
  .rt-col { text-align: right; font-variant-numeric: tabular-nums; color: var(--text-secondary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rt-col.project { text-align: left; font-weight: 500; color: var(--text-primary); }
  .rt-col.model { text-align: left; }
  .rt-col.cost { font-weight: 700; color: var(--accent); }

  .card { min-width: 0; }
  .project-table, .recent-table { overflow-x: auto; overscroll-behavior-inline: contain; }
  .project-table > *, .recent-table > * { min-width: 610px; }

  /* Give each live instance enough room before the rest of the dashboard collapses. */
  @media (max-width: 1180px) {
    .instance-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .instance-tab:nth-child(2n) { border-right: 0; }
    .instance-tab:nth-child(n + 3) { border-top: 1px solid var(--divider); }
  }

  @media (max-width: 1050px) {
    .signal-grid { grid-template-columns: 1fr; }
    .stats-row { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .charts-row { grid-template-columns: 1fr; }
  }

  @media (max-width: 620px) {
    .instance-grid { grid-template-columns: 1fr; }
    .instance-tab { border-right: 0; border-top: 1px solid var(--divider); }
    .instance-tab:first-child { border-top: 0; }
    .stats-row { grid-template-columns: 1fr; }
    .focus-panel { padding: 16px; }
    .telemetry-ledger { padding: 0; border-left: 0; }
    .focus-head { flex-direction: column; }
    .focus-values { grid-template-columns: 1fr; }
    .focus-values > div { padding: 12px 0; border-right: 0; border-bottom: 1px solid var(--divider); }
    .focus-values > div:last-child { border-bottom: 0; }
    .focus-meta span:not(:first-child)::before { display: none; }
    .focus-chart-head { align-items: flex-start; flex-direction: column; }
    .focus-chart-head span { text-align: left; }
    .mix-legend { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .card { padding: 14px; }
    .insight-row { grid-template-columns: 1fr; }
    .extra-grid { grid-template-columns: 1fr; }
    .bd-metrics { flex-wrap: wrap; gap: 8px 14px; }
  }
</style>

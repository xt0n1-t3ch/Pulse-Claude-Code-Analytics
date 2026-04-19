<script lang="ts">
  import { onMount } from "svelte";
  import StatCard from "../components/StatCard.svelte";
  import ProgressBar from "../components/ProgressBar.svelte";
  import SessionCard from "../components/SessionCard.svelte";
  import Sparkline from "../components/Sparkline.svelte";
  import Heatmap from "../components/Heatmap.svelte";
  import { health, metrics, sessions, rateLimits, planInfo } from "../lib/stores";
  import { fmtTokens, fmtCost, fmtDuration, fmtPct, fmtTps, formatResetRelative, formatResetWeekly } from "../lib/utils";
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

  async function handleRefresh(): Promise<void> {
    if (refreshing) return;
    refreshing = true;
    try {
      await refreshUsage();
      addToast("Refreshing usage from Anthropic…", "info", 2500);
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

  onMount(() => { refresh(); const iv = setInterval(refresh, 15000); return () => clearInterval(iv); });

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
    const ratio = showCacheHit;
    if (ratio >= 80) return { letter: "A", color: "var(--success)" };
    if (ratio >= 65) return { letter: "B", color: "#77dd77" };
    if (ratio >= 50) return { letter: "C", color: "var(--warning)" };
    if (ratio >= 30) return { letter: "D", color: "#e8a838" };
    return { letter: "F", color: "var(--danger)" };
  });

  let topModel = $derived.by(() => {
    if (!modelGroups.length) return null;
    const total = modelGroups.reduce((s, m) => s + m.sessions, 0);
    const top = modelGroups[0];
    const pct = total > 0 ? (top.sessions / total) * 100 : 0;
    return { name: top.model, pct, sessions: top.sessions };
  });
</script>

<div class="dashboard">
  <div class="stats-row">
    <StatCard label="Total Cost" value={fmtCost(totalCost)}>
      {#snippet extra()}<Sparkline data={sparkCost} color="var(--accent)" />{/snippet}
    </StatCard>
    <StatCard label="Total Tokens" value={fmtTokens(totalTokens)}>
      {#snippet extra()}<Sparkline data={sparkTokens} color="#7cb9e8" />{/snippet}
    </StatCard>
    <StatCard label="Sessions" value={String(sessionCount)}>
      {#snippet extra()}<Sparkline data={sparkSessions} color="#77dd77" />{/snippet}
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
    <div class="card">
      <div class="usage-header">
        <h3 class="card-title">Plan Usage Limits {#if $planInfo}— {$planInfo.plan_name}{/if}</h3>
        <button class="refresh-btn" class:spinning={refreshing} onclick={handleRefresh} title="Refresh usage from Anthropic API">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M20.49 9A9 9 0 005.64 5.64L1 10m22 4l-4.64 4.36A9 9 0 013.51 15"/></svg>
        </button>
      </div>
      <div class="usage-section">
        <div class="usage-group">
          <div class="usage-group-label">Current session</div>
          {#if $rateLimits && $rateLimits.five_hour_resets !== "N/A"}
            <ProgressBar label="Current Session" pct={$rateLimits.five_hour_pct} meta={formatResetRelative($rateLimits.five_hour_resets)} />
          {:else if $rateLimits}
            <div class="empty-hint">{$rateLimits.source}</div>
          {:else}
            <div class="empty-hint">Waiting for usage data...</div>
          {/if}
        </div>
        <div class="usage-divider"></div>
        <div class="usage-group">
          <div class="usage-group-label">Weekly limits</div>
          {#if $rateLimits && $rateLimits.seven_day_resets !== "N/A"}
            <ProgressBar label="All Models" pct={$rateLimits.seven_day_pct} meta={formatResetWeekly($rateLimits.seven_day_resets)} />
            {#if $rateLimits.sonnet_pct != null}
              <ProgressBar label="Sonnet Only" pct={$rateLimits.sonnet_pct} meta={$rateLimits.sonnet_resets ? formatResetWeekly($rateLimits.sonnet_resets) : ""} />
            {/if}
          {:else if $rateLimits}
            <div class="empty-hint">{$rateLimits.source}</div>
          {/if}
        </div>
        {#if $rateLimits?.source}
          <div class="usage-footer">Source: {$rateLimits.source}</div>
        {/if}
      </div>
      {#if $rateLimits}
        <div class="extra-usage">
          <div class="extra-header">
            <span class="extra-title">Extra usage</span>
            <span class="extra-badge">{$rateLimits.extra_enabled ? "On" : "Off"}</span>
          </div>
          {#if $rateLimits.extra_used != null}
            <div class="extra-row">
              <span>{fmtCost($rateLimits.extra_used)} spent</span>
              <span class="extra-dim">{$rateLimits.extra_pct != null ? fmtPct($rateLimits.extra_pct) + " used" : ""}</span>
            </div>
          {/if}
          {#if $rateLimits.extra_limit != null}
            <div class="extra-row">
              <span>{fmtCost($rateLimits.extra_limit)}</span>
              <span class="extra-dim">Monthly spend limit</span>
            </div>
          {/if}
        </div>
      {/if}
    </div>

    <div class="card">
      <h3 class="card-title">Cost Breakdown</h3>
      {#if showCostTotal > 0}
        <div class="breakdown-table">
          <div class="bd-row"><span class="bd-dot" style="background:var(--info)"></span><span class="bd-label">Input</span><span class="bd-val">{fmtCost(showInputCost)}</span></div>
          <div class="bd-row"><span class="bd-dot" style="background:#7cb9e8"></span><span class="bd-label">Output</span><span class="bd-val">{fmtCost(showOutputCost)}</span></div>
          <div class="bd-row"><span class="bd-dot" style="background:#77dd77"></span><span class="bd-label">Cache Write</span><span class="bd-val">{fmtCost(showCacheWCost)}</span></div>
          <div class="bd-row"><span class="bd-dot" style="background:#c3b1e1"></span><span class="bd-label">Cache Read</span><span class="bd-val">{fmtCost(showCacheRCost)}</span></div>
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
    <div class="card">
      <h3 class="card-title">Token Consumption</h3>
      {#if showTokenTotal > 0}
        <div class="consumption-grid">
          <div class="cons-row">
            <span class="cons-label"><span class="cons-dot" style="background:var(--info)"></span>Input</span>
            <div class="cons-bar-track"><div class="cons-bar-fill" style="width:{(showInput / showTokenTotal) * 100}%; background:var(--info)"></div></div>
            <span class="cons-val">{fmtTokens(showInput)}</span>
          </div>
          <div class="cons-row">
            <span class="cons-label"><span class="cons-dot" style="background:#7cb9e8"></span>Output</span>
            <div class="cons-bar-track"><div class="cons-bar-fill" style="width:{(showOutput / showTokenTotal) * 100}%; background:#7cb9e8"></div></div>
            <span class="cons-val">{fmtTokens(showOutput)}</span>
          </div>
          <div class="cons-row">
            <span class="cons-label"><span class="cons-dot" style="background:#77dd77"></span>Cache Write</span>
            <div class="cons-bar-track"><div class="cons-bar-fill" style="width:{(showCacheW / showTokenTotal) * 100}%; background:#77dd77"></div></div>
            <span class="cons-val">{fmtTokens(showCacheW)}</span>
          </div>
          <div class="cons-row">
            <span class="cons-label"><span class="cons-dot" style="background:#c3b1e1"></span>Cache Read</span>
            <div class="cons-bar-track"><div class="cons-bar-fill" style="width:{(showCacheR / showTokenTotal) * 100}%; background:#c3b1e1"></div></div>
            <span class="cons-val">{fmtTokens(showCacheR)}</span>
          </div>
        </div>
        <div class="cons-total">Total: {fmtTokens(showTokenTotal)}{#if !hasSessions} <small>(historical)</small>{/if}</div>
      {:else}
        <div class="empty-hint">No token data yet</div>
      {/if}
    </div>

    <div class="card">
      <h3 class="card-title">Model Distribution</h3>
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
    <div class="card">
      <h3 class="card-title">Projects (30 days)</h3>
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

  <div class="card">
    <h3 class="card-title">{$sessions.length > 0 ? "Live Sessions" : "Recent Sessions"}</h3>
    <div class="session-list">
      {#if $sessions.length > 0}
        {#each $sessions as session (session.session_id)}
          <SessionCard {session} />
        {/each}
      {:else if histSessions.length > 0}
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
          <div class="empty-icon">✳</div>
          <div class="empty-text">No sessions yet</div>
          <div class="empty-sub">Start a Claude Code session to see data</div>
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .dashboard { display: flex; flex-direction: column; gap: 16px; }
  .stats-row { display: grid; grid-template-columns: repeat(4, 1fr); gap: 12px; }
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
  .usage-section { display: flex; flex-direction: column; gap: 8px; }
  .usage-group { display: flex; flex-direction: column; gap: 4px; }
  .usage-group-label { font-size: 11px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); margin-bottom: 2px; }
  .usage-divider { height: 1px; background: var(--border); margin: 6px 0; }
  .usage-footer { margin-top: 8px; font-size: 11px; color: var(--text-muted); }

  .extra-usage { margin-top: 16px; padding-top: 16px; border-top: 1px solid var(--border); display: flex; flex-direction: column; gap: 8px; }
  .extra-header { display: flex; justify-content: space-between; align-items: center; }
  .extra-title { font-size: 13px; font-weight: 700; }
  .extra-badge { font-size: 11px; color: var(--text-muted); background: var(--bg-elevated); padding: 2px 8px; border-radius: 99px; }
  .extra-row { display: flex; justify-content: space-between; font-size: 12px; }
  .extra-dim { color: var(--text-muted); }

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
  .empty-state { text-align: center; padding: 40px 20px; }
  .empty-icon { font-size: 28px; color: var(--accent); margin-bottom: 10px; }
  .empty-text { font-size: 14px; font-weight: 600; color: var(--text-secondary); }
  .empty-sub { font-size: 12px; color: var(--text-muted); margin-top: 4px; }
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
</style>

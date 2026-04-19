<script lang="ts">
  import { onMount } from "svelte";
  import {
    getCacheHealth,
    getRecommendations,
    getInflectionPoints,
    getModelRouting,
    getToolFrequency,
    getPromptComplexity,
    getSessionHealth,
    copyFixPrompt,
    generateHtmlReport,
    generateMarkdownReport,
    type CacheHealthReport,
    type Recommendation,
    type InflectionPoint,
    type ModelRoutingReport,
    type ToolFrequencyReport,
    type PromptComplexityReport,
    type SessionHealthReport,
    type Severity,
  } from "../lib/api";
  import { addToast } from "../lib/stores";
  import { fmtCost } from "../lib/utils";

  let cache = $state<CacheHealthReport | null>(null);
  let recs = $state<Recommendation[]>([]);
  let inflections = $state<InflectionPoint[]>([]);
  let routing = $state<ModelRoutingReport | null>(null);
  let tools = $state<ToolFrequencyReport | null>(null);
  let prompts = $state<PromptComplexityReport | null>(null);
  let health = $state<SessionHealthReport | null>(null);
  let loading = $state(true);
  let hasLoaded = $state(false);
  let days = $state(30);
  let severityFilter = $state<"all" | Severity>("all");

  function withTimeout<T>(p: Promise<T>, ms: number, label: string): Promise<T | null> {
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        console.warn(`${label} timed out after ${ms}ms`);
        resolve(null);
      }, ms);
      p.then((v) => {
        clearTimeout(timer);
        resolve(v);
      }).catch((err) => {
        clearTimeout(timer);
        console.warn(`${label} failed:`, err);
        resolve(null);
      });
    });
  }

  async function loadReports(): Promise<void> {
    loading = true;
    try {
      const [c, r, i, m, t, p, h] = await Promise.all([
        withTimeout(getCacheHealth(days), 6000, "cache_health"),
        withTimeout(getRecommendations(days), 6000, "recommendations"),
        withTimeout(getInflectionPoints(days), 6000, "inflection"),
        withTimeout(getModelRouting(days), 6000, "model_routing"),
        withTimeout(getToolFrequency(days), 8000, "tool_frequency"),
        withTimeout(getPromptComplexity(days), 8000, "prompt_complexity"),
        withTimeout(getSessionHealth(days), 8000, "session_health"),
      ]);
      cache = c;
      recs = r ?? [];
      inflections = i ?? [];
      routing = m;
      tools = t;
      prompts = p;
      health = h;
    } finally {
      loading = false;
      hasLoaded = true;
    }
  }

  onMount(loadReports);

  let lastDays = $state(days);
  $effect(() => {
    if (days !== lastDays) {
      lastDays = days;
      loadReports();
    }
  });

  let filteredRecs = $derived(
    severityFilter === "all"
      ? recs
      : recs.filter((r) => r.severity === severityFilter),
  );

  async function handleFix(rec: Recommendation): Promise<void> {
    if (!rec.fix_prompt) {
      addToast("No prompt available for this recommendation.", "info", 2500);
      return;
    }
    try {
      const prompt = await copyFixPrompt(rec.id);
      await navigator.clipboard.writeText(prompt || rec.fix_prompt);
      addToast("Fix prompt copied — paste into Claude Code.", "success", 3500);
    } catch (err) {
      addToast(`Copy failed: ${String(err)}`, "danger", 4000);
    }
  }

  async function handleCopyMarkdown(): Promise<void> {
    try {
      const md = await generateMarkdownReport(days);
      await navigator.clipboard.writeText(md);
      addToast("Markdown report copied to clipboard.", "success", 3000);
    } catch (err) {
      addToast(`Copy failed: ${String(err)}`, "danger", 4000);
    }
  }

  async function handleDownloadHtml(): Promise<void> {
    try {
      const html = await generateHtmlReport(days);
      const stamp = new Date().toISOString().slice(0, 10);
      const defaultName = `pulse-report-${stamp}.html`;

      // Native OS save dialog so the user picks where the file lands instead
      // of always defaulting to ~/Downloads.
      const { save } = await import("@tauri-apps/plugin-dialog");
      const { writeTextFile } = await import("@tauri-apps/plugin-fs");

      const path = await save({
        defaultPath: defaultName,
        filters: [{ name: "HTML Report", extensions: ["html"] }],
        title: "Save Pulse report",
      });

      if (!path) return;

      await writeTextFile(path, html);
      addToast(`Saved to ${path}`, "success", 3500);
    } catch (err) {
      addToast(`Download failed: ${String(err)}`, "danger", 4000);
    }
  }

  const severityOrder: Record<Severity, number> = {
    critical: 0,
    warning: 1,
    info: 2,
    positive: 3,
  };

  const severityTabs: { id: "all" | Severity; label: string }[] = [
    { id: "all", label: "All" },
    { id: "critical", label: "Critical" },
    { id: "warning", label: "Warning" },
    { id: "info", label: "Info" },
    { id: "positive", label: "Good" },
  ];

  let sortedRecs = $derived(
    [...filteredRecs].sort(
      (a, b) => severityOrder[a.severity] - severityOrder[b.severity],
    ),
  );
</script>

<div class="reports-view">
  <header class="view-header">
    <div>
      <h2 class="view-title">Reports &amp; Insights</h2>
      <p class="view-sub">
        Deep analysis of your Claude Code usage — cache efficiency, model
        routing, cost spikes, and ready-to-paste fixes.
      </p>
    </div>
    <div class="controls">
      <div class="segmented">
        {#each [7, 30, 90, 365] as d}
          <button
            class="seg-btn"
            class:active={days === d}
            onclick={() => (days = d)}
          >
            {d === 365 ? "1y" : `${d}d`}
          </button>
        {/each}
      </div>
      <button class="btn-secondary" onclick={handleCopyMarkdown}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
        Copy Markdown
      </button>
      <button class="btn-primary" onclick={handleDownloadHtml}>
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
        Download HTML
      </button>
    </div>
  </header>

  {#if loading && !hasLoaded}
    <div class="skeleton-stack">
      <div class="skeleton hero"></div>
      <div class="skeleton row"></div>
      <div class="skeleton row"></div>
      <div class="skeleton row short"></div>
    </div>
  {:else}
    {#if cache}
      <section class="card hero-card">
        <div class="hero-left">
          <div
            class="grade-letter"
            style="color: {cache.color}; text-shadow: 0 0 48px {cache.color}44;"
          >
            {cache.grade}
          </div>
          <div class="grade-meta">
            <div class="label">Cache Health</div>
            <div class="ratio">
              {cache.trend_weighted_ratio.toFixed(0)}<span class="pct">%</span>
              <span class="muted"> hit ratio · {cache.grade_label}</span>
            </div>
          </div>
        </div>
        <div class="hero-right">
          <p class="diagnosis">{cache.diagnosis}</p>
          <div class="hero-stats">
            <div class="hero-stat">
              <span class="hs-label">Cache read</span>
              <span class="hs-value">{(cache.total_cache_read / 1e6).toFixed(1)}M</span>
            </div>
            <div class="hero-stat">
              <span class="hs-label">Cache write</span>
              <span class="hs-value">{(cache.total_cache_write / 1e6).toFixed(1)}M</span>
            </div>
            <div class="hero-stat">
              <span class="hs-label">Pure input</span>
              <span class="hs-value">{(cache.total_input / 1e6).toFixed(1)}M</span>
            </div>
            <div class="hero-stat">
              <span class="hs-label">Sessions</span>
              <span class="hs-value">{cache.sessions_analyzed}</span>
            </div>
          </div>
        </div>
      </section>
    {/if}

    <div class="two-col">
      {#if routing}
        <section class="card">
          <h3 class="card-title">Model Routing</h3>
          <p class="card-sub">{routing.diagnosis}</p>
          <div class="routing-bars">
            {#each [
              { label: "Opus", stats: routing.opus, color: "var(--accent)" },
              { label: "Sonnet", stats: routing.sonnet, color: "#7cb9e8" },
              { label: "Haiku", stats: routing.haiku, color: "#77dd77" },
              { label: "Other", stats: routing.other, color: "#c3b1e1" },
            ] as row}
              {#if row.stats.sessions > 0}
                <div class="bar-row">
                  <div class="bar-label">
                    <span class="dot" style="background: {row.color}"></span>
                    {row.label}
                    <span class="bar-count">· {row.stats.sessions}</span>
                  </div>
                  <div class="bar-track">
                    <div
                      class="bar-fill"
                      style="width: {row.stats.cost_share_pct}%; background: {row.color};"
                    ></div>
                  </div>
                  <div class="bar-value">
                    {row.stats.cost_share_pct.toFixed(0)}%
                    <span class="muted">{fmtCost(row.stats.cost)}</span>
                  </div>
                </div>
              {/if}
            {/each}
          </div>
          {#if routing.estimated_savings_if_rerouted > 0}
            <div class="savings-hint">
              <span>Potential savings if ~30% of Opus moves to Sonnet</span>
              <strong>{fmtCost(routing.estimated_savings_if_rerouted)}</strong>
            </div>
          {/if}
        </section>
      {/if}

      <section class="card">
        <h3 class="card-title">Inflection Timeline</h3>
        <p class="card-sub">
          Days where cost-per-session deviated ≥2× from the rolling baseline.
        </p>
        {#if inflections.length === 0}
          <div class="empty-inline">
            No significant cost shifts detected — usage is consistent.
          </div>
        {:else}
          <ul class="inflection-list">
            {#each inflections.slice(0, 6) as point}
              <li
                class="inflection-item"
                class:spike={point.direction === "spike"}
                class:drop={point.direction === "drop"}
              >
                <div class="inflection-head">
                  <span class="inflection-date">{point.date}</span>
                  <span class="inflection-mult">{point.multiplier.toFixed(1)}×</span>
                </div>
                <div class="inflection-note">{point.note}</div>
                <div class="inflection-stats">
                  {point.sessions_on_day} session{point.sessions_on_day === 1 ? "" : "s"}
                  · {fmtCost(point.cost_on_day)}
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    </div>

    {#if health || tools || prompts}
      <div class="grid-2">
        {#if health && health.available}
          <section class="card">
            <h3 class="card-title">Session Health</h3>
            <div class="health-hero">
              <div class="health-grade grade-{health.grade.toLowerCase()}">{health.grade}</div>
              <div class="health-score">{health.health_score}<span class="health-score-sub">/100</span></div>
            </div>
            <p class="card-sub">{health.diagnosis}</p>
            <div class="mini-grid">
              <div class="mini-kv"><span>Avg duration</span><strong>{health.avg_duration_minutes.toFixed(1)} min</strong></div>
              <div class="mini-kv"><span>P90 duration</span><strong>{health.p90_duration_minutes} min</strong></div>
              <div class="mini-kv"><span>Long sessions</span><strong>{health.long_session_pct.toFixed(0)}%</strong></div>
              <div class="mini-kv"><span>Msgs / session</span><strong>{health.avg_messages_per_session.toFixed(1)}</strong></div>
              <div class="mini-kv"><span>Peak overlap</span><strong>{health.peak_overlap_pct}%</strong></div>
              <div class="mini-kv"><span>Compact gaps</span><strong>{health.compact_gap_pct.toFixed(0)}%</strong></div>
            </div>
          </section>
        {/if}

        {#if tools && tools.available}
          <section class="card">
            <h3 class="card-title">Tool Frequency</h3>
            <p class="card-sub">{tools.diagnosis}</p>
            <div class="mini-grid">
              <div class="mini-kv"><span>Total calls</span><strong>{tools.total_tool_calls.toLocaleString()}</strong></div>
              <div class="mini-kv"><span>Avg / session</span><strong>{tools.avg_tools_per_session.toFixed(1)}</strong></div>
              <div class="mini-kv"><span>Calls / hour</span><strong>{tools.avg_tool_calls_per_hour.toFixed(1)}</strong></div>
              <div class="mini-kv"><span>MCP share</span><strong>{tools.mcp_share_pct.toFixed(0)}%</strong></div>
            </div>
            {#if tools.top_tools.length > 0}
              <div class="tool-list">
                {#each tools.top_tools.slice(0, 8) as t}
                  <div class="tool-row">
                    <span class="tool-name">{t.name}</span>
                    <div class="tool-bar-wrap">
                      <div class="tool-bar" style="width:{Math.min(100, t.share_pct)}%"></div>
                    </div>
                    <span class="tool-count">{t.count} · {t.share_pct.toFixed(1)}%</span>
                  </div>
                {/each}
              </div>
            {/if}
          </section>
        {/if}
      </div>
    {/if}

    {#if prompts && prompts.available}
      <section class="card">
        <h3 class="card-title">Prompt Complexity</h3>
        <p class="card-sub">{prompts.diagnosis}</p>
        <div class="mini-grid four">
          <div class="mini-kv"><span>Prompts analyzed</span><strong>{prompts.prompts_analyzed.toLocaleString()}</strong></div>
          <div class="mini-kv"><span>Avg complexity</span><strong>{prompts.avg_complexity_score.toFixed(0)}/100</strong></div>
          <div class="mini-kv"><span>Avg specificity</span><strong>{prompts.avg_specificity_score.toFixed(0)}/100</strong></div>
          <div class="mini-kv"><span>Low specificity</span><strong>{prompts.low_specificity_sessions}</strong></div>
        </div>
        {#if prompts.top_sessions.length > 0}
          <div class="prompt-list">
            {#each prompts.top_sessions.slice(0, 5) as s}
              <div class="prompt-item">
                <div class="prompt-head">
                  <span class="prompt-project">{s.project}</span>
                  <span class="prompt-label">{s.label}</span>
                  <span class="prompt-scores">C:{s.complexity_score} · S:{s.specificity_score}</span>
                </div>
                <div class="prompt-preview">{s.preview}</div>
              </div>
            {/each}
          </div>
        {/if}
      </section>
    {/if}

    <section class="card">
      <header class="recs-header">
        <div>
          <h3 class="card-title">Recommendations</h3>
          <p class="card-sub">
            Actionable items generated from your real session history.
          </p>
        </div>
        <div class="severity-tabs">
          {#each severityTabs as t}
            <button
              class="seg-btn"
              class:active={severityFilter === t.id}
              onclick={() => (severityFilter = t.id)}
            >
              {t.label}
              {#if t.id !== "all"}
                <span class="count-pill">
                  {recs.filter((r) => r.severity === t.id).length}
                </span>
              {/if}
            </button>
          {/each}
        </div>
      </header>

      {#if !hasLoaded}
        <div class="empty-inline">Loading…</div>
      {:else if recs.length === 0}
        <div class="empty-inline">
          No recommendations yet — start a session to populate the analysis.
        </div>
      {:else if sortedRecs.length === 0}
        <div class="empty-inline">No items match this filter.</div>
      {:else}
        <ul class="rec-list">
          {#each sortedRecs as rec}
            <li class="rec-item" style="--rec-color: {rec.color}">
              <div class="rec-head">
                <span
                  class="severity-pill"
                  style="background: {rec.color}22; color: {rec.color}; border-color: {rec.color}55;"
                >
                  {rec.severity}
                </span>
                <h4 class="rec-title">{rec.title}</h4>
              </div>
              <p class="rec-desc">{rec.description}</p>
              {#if rec.estimated_savings}
                <p class="rec-meta">
                  <span class="meta-key">Potential savings</span>
                  <span class="meta-val accent">{rec.estimated_savings}</span>
                </p>
              {/if}
              <p class="rec-meta">
                <span class="meta-key">Action</span>
                <span class="meta-val">{rec.action}</span>
              </p>
              {#if rec.fix_prompt}
                <div class="rec-footer">
                  <button class="btn-fix" onclick={() => handleFix(rec)}>
                    <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
                    Fix with Claude Code
                  </button>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

<style>
  .reports-view {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .view-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    flex-wrap: wrap;
  }

  .view-title {
    font-size: 22px;
    font-weight: 800;
    letter-spacing: -0.01em;
  }

  .view-sub {
    font-size: 12px;
    color: var(--text-muted);
    max-width: 620px;
    margin-top: 3px;
    line-height: 1.45;
  }

  .controls {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  .segmented {
    display: inline-flex;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    padding: 2px;
    gap: 2px;
  }

  .seg-btn {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    padding: 4px 12px;
    border-radius: calc(var(--radius-md) - 2px);
    background: transparent;
    border: 1px solid transparent;
    cursor: pointer;
    transition: all 0.15s var(--ease);
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .seg-btn:hover {
    color: var(--text-primary);
  }

  .seg-btn.active {
    color: var(--accent);
    background: var(--accent-dim);
  }

  .count-pill {
    font-size: 10px;
    font-weight: 700;
    background: rgba(255, 255, 255, 0.06);
    padding: 1px 6px;
    border-radius: 99px;
  }

  .btn-primary,
  .btn-secondary {
    font-size: 12px;
    font-weight: 600;
    padding: 7px 12px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
    cursor: pointer;
    transition: all 0.15s var(--ease);
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .btn-primary {
    background: var(--accent);
    color: #1a1a1a;
    border-color: var(--accent);
  }
  .btn-primary:hover {
    background: var(--accent-hover);
    filter: brightness(1.05);
  }

  .btn-secondary {
    background: var(--bg-elevated);
    color: var(--text-secondary);
  }
  .btn-secondary:hover {
    color: var(--accent);
    border-color: var(--accent);
  }

  .card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 20px;
  }

  .card-title {
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--accent);
    margin-bottom: 6px;
  }

  .card-sub {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
    margin-bottom: 14px;
  }

  .empty-inline {
    color: var(--text-muted);
    font-size: 13px;
    padding: 18px;
    text-align: center;
  }

  .two-col {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }

  @media (max-width: 960px) {
    .two-col {
      grid-template-columns: 1fr;
    }
  }

  .hero-card {
    display: grid;
    grid-template-columns: 260px 1fr;
    gap: 24px;
    align-items: center;
    padding: 24px;
    background: linear-gradient(135deg, var(--bg-card) 0%, var(--bg-elevated) 100%);
  }

  @media (max-width: 820px) {
    .hero-card {
      grid-template-columns: 1fr;
    }
  }

  .hero-left {
    display: flex;
    align-items: center;
    gap: 20px;
  }

  .grade-letter {
    font-size: 96px;
    font-weight: 900;
    line-height: 0.95;
    letter-spacing: -0.04em;
  }

  .grade-meta .label {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    color: var(--text-muted);
  }

  .ratio {
    font-size: 24px;
    font-weight: 800;
    margin-top: 4px;
  }

  .pct {
    font-size: 16px;
    font-weight: 600;
    color: var(--text-muted);
  }

  .muted {
    color: var(--text-muted);
    font-weight: 500;
    font-size: 14px;
  }

  .hero-right {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .diagnosis {
    font-size: 13px;
    line-height: 1.55;
    color: var(--text-secondary);
  }

  .hero-stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 10px;
  }

  .hero-stat {
    background: var(--bg-card);
    border: 1px solid var(--border);
    padding: 8px 12px;
    border-radius: var(--radius-md);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .hs-label {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .hs-value {
    font-size: 15px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }

  .routing-bars {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .bar-row {
    display: grid;
    grid-template-columns: 130px 1fr 110px;
    gap: 12px;
    align-items: center;
    font-size: 12px;
  }

  .bar-label {
    display: flex;
    gap: 8px;
    align-items: center;
    font-weight: 600;
  }

  .bar-count {
    color: var(--text-muted);
    font-weight: 400;
  }

  .dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    display: inline-block;
  }

  .bar-track {
    height: 8px;
    background: var(--bg-elevated);
    border-radius: 99px;
    overflow: hidden;
  }

  .bar-fill {
    height: 100%;
    border-radius: 99px;
    transition: width 0.3s var(--ease);
  }

  .bar-value {
    text-align: right;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .savings-hint {
    margin-top: 14px;
    padding: 10px 14px;
    background: var(--accent-dim);
    border-radius: var(--radius-md);
    font-size: 12px;
    color: var(--text-secondary);
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .savings-hint strong {
    color: var(--accent);
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }

  .inflection-list {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .inflection-item {
    padding: 12px 14px;
    background: var(--bg-elevated);
    border-radius: var(--radius-md);
    border-left: 3px solid var(--text-muted);
  }

  .inflection-item.spike {
    border-left-color: var(--warning);
  }

  .inflection-item.drop {
    border-left-color: var(--success);
  }

  .inflection-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }

  .inflection-date {
    font-weight: 700;
    font-size: 13px;
  }

  .inflection-mult {
    font-size: 15px;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }

  .inflection-note {
    font-size: 12px;
    color: var(--text-secondary);
    margin-top: 2px;
    line-height: 1.5;
  }

  .inflection-stats {
    font-size: 11px;
    color: var(--text-muted);
    margin-top: 4px;
  }

  .recs-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    margin-bottom: 14px;
    flex-wrap: wrap;
  }

  .severity-tabs {
    display: flex;
    gap: 2px;
    flex-wrap: wrap;
    background: var(--bg-elevated);
    padding: 2px;
    border-radius: var(--radius-md);
    border: 1px solid var(--border);
  }

  .rec-list {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .rec-item {
    padding: 14px 16px;
    background: var(--bg-elevated);
    border-radius: var(--radius-md);
    border-left: 3px solid var(--rec-color, var(--accent));
  }

  .rec-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 6px;
  }

  .severity-pill {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 3px 8px;
    border-radius: 99px;
    border: 1px solid;
  }

  .rec-title {
    font-size: 14px;
    font-weight: 700;
    line-height: 1.3;
  }

  .rec-desc {
    font-size: 13px;
    color: var(--text-secondary);
    line-height: 1.55;
    margin-top: 4px;
  }

  .rec-meta {
    margin-top: 6px;
    font-size: 12px;
    color: var(--text-secondary);
    line-height: 1.5;
    display: flex;
    gap: 8px;
  }

  .meta-key {
    color: var(--text-muted);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 10px;
    flex-shrink: 0;
    padding-top: 2px;
  }

  .meta-val {
    flex: 1;
  }

  .meta-val.accent {
    color: var(--accent);
    font-weight: 700;
  }

  .rec-footer {
    margin-top: 10px;
  }

  .btn-fix {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    background: var(--accent-dim);
    color: var(--accent);
    border: 1px solid var(--accent);
    cursor: pointer;
    transition: all 0.15s var(--ease);
  }

  .btn-fix:hover {
    background: var(--accent);
    color: #1a1a1a;
  }

  .skeleton-stack {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .skeleton {
    background: linear-gradient(
      90deg,
      var(--bg-card) 0%,
      var(--bg-elevated) 50%,
      var(--bg-card) 100%
    );
    background-size: 200% 100%;
    border-radius: var(--radius-lg);
    animation: shimmer 1.5s infinite;
  }

  .skeleton.hero {
    height: 180px;
  }
  .skeleton.row {
    height: 120px;
  }
  .skeleton.row.short {
    height: 60px;
  }

  @keyframes shimmer {
    0% { background-position: 200% 0; }
    100% { background-position: -200% 0; }
  }

  /* cchubber analyzers — phase 4 */
  .grid-2 { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
  @media (max-width: 900px) { .grid-2 { grid-template-columns: 1fr; } }

  .health-hero { display: flex; align-items: baseline; gap: 14px; margin: 6px 0 10px; }
  .health-grade { font-size: 34px; font-weight: 800; letter-spacing: 0.02em; line-height: 1; padding: 6px 14px; border-radius: var(--radius-md); background: var(--bg-elevated); color: var(--text-primary); }
  .health-grade.grade-a { color: #77dd77; background: rgba(119,221,119,0.10); }
  .health-grade.grade-b { color: #c6e377; background: rgba(198,227,119,0.10); }
  .health-grade.grade-c { color: #e7c76a; background: rgba(231,199,106,0.12); }
  .health-grade.grade-d { color: #e89a64; background: rgba(232,154,100,0.12); }
  .health-grade.grade-f { color: #e06c6c; background: rgba(224,108,108,0.12); }
  .health-score { font-size: 28px; font-weight: 700; color: var(--text-primary); font-variant-numeric: tabular-nums; }
  .health-score-sub { font-size: 14px; font-weight: 500; color: var(--text-muted); margin-left: 2px; }

  .mini-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px 14px; margin-top: 12px; }
  .mini-grid.four { grid-template-columns: repeat(4, 1fr); }
  .mini-kv { display: flex; flex-direction: column; padding: 8px 10px; background: var(--bg-primary); border: 1px solid var(--border); border-radius: var(--radius-sm); }
  .mini-kv span { font-size: 10px; font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .mini-kv strong { font-size: 14px; font-weight: 700; color: var(--text-primary); font-variant-numeric: tabular-nums; margin-top: 2px; }

  .tool-list { display: flex; flex-direction: column; gap: 6px; margin-top: 14px; }
  .tool-row { display: grid; grid-template-columns: 120px 1fr 110px; gap: 10px; align-items: center; font-size: 12px; }
  .tool-name { color: var(--text-primary); font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tool-bar-wrap { background: var(--bg-elevated); height: 6px; border-radius: 99px; overflow: hidden; }
  .tool-bar { background: var(--accent); height: 100%; border-radius: 99px; transition: width 0.4s var(--ease); }
  .tool-count { text-align: right; color: var(--text-muted); font-variant-numeric: tabular-nums; font-size: 11px; }

  .prompt-list { display: flex; flex-direction: column; gap: 8px; margin-top: 12px; }
  .prompt-item { padding: 10px 12px; background: var(--bg-primary); border: 1px solid var(--border); border-radius: var(--radius-sm); }
  .prompt-head { display: flex; gap: 10px; align-items: center; margin-bottom: 4px; font-size: 11px; }
  .prompt-project { font-weight: 700; color: var(--text-primary); }
  .prompt-label { color: var(--accent); background: var(--accent-dim); padding: 2px 8px; border-radius: 99px; font-size: 10px; font-weight: 600; letter-spacing: 0.02em; }
  .prompt-scores { margin-left: auto; font-variant-numeric: tabular-nums; color: var(--text-muted); font-size: 11px; }
  .prompt-preview { font-size: 11px; color: var(--text-secondary); line-height: 1.4; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
</style>

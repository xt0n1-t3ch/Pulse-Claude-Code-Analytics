<script lang="ts">
  import IconCopy from "@tabler/icons-svelte/icons/copy";
  import { fmtTokens, fmtPct } from "../lib/utils";
  import {
    getContextBreakdowns,
    type ContextBreakdown,
    type ContextFileEntry,
    type SessionContextBreakdown,
  } from "../lib/api";
  import { addToast, selectedAnalyticsProviderScope } from "../lib/stores";
  import { providerMatchesAnalyticsScope } from "../lib/access";
  import { providerProfile } from "../lib/provider";
  import { sessions } from "../lib/stores";

  let ctx = $state<ContextBreakdown | null>(null);
  let breakdowns = $state<SessionContextBreakdown[]>([]);
  let selectedSessionId = $state<string | null>(null);
  let refreshing = $state(false);
  let loaded = $state(false);
  let contextError = $state<string | null>(null);
  // Keep inventories one click away without making the live context decision
  // path scroll through thousands of tokens of secondary detail.
  let showMcp = $state(false);
  let showMemory = $state(false);
  let showSkills = $state(false);

  let breakdownListRequest = 0;
  let lastBreakdownKey = "";
  let scopedLiveSessions = $derived(
    $sessions.filter(
      (session) =>
        !session.is_idle
        && providerMatchesAnalyticsScope(session.provider, $selectedAnalyticsProviderScope),
    ),
  );

  $effect(() => {
    const list = scopedLiveSessions;
    if (list.length === 0) {
      selectedSessionId = null;
      breakdownListRequest++;
      breakdowns = [];
      ctx = null;
      contextError = null;
      refreshing = false;
      loaded = true;
      return;
    }
    const current = list.find((s) => s.session_id === selectedSessionId);
    if (!current) {
      selectedSessionId = list[0].session_id;
    }
  });

  async function loadBreakdowns(activeIds: string[]): Promise<void> {
    const request = ++breakdownListRequest;
    const scope = $selectedAnalyticsProviderScope;
    const active = new Set(activeIds);
    refreshing = true;
    loaded = false;
    breakdowns = [];
    ctx = null;
    contextError = null;
    try {
      const next = await getContextBreakdowns(activeIds, scope);
      if (request === breakdownListRequest && scope === $selectedAnalyticsProviderScope) {
        const current = next.filter((entry) => active.has(entry.session_id) && !entry.is_idle);
        breakdowns = current;
        const selected = current.find((entry) => entry.session_id === selectedSessionId)
          ?? current[0]
          ?? null;
        if (selected) {
          selectedSessionId = selected.session_id;
          ctx = selected.breakdown;
        } else {
          selectedSessionId = null;
          ctx = null;
        }
        loaded = true;
      }
    } catch (error) {
      if (request === breakdownListRequest && scope === $selectedAnalyticsProviderScope) {
        breakdowns = [];
        ctx = null;
        loaded = true;
        contextError = error instanceof Error && error.message
          ? `Context data unavailable. ${error.message}`
          : "Context data unavailable. Pulse could not read the active context window.";
      }
    } finally {
      if (request === breakdownListRequest) refreshing = false;
    }
  }

  $effect(() => {
    const selected = breakdowns.find((entry) => entry.session_id === selectedSessionId);
    if (selected) {
      ctx = selected.breakdown;
      contextError = null;
      loaded = true;
    }
  });

  $effect(() => {
    const activeSessions = scopedLiveSessions
      .sort((a, b) => a.session_id.localeCompare(b.session_id));
    const activeIds = activeSessions.map((session) => session.session_id);
    const key = `${$selectedAnalyticsProviderScope}:${activeSessions.map((session) => [
      session.session_id,
      session.context_used_tokens ?? 0,
      session.context_window_tokens ?? 0,
      session.tokens,
    ].join(":")).join(",")}`;
    if (key === lastBreakdownKey) return;
    lastBreakdownKey = key;
    if (activeIds.length === 0) {
      breakdownListRequest++;
      breakdowns = [];
      ctx = null;
      contextError = null;
      refreshing = false;
      loaded = true;
      return;
    }
    void loadBreakdowns(activeIds);
  });

  function clampPct(pct: number): number {
    if (!Number.isFinite(pct)) return 0;
    return Math.max(0, Math.min(pct, 100));
  }

  function percent(part: number, total: number): number {
    return total > 0 ? clampPct((part / total) * 100) : 0;
  }

  function retryContext(): void {
    const activeIds = scopedLiveSessions.map((session) => session.session_id);
    if (activeIds.length > 0) void loadBreakdowns(activeIds);
  }

  // Three-tier semantic for context-window utilization: green = healthy
  // headroom (positive), yellow = filling up (neutral/caution), red = near the
  // window limit / autocompact (negative). Bars and the hero badge share this
  // exact scale so a session never reads two different colors.
  function utilizationColor(pct: number): string {
    if (pct >= 85) return "var(--danger)";
    if (pct >= 50) return "var(--warning)";
    return "var(--success)";
  }

  type CtxSeverity = "critical" | "warning" | "info" | "positive";

  interface CtxAdvice {
    id: string;
    severity: CtxSeverity;
    title: string;
    description: string;
    fix_prompt: string;
  }

  function heaviest(items: ContextFileEntry[], n: number): ContextFileEntry[] {
    return [...items].sort((a, b) => b.tokens - a.tokens).slice(0, n);
  }

  function describeList(items: ContextFileEntry[]): string {
    return items.map((i) => `${i.name} (${i.tokens} tokens)`).join(", ");
  }

  function severityColor(sev: CtxSeverity): string {
    switch (sev) {
      case "critical": return "var(--danger)";
      case "warning":  return "var(--warning)";
      case "info":     return "var(--info)";
      case "positive": return "var(--success)";
    }
  }

  let advice = $derived.by<CtxAdvice[]>(() => {
    if (!ctx) return [];
    const out: CtxAdvice[] = [];
    const profile = $providerProfile;
    const product = profile.productName;
    const usedPctValue = percent(ctx.used_tokens, ctx.context_window);
    const freePctValue = percent(ctx.free_space, ctx.context_window);

    if (usedPctValue >= 85) {
      out.push({
        id: "context-near-full",
        severity: "critical",
        title: `Context is ${usedPctValue.toFixed(0)}% full`,
        description:
          `You're ${fmtTokens(ctx.used_tokens)} of ${fmtTokens(ctx.context_window)} tokens in — ` +
          `${product} will auto-compact soon, which loses detail. Clearing or compacting now keeps you in control.`,
        fix_prompt:
          `My ${product} session is ${usedPctValue.toFixed(0)}% full ` +
          `(${ctx.used_tokens} of ${ctx.context_window} tokens). Summarize what we've accomplished, what's left, ` +
          `and any key decisions or file paths I'll need — then I'll clear the session and paste your summary back in to keep context.`,
      });
    } else if (usedPctValue >= 70) {
      out.push({
        id: "context-approaching",
        severity: "warning",
        title: `Context is ${usedPctValue.toFixed(0)}% full`,
        description:
          "Still workable but starting to shrink. If you're about to tackle something big, compacting now gives you more headroom.",
        fix_prompt:
          `My ${product} context is ${usedPctValue.toFixed(0)}% used and I want to keep working without losing detail. ` +
          "Give me a concise summary of the current state of this task (decisions, open threads, relevant files) so I can compact safely.",
      });
    }

    if (ctx.memory_total > 10_000) {
      const heavy = heaviest(ctx.memory_files, 3);
      out.push({
        id: "memory-heavy",
        severity: "info",
        title: `Instruction inventory is about ${fmtTokens(ctx.memory_total)}`,
        description:
          `This is an on-disk estimate, not observed context usage. Heaviest files: ${describeList(heavy)}.`,
        fix_prompt:
          `My instruction files (${ctx.memory_files.map((f) => f.name).join(", ")}) have an estimated on-disk size of ` +
          `${ctx.memory_total} tokens. Read them and identify duplicated or generic sections without assuming every file was loaded in this session.`,
      });
    }

    if (ctx.system_prompt + ctx.system_tools > 20_000) {
      out.push({
        id: "system-heavy",
        severity: "info",
        title: `System prompt + tools: ${fmtTokens(ctx.system_prompt + ctx.system_tools)} tokens`,
        description:
          `This is ${product}'s baseline cost — you can't trim it directly, but it's the floor under every session. ` +
          "Worth knowing when budgeting context.",
        fix_prompt: "",
      });
    }

    if (freePctValue >= 50) {
      out.push({
        id: "context-healthy",
        severity: "positive",
        title: "Context is in great shape",
        description:
          `${fmtPct(freePctValue)} free space based on observed session telemetry.`,
        fix_prompt: "",
      });
    }

    return out;
  });

  async function handleFix(item: CtxAdvice): Promise<void> {
    if (!item.fix_prompt) {
      addToast("No prompt for this item.", "info", 2000);
      return;
    }
    try {
      await navigator.clipboard.writeText(item.fix_prompt);
      addToast(`Fix prompt copied — paste into ${$providerProfile.productName}.`, "success", 3000);
    } catch (err) {
      addToast(`Copy failed: ${String(err)}`, "danger", 3500);
    }
  }

  let usedPct = $derived(ctx ? percent(ctx.used_tokens, ctx.context_window) : 0);
  let freePct = $derived(ctx ? percent(ctx.free_space, ctx.context_window) : 0);
  let autocompactPct = $derived(ctx ? percent(ctx.autocompact_buffer, ctx.context_window) : 0);

  interface CatItem { label: string; tokens: number; pct: number; icon: string; color: string }

  let categories = $derived<CatItem[]>(ctx ? [
      { label: "Observed session usage", tokens: ctx.used_tokens, pct: usedPct, icon: "filled", color: "var(--info)" },
    { label: "Free space", tokens: ctx.free_space, pct: freePct, icon: "hollow", color: "var(--text-muted)" },
    { label: "Autocompact buffer", tokens: ctx.autocompact_buffer, pct: autocompactPct, icon: "cross", color: "var(--text-muted)" },
  ].filter((c) => c.tokens > 0 || c.icon !== "filled") : []);

  let barSegs = $derived<{ pct: number; color: string }[]>(ctx ? [
      { pct: usedPct, color: utilizationColor(usedPct) },
  ] : []);

  let usedBarPct = $derived(barSegs.reduce((s, b) => s + b.pct, 0));
  let selectedEntry = $derived(
    breakdowns.find((entry) => entry.session_id === selectedSessionId) ?? breakdowns[0] ?? null,
  );
</script>

<div class="ctx-page app-view">
  <div class="view-header">
    <div class="view-title-line">
      <h2 class="view-title">Context</h2>
      {#if breakdowns.length > 0}<span class="active-count">{breakdowns.length} active</span>{/if}
    </div>
    <div class="header-meta">
      {#if refreshing}<span class="refreshing-dot" aria-label="Refreshing"></span>{/if}
      {#if ctx}<span class="model-chip">{ctx.model}</span>{/if}
    </div>
  </div>

  {#if contextError}
    <section class="context-state state-panel error" role="alert">
      <span class="state-eyebrow">Context</span>
      <h3>Context data unavailable</h3>
      <p>{contextError.replace(/^Context data unavailable\.\s*/, "")}</p>
      <button type="button" class="btn" onclick={retryContext}>Retry</button>
    </section>
  {:else if ctx}
    {#if breakdowns.length > 0}
      <div class="active-section">
        <div class="advice-title-row">
          <h3 class="advice-title">Active windows</h3>
          <span class="advice-sub">Select a session to inspect how full its context window is.</span>
        </div>
        <div class="active-grid">
          {#each breakdowns as entry (entry.session_id)}
            {@const cardPct = clampPct(
              entry.breakdown.context_window > 0
                ? (entry.breakdown.used_tokens / entry.breakdown.context_window) * 100
                : 0,
            )}
            <button
              class="active-ctx-card"
              class:selected={entry.session_id === selectedSessionId}
              class:idle={entry.is_idle}
              aria-pressed={entry.session_id === selectedSessionId}
              onclick={() => (selectedSessionId = entry.session_id)}
            >
              <div class="act-head">
                <span class="act-project">{entry.project}</span>
                <span class="act-pct" style="color: {utilizationColor(cardPct)}">{fmtPct(cardPct)}</span>
              </div>
              <div class="act-track">
                <div
                  class="act-fill"
                  style="width: {cardPct}%; background: {utilizationColor(cardPct)}"
                ></div>
              </div>
              <div class="act-meta">
                <span class="act-model">{entry.model_id || entry.breakdown.model}</span>
                <span class="act-tokens">
                  {fmtTokens(entry.breakdown.used_tokens)} / {fmtTokens(entry.breakdown.context_window)}
                </span>
              </div>
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <div class="hero-card surface-matte">
      <div class="hero-owner">
        <div>
          <span>Selected session</span>
          <strong>{selectedEntry?.project ?? "Active session"}</strong>
        </div>
        <span>{ctx.model}</span>
      </div>
      <div class="hero-top">
        <div class="hero-numbers">
          <span class="hero-used">{fmtTokens(ctx.used_tokens)}</span>
          <span class="hero-sep">/ {fmtTokens(ctx.context_window)}</span>
          <span class="hero-unit">tokens used</span>
        </div>
        <div class="hero-pct-badge" class:warn={usedPct >= 50} class:crit={usedPct >= 85}>
          {fmtPct(usedPct)}
        </div>
      </div>

      <div class="progress-track">
        <div class="progress-fill" style="width:{Math.min(usedBarPct, 100)}%">
          {#each barSegs as seg}
            {#if seg.pct > 0.2}
              <div class="progress-seg" style="flex:{seg.pct}; background:{seg.color}"></div>
            {/if}
          {/each}
        </div>
        <div class="progress-autocompact" style="width:{autocompactPct}%"></div>
      </div>

      <div class="hero-sub">
        <span class="hero-free">{fmtTokens(ctx.free_space)} free</span>
        <span class="hero-sep-sm">·</span>
        {#if ctx.autocompact_buffer > 0}
          <span class="hero-buffer">{fmtTokens(ctx.autocompact_buffer)} autocompact buffer</span>
        {/if}
      </div>

      <div class="cat-grid">
        {#each categories as cat}
          <div class="cat-row" class:dim={cat.icon !== "filled"}>
            <span class="cat-icon" class:hollow={cat.icon === "hollow"} class:cross={cat.icon === "cross"} style={cat.icon === "filled" ? `background:${cat.color}` : ""}></span>
            <span class="cat-label">{cat.label}</span>
            <span class="cat-val">{fmtTokens(cat.tokens)}</span>
            <span class="cat-pct">{fmtPct(cat.pct)}</span>
          </div>
        {/each}
      </div>
    </div>

    {#if advice.length > 0}
      <div class="advice-card">
        <div class="advice-header">
          <div class="advice-title-row">
            <h3 class="advice-title">Recommendations</h3>
            <span class="advice-count">{advice.length}</span>
          </div>
          <p class="advice-sub">
            Derived from your real context breakdown — each ships with a ready-to-paste prompt for {$providerProfile.productName}.
          </p>
        </div>
        <ul class="advice-list">
          {#each advice as item}
            <li class="advice-item" style="--advice-color: {severityColor(item.severity)}">
              <div class="advice-head">
                <span
                  class="advice-pill"
                  style="background: {severityColor(item.severity)}22; color: {severityColor(item.severity)}; border-color: {severityColor(item.severity)}55;"
                >
                  {item.severity}
                </span>
                <h4 class="advice-item-title">{item.title}</h4>
              </div>
              <p class="advice-desc">{item.description}</p>
              {#if item.fix_prompt}
                <button class="advice-btn" onclick={() => handleFix(item)}>
                  <IconCopy size={13} stroke={2.2} aria-hidden="true" />
                  Fix with {$providerProfile.productName}
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    <div class="sub-grid">
      {#if ctx.mcp_tools.length > 0}
        <div class="sub-card">
          <button class="sub-header" onclick={() => showMcp = !showMcp}>
            <span class="sub-title">Configured MCP inventory</span>
            <span class="sub-count">{ctx.mcp_tools.length}</span>
            <span class="sub-tokens">~{fmtTokens(ctx.mcp_total)}</span>
            <span class="chevron" class:open={showMcp}></span>
          </button>
          {#if showMcp}
            <div class="sub-list">
              {#each ctx.mcp_tools as item}
                <div class="sub-item">
                  <span class="item-name">{item.name}</span>
                  <span class="item-tokens">{item.tokens} tokens</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      {#if ctx.memory_files.length > 0}
        <div class="sub-card">
          <button class="sub-header" onclick={() => showMemory = !showMemory}>
            <span class="sub-title">Instruction inventory</span>
            <span class="sub-count">{ctx.memory_files.length}</span>
            <span class="sub-tokens">~{fmtTokens(ctx.memory_total)}</span>
            <span class="chevron" class:open={showMemory}></span>
          </button>
          {#if showMemory}
            <div class="sub-list">
              {#each ctx.memory_files as item}
                <div class="sub-item">
                  <span class="item-name">{item.name}</span>
                  <span class="item-tokens">{item.tokens} tokens</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}

      {#if ctx.skills.length > 0}
        <div class="sub-card">
          <button class="sub-header" onclick={() => showSkills = !showSkills}>
            <span class="sub-title">Installed skill inventory</span>
            <span class="sub-count">{ctx.skills.length}</span>
            <span class="sub-tokens">~{fmtTokens(ctx.skills_total)}</span>
            <span class="chevron" class:open={showSkills}></span>
          </button>
          {#if showSkills}
            <div class="sub-list">
              {#each ctx.skills as item}
                <div class="sub-item">
                  <span class="item-name">{item.name}</span>
                  <span class="item-tokens">{item.tokens} tokens</span>
                </div>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {:else if !loaded}
    <section class="context-state state-panel" aria-live="polite">
      <span class="state-eyebrow">Context</span>
      <h3>Reading the active context window</h3>
      <p>Pulse is resolving session usage, instruction inventory, and compaction headroom.</p>
      <div class="state-progress" aria-hidden="true"><span></span></div>
    </section>
  {:else}
    <section class="context-state state-panel">
      <span class="state-eyebrow">Context</span>
      <h3>No active context to inspect</h3>
      <p>Start a session and Pulse will place its live window, inventory, and pressure signals here.</p>
    </section>
  {/if}
</div>

<style>
  .ctx-page { display: flex; flex-direction: column; gap: var(--page-gap); }

  .view-header { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; }
  .view-title-line { display: flex; align-items: center; gap: 10px; }
  .active-count { padding: 3px 8px; color: var(--text-muted); border: 1px solid var(--border); border-radius: var(--radius-full); font: 600 10px var(--font-mono); }
  .header-meta { display: flex; align-items: center; gap: 10px; }
  .refreshing-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent);
    animation: ctx-refresh-pulse 1s ease-in-out infinite;
  }
  @keyframes ctx-refresh-pulse {
    0%, 100% { opacity: 0.25; }
    50% { opacity: 0.85; }
  }
  .model-chip {
    font-size: 11px;
    color: var(--text-secondary);
    font-family: var(--font-mono);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    padding: 4px 10px;
    border-radius: 99px;
    letter-spacing: 0.01em;
  }

  /* All-active context cards */
  .active-section { display: flex; flex-direction: column; gap: 12px; }
  .active-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 10px;
  }
  .active-ctx-card {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 14px 16px;
    background: var(--surface-panel);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    cursor: pointer;
    text-align: left;
    font: inherit;
    overflow: hidden;
    transition: border-color 0.15s var(--ease), background 0.15s var(--ease),
      transform 0.15s var(--ease), box-shadow 0.15s var(--ease);
  }
  .active-ctx-card:hover { border-color: var(--border-hover); transform: var(--lift); }
  .active-ctx-card.selected { border-color: var(--provider-accent); background: var(--surface-panel); box-shadow: inset 0 -2px 0 var(--provider-accent); }
  .active-ctx-card.idle { opacity: 0.6; }
  .act-head { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; }
  .act-project { font-size: 13px; font-weight: 700; color: var(--text-primary); }
  .act-pct { font-size: 13px; font-weight: 700; font-variant-numeric: tabular-nums; }
  .act-track {
    height: 6px;
    background: var(--meter-track);
    border-radius: 99px;
    overflow: hidden;
  }
  .act-fill { height: 100%; border-radius: 99px; transition: width 0.4s var(--ease); }
  .act-meta {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 11px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .act-model {
    font-family: var(--font-mono);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .act-tokens { flex-shrink: 0; }

  /* Selected session detail stays matte; hierarchy comes from type and data. */
  .hero-card {
    position: relative;
    padding: 22px 24px;
    overflow: hidden;
  }
  .hero-owner { display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; margin-bottom: 18px; padding-bottom: 14px; border-bottom: 1px solid var(--divider); }
  .hero-owner > div { display: flex; flex-direction: column; gap: 3px; }
  .hero-owner span { color: var(--text-muted); font: 600 10px var(--font-mono); }
  .hero-owner strong { color: var(--text-primary); font-size: 14px; }
  .context-state { min-height: 196px; display: flex; flex-direction: column; justify-content: center; align-items: flex-start; padding: 28px 30px; }
  .context-state h3 { margin-top: 8px; font-size: 18px; }
  .context-state p { max-width: 540px; margin-top: 6px; color: var(--text-muted); font-size: 12px; line-height: 1.55; }
  .context-state.error { border-color: color-mix(in srgb, var(--danger) 34%, var(--border)); }
  .context-state.error h3 { color: var(--danger); }
  .context-state .btn { margin-top: 18px; }
  .state-eyebrow { color: var(--text-secondary); font-size: 10px; font-weight: 700; letter-spacing: 0.1em; text-transform: uppercase; }
  .state-progress { width: min(360px, 100%); height: 3px; margin-top: 22px; overflow: hidden; background: var(--bg-elevated); }
  .state-progress span { display: block; width: 36%; height: 100%; background: var(--info); animation: context-load 1.35s var(--ease-in-out) infinite alternate; }
  @keyframes context-load { from { transform: translateX(-20%); } to { transform: translateX(190%); } }

  .hero-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    margin-bottom: 14px;
  }
  .hero-numbers {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
    font-variant-numeric: tabular-nums;
  }
  .hero-used {
    font-size: 28px;
    font-weight: 800;
    letter-spacing: -0.025em;
    color: var(--text-primary);
  }
  .hero-sep { color: var(--text-muted); font-size: 16px; font-weight: 500; }
  .hero-unit { color: var(--text-muted); font-size: 12px; margin-left: 2px; }
  .hero-pct-badge {
    font-size: 14px;
    font-weight: 700;
    letter-spacing: 0.01em;
    color: var(--success);
    background: var(--success-dim);
    padding: 6px 12px;
    border-radius: 99px;
    font-variant-numeric: tabular-nums;
  }
  .hero-pct-badge.warn { color: var(--warning); background: var(--warning-dim); }
  .hero-pct-badge.crit { color: var(--danger); background: var(--danger-dim); }

  .hero-sub {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 10px 0 18px;
    font-size: 11.5px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .hero-sep-sm { color: var(--border-strong); }
  .hero-free { color: var(--text-secondary); }

  /* Progress bar */
  .progress-track {
    position: relative;
    height: 18px;
    background: var(--meter-track);
    border-radius: 99px;
    overflow: hidden;
    margin-bottom: 10px;
    border: 1px solid var(--border);
  }
  .progress-fill { height: 100%; display: flex; border-radius: 99px; overflow: hidden; transition: width 0.6s var(--ease); }
  .progress-seg { height: 100%; transition: flex 0.4s var(--ease); }
  .progress-autocompact {
    position: absolute;
    right: 0;
    top: 0;
    height: 100%;
    background: repeating-linear-gradient(
      -45deg,
      transparent,
      transparent 3px,
      var(--border) 3px,
      var(--border) 6px
    );
    border-left: 1px solid var(--border-strong);
  }

  /* Category list */
  .cat-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 2px 16px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }
  .cat-row {
    display: grid;
    grid-template-columns: auto 1fr auto auto;
    align-items: center;
    gap: 10px;
    font-size: 12.5px;
    color: var(--text-secondary);
    padding: 6px 8px;
    border-radius: var(--radius-sm);
    transition: background 0.12s ease;
  }
  .cat-row:hover { background: var(--bg-elevated); }
  .cat-row.dim { color: var(--text-muted); }

  .cat-icon { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
  .cat-icon.hollow { background: none !important; border: 1.5px solid var(--text-muted); }
  .cat-icon.cross { background: none !important; position: relative; width: 10px; height: 10px; }
  .cat-icon.cross::before, .cat-icon.cross::after {
    content: ""; position: absolute; background: var(--text-muted); border-radius: 1px;
  }
  .cat-icon.cross::before { width: 10px; height: 1.5px; top: 4px; left: 0; transform: rotate(45deg); }
  .cat-icon.cross::after  { width: 10px; height: 1.5px; top: 4px; left: 0; transform: rotate(-45deg); }

  .cat-label { font-weight: 500; }
  .cat-val {
    font-size: 12.5px;
    font-weight: 600;
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
  }
  .cat-row.dim .cat-val { color: var(--text-secondary); font-weight: 500; }
  .cat-pct { color: var(--text-muted); font-size: 11px; font-variant-numeric: tabular-nums; min-width: 36px; text-align: right; }

  /* Sub-cards */
  .sub-grid { display: flex; flex-direction: column; gap: 8px; }
  .sub-card { background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden; }

  .sub-header {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 14px 18px;
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
    cursor: pointer;
    transition: background 0.15s ease;
    text-align: left;
  }
  .sub-header:hover { background: var(--bg-card-hover); }
  .sub-title { flex: 1; }
  .sub-count { font-size: 10px; color: var(--text-muted); background: var(--bg-elevated); padding: 2px 7px; border-radius: 99px; font-weight: 700; }
  .sub-tokens { font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .chevron { width: 0; height: 0; border-left: 4px solid transparent; border-right: 4px solid transparent; border-top: 5px solid var(--text-muted); transition: transform 0.2s ease; }
  .chevron.open { transform: rotate(180deg); }

  .sub-list { padding: 0 18px 14px; display: flex; flex-direction: column; gap: 1px; }
  .sub-item { display: flex; justify-content: space-between; align-items: center; padding: 5px 10px; border-radius: var(--radius-sm); transition: background 0.1s ease; }
  .sub-item:hover { background: var(--bg-elevated); }
  .item-name {
    min-width: 0;
    overflow: hidden;
    font-family: var(--font-mono);
    font-size: 11px;
    color: var(--text-secondary);
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-tokens {
    flex-shrink: 0;
    margin-left: 12px;
    font-size: 11px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }

  /* Spinner */
  @keyframes spin { to { transform: rotate(360deg); } }

  /* Advice / recommendations */
  .advice-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 20px;
  }
  .advice-header {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 14px;
  }
  .advice-title-row { display: flex; align-items: center; gap: 10px; }
  .advice-title {
    font-size: 14px;
    font-weight: 700;
    letter-spacing: -0.01em;
    color: var(--text-primary);
  }
  .advice-sub {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
    max-width: 640px;
  }
  .advice-count {
    font-size: 10px;
    font-weight: 700;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    min-width: 20px;
    height: 20px;
    padding: 0 6px;
    border-radius: 99px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-variant-numeric: tabular-nums;
  }
  .advice-list {
    list-style: none;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .advice-item {
    padding: 14px 16px;
    background: var(--bg-elevated);
    border-radius: var(--radius-md);
    border-left: 3px solid var(--advice-color, var(--accent));
  }
  .advice-head {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 6px;
  }
  .advice-pill {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    padding: 3px 8px;
    border-radius: 99px;
    border: 1px solid;
  }
  .advice-item-title {
    font-size: 14px;
    font-weight: 700;
    line-height: 1.3;
  }
  .advice-desc {
    font-size: 13px;
    color: var(--text-secondary);
    line-height: 1.55;
    margin-top: 4px;
  }
  .advice-btn {
    margin-top: 10px;
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
  .advice-btn:hover {
    background: var(--accent);
    color: var(--accent-fg);
  }

  @media (max-width: 800px) {
    .view-header { flex-direction: column; }
    .header-meta { width: 100%; justify-content: space-between; }
    .model-chip { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .active-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .cat-grid { grid-template-columns: 1fr; }
    .hero-card { padding: 16px; }
  }

  @media (max-width: 620px) {
    .active-grid { grid-template-columns: 1fr; }
    .hero-top { align-items: flex-start; flex-wrap: wrap; }
    .hero-used { font-size: 23px; }
    .advice-head { align-items: flex-start; flex-wrap: wrap; }
  }
</style>

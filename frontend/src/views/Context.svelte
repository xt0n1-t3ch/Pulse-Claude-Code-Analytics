<script lang="ts">
  import { onMount } from "svelte";
  import { fmtTokens, fmtPct } from "../lib/utils";
  import {
    getContextBreakdown,
    getContextBreakdowns,
    getSessionsContextUsage,
    type ContextBreakdown,
    type ContextFileEntry,
    type SessionContextBreakdown,
    type SessionContextUsage,
  } from "../lib/api";
  import { addToast } from "../lib/stores";
  import { providerProfile } from "../lib/provider";
  import { sessions } from "../lib/stores";

  let ctx = $state<ContextBreakdown | null>(null);
  let breakdowns = $state<SessionContextBreakdown[]>([]);
  let sessionUsage = $state<SessionContextUsage[]>([]);
  let selectedSessionId = $state<string | null>(null);
  let refreshing = $state(false);
  let loaded = $state(false);
  let showMcp = $state(true);
  let showMemory = $state(true);
  let showSkills = $state(true);

  let breakdownRequest = 0;

  $effect(() => {
    const list = $sessions;
    if (list.length === 0) {
      selectedSessionId = null;
      return;
    }
    const current = list.find((s) => s.session_id === selectedSessionId);
    if (!current || current.is_idle) {
      const active = list.find((s) => !s.is_idle) ?? list[0];
      selectedSessionId = active.session_id;
    }
  });

  async function loadBreakdown(): Promise<void> {
    const request = ++breakdownRequest;
    refreshing = true;
    try {
      const next = await getContextBreakdown(selectedSessionId ?? undefined);
      if (request === breakdownRequest) {
        ctx = next;
        loaded = true;
      }
    } finally {
      if (request === breakdownRequest) refreshing = false;
    }
  }

  async function loadUsage(): Promise<void> {
    sessionUsage = await getSessionsContextUsage();
  }

  async function loadBreakdowns(): Promise<void> {
    breakdowns = await getContextBreakdowns();
  }

  $effect(() => {
    void selectedSessionId;
    loadBreakdown();
  });

  onMount(() => {
    loadBreakdowns();
    loadUsage();
    const iv = setInterval(() => {
      loadBreakdown();
      loadBreakdowns();
      loadUsage();
    }, 10000);
    return () => clearInterval(iv);
  });

  function clampPct(pct: number): number {
    if (!Number.isFinite(pct)) return 0;
    return Math.max(0, Math.min(pct, 100));
  }

  function utilizationColor(pct: number): string {
    if (pct >= 95) return "var(--danger)";
    if (pct >= 80) return "var(--warning)";
    if (pct >= 50) return "var(--info)";
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
      case "critical": return "#e5484d";
      case "warning":  return "#f5a524";
      case "info":     return "var(--info)";
      case "positive": return "#62b462";
    }
  }

  let advice = $derived.by<CtxAdvice[]>(() => {
    if (!ctx) return [];
    const out: CtxAdvice[] = [];
    const profile = $providerProfile;
    const product = profile.productName;
    const memoryFile = profile.instructionFile;
    const home = profile.homeDir;
    const usedPctValue = (ctx.used_tokens / ctx.context_window) * 100;
    const freePctValue = (ctx.free_space / ctx.context_window) * 100;

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
        severity: "warning",
        title: `Memory files use ${fmtTokens(ctx.memory_total)} tokens`,
        description:
          `${memoryFile} + rules get re-read every turn. Heaviest: ${describeList(heavy)}. ` +
          "Trimming them pays back on every single message.",
        fix_prompt:
          `My memory files (${ctx.memory_files.map((f) => f.name).join(", ")}) currently total ` +
          `${ctx.memory_total} tokens. Read them, identify which sections are generic boilerplate, ` +
          "duplicated, or rarely-triggered, and suggest concrete edits that cut token count without losing the rules I actually rely on.",
      });
    }

    if (ctx.skills_total > 30_000) {
      const heavy = heaviest(ctx.skills, 5);
      out.push({
        id: "skills-bloat",
        severity: "warning",
        title: `${ctx.skills.length} skills loaded, ${fmtTokens(ctx.skills_total)} tokens`,
        description:
          `Top-5 by size: ${describeList(heavy)}. Skills sit in context whether you use them or not — ` +
          "disabling unused ones buys headroom on every turn.",
        fix_prompt:
          `I have ${ctx.skills.length} skills loaded consuming ${ctx.skills_total} tokens total. ` +
          `The heaviest are: ${describeList(heavy)}. Help me audit which of these I actually need for my current work ` +
          `and which I can disable — check ${home}/skills/<name>/SKILL.md descriptions to decide.`,
      });
    } else if (ctx.skills_total > 15_000) {
      out.push({
        id: "skills-warm",
        severity: "info",
        title: `${fmtTokens(ctx.skills_total)} tokens of skills loaded`,
        description:
          "Not critical, but skills eat context. If any aren't relevant to today's work, temporarily disabling them frees tokens.",
        fix_prompt:
          `I have ${ctx.skills.length} skills loaded (${ctx.skills_total} tokens). ` +
          "List them with one-line summaries of what each is for — then I can decide which to keep for this session.",
      });
    }

    if (ctx.mcp_total > 15_000) {
      const heavy = heaviest(ctx.mcp_tools, 5);
      out.push({
        id: "mcp-heavy",
        severity: "warning",
        title: `MCP tools use ${fmtTokens(ctx.mcp_total)} tokens`,
        description:
          `Top: ${describeList(heavy)}. Each MCP server adds tool definitions to context. ` +
          "Unused servers are silent token drag.",
        fix_prompt:
          `My MCP servers (${ctx.mcp_tools.map((t) => t.name).join(", ")}) consume ${ctx.mcp_total} tokens. ` +
          `Read ${home}/settings.json (or the equivalent MCP config) and tell me which servers I can disable for typical coding work — ` +
          "keep essentials (like context7 for docs) and flag the ones that are rarely useful.",
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

    if (freePctValue >= 50 && ctx.memory_total < 5_000 && ctx.skills_total < 15_000) {
      out.push({
        id: "context-healthy",
        severity: "positive",
        title: "Context is in great shape",
        description:
          `${fmtPct(freePctValue)} free space, memory + skills under budget. Nothing to do — keep it this lean.`,
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

  let usedPct = $derived(ctx ? (ctx.used_tokens / ctx.context_window) * 100 : 0);
  let freePct = $derived(ctx ? (ctx.free_space / ctx.context_window) * 100 : 0);
  let autocompactPct = $derived(ctx ? (ctx.autocompact_buffer / ctx.context_window) * 100 : 0);

  interface CatItem { label: string; tokens: number; pct: number; icon: string; color: string }

  let categories = $derived<CatItem[]>(ctx ? [
    { label: "System prompt", tokens: ctx.system_prompt, pct: (ctx.system_prompt / ctx.context_window) * 100, icon: "filled", color: "var(--info)" },
    { label: "System tools", tokens: ctx.system_tools, pct: (ctx.system_tools / ctx.context_window) * 100, icon: "filled", color: "var(--chart-3)" },
    { label: "Memory files", tokens: ctx.memory_total, pct: (ctx.memory_total / ctx.context_window) * 100, icon: "filled", color: "var(--warning)" },
    { label: "Skills", tokens: ctx.skills_total, pct: (ctx.skills_total / ctx.context_window) * 100, icon: "filled", color: "var(--success)" },
    { label: "Messages", tokens: ctx.messages, pct: (ctx.messages / ctx.context_window) * 100, icon: "filled", color: "#7cb9e8" },
    { label: "Free space", tokens: ctx.free_space, pct: freePct, icon: "hollow", color: "var(--text-muted)" },
    { label: "Autocompact buffer", tokens: ctx.autocompact_buffer, pct: autocompactPct, icon: "cross", color: "var(--text-muted)" },
  ].filter((c) => c.tokens > 0 || c.icon !== "filled") : []);

  let barSegs = $derived<{ pct: number; color: string }[]>(ctx ? [
    { pct: (ctx.system_prompt / ctx.context_window) * 100, color: "var(--info)" },
    { pct: (ctx.system_tools / ctx.context_window) * 100, color: "var(--chart-3)" },
    { pct: (ctx.memory_total / ctx.context_window) * 100, color: "var(--warning)" },
    { pct: (ctx.skills_total / ctx.context_window) * 100, color: "var(--success)" },
    { pct: (ctx.messages / ctx.context_window) * 100, color: "#7cb9e8" },
  ] : []);

  let usedBarPct = $derived(barSegs.reduce((s, b) => s + b.pct, 0));
</script>

<div class="ctx-page">
  {#if ctx}
    <div class="view-header">
      <h2 class="view-title">Context Window</h2>
      <div class="header-meta">
        {#if refreshing}<span class="refreshing-dot" aria-label="Refreshing"></span>{/if}
        <span class="model-chip">{ctx.model}</span>
      </div>
    </div>

    {#if $sessions.length > 0}
      <div class="session-strip" role="tablist">
        {#each $sessions as s (s.session_id)}
          <button
            class="session-pill"
            class:active={s.session_id === selectedSessionId}
            class:idle={s.is_idle}
            role="tab"
            aria-selected={s.session_id === selectedSessionId}
            onclick={() => (selectedSessionId = s.session_id)}
          >
            <span class="pill-project">{s.project}</span>
            <span class="pill-model">{s.model}</span>
          </button>
        {/each}
      </div>
    {/if}

    {#if breakdowns.length > 0}
      <div class="active-section">
        <div class="advice-title-row">
          <h3 class="advice-title">Active context windows</h3>
          <span class="advice-count">{breakdowns.length}</span>
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

    <div class="hero-card">
      <div class="hero-top">
        <div class="hero-numbers">
          <span class="hero-used">{fmtTokens(ctx.used_tokens)}</span>
          <span class="hero-sep">/ {fmtTokens(ctx.context_window)}</span>
          <span class="hero-unit">tokens used</span>
        </div>
        <div class="hero-pct-badge" class:warn={usedPct >= 70} class:crit={usedPct >= 85}>
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
        <span class="hero-buffer">{fmtTokens(ctx.autocompact_buffer)} autocompact buffer</span>
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
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 01-2-2V4a2 2 0 012-2h9a2 2 0 012 2v1"/></svg>
                  Fix with {$providerProfile.productName}
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    {#if sessionUsage.length > 0}
      <div class="usage-card">
        <div class="advice-header">
          <div class="advice-title-row">
            <h3 class="advice-title">Per-session utilization</h3>
            <span class="advice-count">{sessionUsage.length}</span>
          </div>
          <p class="advice-sub">
            Context fill across recent sessions — each with a tailored recommendation.
          </p>
        </div>
        <ul class="usage-list">
          {#each sessionUsage as row (row.session_id)}
            {@const rowPct = clampPct(row.utilization_pct)}
            <li class="usage-row">
              <div class="usage-head">
                <span class="usage-project">{row.project}</span>
                <span class="usage-model">{row.model_display}</span>
                <span class="usage-pct" style="color: {utilizationColor(rowPct)}">
                  {fmtPct(rowPct)}
                </span>
              </div>
              <div class="usage-track">
                <div
                  class="usage-fill"
                  style="width: {rowPct}%; background: {utilizationColor(rowPct)}"
                ></div>
              </div>
              <div class="usage-meta">
                <span>{fmtTokens(row.used_tokens)} / {fmtTokens(row.window_tokens)}</span>
                <span class="usage-rec">{row.recommendation}</span>
              </div>
            </li>
          {/each}
        </ul>
      </div>
    {/if}

    <div class="sub-grid">
      {#if ctx.mcp_tools.length > 0}
        <div class="sub-card">
          <button class="sub-header" onclick={() => showMcp = !showMcp}>
            <span class="sub-title">MCP tools</span>
            <span class="sub-count">{ctx.mcp_tools.length}</span>
            <span class="sub-tokens">{fmtTokens(ctx.mcp_total)}</span>
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
            <span class="sub-title">Memory files</span>
            <span class="sub-count">{ctx.memory_files.length}</span>
            <span class="sub-tokens">{fmtTokens(ctx.memory_total)}</span>
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
            <span class="sub-title">Skills</span>
            <span class="sub-count">{ctx.skills.length}</span>
            <span class="sub-tokens">{fmtTokens(ctx.skills_total)}</span>
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
    <div class="hero-card loading">
      <div class="spinner"></div>
      <span>Loading context data...</span>
    </div>
  {:else}
    <div class="hero-card loading">
      <span>No active sessions to inspect.</span>
    </div>
  {/if}
</div>

<style>
  .ctx-page { display: flex; flex-direction: column; gap: 14px; }

  .view-header { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
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
    font-family: 'JetBrains Mono', monospace;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    padding: 4px 10px;
    border-radius: 99px;
    letter-spacing: 0.01em;
  }

  /* Session pill strip */
  .session-strip {
    display: flex;
    gap: 8px;
    overflow-x: auto;
    padding-bottom: 4px;
  }
  .session-pill {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    flex-shrink: 0;
    padding: 8px 14px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    cursor: pointer;
    transition: all 0.15s var(--ease);
    text-align: left;
    font: inherit;
  }
  .session-pill:hover { border-color: var(--border-hover); }
  .session-pill.active { border-color: var(--accent); background: var(--accent-dim); }
  .session-pill.idle { opacity: 0.6; }
  .pill-project { font-size: 12px; font-weight: 700; color: var(--text-primary); }
  .pill-model {
    font-size: 10px;
    color: var(--text-muted);
    font-family: 'JetBrains Mono', monospace;
  }

  /* All-active context cards */
  .active-section { display: flex; flex-direction: column; gap: 12px; }
  .active-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 10px;
  }
  .active-ctx-card {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 14px 16px;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    cursor: pointer;
    text-align: left;
    font: inherit;
    transition: border-color 0.15s var(--ease), background 0.15s var(--ease);
  }
  .active-ctx-card:hover { border-color: var(--border-hover); }
  .active-ctx-card.selected { border-color: var(--accent); background: var(--accent-dim); }
  .active-ctx-card.idle { opacity: 0.6; }
  .act-head { display: flex; align-items: baseline; justify-content: space-between; gap: 8px; }
  .act-project { font-size: 13px; font-weight: 700; color: var(--text-primary); }
  .act-pct { font-size: 13px; font-weight: 700; font-variant-numeric: tabular-nums; }
  .act-track {
    height: 6px;
    background: var(--bg-elevated);
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
    font-family: 'JetBrains Mono', monospace;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .act-tokens { flex-shrink: 0; }

  /* Per-session utilization */
  .usage-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 20px;
  }
  .usage-list { list-style: none; display: flex; flex-direction: column; gap: 12px; }
  .usage-row {
    padding: 12px 14px;
    background: var(--bg-elevated);
    border-radius: var(--radius-md);
  }
  .usage-head { display: flex; align-items: baseline; gap: 10px; margin-bottom: 8px; }
  .usage-project { font-size: 13px; font-weight: 700; color: var(--text-primary); }
  .usage-model {
    font-size: 11px;
    color: var(--text-muted);
    font-family: 'JetBrains Mono', monospace;
  }
  .usage-pct {
    margin-left: auto;
    font-size: 13px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .usage-track {
    height: 6px;
    background: var(--bg-primary);
    border-radius: 99px;
    overflow: hidden;
    margin-bottom: 8px;
  }
  .usage-fill { height: 100%; border-radius: 99px; transition: width 0.4s var(--ease); }
  .usage-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 11px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .usage-rec { color: var(--text-secondary); line-height: 1.4; }

  /* Hero card */
  .hero-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 22px 24px;
  }
  .hero-card.loading { display: flex; align-items: center; justify-content: center; gap: 12px; padding: 60px; color: var(--text-muted); font-size: 13px; }

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
    background: var(--bg-elevated);
    border-radius: 99px;
    overflow: hidden;
    margin-bottom: 10px;
    border: 1px solid rgba(255,255,255,0.04);
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
      rgba(255,255,255,0.04) 3px,
      rgba(255,255,255,0.04) 6px
    );
    border-left: 1px solid rgba(255,255,255,0.08);
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
  .sub-header:hover { background: rgba(255,255,255,0.02); }
  .sub-title { flex: 1; }
  .sub-count { font-size: 10px; color: var(--text-muted); background: var(--bg-elevated); padding: 2px 7px; border-radius: 99px; font-weight: 700; }
  .sub-tokens { font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
  .chevron { width: 0; height: 0; border-left: 4px solid transparent; border-right: 4px solid transparent; border-top: 5px solid var(--text-muted); transition: transform 0.2s ease; }
  .chevron.open { transform: rotate(180deg); }

  .sub-list { padding: 0 18px 14px; display: flex; flex-direction: column; gap: 1px; }
  .sub-item { display: flex; justify-content: space-between; align-items: center; padding: 5px 10px; border-radius: var(--radius-sm); transition: background 0.1s ease; }
  .sub-item:hover { background: var(--bg-elevated); }
  .item-name { font-family: 'JetBrains Mono', monospace; font-size: 11px; color: var(--text-secondary); }
  .item-tokens { font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }

  /* Spinner */
  .spinner { width: 18px; height: 18px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
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
    color: #1a1a1a;
  }
</style>

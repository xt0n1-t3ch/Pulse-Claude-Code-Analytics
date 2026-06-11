<script lang="ts">
  import { onMount } from "svelte";
  import { health, rateLimits, planInfo, addToast } from "../lib/stores";
  import { provider, providerProfile, setProvider, PROVIDERS, type Provider } from "../lib/provider";
  import { setPlanOverride, exportAllData, clearHistory, getDbSize, getPlanInfo, getAnalyticsSummary } from "../lib/api";
  import type { AnalyticsSummary } from "../lib/api";
  import PulseMark from "../components/PulseMark.svelte";
  import Select from "../components/Select.svelte";

  let {
    onToggleTheme,
    currentTheme,
  }: {
    onToggleTheme: () => void;
    currentTheme: string;
  } = $props();

  let planOverrideValue = $state("auto");
  let planSaving = $state(false);
  let planSavedFlash = $state(false);
  let planSavedTimer: ReturnType<typeof setTimeout> | null = null;

  function normalizePlanOverride(planName: string): string {
    const normalized = planName.trim().toLowerCase();
    if (!normalized) return "auto";
    if (normalized.includes("max 20x")) return "Max 20x ($200/mo)";
    if (normalized.includes("max 5x")) return "Max 5x ($100/mo)";
    if (normalized.startsWith("free")) return $provider === "codex" ? "free" : "Free";
    if (normalized.startsWith("go")) return "go";
    if (normalized.startsWith("plus")) return "plus";
    if (normalized.startsWith("team")) return "team";
    if (normalized.startsWith("business")) return "business";
    if (normalized.startsWith("enterprise")) return "enterprise";
    if (normalized.startsWith("pro")) return $provider === "codex" ? "pro" : "Pro";
    return "auto";
  }

  $effect(() => {
    if (!$planInfo) return;
    if ($planInfo.provider !== $provider) return;
    if (planSaving) return;
    planOverrideValue = $planInfo.detected ? "auto" : normalizePlanOverride($planInfo.plan_name);
  });

  function handleProviderChange(val: string): void {
    provider.set(val as Provider);
  }

  let providerOptions = $derived(
    Object.values(PROVIDERS).map((p) => ({ value: p.id, label: p.productName })),
  );

  let planOptions = $derived.by(() => {
    const opts: { value: string; label: string }[] = [{ value: "auto", label: "Auto-detect" }];
    if ($provider === "claude") {
      opts.push(
        { value: "Free", label: "Free" },
        { value: "Pro", label: "Pro" },
        { value: "Max 5x ($100/mo)", label: "Max 5x" },
        { value: "Max 20x ($200/mo)", label: "Max 20x" },
        { value: "Team", label: "Team" },
        { value: "Enterprise", label: "Enterprise" },
      );
    } else {
      opts.push(
        { value: "free", label: "Free" },
        { value: "go", label: "Go" },
        { value: "plus", label: "Plus" },
        { value: "team", label: "Team" },
        { value: "business", label: "Business" },
        { value: "enterprise", label: "Enterprise" },
        { value: "pro", label: "Pro" },
      );
    }
    return opts;
  });

  let dbSizeBytes = $state(0);
  let confirmClear = $state(false);
  let clearResult = $state<string | null>(null);
  let summary = $state<AnalyticsSummary | null>(null);

  onMount(async () => {
    dbSizeBytes = await getDbSize();
    try { summary = await getAnalyticsSummary(); } catch {}
  });

  async function handlePlanChange(val: string): Promise<void> {
    planOverrideValue = val;
    planSaving = true;

    if ($planInfo) {
      if (val === "auto") {
        planInfo.set({ ...$planInfo, detected: true });
      } else {
        planInfo.set({ ...$planInfo, plan_name: val, detected: false });
      }
    }
    try {
      await setPlanOverride(val === "auto" ? "" : val);
      const fresh = await getPlanInfo();
      planInfo.set(fresh);
      planSavedFlash = true;
      if (planSavedTimer) clearTimeout(planSavedTimer);
      planSavedTimer = setTimeout(() => { planSavedFlash = false; }, 1800);
    } catch {}
    planSaving = false;
  }

  function checkForUpdates(): void {
    window.dispatchEvent(new CustomEvent("pulse:check-updates"));
    addToast("Checking for updates…", "info");
  }

  function fmtBytes(b: number): string {
    if (b < 1024) return b + " B";
    if (b < 1024 * 1024) return (b / 1024).toFixed(1) + " KB";
    return (b / (1024 * 1024)).toFixed(1) + " MB";
  }

  async function handleExport(): Promise<void> {
    const data = await exportAllData();
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `pulse-export-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }

  async function handleClear(): Promise<void> {
    const deleted = await clearHistory();
    clearResult = `Cleared ${deleted} sessions`;
    confirmClear = false;
    dbSizeBytes = await getDbSize();
    setTimeout(() => { clearResult = null; }, 3000);
  }

  let discordStatus = $derived(($health?.discord_status ?? "—").toLowerCase());
  let discordTone = $derived(
    discordStatus.includes("connect") && !discordStatus.includes("dis") ? "ok"
    : discordStatus === "—" ? "muted"
    : "warn"
  );

  let sessionTotal = $derived(summary?.total_sessions ?? 0);
  let isManual = $derived.by(() => !!$planInfo && !$planInfo.detected);
  let planStateLabel = $derived.by(() => {
    if (planSaving) return "Saving";
    if (!$planInfo) return "Detecting";
    return $planInfo.detected ? "Auto" : "Manual";
  });
</script>

<div class="settings-view">
  <div class="view-header">
    <div class="view-title-group">
      <span class="view-kicker">Pulse · Configuration</span>
      <h2 class="view-title">Settings</h2>
      <span class="view-sub">Tune the telemetry source, broadcast identity, and local analytics store.</span>
    </div>
    <button type="button" class="btn check-updates-btn" onclick={checkForUpdates} aria-label="Check for application updates">
      <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 11-2.64-6.36"/><polyline points="21 3 21 9 15 9"/></svg>
      Check for updates
    </button>
  </div>

  <!-- IDENTITY — editorial masthead + control strip -->
  <section class="identity-card" style="--provider-accent: {$providerProfile.accent}">
    <div class="identity-top">
      <div class="it-lead">
        <div class="it-mark" aria-hidden="true">
          <PulseMark size={28} />
        </div>
        <div class="it-text">
          <span class="it-kicker">Active identity</span>
          <div class="it-line">
            <span class="it-product" style="color: {$providerProfile.accent}">{$providerProfile.productName}</span>
            <span class="it-sep">·</span>
            <span class="it-plan">{$planInfo?.plan_name ?? "Detecting plan…"}</span>
          </div>
          <span class="it-sub">
            Broadcasting as <strong>{$providerProfile.label}</strong>
            <span class="it-dim">·</span>
            telemetry from <span class="mono">{$providerProfile.homeDir}</span>
          </span>
        </div>
      </div>
      <div class="it-status">
        <span class="it-pill" class:manual={isManual} class:flash={planSavedFlash}>
          <span class="it-pill-dot"></span>
          {planSavedFlash ? "Saved" : planStateLabel}
        </span>
        <span class="it-pill ipc-{discordTone}">
          <span class="it-pill-dot"></span>
          {$health?.discord_status ?? "—"}
        </span>
      </div>
    </div>

    <div class="rail">
      <div class="rail-ctrl rail-ctrl-select">
        <span class="rail-k">Provider</span>
        <Select
          value={$provider}
          options={providerOptions}
          onchange={handleProviderChange}
          ariaLabel="Active provider"
        />
      </div>

      <div class="rail-ctrl rail-ctrl-select">
        <span class="rail-k">Plan override</span>
        <Select
          value={planOverrideValue}
          options={planOptions}
          onchange={handlePlanChange}
          ariaLabel="Plan override"
        />
      </div>

      <div class="rail-ctrl">
        <span class="rail-k">Appearance</span>
        <div class="theme-toggle" role="radiogroup" aria-label="Theme">
          <button
            type="button"
            class="theme-opt"
            class:active={currentTheme === "dark"}
            aria-pressed={currentTheme === "dark"}
            onclick={() => { if (currentTheme !== "dark") onToggleTheme(); }}
          >Dark</button>
          <button
            type="button"
            class="theme-opt"
            class:active={currentTheme === "light"}
            aria-pressed={currentTheme === "light"}
            onclick={() => { if (currentTheme !== "light") onToggleTheme(); }}
          >Light</button>
        </div>
      </div>
    </div>
  </section>

  <div class="settings-grid">
    <section class="s-card">
      <header class="s-card-head">
        <div class="head-accent" aria-hidden="true"></div>
        <div class="head-text">
          <h3 class="s-card-title">Data Sources</h3>
          <p class="s-card-desc">Where Pulse reads session, usage, and presence signals from.</p>
        </div>
      </header>
      <div class="s-rows">
        <div class="s-row">
          <div class="s-info">
            <span class="s-label">Sessions Directory</span>
            <span class="s-desc">Local transcripts parsed by Pulse.</span>
          </div>
          <span class="s-value mono truncate">{$providerProfile.sessionsPath}</span>
        </div>
        <div class="s-row">
          <div class="s-info">
            <span class="s-label">Rate Limit Source</span>
            <span class="s-desc">How usage quotas are fetched.</span>
          </div>
          <span class="s-value mono truncate">{$rateLimits?.source ?? "—"}</span>
        </div>
        <div class="s-row">
          <div class="s-info">
            <span class="s-label">Instruction File</span>
            <span class="s-desc">Top-level memory file read by {$providerProfile.productName}.</span>
          </div>
          <span class="s-value mono">{$providerProfile.instructionFile}</span>
        </div>
      </div>
    </section>

    <section class="s-card">
      <header class="s-card-head">
        <div class="head-accent" aria-hidden="true"></div>
        <div class="head-text">
          <h3 class="s-card-title">Data Management</h3>
          <p class="s-card-desc">Export or reset the local analytics database. Destructive actions are irreversible.</p>
        </div>
      </header>
      <div class="dm-body">
        <div class="dm-stats">
          <div class="dm-stat">
            <span class="dm-key">Database</span>
            <span class="dm-val mono">{fmtBytes(dbSizeBytes)}</span>
            <span class="dm-sub mono">pulse-analytics.db</span>
          </div>
          <div class="dm-stat">
            <span class="dm-key">Sessions</span>
            <span class="dm-val">{sessionTotal.toLocaleString()}</span>
            <span class="dm-sub">tracked locally</span>
          </div>
        </div>
        <div class="dm-actions">
          <button class="btn" onclick={handleExport}>
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 01-2 2H5a2 2 0 01-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
            Export JSON
          </button>
          {#if confirmClear}
            <button class="btn btn-danger" onclick={handleClear}>Confirm clear</button>
            <button class="btn btn-ghost" onclick={() => confirmClear = false}>Cancel</button>
          {:else}
            <button class="btn btn-danger" onclick={() => confirmClear = true}>
              <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="3 6 5 6 21 6"/><path d="M19 6l-1 14a2 2 0 01-2 2H8a2 2 0 01-2-2L5 6"/></svg>
              Clear history
            </button>
          {/if}
        </div>
      </div>
      {#if clearResult}
        <div class="clear-result">{clearResult}</div>
      {/if}
    </section>
  </div>

  <div class="meta-strip">
    <div class="meta-cell">
      <span class="meta-key">Version</span>
      <span class="meta-val mono">{$health ? "v" + $health.version : "—"}</span>
    </div>
    <div class="meta-cell">
      <span class="meta-key">Engine</span>
      <span class="meta-val">cc-discord-presence</span>
    </div>
    <div class="meta-cell">
      <span class="meta-key">Runtime</span>
      <span class="meta-val">Tauri 2.0 · Svelte 5</span>
    </div>
    <div class="meta-cell">
      <span class="meta-key">Platform</span>
      <span class="meta-val mono">{navigator.platform}</span>
    </div>
  </div>
</div>

<style>
  .settings-view {
    display: flex;
    flex-direction: column;
    gap: 18px;
    max-width: var(--content-max);
    margin: 0 auto;
    width: 100%;
    animation: fadeIn 0.3s var(--ease-out);
  }

  .view-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 24px;
    flex-wrap: wrap;
  }
  .view-title-group { display: flex; flex-direction: column; gap: 4px; }
  .view-kicker {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: var(--letter-wider);
    color: var(--text-muted);
  }
  .view-title {
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: var(--letter-tighter);
    color: var(--text-primary);
  }
  .view-sub {
    font-size: var(--fs-sm);
    color: var(--text-muted);
    line-height: var(--lh-snug);
    max-width: 560px;
  }
  .check-updates-btn {
    flex-shrink: 0;
    padding: 7px 12px;
    font-size: var(--fs-sm);
    font-weight: 500;
  }

  /* ── IDENTITY — flat, Dashboard-aligned; no overflow clip so portal menus escape ── */
  .identity-card {
    position: relative;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    transition: border-color 0.18s var(--ease);
  }
  .identity-card:hover { border-color: var(--border-hover); }

  .identity-top {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 18px;
    padding: 22px 24px 20px;
  }
  .it-lead {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }
  @media (max-width: 760px) {
    .identity-top { grid-template-columns: 1fr; }
    .it-status { justify-content: flex-start; }
  }

  .it-mark {
    width: 40px;
    height: 40px;
    border-radius: var(--radius-sm);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    color: var(--provider-accent);
    flex-shrink: 0;
  }
  .it-text { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .it-kicker {
    font-family: var(--font-mono);
    font-size: 9.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: var(--letter-wider);
    color: var(--text-muted);
  }
  .it-line {
    display: inline-flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
    font-size: 19px;
    font-weight: 700;
    letter-spacing: var(--letter-tight);
    line-height: 1.15;
    color: var(--text-primary);
  }
  .it-product { font-weight: 700; }
  .it-sep { color: var(--border-strong); font-weight: 400; }
  .it-plan { color: var(--text-secondary); font-weight: 500; }
  .it-sub {
    font-size: var(--fs-sm);
    color: var(--text-muted);
    line-height: var(--lh-snug);
  }
  .it-sub strong { font-weight: 600; color: var(--text-secondary); }
  .it-sub .it-dim { margin: 0 5px; color: var(--border-strong); }
  .it-sub .mono { font-family: var(--font-mono); font-size: 11px; color: var(--text-secondary); }

  .it-status {
    display: inline-flex;
    gap: 6px;
    flex-shrink: 0;
  }
  .it-pill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 5px 10px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-full);
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-secondary);
    letter-spacing: 0.02em;
    white-space: nowrap;
  }
  .it-pill-dot {
    width: 6px; height: 6px; border-radius: 50%;
    background: var(--text-muted);
    flex-shrink: 0;
  }
  .it-pill.manual { color: var(--text-primary); }
  .it-pill.manual .it-pill-dot {
    background: var(--warning);
    box-shadow: 0 0 0 2px var(--warning-dim);
  }
  .it-pill.flash { color: var(--success); }
  .it-pill.flash .it-pill-dot {
    background: var(--success);
    box-shadow: 0 0 0 2px var(--success-dim);
    animation: savedPulse 1.8s var(--ease);
  }
  .it-pill.ipc-ok { color: var(--success); }
  .it-pill.ipc-ok .it-pill-dot {
    background: var(--success);
    box-shadow: 0 0 0 2px var(--success-dim);
  }
  .it-pill.ipc-warn { color: var(--warning); }
  .it-pill.ipc-warn .it-pill-dot { background: var(--warning); }
  @keyframes savedPulse {
    0% { transform: scale(0.6); opacity: 0.4; }
    35% { transform: scale(1); opacity: 1; }
    100% { transform: scale(1); opacity: 1; }
  }

  .rail {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(0, 1.1fr) minmax(200px, 0.8fr);
    border-top: 1px solid var(--border);
    background: linear-gradient(180deg, var(--bg-secondary) 0%, var(--bg-card) 100%);
    border-bottom-left-radius: var(--radius-lg);
    border-bottom-right-radius: var(--radius-lg);
    overflow: visible;
  }
  @media (max-width: 900px) {
    .rail { grid-template-columns: 1fr 1fr; }
  }
  @media (max-width: 560px) {
    .rail { grid-template-columns: 1fr; }
  }
  .rail-ctrl {
    display: flex;
    flex-direction: column;
    gap: 9px;
    padding: 16px 22px 18px;
    border-left: 1px solid var(--border);
    min-width: 0;
  }
  .rail-ctrl:first-child { border-left: none; }
  @media (max-width: 900px) {
    .rail-ctrl:nth-child(2n+1) { border-left: none; }
    .rail-ctrl:nth-child(n+3) { border-top: 1px solid var(--border); }
  }
  @media (max-width: 560px) {
    .rail-ctrl { border-left: none; }
    .rail-ctrl + .rail-ctrl { border-top: 1px solid var(--border); }
  }
  .rail-k {
    font-family: var(--font-mono);
    font-size: 9.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: var(--letter-wider);
    color: var(--text-muted);
  }

  .theme-toggle {
    display: inline-flex;
    padding: 3px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    gap: 2px;
    height: 34px;
    width: 100%;
  }
  .theme-opt {
    flex: 1;
    padding: 0 14px;
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--text-muted);
    background: transparent;
    border-radius: 4px;
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    letter-spacing: 0.01em;
  }
  .theme-opt:hover { color: var(--text-secondary); }
  .theme-opt.active {
    background: var(--bg-card-hover);
    color: var(--text-primary);
    box-shadow: var(--shadow-xs), inset 0 0 0 1px var(--border);
  }

  /* ── sub-cards ── */
  .settings-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 14px;
    align-items: start;
  }
  @media (max-width: 900px) {
    .settings-grid { grid-template-columns: 1fr; }
  }

  .s-card {
    position: relative;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    transition: border-color 0.18s var(--ease);
    overflow: hidden;
  }
  .s-card:hover { border-color: var(--border-hover); }

  .s-card-head {
    position: relative;
    padding: 16px 20px 14px;
    border-bottom: 1px solid var(--border);
  }
  .head-accent { display: none; }
  .head-text { min-width: 0; }
  .s-card-title {
    font-size: var(--fs-md);
    font-weight: 600;
    letter-spacing: var(--letter-tight);
    color: var(--text-primary);
    margin: 0 0 3px;
  }
  .s-card-desc {
    font-size: var(--fs-sm);
    color: var(--text-muted);
    line-height: var(--lh-snug);
    margin: 0;
  }

  .s-rows { display: flex; flex-direction: column; }
  .s-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 14px 20px;
    border-top: 1px solid var(--border);
    min-height: 56px;
    transition: background 0.15s var(--ease);
  }
  .s-row:first-child { border-top: none; }
  .s-row:hover { background: rgba(255, 255, 255, 0.012); }

  .s-info { display: flex; flex-direction: column; gap: 3px; min-width: 0; flex: 1; }
  .s-label {
    font-size: var(--fs-base);
    font-weight: 500;
    color: var(--text-primary);
    letter-spacing: var(--letter-tight);
  }
  .s-desc {
    font-size: var(--fs-sm);
    color: var(--text-muted);
    line-height: var(--lh-snug);
  }

  .s-value {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 5px 10px;
    font-size: var(--fs-sm);
    color: var(--text-secondary);
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    white-space: nowrap;
    max-width: 60%;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .s-value.mono { font-family: var(--font-mono); font-size: 11px; }
  .s-value.truncate { min-width: 0; }

  /* ── data management ── */
  .dm-body {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 20px;
    flex: 1;
  }
  .dm-stats { display: flex; gap: 24px; align-items: stretch; flex-wrap: wrap; }
  .dm-stat {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    padding-right: 24px;
    border-right: 1px solid var(--border);
  }
  .dm-stat:last-child { border-right: none; padding-right: 0; }
  .dm-key {
    font-family: var(--font-mono);
    font-size: 9.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: var(--letter-wider);
    color: var(--text-muted);
  }
  .dm-val {
    font-size: 26px;
    font-weight: 700;
    color: var(--text-primary);
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
    line-height: 1.05;
  }
  .dm-val.mono { font-family: var(--font-mono); }
  .dm-sub { font-size: 11px; color: var(--text-muted); }
  .dm-sub.mono { font-family: var(--font-mono); }
  .dm-actions { display: inline-flex; gap: 6px; flex-wrap: wrap; justify-content: flex-end; }
  @media (max-width: 560px) {
    .dm-body { flex-direction: column; align-items: stretch; }
    .dm-actions { justify-content: stretch; }
  }

  .clear-result {
    padding: 9px 20px;
    font-size: var(--fs-sm);
    color: var(--success);
    font-weight: 500;
    background: var(--success-dim);
    border-top: 1px solid color-mix(in srgb, var(--success) 25%, var(--border));
  }

  /* ── meta footer strip ── */
  .meta-strip {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 0;
    padding: 2px 0 0;
    border-top: 1px dashed var(--border);
    margin-top: 4px;
  }
  @media (max-width: 720px) {
    .meta-strip { grid-template-columns: repeat(2, 1fr); }
  }
  .meta-cell {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 12px 20px 0;
    border-right: 1px dashed var(--border);
  }
  .meta-cell:last-child { border-right: none; }
  @media (max-width: 720px) {
    .meta-cell:nth-child(2) { border-right: none; }
  }
  .meta-key {
    font-family: var(--font-mono);
    font-size: 9.5px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: var(--letter-wider);
    color: var(--text-muted);
  }
  .meta-val {
    font-size: var(--fs-sm);
    font-weight: 500;
    color: var(--text-secondary);
  }
  .meta-val.mono { font-family: var(--font-mono); font-size: var(--fs-xs); }

  .dm-actions :global(.btn) {
    padding: 7px 12px;
    font-size: var(--fs-sm);
    font-weight: 500;
  }
</style>

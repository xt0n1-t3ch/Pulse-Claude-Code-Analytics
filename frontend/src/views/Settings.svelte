<script lang="ts">
  import { onMount } from "svelte";
  import { health, rateLimits, planInfo } from "../lib/stores";
  import { setPlanOverride, exportAllData, clearHistory, getDbSize } from "../lib/api";

  let {
    onToggleTheme,
    currentTheme,
  }: {
    onToggleTheme: () => void;
    currentTheme: string;
  } = $props();

  let planOverrideValue = $state("auto");
  let dbSizeBytes = $state(0);
  let confirmClear = $state(false);
  let clearResult = $state<string | null>(null);

  onMount(async () => { dbSizeBytes = await getDbSize(); });

  function handlePlanChange(e: Event): void {
    const val = (e.target as HTMLSelectElement).value;
    planOverrideValue = val;
    setPlanOverride(val === "auto" ? "" : val);
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
</script>

<div class="settings-view">
  <div class="view-header">
    <h2 class="view-title">Settings</h2>
  </div>

  <div class="settings-grid">
    <div class="settings-section">
      <h3 class="section-title">Appearance</h3>
      <div class="settings-card">
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Theme</span>
            <span class="setting-desc">Switch between dark and light mode</span>
          </div>
          <select class="setting-select" value={currentTheme} onchange={() => onToggleTheme()}>
            <option value="dark">Dark</option>
            <option value="light">Light</option>
          </select>
        </div>
      </div>
    </div>

    <div class="settings-section">
      <h3 class="section-title">Plan</h3>
      <div class="settings-card">
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Current Plan</span>
            <span class="setting-desc">
              {#if $planInfo}
                {$planInfo.detected ? "Auto-detected" : "Manual override"}: <strong>{$planInfo.plan_name}</strong>
              {:else}
                Detecting...
              {/if}
            </span>
          </div>
          <select class="setting-select" value={planOverrideValue} onchange={handlePlanChange}>
            <option value="auto">Auto-detect</option>
            <option value="Free">Free</option>
            <option value="Pro">Pro</option>
            <option value="Max 5x ($100/mo)">Max 5x</option>
            <option value="Max 20x ($200/mo)">Max 20x</option>
          </select>
        </div>
      </div>
    </div>

    <div class="settings-section">
      <h3 class="section-title">Data Sources</h3>
      <div class="settings-card">
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Sessions Directory</span>
            <span class="setting-desc">Where Claude Code stores session data</span>
          </div>
          <span class="setting-value mono">~/.claude/projects/</span>
        </div>
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Rate Limit Source</span>
            <span class="setting-desc">How usage data is fetched</span>
          </div>
          <span class="setting-value mono">{$rateLimits?.source ?? "—"}</span>
        </div>
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Discord Status</span>
            <span class="setting-desc">Discord IPC connection state</span>
          </div>
          <span class="setting-value mono">{$health?.discord_status ?? "—"}</span>
        </div>
      </div>
    </div>

    <div class="settings-section">
      <h3 class="section-title">Data Management</h3>
      <div class="settings-card">
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Database Size</span>
            <span class="setting-desc">Local analytics storage</span>
          </div>
          <span class="setting-value mono">{fmtBytes(dbSizeBytes)}</span>
        </div>
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Export All Data</span>
            <span class="setting-desc">Download sessions, stats, and summary as JSON</span>
          </div>
          <button class="setting-btn" onclick={handleExport}>Export</button>
        </div>
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Clear History</span>
            <span class="setting-desc">Delete all stored sessions and daily stats</span>
          </div>
          {#if confirmClear}
            <div class="confirm-group">
              <button class="setting-btn danger" onclick={handleClear}>Confirm Delete</button>
              <button class="setting-btn" onclick={() => confirmClear = false}>Cancel</button>
            </div>
          {:else}
            <button class="setting-btn danger-outline" onclick={() => confirmClear = true}>Clear</button>
          {/if}
        </div>
        {#if clearResult}
          <div class="clear-result">{clearResult}</div>
        {/if}
      </div>
    </div>

    <div class="settings-section">
      <h3 class="section-title">About Pulse</h3>
      <div class="settings-card">
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Version</span>
          </div>
          <span class="setting-value mono">{$health ? "v" + $health.version : "—"}</span>
        </div>
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Engine</span>
          </div>
          <span class="setting-value">cc-discord-presence</span>
        </div>
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Runtime</span>
          </div>
          <span class="setting-value">Tauri 2.0 + Svelte 5</span>
        </div>
        <div class="setting-row">
          <div class="setting-info">
            <span class="setting-label">Platform</span>
          </div>
          <span class="setting-value">{navigator.platform}</span>
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .settings-view {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  .view-header { display: flex; align-items: center; }
  .view-title { font-size: 20px; font-weight: 700; }

  .settings-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }

  .settings-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .section-title {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
  }

  .settings-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 4px 0;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 20px;
    border-bottom: 1px solid var(--border);
    gap: 16px;
  }

  .setting-row:last-child { border-bottom: none; }

  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .setting-label {
    font-size: 13px;
    font-weight: 600;
  }

  .setting-desc {
    font-size: 11px;
    color: var(--text-muted);
  }

  .setting-value {
    font-size: 13px;
    color: var(--text-secondary);
    white-space: nowrap;
  }

  .setting-select {
    font-size: 12px;
    font-weight: 500;
    padding: 8px 36px 8px 14px;
    min-width: 140px;
    color: var(--text-primary);
    background-color: var(--bg-elevated);
    background-image: url("data:image/svg+xml;charset=utf-8,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6'%3E%3Cpath fill='none' stroke='%23a3a3a3' stroke-width='1.4' stroke-linecap='round' stroke-linejoin='round' d='M1 1l4 4 4-4'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: right 14px center;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
    appearance: none;
    -webkit-appearance: none;
    -moz-appearance: none;
    transition: border-color 0.15s ease, background-color 0.15s ease;
    font-family: inherit;
  }
  .setting-select:hover { border-color: var(--border-hover); background-color: var(--bg-card-hover); }
  .setting-select:focus { outline: none; border-color: var(--accent); }
  .setting-select option { background: var(--bg-elevated); color: var(--text-primary); padding: 8px; }

  .mono {
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    font-size: 12px;
  }

  .setting-btn {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 6px 14px;
    cursor: pointer;
    transition: all 0.15s ease;
    white-space: nowrap;
  }

  .setting-btn:hover { color: var(--accent); border-color: var(--accent); }
  .setting-btn.danger { color: #fff; background: var(--danger); border-color: var(--danger); }
  .setting-btn.danger:hover { opacity: 0.9; }
  .setting-btn.danger-outline { color: var(--danger); border-color: var(--danger); background: transparent; }
  .setting-btn.danger-outline:hover { background: rgba(220, 53, 69, 0.1); }

  .confirm-group { display: flex; gap: 6px; }
  .clear-result { padding: 8px 20px; font-size: 11px; color: var(--success); font-weight: 600; }
</style>

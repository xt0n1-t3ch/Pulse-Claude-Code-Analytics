<script lang="ts">
  import { sessions, discordUser, health, discordPreview } from "../lib/stores";
  import { setDiscordEnabled } from "../lib/api";
  import { fmtCost, fmtTokens, fmtDuration } from "../lib/utils";

  let discordEnabled = $state(true);

  $effect(() => {
    if ($health) discordEnabled = $health.discord_enabled;
  });

  function toggleDiscord(): void {
    discordEnabled = !discordEnabled;
    setDiscordEnabled(discordEnabled);
  }

  function toggleSetting(key: keyof typeof $discordPreview): void {
    discordPreview.update((s) => ({ ...s, [key]: !s[key] }));
  }

  const presets: Record<string, typeof $discordPreview> = {
    minimal: { showProject: true, showBranch: false, showModel: false, showActivity: false, showTokens: false, showCost: false },
    standard: { showProject: true, showBranch: true, showModel: true, showActivity: true, showTokens: false, showCost: false },
    full: { showProject: true, showBranch: true, showModel: true, showActivity: true, showTokens: true, showCost: true },
  };

  function applyPreset(name: string): void {
    const p = presets[name];
    if (p) discordPreview.set({ ...p });
  }

  let previewSession = $derived($sessions[0]);

  let detailsLine = $derived.by(() => {
    if (!previewSession) return "No active session";
    const s = $discordPreview;
    let parts: string[] = [];
    if (s.showProject) parts.push(previewSession.project);
    if (s.showBranch && previewSession.branch) parts.push(previewSession.branch);
    if (s.showCost) parts.push(fmtCost(previewSession.cost));
    return parts.join(" · ") || "No active session";
  });

  let stateLine = $derived.by(() => {
    if (!previewSession) return "Idle";
    const s = $discordPreview;
    let parts: string[] = [];
    if (s.showModel) parts.push(previewSession.model);
    if (s.showActivity) parts.push(previewSession.activity);
    if (s.showTokens) parts.push(fmtTokens(previewSession.tokens) + " tokens");
    return parts.join(" · ") || "Idle";
  });
</script>

<div class="discord-view">
  <div class="view-header">
    <h2 class="view-title">Discord Rich Presence</h2>
  </div>

  <div class="discord-layout">
    <div class="config-panel">
      <div class="preset-row">
        <span class="preset-label">Presets</span>
        <div class="preset-buttons">
          <button class="preset-btn" onclick={() => applyPreset("minimal")}>Minimal</button>
          <button class="preset-btn" onclick={() => applyPreset("standard")}>Standard</button>
          <button class="preset-btn" onclick={() => applyPreset("full")}>Full</button>
        </div>
      </div>
      <div class="settings-card">
        <div class="setting-row">
          <span class="setting-label">Enable Rich Presence</span>
          <label class="toggle">
            <input type="checkbox" checked={discordEnabled} onchange={toggleDiscord} />
            <span class="toggle-slider"></span>
          </label>
        </div>
        <div class="setting-row">
          <span class="setting-label">Show Project Name</span>
          <label class="toggle"><input type="checkbox" checked={$discordPreview.showProject} onchange={() => toggleSetting("showProject")} /><span class="toggle-slider"></span></label>
        </div>
        <div class="setting-row">
          <span class="setting-label">Show Git Branch</span>
          <label class="toggle"><input type="checkbox" checked={$discordPreview.showBranch} onchange={() => toggleSetting("showBranch")} /><span class="toggle-slider"></span></label>
        </div>
        <div class="setting-row">
          <span class="setting-label">Show Model</span>
          <label class="toggle"><input type="checkbox" checked={$discordPreview.showModel} onchange={() => toggleSetting("showModel")} /><span class="toggle-slider"></span></label>
        </div>
        <div class="setting-row">
          <span class="setting-label">Show Activity</span>
          <label class="toggle"><input type="checkbox" checked={$discordPreview.showActivity} onchange={() => toggleSetting("showActivity")} /><span class="toggle-slider"></span></label>
        </div>
        <div class="setting-row">
          <span class="setting-label">Show Token Count</span>
          <label class="toggle"><input type="checkbox" checked={$discordPreview.showTokens} onchange={() => toggleSetting("showTokens")} /><span class="toggle-slider"></span></label>
        </div>
        <div class="setting-row">
          <span class="setting-label">Show Cost</span>
          <label class="toggle"><input type="checkbox" checked={$discordPreview.showCost} onchange={() => toggleSetting("showCost")} /><span class="toggle-slider"></span></label>
        </div>
      </div>
    </div>

    <div class="preview-panel">
      <h3 class="card-title">Live Preview</h3>
      <div class="dp-profile">
        {#if $discordUser?.banner_url}
          <div class="dp-banner" style="background-image: url({$discordUser.banner_url}); background-size: cover; background-position: center;"></div>
        {:else}
          <div class="dp-banner"></div>
        {/if}
        <div class="dp-avatar-ring">
          <div class="dp-avatar">
            {#if $discordUser}
              <img src={$discordUser.avatar_url} alt="avatar" />
            {:else}
              <svg width="40" height="40" viewBox="0 0 24 24" fill="none"><path d="M12 2L12 6M12 18L12 22M6 12L2 12M22 12L18 12M5.64 5.64L8.17 8.17M15.83 15.83L18.36 18.36M5.64 18.36L8.17 15.83M15.83 8.17L18.36 5.64" stroke="var(--accent)" stroke-width="2" stroke-linecap="round"/></svg>
            {/if}
          </div>
          <div class="dp-status-dot"></div>
        </div>
        <div class="dp-username">
          {$discordUser?.username ?? "xt0n1"} <span class="dp-tag">ツ</span>
        </div>
        <div class="dp-separator"></div>
        <div class="dp-section-title">CURRENT ACTIVITY</div>
        <div class="dp-activity-card">
          <div class="dp-activity-header">Playing</div>
          <div class="dp-activity-body">
            <div class="dp-activity-icon">
              <svg width="28" height="28" viewBox="0 0 24 24" fill="none"><path d="M12 2L12 6M12 18L12 22M6 12L2 12M22 12L18 12M5.64 5.64L8.17 8.17M15.83 15.83L18.36 18.36M5.64 18.36L8.17 15.83M15.83 8.17L18.36 5.64" stroke="var(--accent)" stroke-width="2" stroke-linecap="round"/></svg>
            </div>
            <div class="dp-activity-info">
              <div class="dp-activity-name">Claude Code</div>
              <div class="dp-activity-details">{detailsLine}</div>
              <div class="dp-activity-state">{stateLine}</div>
              {#if previewSession}
                <div class="dp-activity-elapsed">{fmtDuration(previewSession.duration_secs)} elapsed</div>
              {/if}
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</div>

<style>
  .discord-view {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .view-header {
    display: flex;
    align-items: center;
  }

  .view-title {
    font-size: 20px;
    font-weight: 700;
  }

  .discord-layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }

  .config-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .preset-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 4px;
  }

  .preset-label {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }

  .preset-buttons {
    display: flex;
    gap: 6px;
  }

  .preset-btn {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 4px 12px;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .preset-btn:hover {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-dim);
  }

  .settings-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 8px 0;
  }

  .setting-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
  }

  .setting-row:last-child {
    border-bottom: none;
  }

  .setting-label {
    font-size: 13px;
    font-weight: 500;
  }

  /* toggle styles live in global.css — do NOT override here; overrides
     stack with Svelte scoped rules and break the knob position. */

  .card-title {
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    margin-bottom: 14px;
  }

  .preview-panel {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 20px;
  }

  .dp-profile {
    background: var(--bg-secondary);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }

  .dp-banner {
    height: 60px;
    background: linear-gradient(135deg, #1a1a1a 0%, #0a0a0a 100%);
  }

  .dp-avatar-ring {
    position: relative;
    width: 64px;
    height: 64px;
    margin: -32px 0 0 16px;
  }

  .dp-avatar {
    width: 64px;
    height: 64px;
    border-radius: 50%;
    background: var(--bg-card);
    border: 4px solid var(--bg-secondary);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }

  .dp-avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .dp-status-dot {
    position: absolute;
    bottom: 2px;
    right: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--success);
    border: 3px solid var(--bg-secondary);
  }

  .dp-username {
    padding: 8px 16px 0;
    font-size: 16px;
    font-weight: 700;
  }

  .dp-tag {
    font-size: 12px;
    color: var(--text-muted);
    font-weight: 400;
  }

  .dp-separator {
    margin: 10px 16px;
    height: 1px;
    background: var(--border);
  }

  .dp-section-title {
    padding: 0 16px 8px;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--text-muted);
  }

  .dp-activity-card {
    margin: 0 12px 12px;
    background: var(--bg-card);
    border-radius: var(--radius-md);
    padding: 10px;
  }

  .dp-activity-header {
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    color: var(--text-muted);
    margin-bottom: 8px;
  }

  .dp-activity-body {
    display: flex;
    gap: 10px;
  }

  .dp-activity-icon {
    width: 48px;
    height: 48px;
    border-radius: var(--radius-md);
    background: var(--bg-elevated);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .dp-activity-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .dp-activity-name {
    font-size: 13px;
    font-weight: 700;
  }

  .dp-activity-details,
  .dp-activity-state {
    font-size: 12px;
    color: var(--text-secondary);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dp-activity-elapsed {
    font-size: 11px;
    color: var(--text-muted);
  }
</style>

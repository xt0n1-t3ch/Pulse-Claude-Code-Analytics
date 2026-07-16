<script lang="ts">
  import { onMount } from "svelte";
  import {
    sessions,
    activeSessions,
    rateLimits,
    discordUser,
    health,
    discordPreview,
    discordPresencePreview,
    discordSettings,
    addToast,
    applyDiscordSettings,
    loadDiscordSettings,
    previewToDisplayPrefs,
    refreshDiscordPresencePreview,
  } from "../lib/stores";
  import { provider, providerProfile } from "../lib/provider";
  import {
    setCodexDesktopDesign,
    setDiscordDisplayPrefs,
    setDiscordEnabled,
    setDiscordFieldOrder,
  } from "../lib/api";
  import type { SessionInfo } from "../lib/api";
  import { fmtCost, fmtTokens, fmtDuration } from "../lib/utils";
  import { rpArtFor } from "../lib/rpArt";
  import PulseMark from "../components/PulseMark.svelte";

  let discordEnabled = $state(true);
  let settingsPending = $state(false);

  $effect(() => {
    if ($discordSettings) discordEnabled = $discordSettings.enabled;
    else if ($health) discordEnabled = $health.discord_enabled;
  });

  onMount(async () => {
    try {
      const settings = await loadDiscordSettings();
      discordEnabled = settings.enabled;
      await refreshDiscordPresencePreview();
    } catch (error) {
      addToast(`Discord settings failed to load: ${String(error)}`, "danger", 5000);
    }
  });

  async function toggleDiscord(): Promise<void> {
    if (settingsPending) return;
    const previous = discordEnabled;
    discordEnabled = !previous;
    settingsPending = true;
    try {
      applyDiscordSettings(await setDiscordEnabled(discordEnabled));
      await refreshDiscordPresencePreview();
    } catch (error) {
      discordEnabled = previous;
      addToast(`Rich Presence update failed: ${String(error)}`, "danger", 5000);
    } finally {
      settingsPending = false;
    }
  }

  async function persistPreview(next: typeof $discordPreview): Promise<void> {
    if (settingsPending) return;
    const previous = $discordPreview;
    discordPreview.set(next);
    settingsPending = true;
    try {
      applyDiscordSettings(await setDiscordDisplayPrefs(previewToDisplayPrefs(next)));
      await refreshDiscordPresencePreview();
    } catch (error) {
      discordPreview.set(previous);
      addToast(`Discord privacy update failed: ${String(error)}`, "danger", 5000);
    } finally {
      settingsPending = false;
    }
  }

  function toggleSetting(key: keyof typeof $discordPreview): void {
    void persistPreview({ ...$discordPreview, [key]: !$discordPreview[key] });
  }

  type Preset = "minimal" | "standard" | "full";
  const presets: Record<Preset, typeof $discordPreview> = {
    minimal: { showProject: true, showBranch: false, showModel: false, showActivity: false, showTokens: false, showCost: false, showLimits: false, showCredits: false, showContext: false, showSystems: false },
    standard: { showProject: true, showBranch: true, showModel: true, showActivity: true, showTokens: false, showCost: false, showLimits: true, showCredits: true, showContext: true, showSystems: true },
    full: { showProject: true, showBranch: true, showModel: true, showActivity: true, showTokens: true, showCost: true, showLimits: true, showCredits: true, showContext: true, showSystems: true },
  };
  const presetOrder: Preset[] = ["minimal", "standard", "full"];

  function applyPreset(name: Preset): void {
    void persistPreview({ ...presets[name] });
  }

  async function changeDesktopDesign(
    design: "codex_app" | "chatgpt_app",
  ): Promise<void> {
    if (settingsPending || $discordSettings?.desktop_design === design) return;
    const previous = $discordSettings;
    if (previous) {
      applyDiscordSettings({ ...previous, desktop_design: design });
    }
    settingsPending = true;
    try {
      applyDiscordSettings(await setCodexDesktopDesign(design));
      await refreshDiscordPresencePreview();
    } catch (error) {
      if (previous) applyDiscordSettings(previous);
      addToast(`Desktop identity update failed: ${String(error)}`, "danger", 5000);
    } finally {
      settingsPending = false;
    }
  }

  let activePreset = $derived.by<Preset | null>(() => {
    const cur = $discordPreview;
    for (const name of presetOrder) {
      const p = presets[name];
      let match = true;
      for (const k of Object.keys(p) as (keyof typeof p)[]) {
        if (cur[k] !== p[k]) { match = false; break; }
      }
      if (match) return name;
    }
    return null;
  });

  let previewSession = $derived($activeSessions[0] ?? $sessions[0]);
  let activeSessionCount = $derived($activeSessions.length);
  let previewAppName = $derived(previewSession?.app_name ?? $providerProfile.productName);
  let presenceAppName = $derived($discordPresencePreview?.app_name ?? previewAppName);
  let previewArt = $derived(
    rpArtFor(
      $discordPresencePreview?.provider ?? previewSession?.provider ?? $provider,
      $discordPresencePreview?.large_image_key,
      $discordPresencePreview?.large_text,
    ),
  );
  let previewAssetKey = $derived(previewArt.assetKey);
  let previewFast = $derived(previewSession?.fast ?? false);

  let detailsLine = $derived.by(() => {
    if ($discordPresencePreview) return $discordPresencePreview.details;
    if (!previewSession) return "No active session";
    const s = $discordPreview;
    let parts: string[] = [];
    if (s.showProject) parts.push(previewSession.project);
    if (s.showBranch && previewSession.branch) parts.push(previewSession.branch);
    if (s.showCost) parts.push(fmtCost(previewSession.cost));
    return parts.join(" · ") || "No active session";
  });

  let stateLine = $derived.by(() => {
    if ($discordPresencePreview) return $discordPresencePreview.state;
    if (!previewSession) return "Idle";
    const s = $discordPreview;
    let parts: string[] = [];
    if (s.showModel) {
      const model =
        previewFast && !previewSession.model.includes("⚡")
          ? `⚡ ${previewSession.model}`
          : previewSession.model;
      parts.push(model);
    }
    if (s.showActivity) parts.push(previewSession.activity);
    if (s.showSystems) parts.push(...systemParts(previewSession));
    if (s.showTokens) parts.push(fmtTokens(previewSession.tokens) + " tokens");
    if (s.showContext) {
      const contextLine = contextUsagePart(previewSession);
      if (contextLine) parts.push(contextLine);
    }
    if (s.showLimits) {
      const limitLine = sessionLimitPart();
      if (limitLine) parts.push(limitLine);
    }
    if (s.showCredits) {
      const creditLine = creditsPart();
      if (creditLine) parts.push(creditLine);
    }
    return parts.join(" · ") || "Idle";
  });

  function systemParts(session: SessionInfo): string[] {
    const parts: string[] = [];
    if (session.workflow_label) {
      parts.push(session.workflow_label);
    }
    if (session.subagent_count > 0) {
      parts.push(`${session.subagent_count} ${session.subagent_count === 1 ? "agent" : "agents"}`);
    }
    return parts;
  }

  function contextUsagePart(session: SessionInfo): string | null {
    if (!session.context_used_tokens || !session.context_window_tokens) return null;
    const pct = Math.min(100, Math.max(0, (session.context_used_tokens / session.context_window_tokens) * 100));
    return `Ctx ${pct.toFixed(0)}% used`;
  }

  function sessionLimitPart(): string | null {
    const usage = $rateLimits?.usage;
    if (!usage) return null;
    return usage.scopes
      .flatMap((scope) => scope.windows.map((window) => `${windowLabel(window.window_minutes)} ${window.remaining_percent.toFixed(0)}%`))
      .join(" • ") || null;
  }

  function windowLabel(minutes: number): string {
    if (minutes === 300) return "5h";
    if (minutes === 1440) return "24h";
    if (minutes === 10080) return "7d";
    if (minutes > 0 && minutes % 1440 === 0) return `${minutes / 1440}d`;
    if (minutes > 0 && minutes % 60 === 0) return `${minutes / 60}h`;
    return `${minutes}m`;
  }

  function creditsPart(): string | null {
    const credits = $rateLimits?.usage?.credits;
    if (!credits) return null;
    if (credits.unlimited) return "Credits Unlimited";
    if (credits.balance == null) return null;
    const numeric = Number(credits.balance);
    const display = Number.isFinite(numeric) ? numeric.toLocaleString() : credits.balance;
    return `Credits ${display}`;
  }

  const fieldRows = [
    { id: "project", key: "showProject",  label: "Project name",  hint: "Repository or folder name." },
    { id: "branch", key: "showBranch",   label: "Git branch",    hint: "Current checked-out ref." },
    { id: "model", key: "showModel",    label: "Model",         hint: "Active model identifier." },
    { id: "activity", key: "showActivity", label: "Activity",      hint: "What Pulse thinks you're doing." },
    { id: "tokens", key: "showTokens",   label: "Token count",   hint: "Cumulative tokens this session." },
    { id: "cost", key: "showCost",     label: "Cost",          hint: "Running USD total for the session." },
    { id: "quotas", key: "showLimits",   label: "Usage quotas", hint: "Only quota windows actually reported by the provider." },
    { id: "credits", key: "showCredits", label: "Credits available", hint: "Real Codex account credit balance when available." },
    { id: "context", key: "showContext",  label: "Context usage", hint: "Current context-window fill percentage." },
    { id: "systems", key: "showSystems",  label: "Systems",       hint: "Safe workflow and agent signals." },
  ] as const;
  type FieldId = (typeof fieldRows)[number]["id"];

  let orderedFieldRows = $derived.by(() => {
    const rank = new Map(($discordSettings?.field_order ?? []).map((id, index) => [id, index]));
    return [...fieldRows].sort((a, b) => (rank.get(a.id) ?? 999) - (rank.get(b.id) ?? 999));
  });

  async function moveField(id: FieldId, offset: -1 | 1): Promise<void> {
    if (settingsPending || !$discordSettings?.supports_field_order) return;
    const order = orderedFieldRows.map((row) => row.id);
    const index = order.indexOf(id);
    const next = index + offset;
    if (index < 0 || next < 0 || next >= order.length) return;
    [order[index], order[next]] = [order[next], order[index]];
    const previous = $discordSettings;
    applyDiscordSettings({ ...previous, field_order: order });
    settingsPending = true;
    try {
      applyDiscordSettings(await setDiscordFieldOrder(order));
      await refreshDiscordPresencePreview();
    } catch (error) {
      applyDiscordSettings(previous);
      addToast(`Field order failed to save: ${String(error)}`, "danger", 5000);
    } finally {
      settingsPending = false;
    }
  }

  let activeCount = $derived.by(() => {
    const s = $discordPreview;
    return fieldRows.filter((r) => s[r.key]).length;
  });

  let discordStatus = $derived(($health?.discord_status ?? "—").toLowerCase());
  let ipcConnected = $derived(discordStatus.includes("connect") && !discordStatus.includes("dis"));
</script>

<div class="discord-view" style="--provider-accent: {$providerProfile.accent}">
  <div class="view-header">
    <div class="view-title-group">
      <span class="view-kicker">Pulse · Rich Presence</span>
      <h2 class="view-title">Discord</h2>
      <span class="view-sub">
        Broadcasting as <strong style="color: {$providerProfile.accent}">{presenceAppName}</strong>
        <span class="sub-dot">·</span>
        {activeCount} of {fieldRows.length} fields shown
        {#if activeSessionCount > 1}
          <span class="sub-dot">·</span>
          {activeSessionCount} active sessions
        {/if}
      </span>
    </div>
    <div class="header-meta">
      <span class="hm-pill" class:ok={ipcConnected} class:warn={!ipcConnected} title="Discord IPC status">
        <span class="hm-dot"></span>
        IPC · {ipcConnected ? "Connected" : ($health?.discord_status ?? "—")}
      </span>
      <span class="hm-pill hm-mono" title="Rich Presence asset key">
        {previewAssetKey}
      </span>
      <span class="hm-pill" class:ok={discordEnabled} title="Broadcast state">
        <span class="hm-dot" class:live={discordEnabled}></span>
        {discordEnabled ? "Live" : "Paused"}
      </span>
    </div>
  </div>

  <div class="discord-layout">
    <!-- LEFT: Control column — one tall card, 3 sections -->
    <section class="control-card">
      <!-- Section 1: Master toggle -->
      <div class="cc-toggle-row" class:on={discordEnabled}>
        <label class="big-toggle">
          <input type="checkbox" checked={discordEnabled} disabled={settingsPending} onchange={toggleDiscord} />
          <span class="toggle-track">
            <span class="toggle-thumb"></span>
          </span>
          <span class="bt-text">
            <span class="bt-title">Rich Presence</span>
            <span class="bt-sub">
              {discordEnabled
                ? `Broadcasting your ${presenceAppName} session to Discord`
                : "Presence is paused — Discord shows no activity"}
            </span>
          </span>
        </label>
      </div>

      {#if $discordSettings?.supports_desktop_design}
        <div class="cc-section identity-section">
          <div class="cc-section-head">
            <div class="cc-section-text">
              <h3 class="cc-section-title">Desktop identity</h3>
              <p class="cc-section-desc">Choose the Discord app name and large artwork for Codex Desktop.</p>
            </div>
            <div class="preset-seg identity-seg" role="group" aria-label="Codex desktop design">
              <button
                type="button"
                class="preset-opt"
                class:active={$discordSettings.desktop_design === "codex_app"}
                aria-pressed={$discordSettings.desktop_design === "codex_app"}
                disabled={settingsPending}
                onclick={() => changeDesktopDesign("codex_app")}
              >Codex App</button>
              <button
                type="button"
                class="preset-opt"
                class:active={$discordSettings.desktop_design === "chatgpt_app"}
                aria-pressed={$discordSettings.desktop_design === "chatgpt_app"}
                disabled={settingsPending}
                onclick={() => changeDesktopDesign("chatgpt_app")}
              >ChatGPT App</button>
            </div>
          </div>
        </div>
      {/if}

      <!-- Section 2: Preset -->
      <div class="cc-section">
        <div class="cc-section-head">
          <div class="cc-section-text">
            <h3 class="cc-section-title">Preset</h3>
            <p class="cc-section-desc">Pick a density, or hand-tune the fields below.</p>
          </div>
          <div class="preset-seg" role="tablist" aria-label="Field preset">
            {#each presetOrder as name}
              <button
                type="button"
                role="tab"
                class="preset-opt"
                class:active={activePreset === name}
                aria-selected={activePreset === name}
                disabled={settingsPending}
                onclick={() => applyPreset(name)}
              >{name.charAt(0).toUpperCase() + name.slice(1)}</button>
            {/each}
          </div>
        </div>
      </div>

      <!-- Section 3: Fields -->
      <div class="cc-section cc-section-fields">
        <div class="cc-section-head cc-fields-head">
          <div class="cc-section-text">
            <h3 class="cc-section-title">Fields</h3>
            <p class="cc-section-desc">Toggle visibility and reorder fields. The backend generates the exact preview.</p>
          </div>
          <span class="field-count">
            <span class="fc-num">{activeCount}</span><span class="fc-den">/{fieldRows.length}</span>
          </span>
        </div>
        <div class="field-grid">
          {#each orderedFieldRows as row, index (row.id)}
            <div class="field-cell" class:active={$discordPreview[row.key]}>
              <div class="field-text">
                <span class="field-label">{row.label}</span>
                <span class="field-hint">{row.hint}</span>
              </div>
              {#if $discordSettings?.supports_field_order}
                <div class="field-order" role="group" aria-label={`Reorder ${row.label}`}>
                  <button type="button" aria-label={`Move ${row.label} up`} title="Move up" disabled={settingsPending || index === 0} onclick={() => moveField(row.id, -1)}>↑</button>
                  <button type="button" aria-label={`Move ${row.label} down`} title="Move down" disabled={settingsPending || index === orderedFieldRows.length - 1} onclick={() => moveField(row.id, 1)}>↓</button>
                </div>
              {/if}
              <label class="toggle" aria-label={`Show ${row.label}`}>
                <input
                  type="checkbox"
                  checked={$discordPreview[row.key]}
                  disabled={settingsPending}
                  onchange={() => toggleSetting(row.key)}
                />
                <span class="toggle-slider"></span>
              </label>
            </div>
          {/each}
        </div>
      </div>
    </section>

    <!-- RIGHT: Stage — live Discord profile preview -->
    <aside class="stage">
      <div class="stage-label">
        <span class="sl-text">Live preview</span>
        <span class="sl-meta">
          <span class="sl-preset">{activePreset ? activePreset.charAt(0).toUpperCase() + activePreset.slice(1) : "Custom"}</span>
          <span class="sl-div">·</span>
          <span class="sl-count">{activeCount}/{fieldRows.length}</span>
          <span class="sl-dot" class:on={discordEnabled}></span>
        </span>
      </div>

      <div class="dp-profile">
        {#if $discordUser?.banner_url}
          <div class="dp-banner" style="background-image: url({$discordUser.banner_url});"></div>
        {:else}
          <div class="dp-banner dp-banner-default"></div>
        {/if}
        <div class="dp-body">
          <div class="dp-avatar-ring">
            <div class="dp-avatar">
              {#if $discordUser}
                <img src={$discordUser.avatar_url} alt="avatar" />
              {:else}
                <PulseMark size={40} />
              {/if}
            </div>
            <div class="dp-status-dot" class:offline={!discordEnabled}></div>
          </div>
          <div class="dp-username">
            {$discordUser?.username ?? "xt0n1"} <span class="dp-tag">ツ</span>
          </div>
          <div class="dp-separator"></div>
          <div class="dp-section-title">Current Activity</div>
          <div class="dp-activity-card">
            <div class="dp-activity-header">Playing a game</div>
            <div class="dp-activity-body">
              <div class="dp-activity-art" title={previewArt.largeText}>
                <img class="dp-art-large" src={previewArt.large} alt={previewArt.largeText} draggable="false" />
                {#if previewArt.small}
                  <img class="dp-art-small" src={previewArt.small} alt="" draggable="false" />
                {/if}
              </div>
              <div class="dp-activity-info">
                <div class="dp-activity-name">{presenceAppName}</div>
                <div class="dp-activity-details" title={detailsLine}>{detailsLine}</div>
                <div class="dp-activity-state" title={stateLine}>{stateLine}</div>
                {#if $discordPresencePreview?.duration_secs || previewSession}
                  <div class="dp-activity-elapsed">{fmtDuration($discordPresencePreview?.duration_secs ?? previewSession?.duration_secs ?? 0)} elapsed</div>
                {/if}
              </div>
            </div>
          </div>
        </div>
      </div>
    </aside>
  </div>
</div>

<style>
  .discord-view {
    display: flex;
    flex-direction: column;
    gap: 20px;
    max-width: var(--content-max);
    margin: 0 auto;
    width: 100%;
    animation: fadeIn 0.3s var(--ease-out);
  }

  .view-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 28px;
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
  }
  .view-sub strong { font-weight: 700; }
  .sub-dot { margin: 0 5px; color: var(--border-strong); }

  .header-meta {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .hm-pill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 5px 10px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-full);
    font-size: var(--fs-xs);
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text-secondary);
    white-space: nowrap;
  }
  .hm-pill.hm-mono { font-family: var(--font-mono); font-size: 10.5px; color: var(--text-secondary); }
  .hm-dot {
    width: 6px; height: 6px; border-radius: 50%;
    background: var(--text-muted);
    flex-shrink: 0;
  }
  .hm-dot.live { background: var(--success); box-shadow: 0 0 0 2px var(--success-dim); }
  .hm-pill.ok { color: var(--success); }
  .hm-pill.ok .hm-dot { background: var(--success); box-shadow: 0 0 0 2px var(--success-dim); }
  .hm-pill.warn { color: var(--warning); }
  .hm-pill.warn .hm-dot { background: var(--warning); }

  /* ── LAYOUT ── */
  .discord-layout {
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(360px, 440px);
    gap: 18px;
    align-items: start;
  }
  @media (max-width: 960px) {
    .discord-layout { grid-template-columns: 1fr; }
    .stage { grid-row: 1; position: static; }
    .control-card { grid-row: 2; }
  }

  /* ── CONTROL CARD (flat, Dashboard-aligned) ── */
  .control-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    transition: border-color 0.18s var(--ease);
  }
  .control-card:hover { border-color: var(--border-hover); }

  .cc-toggle-row {
    padding: 20px 22px;
    border-bottom: 1px solid var(--border);
  }

  .big-toggle {
    display: inline-flex;
    align-items: center;
    gap: 16px;
    cursor: pointer;
    width: 100%;
  }
  .big-toggle input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .toggle-track {
    position: relative;
    width: 46px;
    height: 26px;
    border-radius: var(--radius-full);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    transition: background 0.2s var(--ease), border-color 0.2s var(--ease);
    flex-shrink: 0;
  }
  .toggle-thumb {
    position: absolute;
    top: 50%;
    left: 2px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--text-muted);
    transform: translateY(-50%);
    transition: left 0.22s var(--spring), background 0.2s var(--ease);
    box-shadow: 0 1px 2px rgba(0, 0, 0, 0.45);
  }
  .big-toggle input:checked ~ .toggle-track {
    background: color-mix(in srgb, var(--success) 30%, var(--bg-elevated));
    border-color: color-mix(in srgb, var(--success) 50%, var(--border));
  }
  .big-toggle input:checked ~ .toggle-track .toggle-thumb {
    left: 22px;
    background: var(--success);
    box-shadow: 0 0 10px var(--success-glow), 0 1px 2px rgba(0, 0, 0, 0.45);
  }
  .bt-text { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
  .bt-title {
    font-size: var(--fs-lg);
    font-weight: 700;
    color: var(--text-primary);
    letter-spacing: var(--letter-tight);
  }
  .bt-sub {
    font-size: var(--fs-sm);
    color: var(--text-muted);
    line-height: var(--lh-snug);
  }

  .cc-section { border-bottom: 1px solid var(--border); }
  .cc-section:last-child { border-bottom: none; }

  .cc-section-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    padding: 16px 22px 14px;
  }
  .cc-section-text { min-width: 0; }
  .cc-section-title {
    font-size: var(--fs-md);
    font-weight: 600;
    letter-spacing: var(--letter-tight);
    color: var(--text-primary);
    margin: 0 0 2px;
  }
  .cc-section-desc {
    font-size: var(--fs-sm);
    color: var(--text-muted);
    line-height: var(--lh-snug);
    margin: 0;
  }

  .cc-section-fields .cc-section-head { padding-bottom: 10px; }

  .field-count {
    display: inline-flex;
    align-items: baseline;
    font-variant-numeric: tabular-nums;
    font-family: var(--font-mono);
    letter-spacing: var(--letter-tight);
  }
  .fc-num { font-size: 22px; font-weight: 700; color: var(--text-primary); }
  .fc-den { font-size: 13px; color: var(--text-muted); margin-left: 1px; }

  /* ── preset segmented control ── */
  .preset-seg {
    display: inline-flex;
    padding: 3px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    gap: 2px;
    height: 30px;
    flex-shrink: 0;
  }
  .preset-opt {
    padding: 0 14px;
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--text-muted);
    background: transparent;
    border-radius: 5px;
    transition: background 0.15s var(--ease), color 0.15s var(--ease);
    display: inline-flex;
    align-items: center;
    line-height: 1;
  }
  .preset-opt:hover { color: var(--text-secondary); }
  .preset-opt.active {
    background: var(--bg-card-hover);
    color: var(--text-primary);
    box-shadow: var(--shadow-xs);
  }

  /* ── Fields grid ── */
  .field-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0;
    border-top: 1px solid var(--border);
  }
  @media (max-width: 620px) {
    .field-grid { grid-template-columns: 1fr; }
  }
  .field-cell {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 14px 22px;
    border-top: 1px solid var(--border);
    border-left: 1px solid var(--border);
    min-height: 64px;
    transition: background 0.15s var(--ease);
  }
  .field-cell:hover { background: var(--bg-card-hover); }
  .field-cell:nth-child(-n+2) { border-top: none; }
  .field-cell:nth-child(2n+1) { border-left: none; }
  @media (max-width: 620px) {
    .field-cell { border-left: none !important; border-top: 1px solid var(--border) !important; }
    .field-cell:first-child { border-top: none !important; }
  }
  .field-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .field-label {
    font-size: var(--fs-base);
    font-weight: 500;
    color: var(--text-primary);
    letter-spacing: var(--letter-tight);
  }
  .field-hint {
    font-size: var(--fs-sm);
    color: var(--text-muted);
    line-height: var(--lh-snug);
  }
  .field-order {
    display: inline-flex;
    gap: 3px;
    margin-left: auto;
  }
  .field-order button {
    width: 30px;
    height: 30px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    background: var(--bg-input);
  }
  .field-order button:hover:not(:disabled) { border-color: var(--border-hover); color: var(--text-primary); }
  .field-order button:disabled { opacity: 0.35; cursor: default; }

  /* ── STAGE ── */
  .stage {
    display: flex;
    flex-direction: column;
    gap: 10px;
    position: sticky;
    top: 0;
  }
  .stage-label {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 0 4px 2px;
  }
  .sl-text {
    font-family: var(--font-mono);
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: var(--letter-wider);
    color: var(--text-muted);
  }
  .sl-meta {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-secondary);
    letter-spacing: 0.02em;
  }
  .sl-preset { color: var(--text-primary); }
  .sl-div { color: var(--border-strong); }
  .sl-count { font-family: var(--font-mono); color: var(--text-secondary); }
  .sl-dot {
    width: 7px; height: 7px; border-radius: 50%;
    background: var(--text-muted);
    transition: background 0.2s var(--ease), box-shadow 0.2s var(--ease);
    margin-left: 4px;
  }
  .sl-dot.on {
    background: var(--success);
    box-shadow: 0 0 0 3px var(--success-dim), 0 0 10px var(--success-glow);
  }

  /* ── Discord mock card — premium, Discord-faithful, editorial rhythm ── */
  .dp-profile {
    position: relative;
    background: var(--preview-bg);
    border: 1px solid var(--preview-border);
    border-radius: 14px;
    overflow: hidden;
    box-shadow: var(--preview-shadow);
  }
  .dp-body { padding: 0 0 18px; color: var(--preview-text); }

  .dp-banner {
    height: 68px;
    background-size: cover;
    background-position: center;
    position: relative;
  }
  .dp-banner::after {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(180deg, transparent 55%, rgba(0, 0, 0, 0.18) 100%);
    pointer-events: none;
  }
  .dp-banner-default {
    background:
      radial-gradient(120% 140% at 15% 0%, color-mix(in srgb, var(--provider-accent) 22%, transparent) 0%, transparent 62%),
      linear-gradient(135deg, color-mix(in srgb, var(--provider-accent) 14%, var(--preview-bg)) 0%, var(--preview-bg) 70%);
  }

  .dp-avatar-ring {
    position: relative;
    width: 80px;
    height: 80px;
    margin: -40px 0 0 18px;
  }
  .dp-avatar {
    width: 80px;
    height: 80px;
    border-radius: 50%;
    background: var(--preview-surface);
    border: 6px solid var(--preview-bg);
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
  }
  .dp-avatar img { width: 100%; height: 100%; object-fit: cover; }

  .dp-status-dot {
    position: absolute;
    bottom: 2px;
    right: 2px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--success);
    border: 5px solid var(--preview-bg);
    transition: background 0.2s var(--ease);
  }
  .dp-status-dot.offline { background: var(--preview-faint); }

  .dp-username {
    padding: 10px 18px 0;
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.015em;
    color: var(--preview-text);
    line-height: 1.2;
  }
  .dp-tag { font-size: 14px; color: var(--preview-muted); font-weight: 500; margin-left: 4px; letter-spacing: 0; }

  .dp-separator {
    margin: 14px 18px 12px;
    height: 1px;
    background: var(--preview-border);
  }

  .dp-section-title {
    padding: 0 18px 10px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--preview-muted);
  }

  .dp-activity-card {
    margin: 0 14px;
    background: var(--preview-surface);
    border: 1px solid var(--preview-border);
    border-radius: 8px;
    padding: 14px;
  }
  .dp-activity-header {
    font-size: 10.5px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--preview-muted);
    margin-bottom: 10px;
  }
  .dp-activity-body { display: flex; gap: 14px; align-items: flex-start; }
  .dp-activity-art {
    position: relative;
    width: 60px;
    height: 60px;
    flex-shrink: 0;
  }
  .dp-art-large {
    width: 60px;
    height: 60px;
    border-radius: 10px;
    object-fit: cover;
    background: var(--preview-bg);
    box-shadow: var(--shadow-sm);
    -webkit-user-drag: none;
    user-select: none;
  }
  .dp-art-small {
    position: absolute;
    right: -5px;
    bottom: -5px;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    object-fit: cover;
    background: var(--preview-surface);
    border: 2.5px solid var(--preview-surface);
    box-shadow: var(--shadow-sm);
    -webkit-user-drag: none;
    user-select: none;
  }
  .dp-activity-info {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    padding-top: 1px;
    flex: 1;
  }
  .dp-activity-name {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.005em;
    color: var(--preview-text);
    line-height: 1.2;
  }
  .dp-activity-details,
  .dp-activity-state {
    font-size: 12.5px;
    color: var(--preview-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.35;
  }
  .dp-activity-elapsed {
    font-size: 11.5px;
    color: var(--preview-faint);
    margin-top: 4px;
    font-variant-numeric: tabular-nums;
  }

  @media (max-width: 760px) {
    .view-header { gap: 12px; }
    .header-meta { width: 100%; }
    .cc-section-head { align-items: flex-start; flex-direction: column; padding: 14px; }
    .cc-toggle-row { padding-inline: 14px; }
    .field-grid { grid-template-columns: 1fr; }
    .field-cell { border-left: none !important; border-top: 1px solid var(--border) !important; padding: 12px 14px; }
    .field-cell:first-child { border-top: none !important; }
    .field-order button { width: 36px; height: 36px; }
    .dp-activity-card { margin-inline: 10px; }
  }

</style>

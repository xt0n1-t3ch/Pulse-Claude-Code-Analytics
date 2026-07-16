<script lang="ts">
  import { onMount } from "svelte";
  import { fly } from "svelte/transition";
  import { checkAppUpdate, openAppReleasePage, type AppUpdateInfo } from "../lib/api";

  const SKIP_KEY = "pulse-update-skipped-version";
  const CHECK_EVENT = "pulse:check-updates";
  const FAKE_PARAM = "fakeUpdate";
  const RELEASE_TAG_BASE =
    "https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/tag/";

  let info = $state<AppUpdateInfo | null>(null);
  let visible = $state(false);
  let notesOpen = $state(false);
  let opening = $state(false);

  function skippedVersion(): string | null {
    return localStorage.getItem(SKIP_KEY);
  }

  function fakeVersionParam(): string | null {
    if (typeof window === "undefined") return null;
    const value = new URLSearchParams(window.location.search).get(FAKE_PARAM);
    return value && value.trim() ? value.trim() : null;
  }

  function synthFakeUpdate(version: string): AppUpdateInfo {
    const tag = version.replace(/^v/i, "");
    return {
      current_version: "dev",
      latest_version: version,
      update_available: true,
      release_name: `Pulse ${version}`,
      release_notes: "Simulated update from the local fakeUpdate lane.",
      release_url: `${RELEASE_TAG_BASE}v${tag}`,
      published_at: null,
      checked_at: new Date().toISOString(),
      assets: [],
    };
  }

  async function fetchUpdate(): Promise<AppUpdateInfo | null> {
    const fake = fakeVersionParam();
    if (fake) return synthFakeUpdate(fake);
    try {
      return await checkAppUpdate();
    } catch {
      return null;
    }
  }

  async function runCheck(force: boolean): Promise<void> {
    const next = await fetchUpdate();
    if (!next || !next.update_available || !next.latest_version) return;
    if (!force && next.latest_version === skippedVersion()) return;
    info = next;
    notesOpen = false;
    visible = true;
  }

  function later(): void {
    visible = false;
  }

  function skipVersion(): void {
    if (info?.latest_version) {
      localStorage.setItem(SKIP_KEY, info.latest_version);
    }
    visible = false;
  }

  async function openRelease(): Promise<void> {
    if (!info) return;
    opening = true;
    try {
      await openAppReleasePage(info.release_url);
    } catch {
    } finally {
      opening = false;
    }
  }

  onMount(() => {
    void runCheck(false);
    const onForce = (): void => {
      runCheck(true);
    };
    window.addEventListener(CHECK_EVENT, onForce);
    return () => {
      window.removeEventListener(CHECK_EVENT, onForce);
    };
  });
</script>

{#if visible && info}
  <aside
    class="update-pop"
    role="status"
    aria-live="polite"
    aria-label="Application update available"
    in:fly={{ y: 16, duration: 260 }}
  >
    <div class="update-head">
      <span class="update-dot" aria-hidden="true"></span>
      <span class="update-title">Update available</span>
      <span class="update-ver mono">{info.current_version} → {info.latest_version}</span>
    </div>

    {#if info.release_name}
      <div class="update-name truncate">{info.release_name}</div>
    {/if}

    {#if info.release_notes}
      <button
        type="button"
        class="update-notes-toggle"
        aria-expanded={notesOpen}
        onclick={() => (notesOpen = !notesOpen)}
      >
        {notesOpen ? "Hide release notes" : "Release notes"}
      </button>
      {#if notesOpen}
        <pre class="update-notes">{info.release_notes}</pre>
      {/if}
    {/if}

    <div class="update-actions">
      <button type="button" class="btn btn-ghost" onclick={later} aria-label="Dismiss this update for now">
        Later
      </button>
      <button
        type="button"
        class="btn"
        onclick={skipVersion}
        aria-label={`Skip version ${info.latest_version}`}
      >
        Skip version
      </button>
      <button
        type="button"
        class="btn btn-primary"
        onclick={openRelease}
        disabled={opening}
        aria-label="Open the GitHub release page"
      >
        Open release
      </button>
    </div>
  </aside>
{/if}

<style>
  .update-pop {
    position: fixed;
    bottom: 20px;
    left: calc(var(--sidebar-width) + 16px);
    z-index: 9998;
    display: flex;
    flex-direction: column;
    gap: 10px;
    width: 320px;
    max-width: calc(100vw - var(--sidebar-width) - 32px);
    padding: 16px;
    background: var(--bg-card);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    backdrop-filter: blur(12px);
  }

  .update-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .update-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--success);
    box-shadow: 0 0 0 3px var(--success-dim);
    flex-shrink: 0;
  }

  .update-title {
    font-size: var(--fs-base);
    font-weight: 600;
    letter-spacing: var(--letter-tight);
    color: var(--text-primary);
  }

  .update-ver {
    margin-left: auto;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }

  .update-name {
    font-size: var(--fs-sm);
    color: var(--text-secondary);
  }

  .update-notes-toggle {
    align-self: flex-start;
    font-size: var(--fs-sm);
    font-weight: 500;
    color: var(--text-secondary);
    text-decoration: underline;
    text-underline-offset: 2px;
    transition: color 0.15s var(--ease);
  }
  .update-notes-toggle:hover {
    color: var(--text-primary);
  }

  .update-notes {
    margin: 0;
    max-height: 160px;
    overflow-y: auto;
    padding: 10px 12px;
    background: var(--bg-input);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    line-height: var(--lh-snug);
    color: var(--text-secondary);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .update-actions {
    display: flex;
    justify-content: flex-end;
    gap: 6px;
    margin-top: 2px;
  }

  @media (max-width: 560px) {
    .update-pop {
      left: 12px;
      right: 12px;
      width: auto;
      max-width: none;
    }
  }
</style>

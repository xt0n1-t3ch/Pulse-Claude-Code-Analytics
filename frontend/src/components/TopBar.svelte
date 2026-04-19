<script lang="ts">
  import { health, planInfo } from "../lib/stores";

  let { onToggleTheme }: { onToggleTheme: () => void } = $props();

  function minimize(): void {
    window.__TAURI__?.window.getCurrentWindow().minimize();
  }
  function toggleMaximize(): void {
    const win = window.__TAURI__?.window.getCurrentWindow();
    win?.isMaximized().then((m: boolean) => m ? win.unmaximize() : win.maximize());
  }
  function close(): void {
    // Fires WindowEvent::CloseRequested on the Rust side, which prevents the
    // default close, hides the window, and spawns the system-tray icon. If we
    // called hide() directly the tray handler would never run, leaving the
    // user with no way to reopen the app without killing the process.
    window.__TAURI__?.window.getCurrentWindow().close();
  }
</script>

<header class="topbar" data-tauri-drag-region>
  <h1 class="topbar-title">✳ Pulse</h1>
  <span class="topbar-sub">Claude Code Analytics</span>

  <div class="topbar-right">
    <button class="topbar-btn" title="Toggle theme" onclick={onToggleTheme}>
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="4.22" x2="19.78" y2="5.64"/>
      </svg>
    </button>

    {#if $planInfo}
      <span class="badge plan">{$planInfo.plan_name}</span>
    {/if}
    {#if $health}
      <span class="badge">v{$health.version}</span>
    {/if}

    <div class="window-controls">
      <button class="win-btn" title="Minimize" onclick={minimize}>
        <svg width="12" height="12" viewBox="0 0 12 12"><line x1="2" y1="6" x2="10" y2="6" stroke="currentColor" stroke-width="1.5"/></svg>
      </button>
      <button class="win-btn" title="Maximize" onclick={toggleMaximize}>
        <svg width="12" height="12" viewBox="0 0 12 12"><rect x="2" y="2" width="8" height="8" stroke="currentColor" stroke-width="1.5" fill="none" rx="1"/></svg>
      </button>
      <button class="win-btn win-close" title="Close" onclick={close}>
        <svg width="12" height="12" viewBox="0 0 12 12"><line x1="2" y1="2" x2="10" y2="10" stroke="currentColor" stroke-width="1.5"/><line x1="10" y1="2" x2="2" y2="10" stroke="currentColor" stroke-width="1.5"/></svg>
      </button>
    </div>
  </div>
</header>

<style>
  .topbar {
    height: var(--topbar-height);
    display: flex;
    align-items: center;
    padding: 0 16px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    gap: 10px;
    user-select: none;
    -webkit-app-region: drag;
  }

  .topbar-title {
    font-size: 15px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .topbar-sub {
    font-size: 12px;
    color: var(--text-muted);
    font-weight: 500;
  }

  .topbar-right {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
    -webkit-app-region: no-drag;
  }

  .topbar-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    transition: all 0.15s var(--ease);
  }

  .topbar-btn:hover {
    color: var(--text-primary);
    background: var(--bg-elevated);
  }

  .badge {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    background: var(--bg-elevated);
    padding: 3px 8px;
    border-radius: 99px;
    border: 1px solid var(--border);
  }

  .badge.plan {
    color: var(--accent);
    border-color: var(--accent-dim);
    background: var(--accent-dim);
  }

  .window-controls {
    display: flex;
    gap: 6px;
    margin-left: 8px;
  }

  .win-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    transition: all 0.15s var(--ease);
  }

  .win-btn:hover {
    color: var(--text-primary);
    background: var(--bg-elevated);
  }

  .win-close:hover {
    color: #fff;
    background: var(--danger);
  }
</style>

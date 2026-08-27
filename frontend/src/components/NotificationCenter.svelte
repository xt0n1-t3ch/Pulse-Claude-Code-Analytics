<script lang="ts">
  import { onMount } from "svelte";
  import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { IconBell, IconCheck, IconX } from "@tabler/icons-svelte";
  import {
    dismissNotification,
    getNotifications,
    getUnreadNotificationCount,
    markAllNotificationsRead,
    markNotificationRead,
  } from "../lib/api";
  import type { PulseNotification } from "../lib/api";
  import {
    accessSnapshot,
    currentView,
    selectedAccessSourceId,
  } from "../lib/stores";
  import { authenticatedAccessRoutes } from "../lib/access";
  import { provider, setProvider, type Provider } from "../lib/provider";

  let open = $state(false);
  let loading = $state(false);
  let mutating = $state(false);
  let errorMessage = $state<string | null>(null);
  let notifications = $state<PulseNotification[]>([]);
  let unreadCount = $state(0);
  let refreshGeneration = 0;

  function relativeTime(iso: string): string {
    const timestamp = Date.parse(iso);
    if (!Number.isFinite(timestamp)) return "Recently";
    const seconds = Math.max(0, Math.floor((Date.now() - timestamp) / 1000));
    if (seconds < 60) return "Now";
    const minutes = Math.floor(seconds / 60);
    if (minutes < 60) return `${minutes}m`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours}h`;
    return `${Math.floor(hours / 24)}d`;
  }

  function destination(notification: PulseNotification): "dashboard" | "discord" | "settings" {
    if (notification.kind === "discord_connectivity") return "discord";
    if (notification.kind === "provider_health") return "settings";
    return "dashboard";
  }

  async function refresh(): Promise<void> {
    const generation = ++refreshGeneration;
    try {
      const [nextNotifications, nextUnread] = await Promise.all([
        getNotifications(30),
        getUnreadNotificationCount(),
      ]);
      if (generation !== refreshGeneration) return;
      notifications = nextNotifications;
      unreadCount = nextUnread;
      errorMessage = null;
    } catch {
      if (generation !== refreshGeneration) return;
      // Preserve the last provider-backed snapshot. Clearing it would make a
      // transport failure look like an authoritative empty notification feed.
      errorMessage = "Notifications could not be refreshed.";
    }
  }

  async function openPanel(): Promise<void> {
    open = true;
    loading = true;
    await refresh();
    loading = false;
  }

  async function toggle(): Promise<void> {
    if (open) {
      open = false;
      return;
    }
    await openPanel();
  }

  async function openNotification(notification: PulseNotification): Promise<void> {
    // Navigation is the primary action. A persistence failure must not strand
    // the user inside the notification center.
    if (
      notification.kind === "quota_threshold"
      || notification.kind === "quota_reset"
      || notification.kind === "provider_health"
    ) {
      const matchingRoute = authenticatedAccessRoutes($accessSnapshot?.routes ?? [])
        .find((route) => route.source.provider === notification.provider);
      if (!matchingRoute) {
        errorMessage = "The notification source is no longer available.";
        return;
      }
      const nextProvider: Provider | null = matchingRoute.source.kind === "claude_subscription"
        ? "claude"
        : matchingRoute.source.kind === "codex_subscription"
          ? "codex"
          : null;
      if (nextProvider && nextProvider !== $provider) {
        try {
          await setProvider(nextProvider);
        } catch {
          errorMessage = "Pulse could not switch to the notification provider.";
          return;
        }
      }
      selectedAccessSourceId.set(matchingRoute.source.id);
    }
    currentView.set(destination(notification));
    open = false;
    if (notification.read_at) return;
    try {
      await markNotificationRead(notification.id);
      void emit("pulse://notification-state-changed").catch(() => undefined);
      await refresh();
    } catch {
      errorMessage = "The notification could not be marked as read.";
    }
  }

  async function markAllRead(): Promise<void> {
    if (mutating) return;
    mutating = true;
    try {
      await markAllNotificationsRead();
      void emit("pulse://notification-state-changed").catch(() => undefined);
      await refresh();
    } catch {
      errorMessage = "Notifications could not be marked as read.";
    } finally {
      mutating = false;
    }
  }

  async function dismiss(event: MouseEvent, notification: PulseNotification): Promise<void> {
    event.stopPropagation();
    if (mutating) return;
    mutating = true;
    try {
      await dismissNotification(notification.id);
      void emit("pulse://notification-state-changed").catch(() => undefined);
      await refresh();
    } catch {
      errorMessage = "The notification could not be dismissed.";
    } finally {
      mutating = false;
    }
  }

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];
    let destroyed = false;
    const interval = window.setInterval(() => void refresh(), 5_000);
    void listen("pulse://notification", () => void refresh())
      .then((stop) => {
        if (destroyed) stop();
        else unlisteners.push(stop);
      })
      .catch(() => undefined);
    void listen("pulse://open-notifications", () => void openPanel())
      .then((stop) => {
        if (destroyed) stop();
        else unlisteners.push(stop);
      })
      .catch(() => undefined);
    void refresh();
    return () => {
      destroyed = true;
      window.clearInterval(interval);
      for (const unlisten of unlisteners) unlisten();
    };
  });
</script>

<div class="notification-center">
  <button
    class="notification-trigger"
    class:active={open}
    title="Notifications"
    aria-label={unreadCount > 0 ? `Notifications, ${unreadCount} unread` : "Notifications"}
    aria-expanded={open}
    aria-haspopup="dialog"
    onclick={toggle}
  >
    <IconBell size={17} stroke={1.7} />
    {#if unreadCount > 0}
      <span class="unread-badge">{unreadCount > 99 ? "99+" : unreadCount}</span>
    {/if}
  </button>

  {#if open}
    <div class="notification-panel" role="dialog" aria-label="Notification center">
      <header>
        <div>
          <span class="panel-kicker">Recent events</span>
          <h2>Notifications</h2>
        </div>
        {#if unreadCount > 0}
          <button class="mark-all" onclick={markAllRead}>
            <IconCheck size={14} stroke={1.8} />
            {mutating ? "Saving..." : "Mark read"}
          </button>
        {/if}
      </header>

      <div class="notification-list" aria-live="polite">
        {#if errorMessage}
          <div class="sync-error" role="alert">
            <span>{errorMessage}</span>
            <button type="button" onclick={() => void refresh()}>Retry</button>
          </div>
        {/if}
        {#if loading}
          <div class="empty-state">Loading events...</div>
        {:else if notifications.length === 0}
          <div class="empty-state">
            <strong>All clear</strong>
            <span>Quota, provider health, and Discord alerts appear here.</span>
          </div>
        {:else}
          {#each notifications as notification (notification.id)}
            <div class="notification-row" class:unread={!notification.read_at}>
              <button class="event-action" onclick={() => openNotification(notification)}>
                <span class="event-mark" data-kind={notification.kind}></span>
                <span class="event-copy">
                  <span class="event-title">{notification.title}</span>
                  <span class="event-body">{notification.body}</span>
                  <span class="event-meta">
                    {notification.provider ?? "Pulse"} · {relativeTime(notification.created_at)}
                    {#if notification.action} · {notification.action}{/if}
                  </span>
                </span>
              </button>
              <button
                class="dismiss"
                aria-label={`Dismiss ${notification.title}`}
                disabled={mutating}
                onclick={(event) => dismiss(event, notification)}
              >
                <IconX size={14} stroke={1.7} />
              </button>
            </div>
          {/each}
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .notification-center {
    position: relative;
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    flex: 0 0 30px;
    -webkit-app-region: no-drag;
  }
  .notification-trigger {
    position: relative;
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    color: var(--text-muted);
    border-radius: var(--radius-md);
    transition: color 140ms var(--ease), background 140ms var(--ease);
  }
  .notification-trigger:hover { color: var(--text-primary); background: var(--bg-elevated); }
  .notification-trigger.active { color: var(--text-primary); background: var(--bg-elevated); }
  .unread-badge {
    position: absolute;
    top: 4px;
    right: 3px;
    min-width: 14px;
    height: 14px;
    padding: 0 3px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: white;
    background: var(--danger);
    border: 2px solid var(--bg-primary);
    border-radius: var(--radius-full);
    font-size: 8px;
    font-weight: 800;
    line-height: 1;
  }
  .notification-panel {
    position: fixed;
    z-index: 120;
    top: calc(var(--topbar-height) + 8px);
    right: 12px;
    width: min(390px, calc(100vw - 24px));
    overflow: hidden;
    color: var(--text-primary);
    background: color-mix(in srgb, var(--bg-card) 96%, transparent);
    border: 1px solid var(--border-hover);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-xl);
    backdrop-filter: blur(18px);
  }

  @media (max-width: 520px) {
    .notification-panel {
      right: 8px;
      width: calc(100vw - 16px);
    }
  }
  .notification-panel header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 18px;
    padding: 16px;
    border-bottom: 1px solid var(--divider);
  }
  .panel-kicker {
    color: var(--text-muted);
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  h2 { margin-top: 3px; font-size: 16px; letter-spacing: -0.02em; }
  .mark-all {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 6px 8px;
    color: var(--info);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-size: 10px;
  }
  .mark-all:hover { background: var(--info-dim); border-color: var(--info); }
  .notification-list { max-height: min(500px, calc(100vh - 120px)); overflow-y: auto; }
  .sync-error {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 14px;
    color: var(--danger);
    background: var(--danger-dim);
    border-bottom: 1px solid color-mix(in srgb, var(--danger) 32%, var(--divider));
    font-size: 10px;
  }
  .sync-error button {
    flex: 0 0 auto;
    padding: 4px 7px;
    color: inherit;
    border: 1px solid currentColor;
    border-radius: var(--radius-sm);
    font-size: 9px;
    font-weight: 700;
  }
  .notification-row {
    width: 100%;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 24px;
    gap: 6px;
    padding: 0 8px 0 0;
    color: var(--text-secondary);
    border-bottom: 1px solid var(--divider);
  }
  .notification-row:hover { background: var(--bg-elevated); }
  .notification-row.unread { color: var(--text-primary); background: color-mix(in srgb, var(--info-dim) 40%, transparent); }
  .event-action {
    min-width: 0;
    display: grid;
    grid-template-columns: 8px minmax(0, 1fr);
    gap: 10px;
    padding: 13px 8px 13px 16px;
    color: inherit;
    text-align: left;
  }
  .event-mark { width: 7px; height: 7px; margin-top: 5px; background: var(--text-muted); border-radius: 50%; }
  .event-mark[data-kind="quota_threshold"] { background: var(--warning); }
  .event-mark[data-kind="quota_reset"] { background: var(--success); }
  .event-mark[data-kind="provider_health"],
  .event-mark[data-kind="discord_connectivity"] { background: var(--info); }
  .event-copy { min-width: 0; display: flex; flex-direction: column; gap: 4px; }
  .event-title { overflow: hidden; font-size: 12px; font-weight: 700; text-overflow: ellipsis; white-space: nowrap; }
  .event-body { color: var(--text-secondary); font-size: 11px; line-height: 1.45; }
  .event-meta { color: var(--text-muted); font-family: var(--font-mono); font-size: 9px; }
  .dismiss {
    width: 24px;
    height: 24px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    border-radius: var(--radius-sm);
  }
  .dismiss:hover { color: var(--text-primary); background: var(--bg-secondary); }
  .dismiss:disabled { opacity: 0.45; cursor: wait; }
  .empty-state { min-height: 150px; padding: 38px 26px; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 7px; color: var(--text-muted); font-size: 11px; text-align: center; }
  .empty-state strong { color: var(--text-secondary); font-size: 13px; }
</style>

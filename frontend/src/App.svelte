<script lang="ts">
  import { onMount, type Component } from "svelte";
  import TopBar from "./components/TopBar.svelte";
  import AccessSourceBar from "./components/AccessSourceBar.svelte";
  import Toast from "./components/Toast.svelte";
  import UpdateBanner from "./components/UpdateBanner.svelte";
  import {
    currentView,
    invalidateLiveSnapshotForProviderChange,
    loadDiscordUser,
    poll,
    startSnapshotSync,
    stopSnapshotSync,
  } from "./lib/stores";
  import { providerRevision } from "./lib/provider";
  import { fly } from "svelte/transition";
  import { setTheme } from "@tauri-apps/api/app";
  import {
    initialView,
    loadView,
    normalizeViewId,
    type ViewId,
  } from "./lib/view-router";

  let activeViewId = $derived(normalizeViewId($currentView));
  let ActiveView = $state<Component<any> | null>(initialView);
  let viewLoading = $state(false);
  let viewLoadError = $state<string | null>(null);
  let viewRequest = 0;

  function resolveView(viewId: ViewId): void {
    const request = ++viewRequest;
    viewLoadError = null;
    if (viewId === "dashboard") {
      ActiveView = initialView;
      viewLoading = false;
      return;
    }
    ActiveView = null;
    viewLoading = true;
    void loadView(viewId)
      .then((component) => {
        if (request !== viewRequest || viewId !== activeViewId) return;
        ActiveView = component;
      })
      .catch((error: unknown) => {
        if (request !== viewRequest || viewId !== activeViewId) return;
        viewLoadError = error instanceof Error
          ? `This view could not be loaded. ${error.message}`
          : "This view could not be loaded.";
      })
      .finally(() => {
        if (request === viewRequest && viewId === activeViewId) viewLoading = false;
      });
  }

  $effect(() => resolveView(activeViewId));

  const initialTheme: "dark" | "light" =
    localStorage.getItem("pulse-theme") === "light" ? "light" : "dark";
  let theme = $state<"dark" | "light">(initialTheme);

  function applyTheme(next: "dark" | "light"): void {
    theme = next;
    document.documentElement.setAttribute("data-theme", next);
    document.documentElement.style.colorScheme = next;
    localStorage.setItem("pulse-theme", next);
    void setTheme(next).catch(() => undefined);
  }

  applyTheme(initialTheme);

  function toggleTheme(): void {
    applyTheme(theme === "dark" ? "light" : "dark");
  }

  onMount(() => {
    applyTheme(theme);
    startSnapshotSync();
    loadDiscordUser();
    let firstProviderRevision = true;
    const unsubscribeProviderRevision = providerRevision.subscribe(() => {
      if (firstProviderRevision) {
        firstProviderRevision = false;
        return;
      }
      invalidateLiveSnapshotForProviderChange();
      void poll();
    });
    return () => {
      unsubscribeProviderRevision();
      stopSnapshotSync();
    };
  });
</script>

<div class="main-wrapper">
  <TopBar onToggleTheme={toggleTheme} />
  <AccessSourceBar />
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <main class="main-content" tabindex="0" aria-label="Pulse workspace">
    {#key activeViewId}
      <div class="view-host" in:fly={{ y: 4, duration: 80 }}>
        {#if ActiveView && activeViewId === "settings"}
          <ActiveView onToggleTheme={toggleTheme} currentTheme={theme} />
        {:else if ActiveView}
          <ActiveView />
        {:else if viewLoadError}
          <div class="view-load-state error state-panel" role="alert">
            <strong>View unavailable</strong>
            <span>{viewLoadError}</span>
            <button type="button" onclick={() => resolveView(activeViewId)}>Retry</button>
          </div>
        {:else if viewLoading}
          <div class="view-load-state state-panel" role="status" aria-live="polite">
            <strong>Loading workspace</strong>
            <span>Preparing the selected view.</span>
          </div>
        {/if}
      </div>
    {/key}
  </main>
</div>
<Toast />
<UpdateBanner />

<style>
  .main-wrapper {
    height: 100vh;
    height: 100dvh;
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }

  .main-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: var(--page-padding-block) var(--page-padding-inline);
    overflow-y: auto;
    overflow-x: hidden;
    min-width: 0;
    scrollbar-gutter: stable;
  }

  .view-host {
    flex: 1 0 auto;
    width: 100%;
    min-width: 0;
    min-height: 100%;
  }

  .view-load-state {
    min-height: 180px;
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 6px;
    color: var(--text-muted);
    text-align: center;
  }

  .view-load-state strong { color: var(--text-primary); }
  .view-load-state span { max-width: 52ch; font-size: var(--fs-sm); }
  .view-load-state.error strong { color: var(--danger); }
  .view-load-state button {
    margin-top: 6px;
    padding: 6px 12px;
    color: var(--text-primary);
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
</style>

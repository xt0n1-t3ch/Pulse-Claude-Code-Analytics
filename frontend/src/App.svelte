<script lang="ts">
  import { onMount } from "svelte";
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

  const viewLoaders = {
    dashboard: () => import("./views/Dashboard.svelte"),
    sessions: () => import("./views/Sessions.svelte"),
    context: () => import("./views/Context.svelte"),
    costs: () => import("./views/Costs.svelte"),
    reports: () => import("./views/Reports.svelte"),
    discord: () => import("./views/Discord.svelte"),
    settings: () => import("./views/Settings.svelte"),
  } as const;
  type ViewId = keyof typeof viewLoaders;
  let ActiveView = $state<any>(null);
  let activeViewId = $state<ViewId>("dashboard");
  let viewLoadRevision = 0;

  $effect(() => {
    const requested = ($currentView in viewLoaders ? $currentView : "dashboard") as ViewId;
    const revision = ++viewLoadRevision;
    activeViewId = requested;
    ActiveView = null;
    void viewLoaders[requested]().then((module) => {
      if (revision === viewLoadRevision) ActiveView = module.default;
    });
  });

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
  <main class="main-content">
    {#if ActiveView}
      {#key activeViewId}
        <div class="view-host" in:fly={{ y: 8, duration: 200 }}>
          {#if activeViewId === "settings"}
            <ActiveView onToggleTheme={toggleTheme} currentTheme={theme} />
          {:else}
            <ActiveView />
          {/if}
        </div>
      {/key}
    {:else}
      <div class="view-loading" aria-live="polite">Loading view…</div>
    {/if}
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
  .view-loading { color: var(--text-muted); padding: 24px 4px; font-size: var(--fs-sm); }
</style>

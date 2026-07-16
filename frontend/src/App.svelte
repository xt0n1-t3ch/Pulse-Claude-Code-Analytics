<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import TopBar from "./components/TopBar.svelte";
  import Toast from "./components/Toast.svelte";
  import UpdateBanner from "./components/UpdateBanner.svelte";
  import { currentView, startSnapshotSync, stopSnapshotSync, loadDiscordUser, poll, health, metrics, sessions, rateLimits, planInfo } from "./lib/stores";
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
      health.set(null);
      metrics.set(null);
      sessions.set([]);
      rateLimits.set(null);
      planInfo.set(null);
      void poll();
    });
    return () => {
      unsubscribeProviderRevision();
      stopSnapshotSync();
    };
  });
</script>

<Sidebar />
<div class="main-wrapper">
  <TopBar onToggleTheme={toggleTheme} />
  <main class="main-content">
    {#if ActiveView}
      {#key activeViewId}
        <div in:fly={{ y: 8, duration: 200 }}>
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
    margin-left: var(--sidebar-width);
    min-height: 100vh;
    display: flex;
    flex-direction: column;
    min-width: 0;
    overflow: hidden;
  }

  .main-content {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
    overflow-x: hidden;
    max-height: calc(100vh - var(--topbar-height));
    min-width: 0;
  }

  .view-loading { color: var(--text-muted); padding: 24px 4px; font-size: var(--fs-sm); }

  @media (max-width: 800px) {
    .main-content { padding: 14px; }
  }

  @media (max-width: 740px) {
    .main-content { padding: 10px; }
  }
</style>

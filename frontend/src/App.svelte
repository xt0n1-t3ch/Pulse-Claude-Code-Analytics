<script lang="ts">
  import { onMount } from "svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import TopBar from "./components/TopBar.svelte";
  import Toast from "./components/Toast.svelte";
  import Dashboard from "./views/Dashboard.svelte";
  import Sessions from "./views/Sessions.svelte";
  import Context from "./views/Context.svelte";
  import Costs from "./views/Costs.svelte";
  import Reports from "./views/Reports.svelte";
  import Discord from "./views/Discord.svelte";
  import Settings from "./views/Settings.svelte";
  import { currentView, startPolling, stopPolling, loadDiscordUser } from "./lib/stores";
  import { fly } from "svelte/transition";

  let theme = $state(localStorage.getItem("pulse-theme") || "dark");

  function toggleTheme(): void {
    theme = theme === "dark" ? "light" : "dark";
    document.documentElement.setAttribute("data-theme", theme);
    localStorage.setItem("pulse-theme", theme);
  }

  onMount(() => {
    if (theme !== "dark") {
      document.documentElement.setAttribute("data-theme", theme);
    }
    startPolling(5000);
    loadDiscordUser();
    return stopPolling;
  });
</script>

<Sidebar />
<div class="main-wrapper">
  <TopBar onToggleTheme={toggleTheme} />
  <main class="main-content">
    {#if $currentView === "dashboard"}
      <div in:fly={{ y: 8, duration: 200 }}>
        <Dashboard />
      </div>
    {:else if $currentView === "sessions"}
      <div in:fly={{ y: 8, duration: 200 }}>
        <Sessions />
      </div>
    {:else if $currentView === "context"}
      <div in:fly={{ y: 8, duration: 200 }}>
        <Context />
      </div>
    {:else if $currentView === "costs"}
      <div in:fly={{ y: 8, duration: 200 }}>
        <Costs />
      </div>
    {:else if $currentView === "reports"}
      <div in:fly={{ y: 8, duration: 200 }}>
        <Reports />
      </div>
    {:else if $currentView === "discord"}
      <div in:fly={{ y: 8, duration: 200 }}>
        <Discord />
      </div>
    {:else if $currentView === "settings"}
      <div in:fly={{ y: 8, duration: 200 }}>
        <Settings onToggleTheme={toggleTheme} currentTheme={theme} />
      </div>
    {/if}
  </main>
</div>
<Toast />

<style>
  .main-wrapper {
    margin-left: var(--sidebar-width);
    min-height: 100vh;
    display: flex;
    flex-direction: column;
  }

  .main-content {
    flex: 1;
    padding: 20px;
    overflow-y: auto;
    max-height: calc(100vh - var(--topbar-height));
  }
</style>

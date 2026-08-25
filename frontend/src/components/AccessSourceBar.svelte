<script lang="ts">
  import codexMark from "../assets/rp/codex-app.png";
  import claudeMark from "../assets/rp/claude.svg";
  import openAiMark from "../assets/rp/chatgpt-app.jpg";
  import {
    accessSnapshot,
    backendConnection,
    currentView,
    selectedAccessSourceId,
    sourceInspectorExpanded,
    addToast,
  } from "../lib/stores";
  import {
    accessKindLabel,
    accessSourceName,
    displayableAccessRoutes,
    type AccessKind,
    type AccessRouteSnapshot,
  } from "../lib/access";
  import { provider, setProvider, type Provider } from "../lib/provider";
  import { type StatusPillState } from "./StatusPill.svelte";
  import { IconActivityHeartbeat, IconStack2 } from "@tabler/icons-svelte";

  let routes = $derived(displayableAccessRoutes($accessSnapshot?.routes ?? []));
  let diagnosticRoutes = $derived($accessSnapshot?.routes ?? []);
  let isDiscordProviderSelector = $derived($currentView === "discord");
  let visibleRoutes = $derived(
    isDiscordProviderSelector
      ? routes.filter((route) => providerFor(route.source.kind) !== null)
      : routes,
  );
  let selectorCount = $derived(
    visibleRoutes.length + (!isDiscordProviderSelector && visibleRoutes.length > 1 ? 1 : 0),
  );
  let switchingSourceId = $state<string | null>(null);

  $effect(() => {
    if (!$accessSnapshot || isDiscordProviderSelector) return;
    if ($selectedAccessSourceId === "all" && routes.length === 1) {
      void selectSource(routes[0]);
    }
    if ($selectedAccessSourceId !== "all" && !routes.some((route) => route.source.id === $selectedAccessSourceId)) {
      selectedAccessSourceId.set(routes.length > 1 ? "all" : routes[0]?.source.id ?? "all");
    }
  });

  function markFor(kind: AccessKind): string {
    if (kind === "claude_subscription" || kind === "anthropic_api") return claudeMark;
    if (kind === "open_ai_api") return openAiMark;
    return codexMark;
  }

  function isPreviewRoute(routeId: string): boolean {
    return routeId.startsWith("fixture:");
  }

  /** One place that turns a route into the shared pill vocabulary so the bar,
   *  the allowance rail, and the header all speak the same status language. */
  function sourceState(route: AccessRouteSnapshot): { state: StatusPillState; label: string } {
    if (isPreviewRoute(route.source.id)) return { state: "neutral", label: "Preview" };
    if (route.source.proof === "none") {
      if (route.local_history.available) {
        if (route.unavailable_reason === "expired") return { state: "expired", label: "Session expired" };
        return { state: "waiting", label: "Sign in required" };
      }
      return { state: "waiting", label: "Not configured" };
    }
    if (route.availability !== "available") {
      if (route.source.proof === "authenticated_probe") {
        return { state: "live", label: "Authenticated" };
      }
      return { state: "expired", label: "Unavailable" };
    }
    return route.freshness === "fresh"
      ? { state: "live", label: "Live" }
      : { state: "stale", label: "Stale" };
  }

  function providerFor(kind: AccessKind): Provider | null {
    if (kind === "claude_subscription") return "claude";
    if (kind === "codex_subscription") return "codex";
    return null;
  }

  async function selectSource(route: AccessRouteSnapshot): Promise<void> {
    const routeId = route.source.id;
    if (!isDiscordProviderSelector) {
      selectedAccessSourceId.set(routeId);
      return;
    }
    const nextProvider = providerFor(route.source.kind);
    if (!nextProvider) return;
    switchingSourceId = routeId;
    try {
      if (nextProvider !== $provider) {
        await setProvider(nextProvider);
      }
    } catch {
      addToast("Provider switch failed. Pulse kept the previous source selected.", "danger");
    } finally {
      if (switchingSourceId === routeId) switchingSourceId = null;
    }
  }

  function isSelected(route: AccessRouteSnapshot): boolean {
    if (!isDiscordProviderSelector) return $selectedAccessSourceId === route.source.id;
    return providerFor(route.source.kind) === $provider;
  }

  function inspectSourceHealth(): void {
    sourceInspectorExpanded.set(true);
    currentView.set("settings");
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        document.querySelector("[data-source-inspector]")?.scrollIntoView({
          behavior: "smooth",
          block: "nearest",
        });
      });
    });
  }

  function diagnosticNeedsAttention(route: AccessRouteSnapshot): boolean {
    if (route.source.proof !== "none") {
      if (route.source.proof === "authenticated_probe") return false;
      return route.availability !== "available" || route.freshness !== "fresh";
    }
    if (route.local_history?.available) return true;
    const error = route.error?.trim().toLowerCase() ?? "";
    return error.length > 0
      && !error.includes("not configured")
      && !error.includes("credentials are missing");
  }

  let hasPreviewRoutes = $derived(routes.some((route) => route.source.id.startsWith("fixture:")));
  let hasSourceAttention = $derived(diagnosticRoutes.some(diagnosticNeedsAttention));
  let healthState = $derived<StatusPillState>(
    $backendConnection === "disconnected"
      ? "expired"
      : hasPreviewRoutes
        ? "neutral"
        : hasSourceAttention
          ? "waiting"
          : "live",
  );
  let healthLabel = $derived(
    $backendConnection === "disconnected"
      ? "Backend offline"
      : hasPreviewRoutes
        ? "Design preview"
        : hasSourceAttention
          ? "Attention required"
          : "All sources live",
  );
</script>

<section
  class="access-bar"
  class:empty={visibleRoutes.length === 0}
  data-source-count={selectorCount}
  aria-label={isDiscordProviderSelector ? "Discord broadcast provider" : "Usage and analytics sources"}
>
  <div class="source-list">
    {#if visibleRoutes.length === 0}
      <button
        class="source-empty"
        type="button"
        aria-label="Inspect provider diagnostics"
        onclick={inspectSourceHealth}
      >
        <div>
          <strong>No authenticated usage source</strong>
          <span>Provider proof unavailable · open diagnostics</span>
        </div>
      </button>
    {:else}
      {#each visibleRoutes as route (route.source.id)}
        {@const label = accessKindLabel(route.source.kind)}
        {@const status = sourceState(route)}
        <button
          class="source-card"
          class:selected={isSelected(route)}
          data-access-source={route.source.id}
          data-kind={route.source.kind}
          aria-pressed={isSelected(route)}
          disabled={switchingSourceId !== null}
          onclick={() => selectSource(route)}
        >
          <img src={markFor(route.source.kind)} alt="" />
          <span class="source-copy">
            <strong>{accessSourceName(route.source)}</strong>
            {#if status.state === "expired"}
              <small class="st-danger">Session expired</small>
            {:else if status.state === "waiting"}
              <small class="st-warn">{status.label}</small>
            {:else if status.state === "stale"}
              <small>{label.access}</small>
            {:else}
              <small>{label.access}</small>
            {/if}
          </span>
          <span class="source-dot" data-state={status.state} title={status.label} aria-label={status.label}></span>
        </button>
      {/each}

      {#if !isDiscordProviderSelector && visibleRoutes.length > 1}
        <button
          class="source-card aggregate"
          class:selected={$selectedAccessSourceId === "all"}
          data-access-source="all"
          aria-pressed={$selectedAccessSourceId === "all"}
          disabled={switchingSourceId !== null}
          onclick={() => selectedAccessSourceId.set("all")}
        >
          <span class="aggregate-mark" aria-hidden="true">
            <IconStack2 size={17} stroke={1.8} />
          </span>
          <span class="source-copy">
            <strong>All providers</strong>
            <small>Cross-provider analytics</small>
          </span>
        </button>
      {/if}
    {/if}
  </div>

  {#if visibleRoutes.length > 0}
    <button
      class="health-summary"
      onclick={inspectSourceHealth}
      aria-label="Inspect source health"
      aria-describedby="source-health-status"
      title={healthLabel}
    >
      <span class="health-icon" data-state={healthState} aria-hidden="true">
        <IconActivityHeartbeat size={15} stroke={1.9} aria-hidden="true" />
        <span class="health-dot"></span>
      </span>
      <span id="source-health-status" class="sr-only">{healthLabel}</span>
    </button>
  {/if}
</section>

<style>
  .access-bar {
    flex: 0 0 auto;
    position: relative;
    min-height: 54px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 6px 52px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
  }
  .access-bar.empty {
    min-height: 46px;
    align-items: center;
    padding-block: 6px;
  }
  .access-bar.empty .source-list { width: 100%; max-width: none; }

  .source-list {
    width: 100%;
    max-width: 1100px;
    min-width: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(185px, 1fr));
    align-items: stretch;
    gap: 7px;
    overflow-x: auto;
  }
  .access-bar[data-source-count="1"] .source-list { max-width: 340px; grid-template-columns: 1fr; }
  .access-bar[data-source-count="2"] .source-list { max-width: 640px; grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .access-bar[data-source-count="3"] .source-list { max-width: 920px; grid-template-columns: repeat(3, minmax(0, 1fr)); }

  .source-card {
    width: auto;
    min-width: 0;
    min-height: 42px;
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    padding: 6px 9px;
    color: var(--text-secondary);
    background: color-mix(in srgb, var(--bg-card) 70%, transparent);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    text-align: left;
    transition: border-color 140ms var(--ease), background 140ms var(--ease);
  }

  .source-card:hover { background: var(--bg-card-hover); border-color: var(--border-hover); }
  .source-card.selected {
    color: var(--text-primary);
    background: color-mix(in srgb, var(--provider-accent) 8%, var(--bg-card));
    border-color: color-mix(in srgb, var(--provider-accent) 58%, var(--border));
  }

  .source-card img,
  .aggregate-mark {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    object-fit: contain;
  }

  .aggregate-mark {
    display: grid;
    place-items: center;
    color: var(--provider-accent);
    background: var(--provider-accent-dim);
  }

  .source-copy {
    min-width: 0;
    display: grid;
    gap: 2px;
  }

  .source-copy strong {
    overflow: hidden;
    color: inherit;
    font-size: 12px;
    font-weight: 650;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .source-copy small,
  .source-empty span {
    color: var(--text-muted);
    font-size: 11px;
    line-height: 1.25;
  }
  .source-copy small.st-danger { color: var(--danger); font-weight: 600; }
  .source-copy small.st-warn { color: var(--warning); font-weight: 600; }

  /* One small state dot instead of a loud pill on every card. */
  .source-dot {
    justify-self: end;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--text-placeholder);
    flex-shrink: 0;
  }
  .source-dot[data-state="live"] { background: var(--success); }
  .source-dot[data-state="stale"] { background: var(--text-muted); }
  .source-dot[data-state="waiting"] { background: var(--warning); }
  .source-dot[data-state="expired"] { background: var(--danger); }
  .source-dot[data-state="neutral"] { background: var(--provider-accent); }

  .source-empty {
    display: flex;
    align-items: center;
    gap: 9px;
    color: var(--success);
  }

  .health-summary {
    position: absolute;
    right: 14px;
    top: 50%;
    transform: translateY(-50%);
    display: grid;
    place-items: center;
    width: 32px;
    height: 32px;
    padding: 0;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    background: transparent;
    border: 1px solid transparent;
  }

  .health-summary:hover {
    color: var(--text-primary);
    background: var(--bg-elevated);
    border-color: var(--border);
  }

  .health-icon { position: relative; display: grid; place-items: center; }
  .health-icon[data-state="live"] { color: var(--success); }
  .health-icon[data-state="waiting"] { color: var(--warning); }
  .health-icon[data-state="expired"] { color: var(--danger); }
  .health-icon[data-state="neutral"] { color: var(--provider-accent); }
  .health-dot {
    position: absolute;
    right: -3px;
    bottom: -2px;
    width: 5px;
    height: 5px;
    border: 1px solid var(--bg-secondary);
    border-radius: 50%;
    background: currentColor;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  .source-empty div {
    display: grid;
    gap: 2px;
  }

  .source-empty strong {
    color: var(--text-secondary);
    font-size: 11px;
    font-weight: 600;
  }

  .source-empty {
    min-width: 300px;
    justify-content: flex-start;
    padding: 6px 8px;
    color: var(--text-muted);
    background: transparent;
    border: 0;
    text-align: left;
  }

  .source-empty:hover strong,
  .source-empty:focus-visible strong {
    color: var(--text-primary);
  }

  @media (max-width: 900px) {
    .access-bar { min-height: 52px; display: block; padding: 6px 10px; }
    .health-summary { display: none; }
    .source-list {
      max-width: none !important;
      display: flex;
      justify-content: flex-start;
      overflow-x: auto;
      scrollbar-width: thin;
    }
    .source-card { flex: 0 0 190px; width: 190px; }
  }

  @media (max-width: 520px) {
    .access-bar { display: block; min-height: 52px; padding-inline: 8px; }
    .source-list { width: 100%; justify-content: flex-start; }
    .source-list { gap: 7px; }
    .source-card { width: 160px; min-width: 154px; min-height: 40px; padding: 5px 8px; }
    .source-card img, .aggregate-mark { width: 22px; height: 22px; }
    .source-empty { min-width: 0; }
  }
</style>

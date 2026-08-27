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
  let switchingSourceId = $state<string | null>(null);

  $effect(() => {
    if (!$accessSnapshot) return;
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
    if (route.source.proof === "none") {
      selectedAccessSourceId.set(routeId);
      return;
    }
    const nextProvider = providerFor(route.source.kind);
    if (!nextProvider) {
      selectedAccessSourceId.set(routeId);
      return;
    }
    switchingSourceId = routeId;
    try {
      if (nextProvider !== $provider) {
        await setProvider(nextProvider);
      }
      selectedAccessSourceId.set(routeId);
    } catch {
      addToast("Provider switch failed. Pulse kept the previous source selected.", "danger");
    } finally {
      if (switchingSourceId === routeId) switchingSourceId = null;
    }
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
          ? "Needs attention"
          : "All sources live",
  );
</script>

<section class="access-bar" class:empty={routes.length === 0} aria-label="Usage and analytics sources">
  <div class="source-list">
    {#if routes.length === 0}
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
      {#each routes as route (route.source.id)}
        {@const label = accessKindLabel(route.source.kind)}
        {@const status = sourceState(route)}
        <button
          class="source-card"
          class:selected={$selectedAccessSourceId === route.source.id}
          data-access-source={route.source.id}
          data-kind={route.source.kind}
          aria-label={route.source.kind === "claude_subscription" && status.state === "waiting"
            ? `${accessSourceName(route.source)} — sign in required`
            : undefined}
          aria-pressed={$selectedAccessSourceId === route.source.id}
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
          <span class="source-dot" data-state={status.state} aria-label={route.source.kind === "claude_subscription" && status.state === "waiting" ? "Sign in state" : status.label}></span>
        </button>
      {/each}

      {#if routes.length > 1}
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

  {#if routes.length > 0}
    <button class="health-summary" onclick={inspectSourceHealth} aria-label="Inspect source health">
      <span class="hs-chip" data-state={healthState}>
        <IconActivityHeartbeat size={15} stroke={1.9} aria-hidden="true" />
        <span class="hs-label">{healthLabel}</span>
      </span>
    </button>
  {/if}
</section>

<style>
  .access-bar {
    flex: 0 0 auto;
    min-height: 76px;
    display: flex;
    align-items: stretch;
    justify-content: space-between;
    gap: 20px;
    padding: 10px 22px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
  }
  .access-bar.empty {
    min-height: 46px;
    align-items: center;
    padding-block: 6px;
  }
  .access-bar.empty .source-list { width: 100%; }

  .source-list {
    min-width: 0;
    display: flex;
    align-items: stretch;
    gap: 9px;
    overflow-x: auto;
  }

  .source-card {
    width: 244px;
    min-width: 214px;
    min-height: 54px;
    display: grid;
    grid-template-columns: 30px minmax(0, 1fr) auto;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
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
    border-color: var(--provider-accent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--provider-accent) 42%, transparent);
  }

  .source-card img,
  .aggregate-mark {
    width: 28px;
    height: 28px;
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
    font-size: 10px;
    line-height: 1.25;
  }
  .source-copy small.st-danger { color: var(--danger); font-weight: 600; }
  .source-copy small.st-warn { color: var(--warning); font-weight: 600; }

  /* One small state dot instead of a loud pill on every card. */
  .source-dot {
    justify-self: end;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--text-placeholder);
    flex-shrink: 0;
  }
  .source-dot[data-state="live"] { background: var(--success); }
  .source-dot[data-state="stale"] { background: var(--text-muted); }
  .source-dot[data-state="waiting"] { background: var(--warning); }
  .source-dot[data-state="expired"] { background: var(--danger); }
  .source-dot[data-state="neutral"] { background: var(--provider-accent); }

  .health-summary,
  .source-empty {
    display: flex;
    align-items: center;
    gap: 9px;
    color: var(--success);
  }

  .health-summary {
    justify-content: flex-end;
    padding-right: 4px;
  }

  /* One compact chip: icon + status, tinted by state. No stacked label. */
  .hs-chip {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    padding: 0 12px 0 10px;
    border-radius: var(--radius-full);
    color: var(--text-secondary);
    background: var(--surface-panel-soft);
    border: 1px solid var(--border);
    transition: border-color 140ms var(--ease), background 140ms var(--ease), color 140ms var(--ease);
  }
  .hs-label { font-size: 11px; font-weight: 650; letter-spacing: var(--letter-tight); }
  .health-summary:hover .hs-chip { border-color: var(--border-hover); background: var(--bg-elevated); }
  .hs-chip[data-state="live"] { color: var(--success); background: var(--success-dim); border-color: color-mix(in srgb, var(--success) 30%, transparent); }
  .hs-chip[data-state="waiting"] { color: var(--warning); background: var(--warning-dim); border-color: color-mix(in srgb, var(--warning) 30%, transparent); }
  .hs-chip[data-state="expired"] { color: var(--danger); background: var(--danger-dim); border-color: color-mix(in srgb, var(--danger) 30%, transparent); }

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
    .access-bar { min-height: 68px; padding: 8px 12px; }
    .health-summary { display: none; }
    .source-card { width: 176px; }
  }

  @media (max-width: 520px) {
    .access-bar { min-height: 62px; padding-inline: 8px; }
    .source-list { gap: 7px; }
    .source-card { width: 160px; min-width: 154px; min-height: 46px; padding: 7px 9px; }
    .source-card img, .aggregate-mark { width: 26px; height: 26px; }
    .source-empty { min-width: 0; }
  }
</style>

import { fireEvent, render, waitFor } from "@testing-library/svelte";
import { tick } from "svelte";
import { get } from "svelte/store";
import { beforeEach, describe, expect, it } from "vitest";
import {
  accessSnapshot,
  backendConnection,
    currentView,
    selectedAnalyticsProviderScope,
    selectedAccessSourceId,
  sourceInspectorExpanded,
} from "@/lib/stores";
import AccessSourceBar from "@/components/AccessSourceBar.svelte";
import type { AccessRouteSnapshot } from "@/lib/access";
import { provider } from "@/lib/provider";

function route(
  id: string,
  kind: AccessRouteSnapshot["source"]["kind"],
  proof: AccessRouteSnapshot["source"]["proof"] = "quota_response",
): AccessRouteSnapshot {
  return {
    source: {
      id,
      kind,
      provider: kind.includes("claude") ? "claude" : kind.includes("anthropic") ? "anthropic" : kind.includes("open_ai") ? "openai" : "codex",
      auth_method: kind.endsWith("_api") ? "api_key" : kind.startsWith("codex") ? "app_server" : "oauth",
      proof,
      plan: kind === "codex_subscription"
        ? "Pro 20x"
        : kind === "claude_subscription"
          ? "Max 20x"
          : null,
    },
    availability: "available",
    freshness: "fresh",
    provenance: "provider_api",
    observed_at: "2026-08-01T14:00:00Z",
    fetched_at: "2026-08-01T14:00:01Z",
    expires_at: "2026-08-01T14:00:31Z",
    windows: [],
    credits: null,
    extra_usage: null,
    local_history: { available: false, sessions: 0 },
    error: null,
  };
}

describe("AccessSourceBar", () => {
  beforeEach(() => {
    accessSnapshot.set(null);
    backendConnection.set("connecting");
    currentView.set("dashboard");
    provider.set("claude");
    selectedAccessSourceId.set("all");
    sourceInspectorExpanded.set(false);
  });

  it("renders authenticated subscription and API lanes separately", () => {
    accessSnapshot.set({
      routes: [
        route("codex-sub", "codex_subscription"),
        route("openai-api", "open_ai_api", "authenticated_probe"),
        route("unproved", "anthropic_api", "none"),
      ],
    });

    const { container } = render(AccessSourceBar);
    const labels = [...container.querySelectorAll("[data-access-source]")]
      .map((node) => node.textContent?.replace(/\s+/g, " ").trim());

    expect(labels).toEqual([
      expect.stringContaining("Codex"),
      expect.stringContaining("OpenAI"),
      expect.stringContaining("All providers"),
    ]);
    expect(container.textContent).toContain("Subscription");
    expect(container.textContent).toContain("API");
    expect(container.textContent).not.toContain("Anthropic");
  });

  it("treats a successful API authentication without quota counters as healthy", () => {
    const api = route("openai-api", "open_ai_api", "authenticated_probe");
    api.availability = "unavailable";
    api.freshness = "unknown";
    accessSnapshot.set({ routes: [api] });
    backendConnection.set("live");

    const { getByLabelText, getByText } = render(AccessSourceBar);

    expect(getByLabelText("Authenticated")).toBeTruthy();
    expect(getByText("All sources live")).toBeTruthy();
  });

  it("keeps the Discord provider when the only analytics source is auto-selected", async () => {
    provider.set("claude");
    accessSnapshot.set({ routes: [route("codex-sub", "codex_subscription")] });

    render(AccessSourceBar);

    await waitFor(() => {
      expect(get(selectedAccessSourceId)).toBe("codex-sub");
      expect(get(selectedAnalyticsProviderScope)).toBe("codex");
      expect(get(provider)).toBe("claude");
    });
  });

  it("changes the broadcaster only from the Discord provider selector", async () => {
    provider.set("claude");
    currentView.set("discord");
    accessSnapshot.set({ routes: [route("codex-sub", "codex_subscription")] });

    const { getByRole, queryByRole } = render(AccessSourceBar);
    await tick();
    await Promise.resolve();

    expect(queryByRole("button", { name: /All providers/ })).toBeNull();
    expect(get(provider)).toBe("claude");
    expect(get(selectedAccessSourceId)).toBe("all");
    expect(get(selectedAnalyticsProviderScope)).toBe("all");

    const codex = getByRole("button", { name: /Codex Pro 20x Subscription/ });
    expect(codex.getAttribute("aria-pressed")).toBe("false");
    await fireEvent.click(codex);

    await waitFor(() => {
      expect(get(provider)).toBe("codex");
      expect(codex.getAttribute("aria-pressed")).toBe("true");
    });
    expect(get(selectedAccessSourceId)).toBe("all");
    expect(get(selectedAnalyticsProviderScope)).toBe("all");
  });

  it("shows an honest empty state when no route has provider proof", () => {
    accessSnapshot.set({
      routes: [route("unproved", "open_ai_api", "none")],
    });

    const { getByText } = render(AccessSourceBar);
    expect(getByText("No authenticated usage source")).toBeTruthy();
  });

  it("shows expired Claude as selectable local history without claiming live proof", async () => {
    provider.set("codex");
    const claude = route("claude-sub", "claude_subscription", "none");
    claude.source.plan = null;
    claude.availability = "unavailable";
    claude.freshness = "unknown";
    claude.local_history = { available: true, sessions: 300 };
    claude.error = "token expired";
    claude.unavailable_reason = "expired";
    accessSnapshot.set({
      routes: [
        route("codex-sub", "codex_subscription"),
        claude,
      ],
    });

    const { getByRole } = render(AccessSourceBar);
    const source = getByRole("button", { name: /Claude.*Session expired/i });
    expect(source).toBeTruthy();
    expect(source.textContent).not.toContain("Live");
    await fireEvent.click(source);
    expect(get(selectedAccessSourceId)).toBe("claude-sub");
    expect(get(selectedAnalyticsProviderScope)).toBe("claude");
    expect(get(provider)).toBe("codex");
  });

  it("turns the empty source state into one compact diagnostics action", () => {
    accessSnapshot.set({
      routes: [route("unproved", "open_ai_api", "none")],
    });

    const { container, getByRole } = render(AccessSourceBar);
    expect(container.querySelector(".access-bar.empty")).not.toBeNull();
    expect(getByRole("button", { name: "Inspect provider diagnostics" })).toBeTruthy();
    expect(container.querySelectorAll(".health-summary")).toHaveLength(0);
  });

  it("changes the shared workspace filter when a source is selected", async () => {
    accessSnapshot.set({
      routes: [
        route("codex-sub", "codex_subscription"),
        route("claude-sub", "claude_subscription"),
      ],
    });

    const { getByRole } = render(AccessSourceBar);
    await fireEvent.click(getByRole("button", { name: /Claude Max 20x Subscription/ }));

    expect(get(selectedAccessSourceId)).toBe("claude-sub");
    expect(getByRole("button", { name: /Claude Max 20x Subscription/ }).getAttribute("aria-pressed")).toBe("true");
  });

  it("opens provider diagnostics in Settings instead of adding diagnostics to Home", async () => {
    accessSnapshot.set({ routes: [route("codex-sub", "codex_subscription")] });
    backendConnection.set("live");
    currentView.set("sessions");
    const { getByRole } = render(AccessSourceBar);

    await fireEvent.click(getByRole("button", { name: "Inspect source health" }));
    expect(get(sourceInspectorExpanded)).toBe(true);
    expect(get(currentView)).toBe("settings");
  });

  it("includes failed and stale diagnostic lanes in source health", () => {
    const failed = route("anthropic-api", "anthropic_api", "none");
    failed.availability = "unavailable";
    failed.freshness = "unknown";
    failed.error = "authentication failed";

    accessSnapshot.set({
      routes: [
        route("codex-sub", "codex_subscription"),
        failed,
      ],
    });
    backendConnection.set("live");

    const { getByText } = render(AccessSourceBar);
    expect(getByText("Attention required")).toBeTruthy();
  });

  it("never labels stale provider proof as live", () => {
    const stale = route("codex-sub", "codex_subscription");
    stale.freshness = "stale";
    accessSnapshot.set({ routes: [stale] });
    backendConnection.set("live");

    const { getByRole } = render(AccessSourceBar);
    const source = getByRole("button", { name: /Codex Pro 20x Subscription/ });
    expect(source.querySelector(".source-dot")?.getAttribute("data-state")).toBe("stale");
    expect(source.textContent).not.toContain("Live");
  });
});

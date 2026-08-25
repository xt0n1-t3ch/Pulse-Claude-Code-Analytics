import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { render } from "@testing-library/svelte";

describe("Signal Ledger design system", () => {
  it("owns the dark neutral palette and shared operator-console patterns centrally", () => {
    const css = readFileSync(resolve(process.cwd(), "src/styles/global.css"), "utf8");

    expect(css).toContain("--bg-primary: #000000");
    expect(css).toContain("--surface-panel: #070707");
    expect(css).toContain("--accent: #e8edf2");
    expect(css).toContain("--success: #5dbb88");
    expect(css).toContain("--warning: #d6a65c");
    expect(css).toContain("--danger: #d96b73");
    expect(css).not.toContain("--accent: #f5ad19");
    expect(css).toContain(".metric-strip");
    expect(css).toContain(".view-kicker");
    expect(css).toContain(".state-panel");
    expect(css).toContain(".surface-matte");
  });

  it("keeps provider identity scoped and uses blue for Codex", () => {
    const provider = readFileSync(resolve(process.cwd(), "src/lib/provider.ts"), "utf8");
    expect(provider).toContain('codex: {');
    expect(provider).toContain('accent: "#3b82f6"');
  });

  it("gives every primary view one concise header and one centralized matte surface language", () => {
    const views = (name: string) =>
      readFileSync(resolve(process.cwd(), `src/views/${name}.svelte`), "utf8");
    const sessions = views("Sessions");
    const context = views("Context");
    const costs = views("Costs");
    const reports = views("Reports");
    const discord = views("Discord");
    const settings = views("Settings");

    expect(sessions).not.toContain('class="view-kicker"');
    expect(sessions).toContain('<h1 class="view-title">Sessions</h1>');
    expect(sessions).toContain('<div class="stats-row metric-strip">');
    expect(sessions).not.toContain('<div class="empty-icon">✳</div>');
    expect(sessions).not.toContain("var(--panel-sheen)");
    expect(context).toContain('<section class="context-state state-panel"');
    expect(context).toContain("Reading the active context window");
    expect(context).not.toContain("getSessionsContextUsage");
    expect(context).not.toContain("Per-session utilization");
    expect(context).not.toContain('class="view-kicker"');
    expect(context).toContain('<h1 class="view-title">Context</h1>');
    expect(costs).not.toContain('class="view-kicker"');
    expect(costs).toContain('<h1 class="view-title">Usage &amp; cost</h1>');
    expect(reports).not.toContain('class="view-kicker"');
    expect(reports).toContain('<h1 class="view-title">Reports</h1>');
    expect(reports).toContain("selected analysis window");
    expect(discord).not.toContain('class="view-kicker"');
    expect(discord).toContain('<h1 class="view-title">Broadcast</h1>');
    expect(settings).not.toContain('class="view-kicker"');
    expect(settings).toContain('<h1 class="view-title">Settings</h1>');
    expect(settings).toContain('<span class="version-chip">{$health?.version ? `v${$health.version}` : "Version unavailable"}</span>');
    expect(settings).not.toContain("<DataSourceInspector");
    expect(readFileSync(resolve(process.cwd(), "src/views/Dashboard.svelte"), "utf8"))
      .not.toContain("<DataSourceInspector");
  });

  it("shows labeled navigation at desktop widths instead of an icon-only mystery rail", async () => {
    const Sidebar = (await import("@/components/Sidebar.svelte")).default;
    const { container } = render(Sidebar);

    const labels = Array.from(container.querySelectorAll(".nav-label")).map((node) =>
      node.textContent?.trim(),
    );
    expect(labels).toEqual([
      "Dashboard",
      "Sessions",
      "Context",
      "Costs",
      "Reports",
      "Discord",
      "Settings",
    ]);
  });
});

import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { render } from "@testing-library/svelte";

describe("Signal Ledger design system", () => {
  it("fits allowance and session collections to their actual content", () => {
    const dashboard = readFileSync(resolve(process.cwd(), "src/views/Dashboard.svelte"), "utf8");
    const allowances = readFileSync(resolve(process.cwd(), "src/components/AllowanceRail.svelte"), "utf8");
    expect(dashboard).not.toContain("max-height:calc(100dvh - 160px)");
    expect(dashboard).toContain(".instance-grid { display: flex; flex-wrap: wrap; }");
    expect(allowances).toContain("height: auto");
    expect(allowances).toContain("repeat(auto-fit, minmax(min(250px, 100%), 1fr))");
  });

  it("uses a neutral theme-aware active tab and shared centered mobile headings", () => {
    const nav = readFileSync(resolve(process.cwd(), "src/components/TopBar.svelte"), "utf8");
    const css = readFileSync(resolve(process.cwd(), "src/styles/global.css"), "utf8");
    expect(nav).not.toContain("button.active::after");
    expect(nav).toContain("color: var(--text-primary); background: var(--surface-raised); border-color: var(--text-muted)");
    expect(css).toContain(".main-content .app-view > .view-header > :first-child");
  });

  it("owns the dark neutral palette and shared operator-console patterns centrally", () => {
    const css = readFileSync(resolve(process.cwd(), "src/styles/global.css"), "utf8");

    expect(css).toContain("--bg-primary: #050505");
    expect(css).toContain("--surface-panel: #070707");
    expect(css).toContain("--accent: #ffffff");
    expect(css).toContain("--success: #22c55e");
    expect(css).toContain("--warning: #fbbf24");
    expect(css).toContain("--danger: #ef4444");
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
    const costs = views("Costs");
    const reports = views("Reports");
    const discord = views("Discord");
    const settings = views("Settings");

    expect(sessions).not.toContain('class="view-kicker"');
    expect(sessions).toContain('<div class="stats-row metric-strip">');
    expect(sessions).not.toContain('<div class="empty-icon">✳</div>');
    expect(sessions).not.toContain("var(--panel-sheen)");
    expect(costs).not.toContain('class="view-kicker"');
    expect(reports).not.toContain('class="view-kicker"');
    expect(reports).toContain("{windowLabel} window");
    expect(discord).not.toContain('class="view-kicker"');
    expect(discord).toContain('<h2 class="view-title">Discord</h2>');
    expect(settings).not.toContain('class="view-kicker"');
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
      "Costs",
      "Reports",
      "Discord",
      "Settings",
    ]);
  });
});

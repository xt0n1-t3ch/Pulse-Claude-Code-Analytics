import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import type {
  DiscordDisplayPrefs,
  DiscordSettings,
  SessionInfo,
  DiscordUserInfo,
  HealthResponse,
} from "@/lib/api";

const defaultDisplayPrefs: DiscordDisplayPrefs = {
  show_project: true,
  show_branch: true,
  show_model: true,
  show_activity: true,
  show_tokens: false,
  show_cost: false,
  show_limits: true,
  show_credits: true,
  show_context: true,
  show_systems: true,
};
let discordSettings: DiscordSettings;
const setDiscordEnabled = vi.fn(async (enabled: boolean) => {
  discordSettings = { ...discordSettings, enabled };
  return discordSettings;
});
const getDiscordSettings = vi.fn(async () => discordSettings);
const setDiscordDisplayPrefs = vi.fn(async (prefs: DiscordDisplayPrefs) => {
  discordSettings = { ...discordSettings, display_prefs: prefs };
  return discordSettings;
});
const setDiscordFieldOrder = vi.fn(async (order: string[]) => {
  discordSettings = { ...discordSettings, field_order: order };
  return discordSettings;
});
const setCodexDesktopDesign = vi.fn(
  async (design: "codex_app" | "chatgpt_app") => {
    discordSettings = { ...discordSettings, desktop_design: design };
    return discordSettings;
  },
);
let discordPreviewPayload: unknown = null;
const getDiscordPreview = vi.fn(async () => discordPreviewPayload);

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    setDiscordEnabled: (enabled: boolean) => setDiscordEnabled(enabled),
    getDiscordSettings: () => getDiscordSettings(),
    setDiscordDisplayPrefs: (prefs: DiscordDisplayPrefs) => setDiscordDisplayPrefs(prefs),
    setDiscordFieldOrder: (order: string[]) => setDiscordFieldOrder(order),
    setCodexDesktopDesign: (design: "codex_app" | "chatgpt_app") =>
      setCodexDesktopDesign(design),
    getDiscordPreview: () => getDiscordPreview(),
  };
});

function makeSession(id: string, project: string): SessionInfo {
  return {
    session_id: id,
    session_name: null,
    project,
    model: "Claude Opus 4.8",
    model_id: "claude-opus-4-8",
    provider: "claude",
    context_window: "200K",
    cost: 2.5,
    tokens: 120_000,
    input_tokens: 40_000,
    output_tokens: 20_000,
    cache_write_tokens: 10_000,
    cache_read_tokens: 50_000,
    branch: "main",
    activity: "Editing stores.ts",
    activity_target: "stores.ts",
    effort: "High",
    effort_explicit: true,
    is_idle: false,
    started_at: "2026-05-28T10:00:00Z",
    duration_secs: 600,
    has_thinking: true,
    workflow_label: null,
    subagent_count: 0,
    subagents: [],
    tokens_per_sec: 42,
    input_cost: 0.8,
    output_cost: 1,
    cache_write_cost: 0.5,
    cache_read_cost: 0.2,
    speed: "standard",
    fast: false,
    service_tier: null,
    app_name: null,
  };
}

const discordUserFixture: DiscordUserInfo = {
  user_id: "123",
  username: "xt0n1",
  discriminator: "0",
  avatar_hash: "abc",
  avatar_url: "https://cdn.discordapp.com/avatars/123/abc.png",
  banner_hash: null,
  banner_url: null,
};

const healthFixture: HealthResponse = {
  version: "0.1.0",
  uptime_seconds: 120,
  discord_status: "Connected",
  discord_enabled: true,
};

describe("Discord.svelte", () => {
  beforeEach(async () => {
    setDiscordEnabled.mockClear();
    getDiscordSettings.mockClear();
    setDiscordDisplayPrefs.mockClear();
    setDiscordFieldOrder.mockClear();
    setCodexDesktopDesign.mockClear();
    discordSettings = {
      provider: "claude",
      enabled: true,
      status: "Connected",
      publisher: "pulse",
      display_prefs: { ...defaultDisplayPrefs },
      desktop_design: null,
      supports_desktop_design: false,
      supports_field_order: false,
      field_order: ["project", "branch", "model", "activity", "tokens", "cost", "quotas", "credits", "context", "systems"],
    };
    getDiscordPreview.mockClear();
    discordPreviewPayload = null;
    const { provider } = await import("@/lib/provider");
    provider.set("claude");
    const { sessions, discordUser, health, discordPreview, discordPresencePreview } = await import("@/lib/stores");
    sessions.set([makeSession("s1", "pulse")]);
    discordUser.set(discordUserFixture);
    health.set(healthFixture);
    discordPresencePreview.set(null);
    discordPreview.set({
      showProject: true,
      showBranch: true,
      showModel: true,
      showActivity: true,
      showTokens: false,
      showCost: false,
      showLimits: true,
      showCredits: true,
      showContext: true,
      showSystems: true,
    });
  });

  it("mounts and shows the live-preview profile with the active session details", async () => {
    const Discord = (await import("@/views/Discord.svelte")).default;
    const { container, getByText } = render(Discord);
    await tick();

    expect(getByText("Discord")).toBeTruthy();
    expect(container.querySelector(".dp-profile")).not.toBeNull();
    await waitFor(() => {
      expect(container.querySelector(".dp-activity-details")?.textContent).toContain("pulse");
    });
    expect(getByText("xt0n1")).toBeTruthy();
  });

  it("renders the backend Discord payload instead of recomputing branch visibility locally", async () => {
    const { sessions, discordPresencePreview } = await import("@/lib/stores");
    const session = makeSession("active1", "PropertyAlpha-Agent");
    session.branch = "feat/marketplace-addtochat-liveview-management";
    sessions.set([session]);
    discordPresencePreview.set({
      provider: "claude",
      app_name: "Claude Code",
      details: "Thinking · PropertyAlpha-Agent",
      state: "Claude Opus 4.8 · ULTRACODE · 1 agent · 161.5M tokens · $195.35",
      large_image_key: "large",
      large_text: "Claude Code",
      small_image_key: null,
      small_text: null,
      has_session: true,
      duration_secs: 19_200,
    });

    const Discord = (await import("@/views/Discord.svelte")).default;
    const { container } = render(Discord);
    await tick();

    const details = container.querySelector(".dp-activity-details")?.textContent ?? "";
    const state = container.querySelector(".dp-activity-state")?.textContent ?? "";
    expect(details).toBe("Thinking · PropertyAlpha-Agent");
    expect(details).not.toContain("feat/marketplace");
    expect(state).toContain("ULTRACODE");
    expect(state).toContain("1 agent");
  });

  it("renders ChatGPT App identity and art from the canonical backend asset key", async () => {
    const { sessions, discordPresencePreview } = await import("@/lib/stores");
    const { provider } = await import("@/lib/provider");
    const session = makeSession("codex1", "pulse");
    session.provider = "codex";
    session.app_name = "ChatGPT App";
    sessions.set([session]);
    provider.set("codex");
    discordPresencePreview.set({
      provider: "codex",
      app_name: "ChatGPT App",
      details: "Running command · pulse",
      state: "GPT-5.6 Sol · Max | Pro 20x ($200/month)",
      large_image_key: "codex-logo",
      large_text: "ChatGPT App",
      small_image_key: null,
      small_text: null,
      has_session: true,
      duration_secs: 42,
    });

    const Discord = (await import("@/views/Discord.svelte")).default;
    const { container, getAllByText } = render(Discord);
    await tick();

    expect(getAllByText("ChatGPT App").length).toBeGreaterThan(0);
    const art = container.querySelector(".dp-art-large") as HTMLImageElement;
    expect(art.getAttribute("src")).toContain("chatgpt-app");
    expect(art.getAttribute("alt")).toBe("ChatGPT App");
  });

  it("renders the field toggles and the master Rich Presence toggle", async () => {
    const Discord = (await import("@/views/Discord.svelte")).default;
    const { container, getByText } = render(Discord);
    await tick();

    expect(getByText("Rich Presence")).toBeTruthy();
    expect(container.querySelectorAll(".field-cell").length).toBe(10);
    expect(getByText("Usage quotas")).toBeTruthy();
    expect(getByText("Credits available")).toBeTruthy();
    expect(getByText("Context usage")).toBeTruthy();
    expect(container.querySelectorAll(".preset-opt").length).toBe(3);
  });

  it("hydrates privacy from Rust without writing local defaults back", async () => {
    discordSettings = {
      ...discordSettings,
      display_prefs: { ...defaultDisplayPrefs, show_branch: false },
    };
    const Discord = (await import("@/views/Discord.svelte")).default;
    const { getByText } = render(Discord);

    await waitFor(() => expect(getDiscordSettings).toHaveBeenCalledTimes(1));
    const branchRow = getByText("Git branch").closest(".field-cell");
    const branchToggle = branchRow?.querySelector("input") as HTMLInputElement;
    expect(branchToggle.checked).toBe(false);
    expect(setDiscordDisplayPrefs).not.toHaveBeenCalled();
  });

  it("persists a branch toggle and refreshes the canonical payload", async () => {
    discordPreviewPayload = {
      provider: "claude",
      app_name: "Claude Code",
      details: "Thinking · pulse",
      state: "Claude Opus 4.8",
      large_image_key: "large",
      large_text: "Claude Code",
      small_image_key: null,
      small_text: null,
      has_session: true,
      duration_secs: 30,
    };
    const Discord = (await import("@/views/Discord.svelte")).default;
    const { container, getByText } = render(Discord);
    await waitFor(() => expect(getDiscordSettings).toHaveBeenCalledTimes(1));

    const branchToggle = getByText("Git branch")
      .closest(".field-cell")
      ?.querySelector("input") as HTMLInputElement;
    await fireEvent.change(branchToggle);

    await waitFor(() => {
      expect(setDiscordDisplayPrefs).toHaveBeenCalledWith(
        expect.objectContaining({ show_branch: false }),
      );
      expect(getDiscordPreview).toHaveBeenCalled();
      expect(container.querySelector(".dp-activity-details")?.textContent).toBe(
        "Thinking · pulse",
      );
    });
  });

  it("rolls a privacy toggle back when persistence fails", async () => {
    setDiscordDisplayPrefs.mockRejectedValueOnce(new Error("disk full"));
    const Discord = (await import("@/views/Discord.svelte")).default;
    const { getByText } = render(Discord);
    await waitFor(() => expect(getDiscordSettings).toHaveBeenCalledTimes(1));

    const branchToggle = getByText("Git branch")
      .closest(".field-cell")
      ?.querySelector("input") as HTMLInputElement;
    expect(branchToggle.checked).toBe(true);
    await fireEvent.change(branchToggle);

    await waitFor(() => expect(branchToggle.checked).toBe(true));
  });

  it("lets Codex switch desktop identity through the persisted design control", async () => {
    const { provider } = await import("@/lib/provider");
    provider.set("codex");
    discordSettings = {
      ...discordSettings,
      provider: "codex",
      desktop_design: "codex_app",
      supports_desktop_design: true,
    };
    const Discord = (await import("@/views/Discord.svelte")).default;
    const { getByRole } = render(Discord);
    await waitFor(() => expect(getDiscordSettings).toHaveBeenCalledTimes(1));

    await fireEvent.click(getByRole("button", { name: "ChatGPT App" }));

    await waitFor(() => {
      expect(setCodexDesktopDesign).toHaveBeenCalledWith("chatgpt_app");
      expect(getDiscordPreview).toHaveBeenCalled();
    });
  });

  it("persists accessible field reordering for Codex", async () => {
    const { provider } = await import("@/lib/provider");
    provider.set("codex");
    discordSettings = {
      ...discordSettings,
      provider: "codex",
      supports_field_order: true,
    };
    const Discord = (await import("@/views/Discord.svelte")).default;
    const { getByRole } = render(Discord);
    await waitFor(() => expect(getDiscordSettings).toHaveBeenCalledTimes(1));

    await fireEvent.click(getByRole("button", { name: "Move Project name down" }));
    await waitFor(() => expect(setDiscordFieldOrder).toHaveBeenCalledTimes(1));
    expect(setDiscordFieldOrder.mock.calls[0][0].slice(0, 2)).toEqual(["branch", "project"]);
  });

  it("shows safe systems signals without exposing subagent names", async () => {
    const { sessions } = await import("@/lib/stores");
    const active = makeSession("active1", "active-project");
    active.has_thinking = true;
    active.workflow_label = "ULTRACODE";
    active.subagent_count = 1;
    active.subagents = [
      {
        agent_type: "secret-researcher",
        model: "Claude Opus 4.8",
        tokens: 10,
        cost: 0.01,
        activity: "Reading private.md",
      },
    ];
    sessions.set([active]);

    const Discord = (await import("@/views/Discord.svelte")).default;
    const { container } = render(Discord);
    await tick();

    const state = container.querySelector(".dp-activity-state")?.textContent ?? "";
    expect(state).toContain("ULTRACODE");
    expect(state).toContain("1 agent");
    expect(state).not.toContain("secret-researcher");
    expect(state).not.toContain("private.md");
  });

  it("does not label plain Claude thinking as a workflow", async () => {
    const { sessions } = await import("@/lib/stores");
    const active = makeSession("active1", "active-project");
    active.has_thinking = true;
    active.workflow_label = null;
    active.subagent_count = 0;
    sessions.set([active]);

    const Discord = (await import("@/views/Discord.svelte")).default;
    const { container } = render(Discord);
    await tick();

    const state = container.querySelector(".dp-activity-state")?.textContent ?? "";
    expect(state).not.toContain("workflow active");
    expect(state).not.toContain("ULTRACODE");
  });

  it("previews the active session first and ignores idle sessions", async () => {
    const { sessions } = await import("@/lib/stores");
    const idle = makeSession("idle1", "idle-project");
    idle.is_idle = true;
    const active = makeSession("active1", "active-project");
    sessions.set([idle, active]);

    const Discord = (await import("@/views/Discord.svelte")).default;
    const { container } = render(Discord);
    await tick();

    await waitFor(() => {
      expect(container.querySelector(".dp-activity-details")?.textContent).toContain("active-project");
    });
    expect(container.querySelector(".dp-activity-details")?.textContent).not.toContain("idle-project");
  });

  it("calls setDiscordEnabled when the master toggle is flipped off", async () => {
    const Discord = (await import("@/views/Discord.svelte")).default;
    const { container } = render(Discord);
    await tick();

    const toggle = container.querySelector(".big-toggle input") as HTMLInputElement;
    expect(toggle).not.toBeNull();
    await fireEvent.change(toggle);

    await waitFor(() => expect(setDiscordEnabled).toHaveBeenCalledWith(false));
  });

  describe("live preview theme-awareness", () => {
    /** Every colour-bearing rule in the Discord mock must resolve through a
     *  --preview-* token so the light theme can substitute a readable
     *  surface instead of inheriting Discord's dark palette. */
    const THEMED_SELECTORS = [
      ".dp-profile",
      ".dp-body",
      ".dp-avatar",
      ".dp-status-dot",
      ".dp-username",
      ".dp-tag",
      ".dp-separator",
      ".dp-section-title",
      ".dp-activity-card",
      ".dp-activity-header",
      ".dp-art-large",
      ".dp-art-small",
      ".dp-activity-name",
      ".dp-activity-details",
      ".dp-activity-elapsed",
    ];

    /** Svelte scopes component styles at build time and happy-dom does not
     *  materialize them, so the source `<style>` block is the honest place to
     *  assert on. */
    function componentCss(): string {
      // Vitest runs with the frontend package as cwd.
      const source = readFileSync(resolve(process.cwd(), "src/views/Discord.svelte"), "utf8");
      const style = source.match(/<style>([\s\S]*)<\/style>/);
      return style?.[1] ?? "";
    }

    it("declares no hardcoded colours anywhere in the view", () => {
      const css = componentCss();
      expect(css.length).toBeGreaterThan(0);
      // Neither hex literals nor raw rgb()/rgba() may appear: both bypass the
      // theme tokens and are exactly what made the preview unreadable in light
      // mode before the redesign.
      expect(css.match(/#[0-9a-fA-F]{3,8}\b/g)).toBeNull();
      expect(css.match(/rgba?\(/g)).toBeNull();
    });

    it("routes every Discord mock surface through a --dc-* token", () => {
      const css = componentCss();
      for (const selector of THEMED_SELECTORS) {
      const cls = selector.slice(1);
        const block = css.match(new RegExp(`\\.${cls}\\s*(,[^{]*)?\\{([^}]*)\\}`));
        expect(block, `${selector} rule must exist`).not.toBeNull();
        expect(block?.[2] ?? "", `${selector} must use a --preview-* token`).toMatch(
          /var\(--preview-/,
        );
      }
    });

    it("keeps the preview rendered in both dark and light themes", async () => {
      const Discord = (await import("@/views/Discord.svelte")).default;

      for (const theme of ["dark", "light"]) {
        document.documentElement.setAttribute("data-theme", theme);
        const { container, unmount } = render(Discord);
        await tick();

        expect(container.querySelector(".dp-profile"), theme).not.toBeNull();
        expect(container.querySelector(".dp-activity-card"), theme).not.toBeNull();
        expect(container.querySelector(".dp-username")?.textContent, theme).toContain("xt0n1");
        unmount();
      }
      document.documentElement.removeAttribute("data-theme");
    });

    it("renders an intentional empty state when there is no session", async () => {
      const { sessions, discordPresencePreview } = await import("@/lib/stores");
      sessions.set([]);
      discordPresencePreview.set(null);

      const Discord = (await import("@/views/Discord.svelte")).default;
      const { container } = render(Discord);
      await tick();

      expect(container.querySelector(".dp-profile")).not.toBeNull();
      expect(container.querySelector(".dp-activity-details")?.textContent).toBe("No active session");
      expect(container.querySelector(".dp-activity-state")?.textContent).toBe("Idle");
    });

    it("marks the status dot offline when presence is paused", async () => {
      // The component prefers the backend settings payload over health for the
      // enabled flag, so pausing presence has to come through that fixture.
      discordSettings = { ...discordSettings, enabled: false };

      const Discord = (await import("@/views/Discord.svelte")).default;
      const { container } = render(Discord);
      await tick();
      await waitFor(() =>
        expect(container.querySelector(".dp-status-dot.offline")).not.toBeNull(),
      );

      expect(container.querySelector(".dp-status-dot.offline")).not.toBeNull();
    });
  });
});

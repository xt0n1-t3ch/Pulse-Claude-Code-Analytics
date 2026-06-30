import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import type { SessionInfo, DiscordUserInfo, HealthResponse } from "@/lib/api";

const setDiscordEnabled = vi.fn(async () => undefined);

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    setDiscordEnabled: (enabled: boolean) => setDiscordEnabled(enabled),
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
    const { sessions, discordUser, health } = await import("@/lib/stores");
    sessions.set([makeSession("s1", "pulse")]);
    discordUser.set(discordUserFixture);
    health.set(healthFixture);
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

  it("renders the field toggles and the master Rich Presence toggle", async () => {
    const Discord = (await import("@/views/Discord.svelte")).default;
    const { container, getByText } = render(Discord);
    await tick();

    expect(getByText("Rich Presence")).toBeTruthy();
    expect(container.querySelectorAll(".field-cell").length).toBe(6);
    expect(container.querySelectorAll(".preset-opt").length).toBe(3);
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
});

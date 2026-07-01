import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import type {
  SessionInfo,
  ContextBreakdown,
  SessionContextBreakdown,
  SessionContextUsage,
} from "@/lib/api";

const breakdown: ContextBreakdown = {
  model: "Claude Opus 4.8",
  context_window: 200_000,
  used_tokens: 50_000,
  free_space: 140_000,
  autocompact_buffer: 6_600,
  system_prompt: 10_000,
  system_tools: 6_000,
  memory_files: [],
  memory_total: 0,
  skills: [],
  skills_total: 0,
  messages: 24_000,
  mcp_tools: [],
  mcp_total: 0,
};

const usage: SessionContextUsage[] = [
  {
    session_id: "s1",
    project: "pulse",
    model: "claude-opus-4-8",
    model_display: "Claude Opus 4.8",
    used_tokens: 50_000,
    window_tokens: 200_000,
    utilization_pct: 25,
    recommendation: "Context is healthy — plenty of headroom for this session.",
  },
];

const breakdowns: SessionContextBreakdown[] = [
  { session_id: "s1", project: "pulse", model_id: "claude-opus-4-8", is_idle: false, activity: "Idle", breakdown },
  { session_id: "s2", project: "other", model_id: "claude-opus-4-8", is_idle: false, activity: "Idle", breakdown },
];

const getContextBreakdown = vi.fn(async () => breakdown);
const getContextBreakdowns = vi.fn(async () => breakdowns);
const getSessionsContextUsage = vi.fn(async () => usage);

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getContextBreakdown: (...args: unknown[]) => getContextBreakdown(...(args as [])),
    getContextBreakdowns: (...args: unknown[]) => getContextBreakdowns(...(args as [])),
    getSessionsContextUsage: (...args: unknown[]) => getSessionsContextUsage(...(args as [])),
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
    cost: 0,
    tokens: 0,
    input_tokens: 0,
    output_tokens: 0,
    cache_write_tokens: 0,
    cache_read_tokens: 0,
    branch: null,
    activity: "Idle",
    activity_target: null,
    effort: "High",
    effort_explicit: true,
    is_idle: false,
    started_at: null,
    duration_secs: 0,
    has_thinking: false,
    workflow_label: null,
    subagent_count: 0,
    subagents: [],
    tokens_per_sec: 0,
    input_cost: 0,
    output_cost: 0,
    cache_write_cost: 0,
    cache_read_cost: 0,
    speed: "standard",
    fast: false,
    service_tier: null,
    app_name: null,
  };
}

describe("Context.svelte", () => {
  beforeEach(() => {
    getContextBreakdown.mockClear();
    getContextBreakdowns.mockClear();
    getSessionsContextUsage.mockClear();
  });

  it("renders the session pill strip for seeded sessions and a per-session list", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("s1", "pulse"), makeSession("s2", "other")]);

    const Context = (await import("@/views/Context.svelte")).default;
    const { container } = render(Context);
    await tick();

    await waitFor(() => {
      expect(container.querySelectorAll(".session-pill").length).toBe(2);
    });
    const projects = [...container.querySelectorAll(".pill-project")].map((el) => el.textContent?.trim());
    expect(projects).toContain("pulse");
    expect(projects).toContain("other");
    await waitFor(() => {
      expect(container.querySelector(".usage-row")).not.toBeNull();
    });
  });

  it("renders a context card for every active session simultaneously", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("s1", "pulse"), makeSession("s2", "other")]);

    const Context = (await import("@/views/Context.svelte")).default;
    const { container } = render(Context);
    await tick();

    await waitFor(() => {
      expect(container.querySelectorAll(".active-ctx-card").length).toBe(2);
    });
    const cardProjects = [...container.querySelectorAll(".act-project")].map((el) => el.textContent?.trim());
    expect(cardProjects).toContain("pulse");
    expect(cardProjects).toContain("other");
    expect(getContextBreakdowns).toHaveBeenCalled();
  });

  it("selects the detailed breakdown when an active card is clicked", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("s1", "pulse"), makeSession("s2", "other")]);

    const Context = (await import("@/views/Context.svelte")).default;
    const { container } = render(Context);
    await tick();

    let cards: HTMLElement[] = [];
    await waitFor(() => {
      cards = [...container.querySelectorAll<HTMLElement>(".active-ctx-card")];
      expect(cards.length).toBe(2);
    });
    const otherCard = cards.find((c) => c.textContent?.includes("other"));
    expect(otherCard).toBeTruthy();
    await fireEvent.click(otherCard!);

    await waitFor(() => {
      expect(otherCard!.classList.contains("selected")).toBe(true);
    });
  });

  it("queries the breakdown when a session is selected", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("sel", "pulse")]);

    const Context = (await import("@/views/Context.svelte")).default;
    render(Context);
    await tick();

    await waitFor(() => {
      expect(getContextBreakdown).toHaveBeenCalled();
    });
  });
});

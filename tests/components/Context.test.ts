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
  skills: [{ name: "research", tokens: 4_200 }],
  skills_total: 4_200,
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
  beforeEach(async () => {
    getContextBreakdown.mockReset();
    getContextBreakdown.mockResolvedValue(breakdown);
    getContextBreakdowns.mockReset();
    getContextBreakdowns.mockImplementation(async (sessionIds?: string[]) =>
      (sessionIds ?? breakdowns.map((entry) => entry.session_id)).map((sessionId) => ({
        session_id: sessionId,
        project: sessionId === "s2" ? "other" : "pulse",
        model_id: "claude-opus-4-8",
        is_idle: false,
        activity: "Idle",
        breakdown,
      }))
    );
    getSessionsContextUsage.mockReset();
    getSessionsContextUsage.mockResolvedValue(usage);
    const {
      sessions,
      selectedAccessSourceId,
      selectedAnalyticsProviderScope,
    } = await import("@/lib/stores");
    sessions.set([]);
    selectedAccessSourceId.set("all");
    selectedAnalyticsProviderScope.set("all");
  });

  it("renders one active-session selector without a second stale history list", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("s1", "pulse"), makeSession("s2", "other")]);

    const Context = (await import("@/views/Context.svelte")).default;
    const { container } = render(Context);
    await tick();

    await waitFor(() => expect(container.querySelectorAll(".active-ctx-card").length).toBe(2));
    const projects = [...container.querySelectorAll(".act-project")].map((el) => el.textContent?.trim());
    expect(projects).toContain("pulse");
    expect(projects).toContain("other");
    expect(container.querySelector(".session-pill")).toBeNull();
    expect(container.querySelector(".usage-row")).toBeNull();
    expect(getSessionsContextUsage).not.toHaveBeenCalled();
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

  it("queries the list observation when a session is selected", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("sel", "pulse")]);

    const Context = (await import("@/views/Context.svelte")).default;
    render(Context);
    await tick();

    await waitFor(() => {
      expect(getContextBreakdowns).toHaveBeenCalledWith(["sel"], "all");
    });
    expect(getContextBreakdown).not.toHaveBeenCalled();
  });

  it("refreshes the active row and selected detail from the same live snapshot", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([{ ...makeSession("live", "pulse"), context_used_tokens: 10_000, context_window_tokens: 200_000 }]);

    const Context = (await import("@/views/Context.svelte")).default;
    const { container } = render(Context);
    await waitFor(() => expect(getContextBreakdowns).toHaveBeenCalled());
    getContextBreakdowns.mockClear();
    const advanced = {
      ...breakdown,
      used_tokens: 80_000,
      free_space: 110_000,
    };
    getContextBreakdowns.mockResolvedValueOnce([{
      session_id: "live",
      project: "pulse",
      model_id: "claude-opus-4-8",
      is_idle: false,
      activity: "Thinking",
      breakdown: advanced,
    }]);

    sessions.set([{ ...makeSession("live", "pulse"), context_used_tokens: 20_000, context_window_tokens: 200_000 }]);
    await tick();

    await waitFor(() => expect(getContextBreakdowns).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(container.querySelector(".hero-used")?.textContent?.trim()).toBe("80.0K"));
  });

  it("does not pair a newly selected session with the previous session breakdown", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("s1", "pulse")]);

    const Context = (await import("@/views/Context.svelte")).default;
    const { container } = render(Context);
    await waitFor(() => expect(container.querySelector(".hero-used")?.textContent?.trim()).toBe("50.0K"));

    let resolveNext!: (value: SessionContextBreakdown[]) => void;
    getContextBreakdowns.mockImplementationOnce(() => new Promise((resolve) => {
      resolveNext = resolve;
    }));
    sessions.set([makeSession("s1", "pulse"), makeSession("s2", "other")]);
    await tick();
    const otherCard = [...container.querySelectorAll<HTMLElement>(".active-ctx-card")]
      .find((card) => card.textContent?.includes("other"));
    expect(otherCard).toBeTruthy();
    await fireEvent.click(otherCard!);
    await tick();

    expect(otherCard!.classList.contains("selected")).toBe(true);
    expect(container.querySelector(".hero-card")).toBeNull();

    resolveNext([{
      session_id: "s2",
      project: "other",
      model_id: "claude-opus-4-8",
      is_idle: false,
      activity: "Thinking",
      breakdown: { ...breakdown, used_tokens: 90_000, free_space: 100_000 },
    }]);
    await waitFor(() => expect(container.querySelector(".hero-used")?.textContent?.trim()).toBe("90.0K"));
  });

  it("clears the detail instead of falling back to an idle snapshot", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("live", "pulse")]);

    const Context = (await import("@/views/Context.svelte")).default;
    const { container } = render(Context);
    await waitFor(() => expect(container.querySelector(".hero-card")).not.toBeNull());
    getContextBreakdowns.mockClear();

    sessions.set([{ ...makeSession("live", "pulse"), is_idle: true }]);
    await tick();

    await waitFor(() => expect(container.querySelector(".hero-card")).toBeNull());
    expect(getContextBreakdowns).not.toHaveBeenCalled();
  });

  it("labels installed skills as estimated inventory instead of loaded context", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("inventory", "pulse")]);

    const Context = (await import("@/views/Context.svelte")).default;
    const { findByText, queryByText } = render(Context);

    expect(await findByText("Installed skill inventory")).toBeTruthy();
    expect(queryByText(/skills loaded/i)).toBeNull();
  });
});

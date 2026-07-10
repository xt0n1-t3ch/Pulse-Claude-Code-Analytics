import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import { tick } from "svelte";
import type {
  SessionInfo,
  ContextBreakdown,
  SessionContextBreakdown,
  SessionContextUsage,
  ReportsBundle,
} from "@/lib/api";

function breakdownFor(model: string): ContextBreakdown {
  return {
    model,
    context_window: 200_000,
    used_tokens: 40_000,
    free_space: 153_400,
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
}

const usage: SessionContextUsage[] = [
  {
    session_id: "s1",
    project: "pulse",
    model: "claude-opus-4-8",
    model_display: "Claude Opus 4.8",
    used_tokens: 40_000,
    window_tokens: 200_000,
    utilization_pct: 20,
    recommendation: "Context is healthy — plenty of headroom for this session.",
  },
];

const minimalBundle: ReportsBundle = {
  provider: "claude",
  capabilities: { cache_health: true, model_routing: true, extra_usage: true },
  days: 30,
  total_sessions: 1,
  recommendations: [
    {
      id: "rec-x",
      severity: "info",
      title: "Flow recommendation",
      description: "Generated in flow test.",
      estimated_savings: null,
      action: "Do the thing",
      fix_prompt: "",
      color: "#7cb9e8",
    },
  ],
  trace_overview: {
    provider: "claude",
    provider_display: "Claude Code",
    instruction_file: "CLAUDE.md",
    fix_button_label: "Fix with Claude",
    session_store: "",
    global_state_source: "",
    traced_sessions: 1,
    total_sessions: 1,
    user_messages: 1,
    assistant_messages: 1,
    total_tool_calls: 1,
    total_compactions: 0,
    mcp_tool_calls: 0,
    cache_hit_ratio: 0,
    top_tools: [],
    telemetry_mermaid: "",
    cache_mermaid: "",
  },
  tool_frequency: {
    available: false,
    sessions_analyzed: 1,
    traced_sessions: 1,
    total_tool_calls: 0,
    avg_tools_per_session: 0,
    avg_tool_calls_per_hour: 0,
    mcp_tool_calls: 0,
    mcp_share_pct: 0,
    compact_gap_sessions: 0,
    diagnosis: "",
    top_tools: [],
  },
  prompt_complexity: {
    available: false,
    sessions_analyzed: 1,
    prompts_analyzed: 0,
    avg_complexity_score: 0,
    avg_specificity_score: 0,
    high_complexity_sessions: 0,
    low_specificity_sessions: 0,
    diagnosis: "",
    top_sessions: [],
  },
  session_health: {
    available: false,
    sessions_analyzed: 1,
    health_score: 0,
    grade: "A",
    avg_duration_minutes: 0,
    p90_duration_minutes: 0,
    long_session_pct: 0,
    avg_messages_per_session: 0,
    peak_overlap_pct: 0,
    compact_gap_pct: 0,
    diagnosis: "",
  },
  cache_health: {
    grade: "A",
    grade_label: "Excellent",
    color: "#62b462",
    hit_ratio: 0,
    trend_weighted_ratio: 0,
    total_cache_read: 0,
    total_cache_write: 0,
    total_input: 0,
    sessions_analyzed: 1,
    diagnosis: "Flow cache diagnosis.",
  },
  model_routing: {
    total_sessions: 1,
    total_cost: 0,
    opus: { sessions: 0, cost: 0, cost_share_pct: 0, avg_cost_per_session: 0 },
    sonnet: { sessions: 0, cost: 0, cost_share_pct: 0, avg_cost_per_session: 0 },
    haiku: { sessions: 0, cost: 0, cost_share_pct: 0, avg_cost_per_session: 0 },
    other: { sessions: 0, cost: 0, cost_share_pct: 0, avg_cost_per_session: 0 },
    estimated_savings_if_rerouted: 0,
    diagnosis: "",
  },
  inflection_points: [],
};

const breakdownsFixture: SessionContextBreakdown[] = [
  { session_id: "s1", project: "pulse", model_id: "claude-opus-4-8", is_idle: false, activity: "Idle", breakdown: breakdownFor("s1") },
  { session_id: "s2", project: "other", model_id: "claude-opus-4-8", is_idle: false, activity: "Idle", breakdown: breakdownFor("s2") },
];

const getContextBreakdown = vi.fn(async (sessionId?: string) =>
  breakdownFor(sessionId ?? "default"),
);
const getContextBreakdowns = vi.fn(async () => breakdownsFixture);
const getSessionsContextUsage = vi.fn(async () => usage);
const getReportsBundle = vi.fn(async () => minimalBundle);

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getContextBreakdown: (sessionId?: string) => getContextBreakdown(sessionId),
    getContextBreakdowns: () => getContextBreakdowns(),
    getSessionsContextUsage: () => getSessionsContextUsage(),
    getReportsBundle: () => getReportsBundle(),
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

describe("Phase 5 flow", () => {
  beforeEach(() => {
    getContextBreakdown.mockClear();
    getContextBreakdowns.mockClear();
    getSessionsContextUsage.mockClear();
    getReportsBundle.mockClear();
  });

  it("re-queries the breakdown when a different session pill is selected", async () => {
    const { sessions } = await import("@/lib/stores");
    sessions.set([makeSession("s1", "pulse"), makeSession("s2", "other")]);

    const Context = (await import("@/views/Context.svelte")).default;
    const { container } = render(Context);
    await tick();

    await waitFor(() => expect(getContextBreakdown).toHaveBeenCalled());
    const callsBefore = getContextBreakdown.mock.calls.length;

    let otherPill: HTMLElement | undefined;
    await waitFor(() => {
      otherPill = [...container.querySelectorAll<HTMLElement>(".session-pill")].find((p) =>
        p.textContent?.includes("other"),
      );
      expect(otherPill).toBeTruthy();
    });
    await fireEvent.click(otherPill!);

    await waitFor(() => {
      expect(getContextBreakdown.mock.calls.length).toBeGreaterThan(callsBefore);
    });
    expect(getContextBreakdown).toHaveBeenCalledWith("s2");
  });

  it("renders the reports bundle through a single bundle call", async () => {
    const Reports = (await import("@/views/Reports.svelte")).default;
    const { findByText } = render(Reports);

    expect(await findByText("Flow recommendation")).toBeTruthy();
    expect(getReportsBundle).toHaveBeenCalledTimes(1);
  });
});

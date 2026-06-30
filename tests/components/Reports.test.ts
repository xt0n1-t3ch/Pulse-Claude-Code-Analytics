import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, waitFor, fireEvent } from "@testing-library/svelte";
import type { ReportsBundle } from "@/lib/api";

function makeBundle(): ReportsBundle {
  return {
    provider: "claude",
    days: 30,
    total_sessions: 3,
    recommendations: [
      {
        id: "rec-1",
        severity: "warning",
        title: "Trim memory files",
        description: "Your memory footprint is heavy.",
        estimated_savings: "$1.20",
        action: "Edit CLAUDE.md",
        fix_prompt: "Help me trim my memory files.",
        color: "#fbbf24",
      },
    ],
    trace_overview: {
      provider: "claude",
      provider_display: "Claude Code",
      instruction_file: "CLAUDE.md",
      fix_button_label: "Fix with Claude",
      session_store: "",
      global_state_source: "",
      traced_sessions: 2,
      total_sessions: 3,
      user_messages: 10,
      assistant_messages: 12,
      total_tool_calls: 40,
      total_compactions: 1,
      mcp_tool_calls: 4,
      cache_hit_ratio: 80,
      top_tools: [],
      telemetry_mermaid: "",
      cache_mermaid: "",
    },
    tool_frequency: {
      available: true,
      sessions_analyzed: 3,
      traced_sessions: 2,
      total_tool_calls: 40,
      avg_tools_per_session: 13,
      avg_tool_calls_per_hour: 5,
      mcp_tool_calls: 4,
      mcp_share_pct: 10,
      compact_gap_sessions: 0,
      diagnosis: "Healthy tool mix.",
      top_tools: [],
    },
    prompt_complexity: {
      available: true,
      sessions_analyzed: 3,
      prompts_analyzed: 9,
      avg_complexity_score: 50,
      avg_specificity_score: 60,
      high_complexity_sessions: 1,
      low_specificity_sessions: 0,
      diagnosis: "Prompts are specific.",
      top_sessions: [],
    },
    session_health: {
      available: true,
      sessions_analyzed: 3,
      health_score: 88,
      grade: "A",
      avg_duration_minutes: 12,
      p90_duration_minutes: 30,
      long_session_pct: 10,
      avg_messages_per_session: 8,
      peak_overlap_pct: 5,
      compact_gap_pct: 0,
      diagnosis: "Sessions look healthy.",
    },
    cache_health: {
      grade: "A",
      grade_label: "Excellent",
      color: "#62b462",
      hit_ratio: 80,
      trend_weighted_ratio: 82,
      total_cache_read: 5_000_000,
      total_cache_write: 1_000_000,
      total_input: 2_000_000,
      sessions_analyzed: 3,
      diagnosis: "Cache is doing its job.",
    },
    model_routing: {
      total_sessions: 3,
      total_cost: 10,
      opus: { sessions: 2, cost: 8, cost_share_pct: 80, avg_cost_per_session: 4 },
      sonnet: { sessions: 1, cost: 2, cost_share_pct: 20, avg_cost_per_session: 2 },
      haiku: { sessions: 0, cost: 0, cost_share_pct: 0, avg_cost_per_session: 0 },
      other: { sessions: 0, cost: 0, cost_share_pct: 0, avg_cost_per_session: 0 },
      estimated_savings_if_rerouted: 1.5,
      diagnosis: "Mostly Opus.",
    },
    inflection_points: [],
  };
}

let resolvers: Array<() => void> = [];
const getReportsBundle = vi.fn(
  () =>
    new Promise<ReportsBundle>((resolve) => {
      resolvers.push(() => resolve(makeBundle()));
    }),
);

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    getReportsBundle: () => getReportsBundle(),
  };
});

function flushAll(): void {
  resolvers.forEach((r) => r());
  resolvers = [];
}

describe("Reports.svelte", () => {
  beforeEach(() => {
    getReportsBundle.mockClear();
    resolvers = [];
  });

  it("populates sections from a single bundle call", async () => {
    const Reports = (await import("@/views/Reports.svelte")).default;
    const { findByText } = render(Reports);

    await waitFor(() => expect(resolvers.length).toBeGreaterThan(0));
    flushAll();

    expect(await findByText("Trim memory files")).toBeTruthy();
    expect(await findByText("Cache is doing its job.")).toBeTruthy();
    expect(getReportsBundle).toHaveBeenCalledTimes(1);
  });

  it("shows loading feedback on a re-fetch triggered by a filter change", async () => {
    const Reports = (await import("@/views/Reports.svelte")).default;
    const { container, getByText } = render(Reports);

    await waitFor(() => expect(resolvers.length).toBeGreaterThan(0));
    flushAll();
    await waitFor(() => {
      expect(container.querySelector(".report-body")).not.toBeNull();
    });

    await fireEvent.click(getByText("7d"));

    await waitFor(() => {
      expect(container.querySelector(".reload-banner")).not.toBeNull();
    });

    flushAll();
    await waitFor(() => {
      expect(container.querySelector(".reload-banner")).toBeNull();
    });
  });
});

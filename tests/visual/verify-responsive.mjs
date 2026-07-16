import { chromium } from "../../frontend/node_modules/playwright/index.mjs";
import fs from "node:fs/promises";
import path from "node:path";

const baseUrl = process.env.PULSE_VISUAL_URL ?? "http://127.0.0.1:1420";
const outputDir = path.resolve(process.env.PULSE_VISUAL_OUTPUT_DIR ?? "artifacts/visual");
const fullMatrix = [
  { width: 1280, height: 860 },
  { width: 900, height: 600 },
  { width: 720, height: 560 },
];
const matrix = process.env.PULSE_VISUAL_QUICK === "1" ? [fullMatrix[0]] : fullMatrix;
const themes = process.env.PULSE_VISUAL_QUICK === "1" ? ["dark"] : ["dark", "light"];
const idleVerificationMs = Number(process.env.PULSE_IDLE_VERIFY_MS ?? 1200);
const views = ["Dashboard", "Discord", "Sessions", "Cost Analysis", "Reports", "Settings"];

await fs.mkdir(outputDir, { recursive: true });
const browser = await chromium.launch({ headless: true });
const results = [];

for (const theme of themes) {
  for (const viewport of matrix) {
    const context = await browser.newContext({ viewport });
    await context.addInitScript(({ selectedTheme }) => {
      localStorage.setItem("pulse-theme", selectedTheme);
      const session = {
        session_id: "visual-codex", session_name: "Pulse v1.6 overhaul", project: "cc-discord-presence",
        model: "GPT-5.6 Sol · Extra High · ⚡ Fast", model_id: "gpt-5.6-sol", provider: "codex",
        context_window: "1M", cost: 195.79, tokens: 267600000, input_tokens: 8500000,
        output_tokens: 809200, cache_write_tokens: 0, cache_read_tokens: 258400000,
        branch: "codex/pulse-v1-6-overhaul", activity: "Implementing premium native UI", activity_target: "Discord.svelte",
        effort: "Extra High", effort_explicit: true, is_idle: false, started_at: "2026-07-16T00:00:00Z",
        duration_secs: 6420, has_thinking: true, workflow_label: "ULTRACODE", subagent_count: 2, subagents: [],
        tokens_per_sec: 42, input_cost: 42.33, output_cost: 24.27, cache_write_cost: 0, cache_read_cost: 129.19,
        speed: "fast", fast: true, service_tier: "priority", app_name: "ChatGPT App",
      };
      const usage = {
        provider: "codex",
        scopes: [
          { id: "codex", name: null, kind: "global", windows: [{ window_minutes: 10080, used_percent: 4, remaining_percent: 96, resets_at: "2026-07-22T23:16:00Z" }] },
          { id: "codex_bengalfox", name: "GPT-5.3-Codex-Spark", kind: "model", windows: [{ window_minutes: 10080, used_percent: 0, remaining_percent: 100, resets_at: null }] },
        ],
        credits: { balance: "2500", has_credits: true, unlimited: false },
        observed_at: "2026-07-16T02:00:00Z", source: "Codex JSONL telemetry",
      };
      const prefs = { show_project: true, show_branch: false, show_model: true, show_activity: true, show_tokens: true, show_cost: true, show_limits: true, show_credits: true, show_context: true, show_systems: true };
      const discordSettings = { provider: "codex", enabled: true, status: "Connected", publisher: "pulse", display_prefs: prefs, desktop_design: "chatgpt_app", supports_desktop_design: true, supports_field_order: true, field_order: ["project", "branch", "model", "activity", "tokens", "cost", "quotas", "credits", "context", "systems"] };
      const discordPreview = { provider: "codex", app_name: "ChatGPT App", details: "Implementing premium native UI · cc-discord-presence", state: "GPT-5.6 Sol · Extra High · ⚡ Fast · 7d 96% · Credits 2,500", large_image_key: "chatgpt-app", large_text: "ChatGPT App", small_image_key: null, small_text: null, has_session: true, duration_secs: 6420 };
      const health = { version: "1.6.0", uptime_seconds: 6420, discord_status: "Connected", discord_enabled: true };
      const metrics = { total_cost: 195.79, input_tokens: 8500000, pure_input_tokens: 8500000, output_tokens: 809200, cache_write_tokens: 0, cache_read_tokens: 258400000, total_tokens: 267600000, session_count: 1, input_cost: 42.33, output_cost: 24.27, cache_write_cost: 0, cache_read_cost: 129.19, cache_hit_ratio: 97, models: [{ model: session.model, sessions: 1, cost: 195.79, tokens: 267600000 }] };
      const plan = { provider: "codex", plan_key: "pro_20x", plan_name: "Pro 20x ($200/month)", detected: true };
      const rateLimits = { provider: "codex", usage, five_hour_pct: 0, five_hour_resets: "N/A", five_hour_label: "", five_hour_window_minutes: null, seven_day_pct: 4, seven_day_resets: "2026-07-22T23:16:00Z", seven_day_label: "7d Window", seven_day_window_minutes: 10080, sonnet_pct: null, sonnet_resets: null, extra_enabled: false, extra_limit: null, extra_used: null, extra_pct: null, source: usage.source };
      const snapshot = { revision: 1, health, metrics, sessions: [session], rate_limits: rateLimits, discord_preview: discordPreview, discord_settings: discordSettings, plan };
      const history = [{ id: "visual-codex", session_name: "Pulse v1.6 overhaul", project: session.project, model: session.model, model_id: session.model_id, context_window: "1M", branch: session.branch, effort: session.effort, started_at: session.started_at, ended_at: null, duration_secs: session.duration_secs, total_cost: session.cost, input_tokens: session.input_tokens, output_tokens: session.output_tokens, cache_write_tokens: 0, cache_read_tokens: session.cache_read_tokens, total_tokens: session.tokens, input_cost: session.input_cost, output_cost: session.output_cost, cache_write_cost: 0, cache_read_cost: session.cache_read_cost, has_thinking: true, workflow_label: "ULTRACODE", subagent_count: 2, is_active: true }];
      const reports = {
        provider: "codex", capabilities: { cache_health: true, model_routing: false, extra_usage: false }, days: 30, total_sessions: 1, recommendations: [], inflection_points: [], model_routing: null,
        cache_health: { grade: "A", grade_label: "Excellent", color: "var(--success)", hit_ratio: 97, trend_weighted_ratio: 97, total_cache_read: 258400000, total_cache_write: 0, total_input: 8500000, sessions_analyzed: 1, diagnosis: "Cache reuse is excellent." },
        trace_overview: { provider: "codex", provider_display: "Codex", instruction_file: "AGENTS.md", fix_button_label: "Fix with Codex", session_store: "JSONL", global_state_source: "config.toml", traced_sessions: 1, total_sessions: 1, user_messages: 24, assistant_messages: 38, total_tool_calls: 92, total_compactions: 1, mcp_tool_calls: 8, cache_hit_ratio: 97, top_tools: [], telemetry_mermaid: "", cache_mermaid: "" },
        tool_frequency: { available: true, sessions_analyzed: 1, traced_sessions: 1, total_tool_calls: 92, avg_tools_per_session: 92, avg_tool_calls_per_hour: 12, mcp_tool_calls: 8, mcp_share_pct: 8.7, compact_gap_sessions: 0, diagnosis: "Healthy tool mix.", top_tools: [] },
        prompt_complexity: { available: true, sessions_analyzed: 1, prompts_analyzed: 24, avg_complexity_score: 82, avg_specificity_score: 91, high_complexity_sessions: 1, low_specificity_sessions: 0, diagnosis: "Prompts are specific.", top_sessions: [] },
        session_health: { available: true, sessions_analyzed: 1, health_score: 92, grade: "A", avg_duration_minutes: 107, p90_duration_minutes: 107, long_session_pct: 0, avg_messages_per_session: 62, peak_overlap_pct: 0, compact_gap_pct: 0, diagnosis: "Session shape is healthy." },
      };
      const values = {
        get_active_provider: "codex", get_app_snapshot: snapshot, get_health: health, get_metrics: metrics,
        get_live_sessions: [session], get_rate_limits: rateLimits, get_plan_info: plan, get_discord_settings: discordSettings,
        get_discord_preview: discordPreview, get_discord_user: null,
        get_analytics_summary: { total_sessions: 1, total_cost: 195.79, total_tokens: 267600000, total_cache_read: 258400000, total_cache_write: 0, avg_duration_secs: 6420, avg_tokens_per_session: 267600000, avg_cost_per_session: 195.79, top_project: session.project, top_model: session.model, days_tracked: 30 },
        get_session_history: history, get_session_history_filtered: history, get_top_sessions: history,
        get_cost_forecast: { spent_this_month: 195.79, days_elapsed: 16, days_in_month: 31, projected_monthly: 379.34, daily_average: 12.24 },
        get_hourly_activity: [{ hour: 2, session_count: 1, total_cost: 195.79 }], get_daily_stats: [],
        get_project_stats: [{ project: session.project, session_count: 1, total_cost: 195.79, total_tokens: 267600000, avg_session_cost: 195.79, avg_duration_secs: 6420, cache_read_tokens: 258400000, cache_write_tokens: 0, top_model: session.model }],
        get_reports_bundle: reports, get_recommendations: [], get_inflection_points: [], get_model_routing: null,
        get_cache_health: reports.cache_health, get_tool_frequency: reports.tool_frequency, get_prompt_complexity: reports.prompt_complexity, get_session_health: reports.session_health,
        get_trace_overview: reports.trace_overview, get_context_breakdowns: [], get_context_breakdown: null, get_sessions_context_usage: [],
        get_budget_status: { monthly_budget: null, spent: 195.79, remaining: null, used_pct: null, projected: 379.34 },
        get_db_size: 2097152, get_provider_copy: { provider: "codex", provider_label: "Codex", instruction_file: "AGENTS.md", home_dir: "~/.codex", sessions_store: "JSONL", fix_label: "Fix with Codex", global_state_source: "config.toml" },
        check_app_update: null,
      };
      let callbackId = 0;
      window.__PULSE_INVOKES__ = {};
      window.__TAURI_INTERNALS__ = {
        invoke: async (command) => {
          window.__PULSE_INVOKES__[command] = (window.__PULSE_INVOKES__[command] ?? 0) + 1;
          if (command === "plugin:event|listen") return 1;
          if (command.startsWith("plugin:") || command.startsWith("set_")) return null;
          return values[command] ?? null;
        },
        transformCallback: (callback) => {
          const id = ++callbackId;
          window[`_${id}`] = callback;
          return id;
        },
        unregisterCallback: (id) => { delete window[`_${id}`]; },
        metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main", windowLabel: "main" } },
      };
    }, { selectedTheme: theme });

    const page = await context.newPage();
    const errors = [];
    page.on("pageerror", (error) => errors.push(error.message));
    await page.goto(baseUrl, { waitUntil: "networkidle" });
    await page.waitForSelector(".main-content");
    await page.getByRole("tab", { name: "Codex" }).click();
    await page.waitForTimeout(600);

    const idleBefore = await page.evaluate(() => ({ ...window.__PULSE_INVOKES__ }));
    await page.waitForTimeout(idleVerificationMs);
    const idleAfter = await page.evaluate(() => ({ ...window.__PULSE_INVOKES__ }));
    if (JSON.stringify(idleBefore) !== JSON.stringify(idleAfter)) {
      throw new Error(`Frontend invoked Tauri while idle: ${JSON.stringify({ idleBefore, idleAfter })}`);
    }

    for (const view of views) {
      if (view !== "Dashboard") await page.getByTitle(view).click();
      await page.waitForTimeout(200);
      const overflow = await page.evaluate(() => ({
        document: document.documentElement.scrollWidth - window.innerWidth,
        body: document.body.scrollWidth - window.innerWidth,
        main: document.querySelector(".main-content")?.scrollWidth - document.querySelector(".main-content")?.clientWidth,
        theme: document.documentElement.getAttribute("data-theme"),
        offenders: [...document.querySelectorAll(".main-content *")]
          .filter((element) => element.scrollWidth > element.clientWidth + 1)
          .slice(0, 8)
          .map((element) => ({ tag: element.tagName, class: element.className, scroll: element.scrollWidth, client: element.clientWidth })),
      }));
      const file = `${theme}-${viewport.width}x${viewport.height}-${view.toLowerCase().replaceAll(" ", "-")}.png`;
      await page.screenshot({ path: path.join(outputDir, file), fullPage: false });
      results.push({ theme, viewport: `${viewport.width}x${viewport.height}`, view, overflow, errors: [...errors], idle: { duration_ms: idleVerificationMs, before: idleBefore, after: idleAfter } });
      if (overflow.document > 0 || overflow.body > 0 || (overflow.main ?? 0) > 0) {
        throw new Error(`Global overflow in ${theme} ${viewport.width}x${viewport.height} ${view}: ${JSON.stringify(overflow)}`);
      }
      if (overflow.theme !== theme) throw new Error(`Theme mismatch: ${JSON.stringify(overflow)}`);
    }
    if (errors.length) throw new Error(`Browser errors in ${theme} ${viewport.width}x${viewport.height}: ${errors.join(" | ")}`);
    await context.close();
  }
}

await browser.close();
await fs.writeFile(path.join(outputDir, "matrix.json"), JSON.stringify(results, null, 2));
console.log(`Verified ${results.length} visual states with no global horizontal overflow.`);

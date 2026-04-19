import { invoke } from "@tauri-apps/api/core";

export interface HealthResponse {
  version: string;
  uptime_seconds: number;
  discord_status: string;
  discord_enabled: boolean;
}

export interface ModelMetric {
  model: string;
  sessions: number;
  cost: number;
  tokens: number;
}

export interface MetricsResponse {
  total_cost: number;
  input_tokens: number;
  pure_input_tokens: number;
  output_tokens: number;
  cache_write_tokens: number;
  cache_read_tokens: number;
  total_tokens: number;
  session_count: number;
  input_cost: number;
  output_cost: number;
  cache_write_cost: number;
  cache_read_cost: number;
  cache_hit_ratio: number;
  models: ModelMetric[];
}

export interface SubagentDetail {
  agent_type: string;
  model: string;
  tokens: number;
  cost: number;
  activity: string;
}

export interface SessionInfo {
  session_id: string;
  session_name: string | null;
  project: string;
  model: string;
  model_id: string;
  context_window: string;
  cost: number;
  tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_write_tokens: number;
  cache_read_tokens: number;
  branch: string | null;
  activity: string;
  activity_target: string | null;
  effort: string;
  effort_explicit: boolean;
  is_idle: boolean;
  started_at: string | null;
  duration_secs: number;
  has_thinking: boolean;
  subagent_count: number;
  subagents: SubagentDetail[];
  tokens_per_sec: number;
  input_cost: number;
  output_cost: number;
  cache_write_cost: number;
  cache_read_cost: number;
}

export interface RateLimitInfo {
  five_hour_pct: number;
  five_hour_resets: string;
  seven_day_pct: number;
  seven_day_resets: string;
  sonnet_pct: number | null;
  sonnet_resets: string | null;
  extra_enabled: boolean;
  extra_limit: number | null;
  extra_used: number | null;
  extra_pct: number | null;
  source: string;
}

export interface DiscordUserInfo {
  user_id: string;
  username: string;
  discriminator: string;
  avatar_hash: string;
  avatar_url: string;
  banner_hash: string | null;
  banner_url: string | null;
}

export interface PlanInfo {
  plan_name: string;
  detected: boolean;
}

export interface DiscordDisplayPrefs {
  show_project: boolean;
  show_branch: boolean;
  show_model: boolean;
  show_activity: boolean;
  show_tokens: boolean;
  show_cost: boolean;
}

export function getHealth(): Promise<HealthResponse> {
  return invoke("get_health");
}

export function getMetrics(): Promise<MetricsResponse> {
  return invoke("get_metrics");
}

export function getLiveSessions(): Promise<SessionInfo[]> {
  return invoke("get_live_sessions");
}

export function getRateLimits(): Promise<RateLimitInfo | null> {
  return invoke("get_rate_limits");
}

export function getDiscordUser(): Promise<DiscordUserInfo | null> {
  return invoke("get_discord_user");
}

export function setDiscordEnabled(enabled: boolean): Promise<void> {
  return invoke("set_discord_enabled", { enabled });
}

export function setDiscordDisplayPrefs(prefs: DiscordDisplayPrefs): Promise<void> {
  return invoke("set_discord_display_prefs", { ...prefs });
}

export function getPlanInfo(): Promise<PlanInfo> {
  return invoke("get_plan_info");
}

export function setPlanOverride(plan: string): Promise<void> {
  return invoke("set_plan_override", { plan });
}

export interface HistoricalSession {
  id: string;
  session_name: string | null;
  project: string;
  model: string;
  model_id: string;
  context_window: string;
  branch: string | null;
  effort: string;
  started_at: string | null;
  ended_at: string | null;
  duration_secs: number;
  total_cost: number;
  input_tokens: number;
  output_tokens: number;
  cache_write_tokens: number;
  cache_read_tokens: number;
  total_tokens: number;
  input_cost: number;
  output_cost: number;
  cache_write_cost: number;
  cache_read_cost: number;
  has_thinking: boolean;
  subagent_count: number;
  is_active: boolean;
}

export interface DailyStat {
  date: string;
  project: string;
  model: string;
  session_count: number;
  total_cost: number;
  total_tokens: number;
  input_tokens: number;
  output_tokens: number;
  cache_write_tokens: number;
  cache_read_tokens: number;
}

export interface AnalyticsSummary {
  total_sessions: number;
  total_cost: number;
  total_tokens: number;
  total_cache_read: number;
  total_cache_write: number;
  avg_duration_secs: number;
  avg_tokens_per_session: number;
  avg_cost_per_session: number;
  top_project: string;
  top_model: string;
  days_tracked: number;
}

export interface ProjectStat {
  project: string;
  session_count: number;
  total_cost: number;
  total_tokens: number;
  avg_session_cost: number;
  avg_duration_secs: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  top_model: string;
}

export interface HourlyActivity {
  hour: number;
  session_count: number;
  total_cost: number;
}

export interface CostForecast {
  spent_this_month: number;
  days_elapsed: number;
  days_in_month: number;
  projected_monthly: number;
  daily_average: number;
}

export interface BudgetStatus {
  monthly_budget: number;
  alert_threshold_pct: number;
  spent_this_month: number;
  pct_used: number;
  projected_monthly: number;
  over_budget: boolean;
}

export type ModelDistribution = [string, number, number];

export function getSessionHistory(days?: number, project?: string, limit?: number): Promise<HistoricalSession[]> {
  return invoke("get_session_history", { days: days ?? null, project: project ?? null, limit: limit ?? null });
}

export function searchSessions(query: string, limit?: number): Promise<HistoricalSession[]> {
  return invoke("search_sessions", { query, limit: limit ?? null });
}

export function getDailyStats(days?: number): Promise<DailyStat[]> {
  return invoke("get_daily_stats", { days: days ?? null });
}

export function getAnalyticsSummary(): Promise<AnalyticsSummary> {
  return invoke("get_analytics_summary");
}

export interface ContextFileEntry {
  name: string;
  tokens: number;
}

export interface ContextBreakdown {
  model: string;
  context_window: number;
  used_tokens: number;
  free_space: number;
  autocompact_buffer: number;
  system_prompt: number;
  system_tools: number;
  memory_files: ContextFileEntry[];
  memory_total: number;
  skills: ContextFileEntry[];
  skills_total: number;
  messages: number;
  mcp_tools: ContextFileEntry[];
  mcp_total: number;
}

export function getContextBreakdown(): Promise<ContextBreakdown> {
  return invoke("get_context_breakdown");
}

export function getProjectStats(days?: number): Promise<ProjectStat[]> {
  return invoke("get_project_stats", { days: days ?? null });
}

export function getHourlyActivity(days?: number): Promise<HourlyActivity[]> {
  return invoke("get_hourly_activity", { days: days ?? null });
}

export function getTopSessions(limit?: number, days?: number): Promise<HistoricalSession[]> {
  return invoke("get_top_sessions", { limit: limit ?? null, days: days ?? null });
}

export function getCostForecast(): Promise<CostForecast> {
  return invoke("get_cost_forecast");
}

export function getBudgetStatus(): Promise<BudgetStatus> {
  return invoke("get_budget_status");
}

export function setBudget(monthlyBudget: number, alertThresholdPct?: number): Promise<void> {
  return invoke("set_budget", { monthlyBudget, alertThresholdPct: alertThresholdPct ?? null });
}

export function getModelDistribution(days?: number): Promise<ModelDistribution[]> {
  return invoke("get_model_distribution", { days: days ?? null });
}

export function exportAllData(): Promise<Record<string, unknown>> {
  return invoke("export_all_data");
}

export function clearHistory(): Promise<number> {
  return invoke("clear_history");
}

export function getDbSize(): Promise<number> {
  return invoke("get_db_size");
}

export function generateHtmlReport(days?: number, project?: string): Promise<string> {
  return invoke("generate_html_report", { days: days ?? null, project: project ?? null });
}

export function generateMarkdownReport(days?: number, project?: string): Promise<string> {
  return invoke("generate_markdown_report", { days: days ?? null, project: project ?? null });
}

// ── cchubber-style analyzers (Phase 3) ─────────────────────────────────

export type Severity = "critical" | "warning" | "info" | "positive";

export interface CacheHealthReport {
  grade: string;
  grade_label: string;
  color: string;
  hit_ratio: number;
  trend_weighted_ratio: number;
  total_cache_read: number;
  total_cache_write: number;
  total_input: number;
  sessions_analyzed: number;
  diagnosis: string;
}

export interface InflectionPoint {
  date: string;
  multiplier: number;
  direction: "spike" | "drop" | "";
  sessions_on_day: number;
  cost_on_day: number;
  baseline_cost: number;
  note: string;
}

export interface FamilyStats {
  sessions: number;
  cost: number;
  cost_share_pct: number;
  avg_cost_per_session: number;
}

export interface ModelRoutingReport {
  total_sessions: number;
  total_cost: number;
  opus: FamilyStats;
  sonnet: FamilyStats;
  haiku: FamilyStats;
  other: FamilyStats;
  estimated_savings_if_rerouted: number;
  diagnosis: string;
}

export interface Recommendation {
  id: string;
  severity: Severity;
  title: string;
  description: string;
  estimated_savings: string | null;
  action: string;
  fix_prompt: string;
  color: string;
}

export function getCacheHealth(days?: number): Promise<CacheHealthReport> {
  return invoke("get_cache_health", { days: days ?? null });
}

export function getInflectionPoints(days?: number): Promise<InflectionPoint[]> {
  return invoke("get_inflection_points", { days: days ?? null });
}

export function getModelRouting(days?: number): Promise<ModelRoutingReport> {
  return invoke("get_model_routing", { days: days ?? null });
}

export function getRecommendations(days?: number): Promise<Recommendation[]> {
  return invoke("get_recommendations", { days: days ?? null });
}

export function copyFixPrompt(recId: string): Promise<string> {
  return invoke("copy_fix_prompt", { recId });
}

// ── cchubber-ported analyzers (Phase 4) ────────────────────────────────

export interface ToolUsageEntry {
  name: string;
  count: number;
  share_pct: number;
}

export interface ToolFrequencyReport {
  available: boolean;
  sessions_analyzed: number;
  traced_sessions: number;
  total_tool_calls: number;
  avg_tools_per_session: number;
  avg_tool_calls_per_hour: number;
  mcp_tool_calls: number;
  mcp_share_pct: number;
  compact_gap_sessions: number;
  diagnosis: string;
  top_tools: ToolUsageEntry[];
}

export interface PromptComplexitySession {
  session_id: string;
  project: string;
  complexity_score: number;
  specificity_score: number;
  label: string;
  preview: string;
}

export interface PromptComplexityReport {
  available: boolean;
  sessions_analyzed: number;
  prompts_analyzed: number;
  avg_complexity_score: number;
  avg_specificity_score: number;
  high_complexity_sessions: number;
  low_specificity_sessions: number;
  diagnosis: string;
  top_sessions: PromptComplexitySession[];
}

export interface SessionHealthReport {
  available: boolean;
  sessions_analyzed: number;
  health_score: number;
  grade: string;
  avg_duration_minutes: number;
  p90_duration_minutes: number;
  long_session_pct: number;
  avg_messages_per_session: number;
  peak_overlap_pct: number;
  compact_gap_pct: number;
  diagnosis: string;
}

export function getToolFrequency(days?: number): Promise<ToolFrequencyReport> {
  return invoke("get_tool_frequency", { days: days ?? null });
}

export function getPromptComplexity(days?: number): Promise<PromptComplexityReport> {
  return invoke("get_prompt_complexity", { days: days ?? null });
}

export function getSessionHealth(days?: number): Promise<SessionHealthReport> {
  return invoke("get_session_health", { days: days ?? null });
}

// ── granular filtering (Phase 5) ────────────────────────────────────────

export interface SessionHistoryFilter {
  from_iso?: string | null;
  to_iso?: string | null;
  project?: string | null;
  model?: string | null;
  min_cost?: number | null;
  max_cost?: number | null;
  limit?: number | null;
}

export function getSessionHistoryFiltered(filter: SessionHistoryFilter): Promise<HistoricalSession[]> {
  return invoke("get_session_history_filtered", {
    fromIso: filter.from_iso ?? null,
    toIso: filter.to_iso ?? null,
    project: filter.project ?? null,
    model: filter.model ?? null,
    minCost: filter.min_cost ?? null,
    maxCost: filter.max_cost ?? null,
    limit: filter.limit ?? null,
  });
}

export function getSessionsByHourRange(
  startHour: number,
  endHour: number,
  days?: number,
): Promise<HistoricalSession[]> {
  return invoke("get_sessions_by_hour_range", {
    startHour,
    endHour,
    days: days ?? null,
  });
}

/// Ask the background poller to drop its usage cache and hit the API on the
/// next tick (~5s cycle). Returns immediately; stores re-poll picks up fresh data.
export function refreshUsage(): Promise<void> {
  return invoke("refresh_usage");
}

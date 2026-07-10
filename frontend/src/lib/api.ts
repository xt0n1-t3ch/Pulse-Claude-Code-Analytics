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

export interface ModelPricingRates {
    input_per_million: number;
    output_per_million: number;
    cache_write_per_million: number;
    cache_read_per_million: number;
}

export interface IntroPricingInfo {
    intro: ModelPricingRates;
    regular: ModelPricingRates;
    ends_at: string;
}

export interface SessionInfo {
    session_id: string;
    session_name: string | null;
    project: string;
    model: string;
    model_id: string;
    provider: string;
    context_window: string;
    cost: number;
    tokens: number;
    input_tokens: number;
    output_tokens: number;
    cache_write_tokens: number;
    cache_read_tokens: number;
    context_used_tokens?: number;
    context_window_tokens?: number;
    branch: string | null;
    activity: string;
    activity_target: string | null;
    effort: string;
    effort_explicit: boolean;
    is_idle: boolean;
    started_at: string | null;
    duration_secs: number;
    has_thinking: boolean;
    workflow_label: string | null;
    subagent_count: number;
    subagents: SubagentDetail[];
    tokens_per_sec: number;
    input_cost: number;
    output_cost: number;
    cache_write_cost: number;
    cache_read_cost: number;
    speed: string;
    fast: boolean;
    service_tier: string | null;
    app_name?: string | null;
    intro_pricing: IntroPricingInfo | null;
    has_inflated_tokenizer: boolean;
}

export interface RateLimitInfo {
    provider: string;
    five_hour_pct: number;
    five_hour_resets: string;
    five_hour_label: string;
    five_hour_window_minutes: number | null;
    seven_day_pct: number;
    seven_day_resets: string;
    seven_day_label: string;
    seven_day_window_minutes: number | null;
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
    provider: string;
    /** Canonical plan key for the Settings select (e.g. "max_20x"); "" when unknown. */
    plan_key: string;
    plan_name: string;
    detected: boolean;
}

export interface ProviderInfo {
    active_provider: string;
}

export interface ProviderCapabilities {
    cache_health: boolean;
    model_routing: boolean;
    extra_usage: boolean;
}

export interface ProviderCopyInfo {
    provider: string;
    provider_label: string;
    instruction_file: string;
    home_dir: string;
    sessions_store: string;
    fix_label: string;
    global_state_source: string;
}

export interface TraceToolUsage {
    name: string;
    calls: number;
    share_pct: number;
}

export interface TraceOverview {
    provider: string;
    provider_display: string;
    instruction_file: string;
    fix_button_label: string;
    session_store: string;
    global_state_source: string;
    traced_sessions: number;
    total_sessions: number;
    user_messages: number;
    assistant_messages: number;
    total_tool_calls: number;
    total_compactions: number;
    mcp_tool_calls: number;
    cache_hit_ratio: number;
    top_tools: TraceToolUsage[];
    telemetry_mermaid: string;
    cache_mermaid: string;
}

export interface DiscordDisplayPrefs {
    show_project: boolean;
    show_branch: boolean;
    show_model: boolean;
    show_activity: boolean;
    show_tokens: boolean;
    show_cost: boolean;
    show_limits: boolean;
    show_context: boolean;
    show_systems: boolean;
}

export interface DiscordPresencePreview {
    provider: string;
    app_name: string;
    details: string;
    state: string;
    large_image_key: string;
    large_text: string;
    small_image_key: string | null;
    small_text: string | null;
    has_session: boolean;
    duration_secs: number;
}

export interface DiscordSettings {
    provider: string;
    enabled: boolean;
    status: string;
    publisher: string;
    display_prefs: DiscordDisplayPrefs;
    desktop_design: "codex_app" | "chatgpt_app" | null;
    supports_desktop_design: boolean;
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

export function getDiscordPreview(): Promise<DiscordPresencePreview> {
    return invoke("get_discord_preview");
}

export function getDiscordSettings(): Promise<DiscordSettings> {
    return invoke("get_discord_settings");
}

export function getRateLimits(): Promise<RateLimitInfo | null> {
    return invoke("get_rate_limits");
}

export function getDiscordUser(): Promise<DiscordUserInfo | null> {
    return invoke("get_discord_user");
}

export function discordDisplayPrefsArgs(prefs: DiscordDisplayPrefs): Record<string, boolean> {
    return {
        showProject: prefs.show_project,
        showBranch: prefs.show_branch,
        showModel: prefs.show_model,
        showActivity: prefs.show_activity,
        showTokens: prefs.show_tokens,
        showCost: prefs.show_cost,
        showLimits: prefs.show_limits,
        showContext: prefs.show_context,
        showSystems: prefs.show_systems,
    };
}

export function setDiscordEnabled(enabled: boolean): Promise<DiscordSettings> {
    return invoke("set_discord_enabled", { enabled });
}

export function setDiscordDisplayPrefs(prefs: DiscordDisplayPrefs): Promise<DiscordSettings> {
    return invoke("set_discord_display_prefs", discordDisplayPrefsArgs(prefs));
}

export function setCodexDesktopDesign(
    design: "codex_app" | "chatgpt_app",
): Promise<DiscordSettings> {
    return invoke("set_codex_desktop_design", { design });
}

export function getPlanInfo(): Promise<PlanInfo> {
    return invoke("get_plan_info");
}

export function getActiveProvider(): Promise<ProviderInfo> {
    return invoke("get_active_provider");
}

export function setActiveProvider(provider: string): Promise<void> {
    return invoke("set_active_provider", { provider });
}

export function getProviderCopy(): Promise<ProviderCopyInfo> {
    return invoke("get_provider_copy");
}

export function getTraceOverview(days?: number): Promise<TraceOverview> {
    return invoke("get_trace_overview", { days: days ?? null });
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

export function getSessionHistory(
    days?: number,
    project?: string,
    limit?: number,
): Promise<HistoricalSession[]> {
    return invoke("get_session_history", {
        days: days ?? null,
        project: project ?? null,
        limit: limit ?? null,
    });
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

export function getContextBreakdown(sessionId?: string): Promise<ContextBreakdown> {
    return invoke("get_context_breakdown", { sessionId: sessionId ?? null });
}

export interface SessionContextBreakdown {
    session_id: string;
    project: string;
    model_id: string;
    is_idle: boolean;
    activity: string;
    breakdown: ContextBreakdown;
}

export function getContextBreakdowns(sessionIds?: string[]): Promise<SessionContextBreakdown[]> {
    return invoke("get_context_breakdowns", { sessionIds: sessionIds ?? null });
}

export interface SessionContextUsage {
    session_id: string;
    project: string;
    model: string;
    model_display: string;
    used_tokens: number;
    window_tokens: number;
    utilization_pct: number;
    recommendation: string;
}

export function getSessionsContextUsage(days?: number): Promise<SessionContextUsage[]> {
    return invoke("get_sessions_context_usage", { days: days ?? null });
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

export function getModelRouting(days?: number): Promise<ModelRoutingReport | null> {
    return invoke("get_model_routing", { days: days ?? null });
}

export function getRecommendations(days?: number): Promise<Recommendation[]> {
    return invoke("get_recommendations", { days: days ?? null });
}

export function copyFixPrompt(recId: string): Promise<string> {
    return invoke("copy_fix_prompt", { recId });
}


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


export interface ReportsBundle {
    provider: string;
    capabilities: ProviderCapabilities;
    days: number;
    total_sessions: number;
    recommendations: Recommendation[];
    trace_overview: TraceOverview;
    tool_frequency: ToolFrequencyReport;
    prompt_complexity: PromptComplexityReport;
    session_health: SessionHealthReport;
    cache_health: CacheHealthReport;
    model_routing: ModelRoutingReport | null;
    inflection_points: InflectionPoint[];
}

export function getReportsBundle(days?: number, project?: string): Promise<ReportsBundle> {
    return invoke("get_reports_bundle", { days: days ?? null, project: project ?? null });
}

export interface SessionHistoryFilter {
    from_iso?: string | null;
    to_iso?: string | null;
    project?: string | null;
    model?: string | null;
    min_cost?: number | null;
    max_cost?: number | null;
    limit?: number | null;
}

export function getSessionHistoryFiltered(
    filter: SessionHistoryFilter,
): Promise<HistoricalSession[]> {
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

export interface AppUpdateAsset {
    name: string;
    download_url: string;
    size: number;
    content_type: string;
}

export interface AppUpdateInfo {
    current_version: string;
    latest_version: string | null;
    update_available: boolean;
    release_name: string | null;
    release_notes: string | null;
    release_url: string;
    published_at: string | null;
    checked_at: string;
    assets: AppUpdateAsset[];
}

export function checkAppUpdate(): Promise<AppUpdateInfo> {
    return invoke("check_app_update");
}

export function openAppReleasePage(url?: string | null): Promise<void> {
    return invoke("open_app_release_page", { url: url ?? null });
}

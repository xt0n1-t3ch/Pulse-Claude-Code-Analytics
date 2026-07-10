import { writable, derived } from "svelte/store";
import type {
  HealthResponse,
  MetricsResponse,
  SessionInfo,
  RateLimitInfo,
  DiscordUserInfo,
  DiscordPresencePreview,
  DiscordSettings,
  DiscordDisplayPrefs,
  PlanInfo,
} from "./api";
import {
  getHealth,
  getMetrics,
  getLiveSessions,
  getDiscordPreview,
  getDiscordSettings,
  getRateLimits,
  getDiscordUser,
  getPlanInfo,
} from "./api";

export const health = writable<HealthResponse | null>(null);
export const metrics = writable<MetricsResponse | null>(null);
export const sessions = writable<SessionInfo[]>([]);
export const rateLimits = writable<RateLimitInfo | null>(null);
export const discordUser = writable<DiscordUserInfo | null>(null);
export const discordPresencePreview = writable<DiscordPresencePreview | null>(null);
export const discordSettings = writable<DiscordSettings | null>(null);
export const planInfo = writable<PlanInfo | null>(null);
export const currentView = writable<string>("dashboard");

export interface DiscordPreviewSettings {
  showProject: boolean;
  showBranch: boolean;
  showModel: boolean;
  showActivity: boolean;
  showTokens: boolean;
  showCost: boolean;
  showLimits: boolean;
  showContext: boolean;
  showSystems: boolean;
}

export const discordPreview = writable<DiscordPreviewSettings>({
  showProject: true,
  showBranch: true,
  showModel: true,
  showActivity: true,
  showTokens: false,
  showCost: false,
  showLimits: true,
  showContext: true,
  showSystems: true,
});

export function displayPrefsToPreview(prefs: DiscordDisplayPrefs): DiscordPreviewSettings {
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

export function previewToDisplayPrefs(preview: DiscordPreviewSettings): DiscordDisplayPrefs {
  return {
    show_project: preview.showProject,
    show_branch: preview.showBranch,
    show_model: preview.showModel,
    show_activity: preview.showActivity,
    show_tokens: preview.showTokens,
    show_cost: preview.showCost,
    show_limits: preview.showLimits,
    show_context: preview.showContext,
    show_systems: preview.showSystems,
  };
}

export function applyDiscordSettings(settings: DiscordSettings): void {
  discordSettings.set(settings);
  discordPreview.set(displayPrefsToPreview(settings.display_prefs));
}

export async function loadDiscordSettings(): Promise<DiscordSettings> {
  const settings = await getDiscordSettings();
  applyDiscordSettings(settings);
  return settings;
}

export interface Toast {
  id: number;
  message: string;
  type: "info" | "warning" | "danger" | "success";
}

let toastId = 0;
export const toasts = writable<Toast[]>([]);

export function addToast(
  message: string,
  type: Toast["type"] = "info",
  duration = 5000,
): void {
  const id = ++toastId;
  toasts.update((t) => [...t.slice(-2), { id, message, type }]);
  setTimeout(() => {
    toasts.update((t) => t.filter((x) => x.id !== id));
  }, duration);
}

export const activeSessions = derived(sessions, ($s) =>
  $s.filter((s) => !s.is_idle),
);

let prevRateLimits: RateLimitInfo | null = null;

export async function poll(): Promise<void> {
  try {
    const [h, m, s, preview, r, p] = await Promise.all([
      getHealth(),
      getMetrics(),
      getLiveSessions(),
      getDiscordPreview(),
      getRateLimits(),
      getPlanInfo(),
    ]);
    health.set(h);
    metrics.set(m);
    sessions.set(s);
    discordPresencePreview.set(preview);
    rateLimits.set(r);
    planInfo.set(p);

    if (r && prevRateLimits) {
      if (r.five_hour_pct > 80 && prevRateLimits.five_hour_pct <= 80) {
        addToast("Session usage above 80%", "warning");
      }
      if (r.seven_day_pct > 95 && prevRateLimits.seven_day_pct <= 95) {
        addToast("Weekly usage above 95%!", "danger");
      }
      if (
        r.extra_used !== null &&
        prevRateLimits.extra_used !== null &&
        r.extra_used > prevRateLimits.extra_used
      ) {
        addToast(
          `Extra usage charge: $${r.extra_used.toFixed(2)}`,
          "danger",
          8000,
        );
      }
    }
    prevRateLimits = r;
  } catch (e) {
    console.warn("Poll error:", e);
  }
}

export async function refreshDiscordPresencePreview(): Promise<void> {
  try {
    discordPresencePreview.set(await getDiscordPreview());
  } catch (e) {
    console.warn("Discord preview:", e);
  }
}

export async function loadDiscordUser(): Promise<void> {
  try {
    const user = await getDiscordUser();
    discordUser.set(user);
  } catch (e) {
    console.warn("Discord user:", e);
  }
}

let pollInterval: ReturnType<typeof setInterval> | null = null;

export function startPolling(intervalMs = 5000): void {
  poll();
  pollInterval = setInterval(poll, intervalMs);
}

export function stopPolling(): void {
  if (pollInterval) {
    clearInterval(pollInterval);
    pollInterval = null;
  }
}

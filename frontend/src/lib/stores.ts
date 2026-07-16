import { writable, derived } from "svelte/store";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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
  AppSnapshot,
} from "./api";
import {
  getAppSnapshot,
  getDiscordPreview,
  getDiscordSettings,
  getDiscordUser,
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
  showCredits: boolean;
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
  showCredits: true,
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
    showCredits: prefs.show_credits,
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
    show_credits: preview.showCredits,
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
    applySnapshot(await getAppSnapshot());
  } catch (e) {
    console.warn("Snapshot error:", e);
  }
}

function applySnapshot(snapshot: AppSnapshot): void {
    health.set(snapshot.health);
    metrics.set(snapshot.metrics);
    sessions.set(snapshot.sessions);
    discordPresencePreview.set(snapshot.discord_preview);
    rateLimits.set(snapshot.rate_limits);
    planInfo.set(snapshot.plan);
    applyDiscordSettings(snapshot.discord_settings);
    const r = snapshot.rate_limits;
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

let snapshotUnlisten: Promise<UnlistenFn> | null = null;

export function startSnapshotSync(): void {
  if (snapshotUnlisten) return;
  snapshotUnlisten = listen<AppSnapshot>("pulse://snapshot", (event) => {
    applySnapshot(event.payload);
  });
  void snapshotUnlisten
    .then(() => poll())
    .catch((error) => {
      snapshotUnlisten = null;
      console.warn("Snapshot listener:", error);
      void poll();
    });
}

export function stopSnapshotSync(): void {
  const pendingUnlisten = snapshotUnlisten;
  snapshotUnlisten = null;
  void pendingUnlisten?.then((unlisten) => unlisten());
}

import { writable, derived, type Writable } from "svelte/store";
import type {
  HealthResponse,
  MetricsResponse,
  SessionInfo,
  RateLimitInfo,
  DiscordUserInfo,
  DiscordPresencePreview,
  PlanInfo,
} from "./api";
import {
  getHealth,
  getMetrics,
  getLiveSessions,
  getDiscordPreview,
  getRateLimits,
  getDiscordUser,
  getPlanInfo,
  setDiscordDisplayPrefs,
} from "./api";

function persisted<T>(key: string, initial: T): Writable<T> {
  const storage = globalThis.localStorage;
  if (!storage) return writable<T>(initial);
  const stored = storage.getItem(key);
  const parsed = stored !== null ? (JSON.parse(stored) as T) : initial;
  const value =
    typeof initial === "object" &&
    initial !== null &&
    typeof parsed === "object" &&
    parsed !== null &&
    !Array.isArray(initial) &&
    !Array.isArray(parsed)
      ? ({ ...initial, ...parsed } as T)
      : parsed;
  const store = writable<T>(value);
  store.subscribe((v) => storage.setItem(key, JSON.stringify(v)));
  return store;
}

export const health = writable<HealthResponse | null>(null);
export const metrics = writable<MetricsResponse | null>(null);
export const sessions = writable<SessionInfo[]>([]);
export const rateLimits = writable<RateLimitInfo | null>(null);
export const discordUser = writable<DiscordUserInfo | null>(null);
export const discordPresencePreview = writable<DiscordPresencePreview | null>(null);
export const planInfo = writable<PlanInfo | null>(null);
export const currentView = writable<string>("dashboard");

export interface DiscordPreviewSettings {
  showProject: boolean;
  showBranch: boolean;
  showModel: boolean;
  showActivity: boolean;
  showTokens: boolean;
  showCost: boolean;
  showSystems: boolean;
}

export const discordPreview = persisted<DiscordPreviewSettings>("pulse-discord-preview", {
  showProject: true,
  showBranch: true,
  showModel: true,
  showActivity: true,
  showTokens: false,
  showCost: false,
  showSystems: true,
});

let discordPrefsInitialized = false;
discordPreview.subscribe((s) => {
  if (!discordPrefsInitialized) {
    discordPrefsInitialized = true;
  }
  setDiscordDisplayPrefs({
    show_project: s.showProject,
    show_branch: s.showBranch,
    show_model: s.showModel,
    show_activity: s.showActivity,
    show_tokens: s.showTokens,
    show_cost: s.showCost,
    show_systems: s.showSystems,
  })
    .then(refreshDiscordPresencePreview)
    .catch(() => {});
});

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

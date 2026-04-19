export function fmtTokens(n: number): string {
  if (n >= 1e6) return (n / 1e6).toFixed(1) + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(1) + "K";
  return String(n);
}

export function fmtCost(n: number): string {
  return "$" + n.toFixed(2);
}

export function fmtDuration(secs: number): string {
  if (secs < 60) return secs + "s";
  if (secs < 3600) return Math.floor(secs / 60) + "m";
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return h + "h " + m + "m";
}

export function fmtTps(tps: number): string {
  if (tps >= 1000) return (tps / 1000).toFixed(1) + "K/s";
  return tps.toFixed(0) + "/s";
}

export function fmtPct(n: number): string {
  return Math.round(n) + "%";
}

export function usageColor(pct: number): "normal" | "warning" | "danger" {
  if (pct > 80) return "danger";
  if (pct > 50) return "warning";
  return "normal";
}

function parseResetDate(raw: string): Date | null {
  const iso = Date.parse(raw);
  if (!isNaN(iso)) return new Date(iso);
  const match = raw.match(/(\d{1,2}):(\d{2})\s*UTC/);
  if (!match) return null;
  const now = new Date();
  const d = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate(), +match[1], +match[2]));
  if (d.getTime() <= now.getTime()) d.setUTCDate(d.getUTCDate() + 1);
  return d;
}

/// Format an ISO-8601 / RFC3339 timestamp (or `HH:MM` legacy) as local `HH:MM`.
/// Falls back to `—` if the input is missing or unparseable.
export function fmtClock(raw: string | null | undefined): string {
  if (!raw) return "—";
  if (/^\d{1,2}:\d{2}$/.test(raw)) return raw;
  const d = new Date(raw);
  if (Number.isNaN(d.getTime())) return raw;
  return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}

export function formatResetRelative(raw: string): string {
  const reset = parseResetDate(raw);
  if (!reset) return raw;
  const diff = reset.getTime() - Date.now();
  if (diff <= 0) return "Resetting...";
  const mins = Math.floor(diff / 60000);
  if (mins < 60) return `Resets in ${mins} min`;
  const hrs = Math.floor(mins / 60);
  const remMins = mins % 60;
  return `Resets in ${hrs} hr ${remMins} min`;
}

export function formatResetWeekly(raw: string): string {
  const reset = parseResetDate(raw);
  if (!reset) return raw;
  const days = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  const day = days[reset.getDay()];
  const h = reset.getHours();
  const m = reset.getMinutes();
  const ampm = h >= 12 ? "PM" : "AM";
  const h12 = h % 12 || 12;
  const mStr = m.toString().padStart(2, "0");
  return `Resets ${day} ${h12}:${mStr} ${ampm}`;
}

export type ActivityType =
  | "thinking"
  | "editing"
  | "reading"
  | "running"
  | "waiting"
  | "idle";

export function classifyActivity(activity: string): ActivityType {
  const a = activity.toLowerCase();
  if (a.includes("thinking")) return "thinking";
  if (a.includes("edit")) return "editing";
  if (a.includes("read")) return "reading";
  if (a.includes("running") || a.includes("command")) return "running";
  if (a.includes("waiting")) return "waiting";
  return "idle";
}

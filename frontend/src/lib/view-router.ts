import type { Component } from "svelte";
import Dashboard from "../views/Dashboard.svelte";

export const viewIds = [
  "dashboard",
  "sessions",
  "context",
  "costs",
  "reports",
  "discord",
  "settings",
] as const;

export type ViewId = (typeof viewIds)[number];
type LazyViewId = Exclude<ViewId, "dashboard">;
type ViewModule = { default: Component<any> };

const knownViews = new Set<string>(viewIds);
const lazyViews: Record<LazyViewId, () => Promise<ViewModule>> = {
  sessions: () => import("../views/Sessions.svelte"),
  context: () => import("../views/Context.svelte"),
  costs: () => import("../views/Costs.svelte"),
  reports: () => import("../views/Reports.svelte"),
  discord: () => import("../views/Discord.svelte"),
  settings: () => import("../views/Settings.svelte"),
};

export const initialView: Component<any> = Dashboard;

export function normalizeViewId(value: string): ViewId {
  return knownViews.has(value) ? value as ViewId : "dashboard";
}

export async function loadView(viewId: ViewId): Promise<Component<any>> {
  if (viewId === "dashboard") return initialView;
  return (await lazyViews[viewId]()).default;
}

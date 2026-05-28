import { vi } from "vitest";

const helpers = vi.hoisted(() => {
  const emptyArrayCmds = new Set<string>([
    "get_live_sessions",
    "get_session_history",
    "get_session_history_filtered",
    "get_sessions_by_hour_range",
    "search_sessions",
    "get_top_sessions",
    "get_inflection_points",
    "get_hourly_activity",
    "get_daily_stats",
    "get_recommendations",
  ]);
  return {
    pulseInvoke: async (cmd: string): Promise<unknown> => {
      if (emptyArrayCmds.has(cmd)) return [];
      if (cmd === "get_db_size") return 0;
      if (cmd === "get_active_provider") return "claude";
      return undefined;
    },
  };
});

const fakeWindow = {
  minimize: async () => undefined,
  maximize: async () => undefined,
  unmaximize: async () => undefined,
  isMaximized: async () => false,
  close: async () => undefined,
};

if (typeof globalThis !== "undefined") {
  const internals = {
    invoke: vi.fn(helpers.pulseInvoke),
    transformCallback: (cb: unknown) => {
      const id = Math.floor(Math.random() * 1e9);
      (globalThis as Record<string, unknown>)[`_${id}`] = cb;
      return id;
    },
    convertFileSrc: (p: string) => p,
    metadata: { currentWindow: { label: "main" }, currentWebview: { windowLabel: "main", label: "main" } },
  };
  const eventInternals = { unregisterListener: () => undefined };
  (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = internals;
  (globalThis as Record<string, unknown>).__TAURI_EVENT_PLUGIN_INTERNALS__ = eventInternals;
  if (typeof window !== "undefined") {
    const w = window as unknown as Record<string, unknown>;
    w.__TAURI_INTERNALS__ = internals;
    w.__TAURI_EVENT_PLUGIN_INTERNALS__ = eventInternals;
    w.__TAURI__ = { window: { getCurrentWindow: () => fakeWindow } };
  }
}

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(helpers.pulseInvoke),
  convertFileSrc: vi.fn((p: string) => p),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => undefined),
  emit: vi.fn(async () => undefined),
}));

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(async () => "0.0.0-test"),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => fakeWindow),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn(async () => null),
  save: vi.fn(async () => null),
  confirm: vi.fn(async () => true),
  message: vi.fn(async () => undefined),
}));

vi.mock("@tauri-apps/plugin-fs", () => ({
  writeTextFile: vi.fn(async () => undefined),
  readTextFile: vi.fn(async () => ""),
  exists: vi.fn(async () => false),
}));

vi.mock("chart.js/auto", () => ({
  default: class {
    destroy() {}
    update() {}
    resize() {}
  },
}));
vi.mock("chart.js", () => {
  class Chart {
    static register() {}
    destroy() {}
    update() {}
    resize() {}
  }
  return { Chart, registerables: [], default: Chart };
});

if (!("randomUUID" in globalThis.crypto)) {
  let counter = 0;
  Object.defineProperty(globalThis.crypto, "randomUUID", {
    value: () => `00000000-0000-4000-8000-${(counter++).toString().padStart(12, "0")}`,
  });
}

if (typeof Element !== "undefined") {
  Element.prototype.animate = function animateStub() {
    const anim: Record<string, unknown> = {
      onfinish: null,
      oncancel: null,
      cancel() {},
      finish() {},
      play() {},
      pause() {},
      reverse() {},
      persist() {},
      updatePlaybackRate() {},
      currentTime: 0,
      startTime: 0,
      playbackRate: 1,
      effect: null,
      playState: "finished",
      finished: Promise.resolve(),
    };
    return anim as unknown as Animation;
  } as typeof Element.prototype.animate;
}

if (typeof window !== "undefined" && !window.matchMedia) {
  window.matchMedia = ((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
    addListener: () => undefined,
    removeListener: () => undefined,
    dispatchEvent: () => false,
  })) as unknown as typeof window.matchMedia;
}

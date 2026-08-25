import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  initialView,
  loadView,
  normalizeViewId,
} from "@/lib/view-router";

describe("lazy view router", () => {
  it("falls back to the eagerly available dashboard", () => {
    expect(normalizeViewId("dashboard")).toBe("dashboard");
    expect(normalizeViewId("unknown-view")).toBe("dashboard");
    expect(typeof initialView).toBe("function");
  });

  it("resolves non-dashboard routes through dynamic loaders", async () => {
    expect(typeof await loadView("discord")).toBe("function");
    expect(typeof await loadView("settings")).toBe("function");
  });

  it("keeps view implementations out of the app entry module", () => {
    const source = readFileSync(resolve(process.cwd(), "src/App.svelte"), "utf8");
    expect(source).not.toMatch(/from ["']\.\/views\//);
  });
});

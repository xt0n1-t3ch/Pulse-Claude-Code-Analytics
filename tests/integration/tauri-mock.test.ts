import { describe, it, expect } from "vitest";
import { getLiveSessions, getActiveProvider, getContextBreakdown } from "@/lib/api";

describe("tauri IPC mock through the api layer", () => {
  it("resolves list commands to empty arrays", async () => {
    await expect(getLiveSessions()).resolves.toEqual([]);
  });

  it("resolves mapped scalar commands to their stub value", async () => {
    await expect(getActiveProvider()).resolves.toBe("claude");
  });

  it("resolves unmapped commands to undefined without throwing", async () => {
    await expect(getContextBreakdown()).resolves.toBeUndefined();
  });
});

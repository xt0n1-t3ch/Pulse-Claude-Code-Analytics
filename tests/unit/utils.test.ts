import { describe, it, expect } from "vitest";
import {
  fmtTokens,
  fmtCost,
  fmtDuration,
  fmtTps,
  fmtPct,
  usageColor,
  classifyActivity,
  fmtClock,
  fmtPromoEndDate,
} from "@/lib/utils";

describe("fmtTokens", () => {
  it("formats millions, thousands, and units", () => {
    expect(fmtTokens(2_500_000)).toBe("2.5M");
    expect(fmtTokens(1_500)).toBe("1.5K");
    expect(fmtTokens(42)).toBe("42");
  });
});

describe("fmtCost", () => {
  it("renders a two-decimal dollar amount", () => {
    expect(fmtCost(3)).toBe("$3.00");
    expect(fmtCost(12.345)).toBe("$12.35");
  });
});

describe("fmtDuration", () => {
  it("renders seconds, minutes, and hour+minute tiers", () => {
    expect(fmtDuration(45)).toBe("45s");
    expect(fmtDuration(90)).toBe("1m");
    expect(fmtDuration(3 * 3600 + 25 * 60)).toBe("3h 25m");
  });
});

describe("fmtTps", () => {
  it("switches to K/s above a thousand", () => {
    expect(fmtTps(820)).toBe("820/s");
    expect(fmtTps(2_400)).toBe("2.4K/s");
  });
});

describe("fmtPct", () => {
  it("rounds to a whole percent", () => {
    expect(fmtPct(83.4)).toBe("83%");
    expect(fmtPct(99.6)).toBe("100%");
  });
});

describe("usageColor", () => {
  it("maps the three severity tiers", () => {
    expect(usageColor(20)).toBe("normal");
    expect(usageColor(60)).toBe("warning");
    expect(usageColor(95)).toBe("danger");
  });
});

describe("classifyActivity", () => {
  it("maps activity strings to canonical types", () => {
    expect(classifyActivity("Thinking")).toBe("thinking");
    expect(classifyActivity("Editing file.ts")).toBe("editing");
    expect(classifyActivity("Reading foo")).toBe("reading");
    expect(classifyActivity("Running command")).toBe("running");
    expect(classifyActivity("Waiting on input")).toBe("waiting");
    expect(classifyActivity("doing nothing")).toBe("idle");
  });
});

describe("fmtClock", () => {
  it("passes through HH:MM and falls back to an em dash on empty input", () => {
    expect(fmtClock("14:30")).toBe("14:30");
    expect(fmtClock(null)).toBe("—");
    expect(fmtClock(undefined)).toBe("—");
  });
});

describe("fmtPromoEndDate", () => {
  it("renders the exclusive UTC cutoff instant as the last inclusive calendar day", () => {
    expect(fmtPromoEndDate("2026-09-01T00:00:00+00:00")).toBe("Aug 31, 2026");
    expect(fmtPromoEndDate("2026-09-01T00:00:00Z")).toBe("Aug 31, 2026");
  });

  it("falls back to the raw string on an unparseable date", () => {
    expect(fmtPromoEndDate("not-a-date")).toBe("not-a-date");
  });
});

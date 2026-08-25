import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const VERSION = "1.7.7";

describe("Pulse version contract", () => {
  const read = (path: string) => readFileSync(resolve(process.cwd(), "..", path), "utf8");

  it("keeps every product and release owner on v1.7.7", () => {
    expect(read("Cargo.toml")).toContain(`version = "${VERSION}"`);
    expect(read("src-tauri/Cargo.toml")).toContain(`version = "${VERSION}"`);
    expect(JSON.parse(read("src-tauri/tauri.conf.json")).version).toBe(VERSION);
    expect(JSON.parse(read("frontend/package.json")).version).toBe(VERSION);
    expect(JSON.parse(read("frontend/package-lock.json")).version).toBe(VERSION);
    expect(JSON.parse(read("package.json")).version).toBe(VERSION);
    expect(JSON.parse(read("scripts/release-contract.json")).product.version).toBe(VERSION);
    const upstream = JSON.parse(read("src/codex/UPSTREAM.json"));
    expect(upstream.compatibility.pulse).toBe(VERSION);
    expect(upstream.canonical_release).toBe("v1.10.3");
    expect(upstream.canonical_commit).toBe("9d20ffdb1c4ec6fa37edc00952badd041ec5bc02");
    expect(read("CHANGELOG.md")).toContain(`## [${VERSION}] - 2026-08-25`);
  });
});

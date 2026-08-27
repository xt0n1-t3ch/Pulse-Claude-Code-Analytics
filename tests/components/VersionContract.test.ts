import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const VERSION = "1.7.9";

describe("Pulse version contract", () => {
  const read = (path: string) => readFileSync(resolve(process.cwd(), "..", path), "utf8");

  it("keeps every product and release owner on v1.7.9", () => {
    expect(read("Cargo.toml")).toContain(`version = "${VERSION}"`);
    expect(read("src-tauri/Cargo.toml")).toContain(`version = "${VERSION}"`);
    expect(JSON.parse(read("src-tauri/tauri.conf.json")).version).toBe(VERSION);
    expect(JSON.parse(read("frontend/package.json")).version).toBe(VERSION);
    expect(JSON.parse(read("frontend/package-lock.json")).version).toBe(VERSION);
    expect(JSON.parse(read("package.json")).version).toBe(VERSION);
    expect(JSON.parse(read("scripts/release-contract.json")).product.version).toBe(VERSION);
    const upstream = JSON.parse(read("src/codex/UPSTREAM.json"));
    expect(upstream.compatibility.pulse).toBe(VERSION);
    expect(upstream.canonical_release).toBe("v1.10.2");
    expect(upstream.canonical_commit).toBe("a508507e0849fd5c9e09c7d1c55eebe2d199cfc0");
    expect(read("CHANGELOG.md")).toContain(`## [${VERSION}] - 2026-08-27`);
  });
});

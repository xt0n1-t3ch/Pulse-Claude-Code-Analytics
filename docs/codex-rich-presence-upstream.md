# Canonical Codex presence core

Pulse consumes Codex telemetry and Rich Presence composition from the
standalone [Codex Discord Rich Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence)
repository. `codex-presence-core` is the UI-free owner. Pulse owns Tauri
integration, analytics persistence, and presentation; it must not recreate
parsing or Discord line composition in TypeScript or a second Rust module.

## Promoted contract

Pulse **v1.7.2** consumes `codex-presence-core` **2.0.0** from the immutable
upstream **v1.10.2** release at commit
`a508507e0849fd5c9e09c7d1c55eebe2d199cfc0`.

| Surface | Promoted value | Release proof |
| --- | --- | --- |
| Core package | `codex-presence-core` 2.0.0 | Published upstream v1.10.2 release |
| Git dependency | Canonical repository plus full `rev` | Cargo manifests and lockfile resolve to the same commit |
| Canonical manifest | v1.10.2 + exact commit | `src/codex/UPSTREAM.json` matches the Cargo Git pin |
| Presence config | Schema 13 | Migration fixtures pass |
| Pulse database | Schema 5 | Migration and query-plan fixtures pass |

Branches, shortened SHAs, path dependencies, and unpeeled tag objects are not
release identities. The release contract requires the exact commit reached by
the annotated upstream tag.

## Source and compatibility boundary

The core exports model identity, reasoning effort, semantic usage snapshots,
quota scopes/windows, Credits, service tier, configuration layout, and
deterministic Rich Presence composition. Pulse may translate those DTOs into
Tauri responses but may not reinterpret positional limits, guess unsupported
pricing, or infer unavailable provider capabilities.

Files under `src/codex/` are Pulse-owned adapters or compatibility surfaces
listed in `src/codex/UPSTREAM.json`. The manifest records schema compatibility
and the immutable upstream identity. `scripts/check-codex-rich-presence-upstream.ps1`
checks the manifest against the Cargo dependency before a release is accepted.

## Validation

Run the complete local gate before promotion:

```powershell
npm run verify
npm --prefix frontend run build
pwsh -File scripts/e2e/run-pulse.ps1 -Mode Tauri -RunPlaywright
```

Browser QA uses the authenticated loopback bridge and real local history; the
Tauri runner uses an isolated profile so it cannot collide with installed
Pulse. Packaging must then produce exact-version NSIS/MSI installers, a
validated Windows SPDX SBOM, and a checksum manifest.

## Future upstream updates

1. Publish and verify a new immutable upstream release.
2. Record its peeled commit in both Cargo manifests and
   `src/codex/UPSTREAM.json`.
3. Refresh `Cargo.lock` and bump the Pulse compatibility version.
4. Run upstream, model, parser, analytics, localhost, Tauri, SBOM, and checksum
   gates again.
5. Publish a new Pulse patch or minor release; never move an existing tag or
   replace an immutable asset.

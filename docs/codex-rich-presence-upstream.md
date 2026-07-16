# Canonical Codex presence core

Pulse consumes Codex telemetry and Rich Presence composition from the standalone [Codex Discord Rich Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence) repository. `codex-presence-core` is the UI-free owner. Pulse owns Tauri integration, analytics persistence, and presentation; it must not recreate parsing or Discord line composition in TypeScript or a second Rust module.

## Versioned contract

| Surface | Local v1.6 candidate | Promotion requirement |
| --- | --- | --- |
| Core package | `codex-presence-core` 1.0.0 | Same version from canonical `v1.8.0` |
| Config schema | 13 | Migration fixtures pass from schema 12 |
| Pulse database schema | 5 | Migration and query-plan fixtures pass |
| Development dependency | Local `path` worktree | Replace with canonical Git URL plus full 40-character `rev` |
| Canonical manifest | Local candidate metadata | Core version, release tag, and commit equal the Cargo Git pin |

The path dependency is intentional only while both worktrees are under local validation. A Pulse release must fail until the canonical release exists and the dependency uses its exact commit SHA. Moving branches, tags without a `rev`, and shortened SHAs are not release inputs.

## Source and compatibility boundary

The core exports semantic usage snapshots, quota scopes/windows, Credits, service tier, configuration layout, and deterministic Rich Presence composition. Pulse may translate those DTOs into Tauri responses but may not reinterpret positional `primary`/`secondary` limits or infer unavailable provider capabilities.

Canonical code still carried under `src/codex/` is migration residue unless it is an explicit Pulse adapter. The core manifest records compatibility with schema 13 and the immutable canonical commit. The release gate checks that manifest against Cargo before any bundle is published.

## Local validation

During the local phase:

```powershell
cargo test --workspace
npm --prefix frontend run check
npm --prefix frontend run test
npm --prefix frontend run build
```

The local path is allowed here so canonical and Pulse changes can be validated together without publishing either repository.

## Promotion sequence

1. Complete canonical v1.8.0 validation and obtain explicit approval.
2. Publish the annotated canonical tag/release and record its full commit SHA.
3. Replace Pulse's path dependency with the canonical Git dependency and exact `rev`.
4. Update the canonical manifest with core 1.0.0, tag v1.8.0, schema compatibility, and the same SHA.
5. Run the full Pulse gates again, including Windows runtime, migrations, Dark/Light viewports, performance, SPDX SBOM, and checksums.
6. Only then approve the annotated Pulse v1.6.0 tag.

No tag, pull request, or release is created by the local validation phase.

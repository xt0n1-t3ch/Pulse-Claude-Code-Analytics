# Canonical Codex presence core

Pulse consumes Codex telemetry and Rich Presence composition from the standalone [Codex Discord Rich Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence) repository. `codex-presence-core` is the UI-free owner. Pulse owns Tauri integration, analytics persistence, and presentation; it must not recreate parsing or Discord line composition in TypeScript or a second Rust module.

## Versioned contract

The current consumer is the Pulse **v1.7.0** release candidate. The upstream
repository is being validated as a **v1.9.0 / `codex-presence-core` 2.0.0**
candidate; that source is local validation material, not a published tag or
release. No upstream commit or tag is promotion evidence until the upstream
owner pushes it and the consumer records the immutable revision.

| Surface | Local v1.7.0 candidate | Promotion requirement |
| --- | --- | --- |
| Core package | `codex-presence-core` 2.0.0 candidate from upstream v1.9.0 work | Same 2.0.0 package from a published upstream v1.9.0 release |
| Config schema | 13 | Migration fixtures pass from schema 12 |
| Pulse database schema | 5 | Migration and query-plan fixtures pass |
| Development dependency | Git dependency at the candidate revision | Canonical Git URL plus the pushed full 40-character `rev` |
| Canonical manifest | Candidate metadata only | Core version, release tag, and commit equal the Cargo Git pin |

The candidate dependency is allowed only while both worktrees are under local
validation. A Pulse release must fail until the upstream release exists and the
dependency uses its exact pushed commit SHA. Moving branches, tags without a
`rev`, and shortened SHAs are not release inputs.

## Source and compatibility boundary

The core exports semantic usage snapshots, quota scopes/windows, Credits, service tier, configuration layout, and deterministic Rich Presence composition. Pulse may translate those DTOs into Tauri responses but may not reinterpret positional `primary`/`secondary` limits or infer unavailable provider capabilities.

Code still carried under `src/codex/` is migration residue unless it is an
explicit Pulse adapter. During candidate validation, the upstream manifest is
an integration record and must not be presented as proof of a published
release. After promotion it records compatibility with schema 13 and the
immutable upstream commit; the release gate then checks that manifest against
Cargo before any bundle is published.

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

1. Complete local v1.9.0 / `codex-presence-core` 2.0.0 candidate validation and obtain explicit approval.
2. After the upstream owner publishes the annotated v1.9.0 release, record its full commit SHA.
3. Replace the candidate dependency with the canonical Git dependency and exact pushed `rev`.
4. Update the canonical manifest with core 2.0.0, tag v1.9.0, schema compatibility, and the same SHA.
5. Run the full Pulse gates again, including Windows runtime, migrations, Dark/Light viewports, performance, SPDX SBOM, and checksums.
6. Only then approve the annotated Pulse v1.7.0 tag.

No tag, pull request, or release is created by the local validation phase.

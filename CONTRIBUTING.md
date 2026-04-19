# Contributing to Pulse

Thanks for your interest in Pulse. Every contribution — code, docs, bug reports, feature ideas — makes the project better.

## Ground rules

- Be kind. See the [Code of Conduct](CODE_OF_CONDUCT.md).
- One logical change per PR. Small PRs merge faster.
- Every PR must pass CI (`fmt`, `clippy -D warnings`, `cargo test --workspace`, frontend build).
- Add or update tests where behavior changes.
- No telemetry, ever. Pulse is local-first by design.

## Dev setup

### Prereqs
- Rust (stable, 2024 edition)
- Node.js 20+
- Platform Tauri deps — see [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

### Build & run
```bash
# frontend dev server
cd frontend && npm install && npm run dev

# in another terminal: Tauri app (connects to the dev server)
cd src-tauri && cargo tauri dev
```

### Tests
```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Commit style

Conventional Commits:
- `feat:` new feature
- `fix:` bug fix
- `docs:` documentation
- `perf:` performance
- `refactor:` non-behavioral code change
- `test:` tests
- `chore:` tooling / deps

## Pull requests

1. Fork → branch off `main` with a descriptive name (`feat/linux-tray`).
2. Make your changes. Keep the diff focused.
3. Run the full CI suite locally.
4. Open a PR. Link the issue it closes. Fill in the PR template.
5. A maintainer will review. Expect suggestions; don't take them personally.

## Releasing (maintainers)

1. Bump versions in `Cargo.toml`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `frontend/package.json`.
2. Update `CHANGELOG.md`.
3. Commit: `chore(release): vX.Y.Z`.
4. Tag: `git tag vX.Y.Z && git push origin main --tags`.
5. The Release workflow builds tri-OS bundles and publishes automatically.

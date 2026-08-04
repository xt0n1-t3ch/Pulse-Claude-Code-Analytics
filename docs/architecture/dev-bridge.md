# Pulse browser development bridge

## Introduction

This reference documents the debug-only HTTP seam that lets the Vite browser
render Pulse from the same Rust state as the Tauri window. It is a development
transport, not a production API and not a fixture provider.

## Table of contents

- [Contract](#contract)
- [Running the standalone bridge](#running-the-standalone-bridge)
- [Proof and limits](#proof-and-limits)

## Contract

- Bind: `127.0.0.1:1421` only.
- Request: authenticated `POST /invoke` with JSON
  `{ "command": "get_metrics", "args": { ... } }`.
- Authentication: `Authorization: Bearer <token>`, where the token is supplied
  through `PULSE_DEV_BRIDGE_TOKEN`. An unset token disables the listener.
- CORS: only `http://localhost:1420` and `http://127.0.0.1:1420`; wildcard
  origins are never emitted.
- Dispatch: an explicit allowlist calls `src-tauri/src/commands.rs` and the
  shared analytics database/live poller. Safe authenticated settings,
  notification, refresh, and history-confirmation controls use the same real
  commands; shell/open-url actions remain unavailable. Unknown commands return
  `404`; a real backend error returns `503`.
- Access snapshots include the SQLite-derived `local_history` capability for
  each discovered provider route. That field makes local analytics selectable;
  it does not create provider proof, quota windows, plan detection, or Discord
  authority.
- Cost totals retain provider-scoped token categories when monetary coverage is
  partial or unavailable. Consumers must continue to honor `cost_basis` and
  `priced_sessions`; the bridge never converts missing subscription spend to
  zero.

## Running the standalone bridge

For the normal browser development workflow, use the repository-owned launcher:

```powershell
cd frontend
bun run dev
```

It creates one random per-run token, exports it to the programmatic Vite
configuration, builds and starts the hidden Rust bridge, and waits for the
authenticated backend before binding the UI on `127.0.0.1:1420`. Opening the
local UI therefore either renders real backend state or fails closed; there is
no browser fixture fallback.

For low-level bridge debugging, set the same token in both processes, then run
the backend from the repository root:

```powershell
$env:PULSE_DEV_BRIDGE_TOKEN = "a-long-local-secret"
cargo run -p pulse --bin pulse-dev-bridge
```

The standalone process starts the real background poller before accepting
requests. It is debug-only and exits when the token is missing. The Tauri debug
binary can also start the listener through its normal `setup` path when the
same environment variable is present.

## Proof and limits

`src-tauri/src/dev_bridge.rs` tests the loopback address, token rejection,
strict origin allowlist, real command dispatch, and explicit unavailable/unknown
paths. Browser rendering proves the bridge seam only, not provider production
availability or Discord IPC. A live local-history count proves that Pulse can
read stored sessions for that provider; only a fresh authenticated route proves
provider allowances.

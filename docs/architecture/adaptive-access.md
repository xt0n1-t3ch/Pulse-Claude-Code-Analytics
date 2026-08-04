# Adaptive access fixture contract

## Introduction

This reference is for maintainers validating provider access presentation without
calling a provider or Discord. It records the versioned fixture envelope, the
consumer DTO shape, and the exact boundary between mocked frontend tests and
live desktop probes. The fixtures are synthetic and contain no credentials or
personal data.

## Fixture envelope

`tests/fixtures/providers/manifest.json` is the inventory owner. Every listed
fixture is a `fixture.json` with these required fields:

| Field | Contract |
| --- | --- |
| `fixture_version` | Integer matching the manifest version (`1` today). |
| `raw_payload` | Sanitized provider/IPC-shaped JSON; never a live response. |
| `status` | Scenario outcome (`ok`, `unauthorized`, `upstream_error`, `no_data`, `stale`, `window_removed`, or `proof`). |
| `source_contract` | Versioned source identity such as `codex_subscription.v1` or `discord_ipc.v1`; it is a fixture contract, not an endpoint claim. |
| `captured_at` | Fixed ISO-8601 timestamp identifying the synthetic capture. |
| `expected_dto` | Expected consumer DTO or `{ "routes": [...] }` aggregate. |

The inventory covers Codex and Claude subscriptions, negative OpenAI and
Anthropic API outcomes, hybrid/no-data/stale/window-removal states, and Discord
IPC proof. API negative fixtures describe an error response only; they do not
contain a placeholder API key.

## Access route DTO

An authenticated route is represented by the following fields. `source.kind`
keeps subscription access separate from API access (`codex_subscription`,
`open_ai_api`, `claude_subscription`, `anthropic_api`). `availability` and
`freshness` are independent: stale data may remain visible as provenance while
its numeric window percentage is not displayable. A missing route is represented
by an empty `routes` array, not a fabricated zero.

```json
{
  "source": {
    "id": "codex-subscription:fixture",
    "kind": "codex_subscription",
    "provider": "codex",
    "auth_method": "app_server",
    "proof": "quota_response",
    "plan": "Pro 20x"
  },
  "availability": "available",
  "freshness": "fresh",
  "provenance": "app_server",
  "observed_at": "2026-08-01T00:00:00Z",
  "fetched_at": "2026-08-01T00:00:00Z",
  "expires_at": "2026-08-01T00:00:30Z",
  "windows": [],
  "credits": null,
  "extra_usage": null,
  "error": null
}
```

This is an expected consumer shape only. The fixture runner does not invent or
call a backend endpoint, and it does not replace the canonical Rust/Tauri
command contract.

## Source identity and analytics scope

Pulse keeps two keys deliberately separate:

- `source.id` is the stable account/access-lane identity used for quotas,
  freshness, provenance, reset credits, and source diagnostics.
- `source.provider` is the intentional analytics aggregation key passed to
  history, summary, context, cost, forecast, hourly, and report commands.

The supported provider scopes are `codex`, `claude`, `openai`, `anthropic`, and
the explicit aggregate `all`. API lanes are never aliases for subscription
session stores: selecting `openai` cannot show Codex subscription sessions, and
selecting `anthropic` cannot show Claude subscription sessions. If an API lane
has no provider-owned session telemetry, its analytics result is empty or
unavailable rather than borrowed from another lane. `all` is the only scope
that combines providers.

## Runner boundaries

Use `scripts/e2e/run-pulse.ps1` for a bounded probe or native Playwright run. It stages temporary
`CLAUDE_HOME`, `CODEX_HOME`, SQLite path, provider config, and fixture copy, and
restores environment variables on exit.

- **Browser** starts the repo Vite dev server on an owned loopback port and
  probes `/`. It proves static/browser startup only; Vitest remains the owner
  of mocked component and IPC tests.
- **Tauri** requires the repo-owned
  `target/debug/pulse.exe`, adds a temporary WebView2 CDP port and isolated
  `WEBVIEW2_USER_DATA_FOLDER`, then probes `/json/version`. Pass
  `-RunPlaywright` to run the Tauri spec through `chromium.connectOverCDP` while
  the owned process remains alive. A missing binary or CDP endpoint is a clear
  blocker; the runner never inspects or stops an installed `pulse.exe`.
- **Discord** is opt-in with `PULSE_DISCORD_LIVE=1`. Discord must already be
  running, and `-DiscordProofPath` (or `PULSE_DISCORD_PROOF`) must contain raw
  `SET_ACTIVITY` plus a clear-activity event. The runner writes a validated
  proof artifact and never starts, restarts, or stops Discord.

All spawned processes are recorded with PID, creation time, executable path,
and command line. Cleanup acts only when that identity still matches. No
global process-name kill is used.

## Cost exactness boundary

Cost is not a field on an access route. The two additive metrics vectors live
under `tests/fixtures/metrics/` so a quota fixture cannot accidentally invent a
price. `true_zero.json` carries `cost: 0`, `cost_available: true`, and
`cost_basis: "exact"`; `unavailable.json` carries a transport zero with
`cost_available: false` and `cost_basis: "unavailable"`. Consumers must keep
those states distinct and must not emit a Discord cost field for the unavailable
case. The contract probe validates both vectors on every runner invocation.

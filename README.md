# Pulse

Local analytics and Discord Rich Presence for Claude Code, Codex and OpenCode. Pulse v1.8.1 keeps live activity, historical usage and account limits separate, so an old session or a missing price never becomes a live metric.

<div align="center">
<picture>
  <source media="(prefers-color-scheme: light)" srcset="assets/pulse-logo-dual-light.png">
  <img src="assets/pulse-logo-dual-dark.png" alt="Pulse analytics" width="560" height="124">
</picture>

[![Release v1.8.1](https://img.shields.io/badge/Release-v1.8.1-171717)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/latest)
[![Apache-2.0](https://img.shields.io/badge/License-Apache--2.0-171717)](LICENSE)

[Download](#install) · [What is new](#whats-new-in-v180) · [Use Pulse](#use-pulse) · [Build](#build-from-source) · [Documentation](docs/index.md)
</div>

## What's new in v1.8.1

- Native OpenCode collection from local SQLite, with arbitrary model providers, mixed-model history and reported cost provenance.
- OpenCode Go account limits for the five-hour, weekly and monthly windows. These limits do not stand in for another provider's allowance.
- GPT-6 Astra identity, context and Standard/Fast pricing, with incomplete monetary coverage kept explicit.
- A seven-day provider-scoped dashboard, content-sized account limits and consistent light/dark controls.
- A notification center with read/unread actions, bulk controls, confirmation and persistent Undo for the last confirmed clear.
- Windows Efficiency mode through EcoQoS and reduced process priority.
- Model-first OpenCode presence, independent field toggles, saved order and two-decimal currency. Completed sessions stop publishing instead of lingering as active work.

See the [changelog](CHANGELOG.md) for the release history.

## Install

Download an asset for your platform from [GitHub Releases](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/latest). The local Windows release includes:

| Asset | Use |
| --- | --- |
| `Pulse_1.8.1_x64-setup.exe` | Windows x64 installer |
| `Pulse_1.8.1_x64_en-US.msi` | Windows x64 MSI package |
| `pulse-windows-x64.spdx.json` | Software bill of materials |
| `SHA256SUMS.txt` | SHA-256 checksums for the release files |

Platform availability follows the files attached to each release. The Rust/Tauri source supports Windows, macOS and Linux; a source target is not proof that a binary for that target was published.

For Windows installation through the repository script:

```powershell
irm https://raw.githubusercontent.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/main/scripts/install.ps1 | iex
```

Manual-download releases do not include a signed updater manifest unless that manifest appears in their asset list. Use the installer when the in-app updater cannot offer the release.

## Use Pulse

Start your coding client, then open Pulse. Use the provider bar to select Claude, Codex, OpenCode or combined analytics.

| View | Purpose |
| --- | --- |
| Home | Active sessions, provider limits, cache ratio and the last seven days of activity |
| Sessions | Live sessions and searchable, filtered history |
| Usage & cost | Provider-billed spend and API-equivalent value, with their provenance |
| Reports | Provider-supported analysis and exports |
| Discord | Broadcast application, presets, field order and the backend-owned preview |
| Settings | Provider, plan display, appearance, window behavior and data controls |

Choosing a provider changes its workspace context. **All providers** combines analytics without changing the last broadcast application. The **Broadcast from** control in Discord makes publication ownership explicit.

A completed OpenCode session stays in history but leaves the live focus and Discord publisher. **Idle** does not borrow the last model, token count or monetary value. Unknown cost is unavailable; a provider-reported zero is displayed as `$0.00`.

### OpenCode and Go

Pulse reads `opencode.db` and channel databases from OpenCode's local data directory. It supports both message-table schemas and respects `XDG_DATA_HOME` and `OPENCODE_DB`. Additional databases can be set in `~/.claude/pulse-opencode.json`.

When an existing OpenCode Go credential is available, Pulse requests its account usage from the Go API. Pulse does not copy that credential into its own settings or logs. The monthly window uses the provider's reset date rather than an invented fixed duration.

Desktop, CLI and OpenChamber can share this local store. Unknown client attribution remains OpenCode; a managed OpenChamber label requires matching process identity. Remote stores on other machines are outside this local integration. Read the [OpenCode and Astra guide](docs/opencode-and-astra.md) for the source and absence rules.

### Discord controls

Open Discord, choose a broadcast application in Pulse, and enable Rich Presence. Use **Minimal**, **Standard** or **Full**, then adjust individual fields. OpenCode defaults to model, activity, project and branch before its numeric fields. Field order controls priority within each Discord line.

Unavailable fields stay disabled. Go quotas identify Go explicitly and appear only while their data is fresh and the field is enabled. The profile preview uses the same Rust compositor as the publisher. The connected user's name and avatar come from local Discord IPC; an unavailable banner is not fabricated.

### Notifications

The bell opens the local notification history. You can mark one item or all items as read/unread, dismiss an item, or clear the list. **Clear all** requires confirmation and preserves the records. **Undo** restores the last confirmed clear and its original read states. The last Undo receipt survives an app restart on the same client.

If Pulse cannot refresh the history, it labels the retained snapshot and disables bulk changes until the connection recovers.

### Windows Efficiency mode

Pulse requests EcoQoS and Idle process priority at startup. Windows owns the Task Manager leaf indicator; Pulse does not draw a substitute. Other operating systems keep their normal scheduling behavior.

Set this environment variable before launch to opt out:

```powershell
$env:PULSE_EFFICIENCY_MODE = "0"
```

See [Windows Efficiency mode](docs/windows-efficiency.md) for behavior and the read-only verification command.

## Your data and network access

Session analytics stay in `~/.claude/pulse-analytics.db`, or under `CLAUDE_HOME` when configured. Pulse reads local Claude/Codex transcripts and OpenCode SQLite records. It sends no analytics telemetry or transcript uploads.

Network use is limited to the configured provider quota checks, release/update checks and the Discord presence fields you enable. Project, branch and activity controls determine what the presence exposes. Keep private prompts, credentials and local reports out of public issues.

On Windows, WSL session discovery is off by default. Enable it only when you need those transcript roots:

```powershell
$env:CC_PRESENCE_INCLUDE_WSL = "1"
```

## Build from source

Install Rust, Node.js, the Tauri CLI and the platform build prerequisites. Then run:

```powershell
git clone https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics.git
cd Pulse-Claude-Code-Analytics
npm --prefix frontend ci
npm run verify
npm run build:portable
```

`build:portable` compiles the frontend and runs `cargo tauri build --no-bundle`. The Tauri CLI enables `tauri/custom-protocol`, so the executable contains its UI. Do not promote a raw Cargo GUI build that still points at the development URL.

Use `npm run build` for installers. Use `npm run dev` for the authenticated, loopback-only browser development bridge. The development bridge is not required by the installed application.

## Compatibility and recovery

Pulse v1.8.1 uses Claude config schema 6, Codex config schema 13 and analytics schema 6. The analytics migration adds OpenCode metadata without discarding previous sessions.

Back up the executable, configuration and database through SQLite Backup before replacing an installation. Pulse 1.7.9 cannot open analytics schema 6: rollback requires the schema-5 backup, while the newer database should be preserved separately.

Pulse consumes `codex-presence-core` 2.0.0 through an immutable Git pin recorded in [UPSTREAM.json](src/codex/UPSTREAM.json). The standalone [Codex Discord Rich Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence) runtime has its own releases and terminal interface.

See the [dependency audit scope](docs/dependency-audit.md) for inherited Tauri advisories and platform boundaries.

## Contribute and report problems

Read [CONTRIBUTING.md](CONTRIBUTING.md), the [test map](tests/index.md), [release procedure](docs/releasing.md) and [Code of Conduct](CODE_OF_CONDUCT.md). Report security issues privately through [GitHub Security Advisories](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/security/advisories/new).

## License

[Apache-2.0](LICENSE), copyright 2026 xt0n1-t3ch. Redistributed and derivative versions must preserve the license, copyright notice and [NOTICE](NOTICE) attribution.

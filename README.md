<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/pulse-logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/pulse-logo-light.svg">
  <img src="assets/pulse-logo-dark.svg" alt="Pulse — Claude Code Analytics" width="560">
</picture>

### See where every Claude Code dollar actually goes.

The open-source **Claude Code &amp; OpenAI Codex analytics dashboard** + **Discord Rich Presence** for devs on **Claude Pro / Max / Teams** and **ChatGPT Plus / Pro / Business**.<br>Grade your cache A – F, catch runaway sessions before they burn your plan, and copy one-click <i>Fix with Claude Code</i> prompts. Native desktop. 100 % local. Zero telemetry.

[![CI](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/actions/workflows/ci.yml/badge.svg)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/actions/workflows/ci.yml)
[![Release](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/actions/workflows/release.yml/badge.svg)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/actions/workflows/release.yml)
[![Latest](https://img.shields.io/github/v/release/xt0n1-t3ch/Pulse-Claude-Code-Analytics?color=0a0a0a&label=latest&logo=github)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/xt0n1-t3ch/Pulse-Claude-Code-Analytics/total?color=0a0a0a&logo=github)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases)
[![License](https://img.shields.io/badge/license-Apache%202.0-0a0a0a.svg)](LICENSE)
[![Stars](https://img.shields.io/github/stars/xt0n1-t3ch/Pulse-Claude-Code-Analytics?style=flat&color=0a0a0a&logo=github)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/stargazers)
[![Sponsor](https://img.shields.io/badge/sponsor-%E2%9D%A4-0a0a0a?logo=githubsponsors)](https://github.com/sponsors/xt0n1-t3ch)

<a href="#install"><b>Download</b></a>&nbsp; · &nbsp;<a href="#whats-new"><b>What's New</b></a>&nbsp; · &nbsp;<a href="#about"><b>About</b></a>&nbsp; · &nbsp;<a href="#screenshots"><b>Screenshots</b></a>&nbsp; · &nbsp;<a href="#features"><b>Features</b></a>&nbsp; · &nbsp;<a href="docs/"><b>Docs</b></a>&nbsp; · &nbsp;<a href="https://github.com/sponsors/xt0n1-t3ch"><b>Sponsor</b></a>

</div>

---

<h2 id="whats-new"><img src="assets/icons/sparkles.svg" alt="" width="28" align="center"> &nbsp;What's New in v1.2.0</h2>

- **Claude Fable 5 + Mythos 5** — official pricing ($10 input / $50 output per MTok), 1M context by default, 128K max output, cache-write/read rates, and clean Discord labels: `Fable 5 (1M)` / `Mythos 5 (1M)`.
- **All active Context Windows at once** — the Context tab now shows every live session as a top card, then lets you click into the detailed breakdown below. Historical per-session rows use the last context snapshot and clamp at 100%, not lifetime token totals.
- **In-app release check popup** — Pulse now compares the packaged version against GitHub Releases, shows a polished current → latest update banner, supports release notes, Later, Skip version, Open release, and a manual Settings check.
- **Codex Rich Presence stays fresh** — Pulse syncs its Codex presence core from our sibling repo, [Codex Discord Rich Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence), so the standalone Codex project gets visibility and Pulse keeps the latest Codex RP logic. Windows probes use no-console process spawning.
- **Active-session aware previews** — Discord preview and context fallbacks now prefer live sessions before historical rows; actual Rich Presence still publishes one primary payload because Discord only accepts one presence.
- **Provider-accurate plan labels** — usage-limit copy now says 5-hour window instead of pretending Anthropic's usage API window is literally one session.
- **Repo hygiene pass** — docs/test index refreshed, frontend package version aligned to v1.2.0, and stale Markdown issue templates removed in favor of YAML templates.

**[Download v1.2.0](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/latest)** &nbsp;·&nbsp; **[Full changelog](CHANGELOG.md)**

<h2 id="about"><img src="assets/icons/info.svg" alt="" width="28" align="center"> &nbsp;About</h2>

You pay for Claude Code every month. Maybe $20 on **Pro**, $100 – $200 on **Max**, or a seat on **Teams**. And yet, if someone asked *"which of your sessions burned the most context this week?"* or *"what percentage of your tokens are actually cache hits?"* — you probably can't answer.

**Pulse answers.** It reads Claude Code's own JSONL transcripts (zero-config — just install and launch) and surfaces the things that actually move the needle on your plan:

- **A – F cache-health letter grade** — trend-weighted, so you see the *direction* your cache efficiency is heading, not just today's number.
- **Opus-4.7-tokenizer aware** — Opus 4.7's new tokenizer inflates tokens by up to 35 %; Pulse flags the inflation so you know when you're hitting limits faster than expected.
- **1 M context GA pricing** — flat per-token rate across the full 1 M window for **Fable 5 · Mythos 5 · Opus 4.6 · Opus 4.7 · Opus 4.8 · Sonnet 4.6** (per [Anthropic's official pricing](https://platform.claude.com/docs/en/about-claude/pricing)). Older betas (Sonnet 4 / 4.5, Opus 4 / 4.5) still get 2× input · 1.5× output · 2× cache at > 200 K. Pulse applies the correct math per-model so you compare sessions like-for-like. Plan-level **Extra Usage** on Pro / Max / Teams is tracked separately.
- **Inflection alerts** — any session that blows past 2 × your rolling baseline gets flagged with context and a suggested fix.
- **Fix-with-Claude prompts** — every recommendation has a **Copy Fix Prompt** button. Paste it into Claude Code. Problem fixed.
- **Plan usage limits** — live tracking of your 5-hour window, weekly, Sonnet-only, and Extra Usage quotas. Sound alert when Extra Usage spikes.
- **Release awareness** — startup and 6-hour update checks surface new stable GitHub Releases inside the app without pretending a signed auto-installer exists before release metadata is published.
- **Discord Rich Presence** — five-tier reasoning effort, live project / model / branch. Your flow state, on your profile.

**Works with OpenAI Codex too.** Flip the provider toggle and Pulse reads your Codex CLI sessions the same way — per-model pricing for the GPT-5 family **including GPT-5.5** ($5 / $30 per Mtok), 400 K context tracking, and reasoning-effort detection. **Fast mode** (`/fast`) is priced correctly on both sides: Codex **GPT-5.5 bills at 2.5×** and **GPT-5.4 at 2×** the standard rate, and Claude **Opus 4.8 Fast** bills at **2×** — each flagged with a ⚡ marker in Sessions and Discord presence.

Codex Discord Rich Presence has its own source-of-truth repo: **[xt0n1-t3ch/Codex-Discord-Rich-Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence)**. Pulse mirrors that Rust core into `src/codex/` through checked sync scripts and CI freshness gates, so Codex support can move fast here while the standalone Rich Presence project keeps its own audience.

Written in **Rust** + **Tauri 2** + **Svelte 5**. ≈ 12 MB on Windows, ≈ 18 MB on macOS. Cold-starts in under 200 ms. One-click installers for Windows (NSIS + MSI), macOS (DMG — Apple Silicon + Intel), and Linux (deb, rpm, AppImage). Apache-2.0 licensed (attribution required — see [`NOTICE`](NOTICE)). The data never leaves your machine.

<h2 id="screenshots"><img src="assets/icons/image.svg" alt="" width="28" align="center"> &nbsp;Screenshots</h2>

<div align="center">

<img src="assets/screenshots/dashboard.png" alt="Pulse dashboard — Claude Code cost, tokens, cache health grade, plan usage limits, activity heatmap" width="900">

<sub><b>Dashboard</b> — at-a-glance cost · tokens · cache-hit ratio · plan limits · extra usage · activity heatmap.</sub>

<br><br>

<img src="assets/screenshots/reports.png" alt="Pulse Reports & Insights — A-F cache health grade, model routing, inflection timeline, cost spikes" width="900">

<sub><b>Reports & Insights</b> — letter-grade cache health · rule-based recommendations · cost inflection detection · one-click <i>Fix with Claude Code</i> prompts.</sub>

<br><br>

<img src="assets/screenshots/discord-rich-presence.png" alt="Pulse Discord Rich Presence on a profile — Claude Code activity with model, reasoning effort, tokens, cost, plan usage" width="420">

<sub><b>Discord Rich Presence</b> — live model · reasoning effort · project · branch · tokens · cost · 5 h / 7 d / Extra Usage, right on your profile.</sub>

</div>

<h2 id="install"><img src="assets/icons/download.svg" alt="" width="28" align="center"> &nbsp;Install</h2>

### Windows

```powershell
irm https://raw.githubusercontent.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/main/scripts/install.ps1 | iex
```

Or grab an installer from the [latest release](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/latest):

| Asset | Description |
| :--- | :--- |
| `Pulse_x.y.z_x64-setup.exe` | NSIS installer — recommended |
| `Pulse_x.y.z_x64_en-US.msi` | MSI installer |

### macOS

```bash
curl -fsSL https://raw.githubusercontent.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/main/scripts/install.sh | bash
```

| Asset | Architecture |
| :--- | :--- |
| `Pulse_x.y.z_aarch64.dmg` | Apple Silicon (M1 / M2 / M3 / M4) |
| `Pulse_x.y.z_x64.dmg` | Intel |

### Linux

```bash
curl -fsSL https://raw.githubusercontent.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/main/scripts/install.sh | bash
```

| Asset | Distro |
| :--- | :--- |
| `pulse_x.y.z_amd64.deb` | Debian / Ubuntu |
| `pulse-x.y.z-1.x86_64.rpm` | Fedora / RHEL |
| `pulse_x.y.z_amd64.AppImage` | Any (portable) |

### From source

```bash
git clone https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics.git
cd Pulse-Claude-Code-Analytics
cd frontend && npm install && npm run build && cd ..
cd src-tauri && cargo tauri build
```

<h2 id="features"><img src="assets/icons/sparkles.svg" alt="" width="28" align="center"> &nbsp;Features</h2>

### Analytics dashboard

| | |
| :--- | :--- |
| **Accurate cost math** | Per session · day · model. Published pricing for every Claude — Fable 5, Mythos 5, Opus 4.8 (with fast mode), 4.7, 4.6, 4.5, 4.1, 4, 3; Sonnet 4.6, 4.5, 4; Haiku 4.5, 3.5, 3. Speed-aware per-turn, so fast turns price at 2×. Compare like-for-like whatever plan you're on. |
| **A – F cache health grade** | Trend-weighted hit ratio — cchubber-style. See your direction, not just your current number. |
| **Model routing insights** | Opus / Sonnet / Haiku split + *"you could save $X by rerouting N sessions to Sonnet"* estimate. |
| **Inflection detection** | Any session ≥ 2 × baseline cost-per-session gets flagged with context. |
| **Recommendations engine** | Every finding has a **Copy Fix Prompt** button. Paste it into Claude Code. Problem fixed. |
| **Plan usage limits** | 5-hour window · weekly all-models · Sonnet-only · Extra Usage monthly spend. Auto-detects Pro / Max / Teams. Sound alert on Extra Usage spikes. |
| **Heatmap · sparklines · charts** | All-local Chart.js. Zero network. |
| **Reports export** | Branded HTML + Markdown. One click. |
| **OpenAI Codex support** | Full GPT-5 family pricing — GPT-5.5, 5.4, 5.3-Codex, 5.2, 5.1 (+ Codex / max / mini), 5, mini, nano. **Fast mode** (`/fast`) surfaced at 2.5× (5.5) / 2× (5.4). One-click provider switch. |
| **Codex RP upstream sync** | Codex Discord Rich Presence logic is mirrored from [Codex-Discord-Rich-Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence) with scripts and CI checks, instead of drifting as a private fork. |

### Discord Rich Presence

| | |
| :--- | :--- |
| **Live fields** | Project · git branch · model · reasoning effort · activity status. |
| **Session timer** | Elapsed since start. Persists through Discord restarts. |
| **Reasoning tiers** | Five tiers (Low / Medium / High / Extra High / Max) — Opus 4.7 native. |
| **Asset resolver** | Multi-tier — portal keys → Media Proxy → plain URLs → fallback. |
| **Presets** | Minimal · Standard · Full. Customize per-field too. |

<!--
Keywords: Claude Code analytics, Claude Code cost tracker, Claude Code usage dashboard,
Claude Fable 5 pricing, Claude Mythos 5 pricing, Claude Opus 4.8 fast mode pricing, Anthropic token cost, cache hit ratio, prompt caching,
1M context window usage, model routing, OpenAI Codex usage analytics, Discord Rich Presence,
reasoning effort, Claude Pro Max Teams plan usage limits, Extra Usage, JSONL transcript,
local-first, zero telemetry, open source, Rust, Tauri 2, Svelte 5, Windows, macOS, Linux.
See llms.txt for a machine-readable project summary.
-->


### Privacy & ownership

- **100 % local.** Your session data never leaves your machine. No telemetry, no phone-home, no cloud.
- **SQLite at `~/.claude/pulse-analytics.db`** — yours to inspect, export, back up, or delete.
- **Apache-2.0 licensed** with required attribution. Fork it, audit it, ship your own version — just keep the [`NOTICE`](NOTICE) file and credit the original author per the license.

<h2><img src="assets/icons/brain.svg" alt="" width="28" align="center"> &nbsp;What makes Pulse different</h2>

| | Pulse | Generic dashboards |
| :--- | :---: | :---: |
| Opus 4.7 tokenizer awareness (flags inflated counts) | ✓ | — |
| 1 M context pricing correctly applied per model (GA flat vs. beta surcharge) | ✓ | — |
| A – F cache health grade (trend-weighted) | ✓ | — |
| *Fix with Claude Code* one-click prompts | ✓ | — |
| Zero-config — reads JSONL transcripts directly | ✓ | setup required |
| Discord Rich Presence | ✓ | — |
| Native desktop (Tauri 2 + Rust, no Electron bloat) | ✓ | — |
| Open source | Apache-2.0 | varies |

<h2 id="usage"><img src="assets/icons/terminal.svg" alt="" width="28" align="center"> &nbsp;Usage</h2>

**First launch** → install Pulse → launch → it auto-detects your `~/.claude/` folder → keep using Claude Code; sessions stream in live.

**Discord Rich Presence** → open the **Discord** tab → pick a preset (Minimal · Standard · Full) or customize fields → toggle on. Custom presence artwork in [`docs/discord-assets.md`](docs/discord-assets.md).

**Reports** → open the **Reports** tab → read your cache grade + recommendations → click **Copy Fix Prompt** on any item → paste into Claude Code → done. Export HTML / Markdown for your team.

<h2 id="roadmap"><img src="assets/icons/map.svg" alt="" width="28" align="center"> &nbsp;Roadmap</h2>

- **Linux tray icon** — system-tray quick toggles (Discord presence on/off, pause analytics, open dashboard) via Tauri's `tray-icon` feature. Parity with the Windows tray.
- **MCP server inventory** — list every MCP server Claude Code has loaded for each session, with its tool count, per-tool invocation frequency, and an estimated token cost per tool call. Helps spot noisy MCPs silently inflating your context.
- **Budget-threshold desktop notifications** — native OS notification when you cross configurable thresholds (e.g. "80 % of weekly limit", "Extra Usage just passed $150 of your $200 cap"). Replaces eyeballing the dashboard.
- **Custom Discord presence templates** — save / share named field layouts beyond Minimal · Standard · Full. Export a preset as JSON; import one from a teammate.
- **Session replay** — step through a past session's prompts and tool-call timeline with the same filters the live dashboard uses. Currently the data is there (JSONL traces) but there's no dedicated UI.
- **Smarter cost forecast** — weekly-reset-aware projections with a confidence band, instead of today's linear daily-average multiplication.

Track on the [project board](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/projects).

<h2 id="contributing"><img src="assets/icons/git-pull-request.svg" alt="" width="28" align="center"> &nbsp;Contributing</h2>

PRs welcome. [`CONTRIBUTING.md`](CONTRIBUTING.md) has the dev setup, style guide, and release process. Please read the [Code of Conduct](CODE_OF_CONDUCT.md) first.

<h2 id="sponsor"><img src="assets/icons/heart.svg" alt="" width="28" align="center"> &nbsp;Sponsor</h2>

If Pulse saves you money (or sanity), sponsor its development:

[![Sponsor xt0n1-t3ch on GitHub](https://img.shields.io/badge/GitHub_Sponsors-%E2%9D%A4-0a0a0a?style=for-the-badge&logo=githubsponsors)](https://github.com/sponsors/xt0n1-t3ch)

Every contribution goes toward faster releases, better analyzers, and keeping Pulse free and open-source forever.

<h2 id="security"><img src="assets/icons/shield.svg" alt="" width="28" align="center"> &nbsp;Security</h2>

Responsible-disclosure policy in [`SECURITY.md`](SECURITY.md). Report privately via [GitHub Security Advisories](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/security/advisories/new).

<h2 id="license"><img src="assets/icons/scale.svg" alt="" width="28" align="center"> &nbsp;License</h2>

[Apache-2.0](LICENSE) © 2026 xt0n1-t3ch. Use Pulse for anything (personal or commercial) — but per the license you must keep the copyright notice and the [`NOTICE`](NOTICE) file with original-author attribution in any redistribution or derivative work.

---

<div align="center">
<sub>Built with Rust, Tauri, Svelte, and Claude Code. &nbsp; · &nbsp; <a href="https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics">github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics</a></sub>
</div>


## Windows + WSL session roots

Pulse is Windows-native by default. On Windows it does **not** launch `wsl.exe` while polling sessions, because broken WSL installs can raise OS-level crash dialogs. If you intentionally keep Claude/Codex transcripts inside WSL and want Pulse to scan them, opt in before launch:

```powershell
$env:CC_PRESENCE_INCLUDE_WSL = "1"
```

Linux and macOS continue to use their native session paths; WSL path bridging remains available only through the explicit Windows opt-in.

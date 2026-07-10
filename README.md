<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/pulse-logo-dual-dark.png">
  <source media="(prefers-color-scheme: light)" srcset="assets/pulse-logo-dual-light.png">
  <img src="assets/pulse-logo-dual-dark.png" alt="Pulse — Claude Code and Codex Analytics" width="560" height="124">
</picture>

### See where your Claude Code and Codex usage actually goes.

The open-source **Claude Code + OpenAI Codex (ChatGPT App) analytics dashboard** and **Discord Rich Presence** for **Claude Pro / Max / Teams** and **ChatGPT Plus / Pro / Business**.<br>Measure provider-aware cost, cache, context, and limits; catch runaway sessions; and send one-click fix prompts back to the active coding agent. Native desktop. 100 % local. Zero telemetry.

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

<h2 id="whats-new"><img src="assets/icons/sparkles.svg" alt="" width="28" align="center"> &nbsp;What's New in v1.5.3</h2>

- **No five-second console flashes** — Codex Git branch probes now use the shared Windows `CREATE_NO_WINDOW` launcher during Pulse's background refresh.
- **Canonical fix, immutable pin** — Pulse vendors Codex Discord Rich Presence `v1.7.6` by annotated tag, exact commit, and per-file SHA-256 hashes.
- **Adapters stay local** — the sync updates only canonical Rust owners; Pulse process detection and Tauri adapters remain outside the vendored mirror.
- **Regression locked** — CI rejects the old upstream pin and any periodic Git probe that bypasses the silent launcher.

<details>
<summary>Previous v1.5.2 highlights</summary>

- **GPT-5.6 without guessed economics** — one vendored catalog owns Sol, Terra, and Luna aliases, labels, API and Codex-credit rates, cache policy, reasoning tiers, and the 372K raw / 353.4K usable context contract. Unknown models and missing telemetry remain explicit.
- **Discord controls that match Discord** — all nine privacy fields flow through one backend presentation contract. Disabling Git branch removes it from both the Live Preview and the real Discord card.
- **Correct Codex identities** — choose `Codex App` or `ChatGPT App` for desktop sessions; CLI and VS Code Extension are detected and labeled separately instead of being collapsed into one ambiguous surface.
- **Readable model state** — Discord shows model, reasoning, plan, speed, cost, tokens, context, and quota windows as deliberate fields, including `GPT-5.6 Sol · Max` and an independent `Fast` marker when observed.
- **Durable analytics** — SQLite schema v4 records provenance and completeness for speed, pricing, cache savings, and context, while daily aggregates are derived idempotently from sessions.
- **One product, two first-class providers** — refreshed Pulse artwork, reports, docs, and fix prompts now name Claude Code and Codex (ChatGPT App) consistently.

</details>

**[Download v1.5.3](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/latest)** &nbsp;·&nbsp; **[Full changelog](CHANGELOG.md)**

<h2 id="about"><img src="assets/icons/info.svg" alt="" width="28" align="center"> &nbsp;About</h2>

You may pay for Claude Code every month, ChatGPT every month, or both: **Claude Pro / Max / Teams** on one side and **ChatGPT Plus / Pro / Business** on the other. Yet if someone asked *"which session consumed the most context this week?"* or *"what percentage of my input was served from cache?"* you probably could not answer with evidence.

**Pulse answers.** It reads the local JSONL transcripts and runtime metadata written by Claude Code and Codex, then normalizes provider-specific facts without sending session data anywhere. Install, launch, and switch providers from the same desktop app:

- **A – F cache-health letter grade** — trend-weighted, so you see the *direction* your cache efficiency is heading, not just today's number.
- **Opus-4.7-tokenizer aware** — Opus 4.7's new tokenizer inflates tokens by up to 35 %; Pulse flags the inflation so you know when you're hitting limits faster than expected.
- **1 M context GA pricing** — flat per-token rate across the full 1 M window for **Fable 5 · Mythos 5 · Opus 4.6 · Opus 4.7 · Opus 4.8 · Sonnet 4.6 · Sonnet 5** (per [Anthropic's official pricing](https://platform.claude.com/docs/en/about-claude/pricing)). Older betas (Sonnet 4 / 4.5, Opus 4 / 4.5) still get 2× input · 1.5× output · 2× cache at > 200 K. Pulse applies the correct math per-model so you compare sessions like-for-like. Plan-level **Extra Usage** on Pro / Max / Teams is tracked separately.
- **Inflection alerts** — any session that blows past 2 × your rolling baseline gets flagged with context and a suggested fix.
- **Provider-aware fix prompts** — every recommendation has a **Copy Fix Prompt** action labeled for the active provider: **Fix with Claude Code** or **Fix with Codex**.
- **Plan usage limits** — live tracking of the windows each provider actually exposes, including Claude's Sonnet/Extra Usage telemetry and Codex's primary/secondary quota windows.
- **Release awareness** — startup and 6-hour update checks surface new stable GitHub Releases inside the app without pretending a signed auto-installer exists before release metadata is published.
- **Discord Rich Presence** — five-tier reasoning effort, live project / model / branch. Your flow state, on your profile.

**One product, two factual lanes.** Claude sessions keep Anthropic-specific model routing, cache TTL, Extra Usage, and statusline authority. Codex sessions use the canonical GPT catalog and current local App metadata, including **GPT-5.6 Sol / Terra / Luna**, independent reasoning and Standard/Fast modes, and **372K raw / 353.4K usable** context. Unpublished Fast economics or missing cache-write telemetry stay partial or unavailable instead of becoming an invented multiplier.

Codex Discord Rich Presence has its own source-of-truth repo: **[xt0n1-t3ch/Codex-Discord-Rich-Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence)**. Pulse mirrors that Rust core into `src/codex/` through checked sync scripts and CI freshness gates, so Codex support can move fast here while the standalone Rich Presence project keeps its own audience.

Written in **Rust** + **Tauri 2** + **Svelte 5**. ≈ 12 MB on Windows, ≈ 18 MB on macOS. Cold-starts in under 200 ms. One-click installers for Windows (NSIS + MSI), macOS (DMG — Apple Silicon + Intel), and Linux (deb, rpm, AppImage). Apache-2.0 licensed (attribution required — see [`NOTICE`](NOTICE)). The data never leaves your machine.

<h2 id="screenshots"><img src="assets/icons/image.svg" alt="" width="28" align="center"> &nbsp;Screenshots</h2>

<div align="center">

<img src="assets/screenshots/dashboard.png" alt="Pulse dashboard — Claude Code and Codex cost, tokens, cache health grade, plan usage limits, and activity heatmap" width="900">

<sub><b>Dashboard</b> — at-a-glance cost · tokens · cache-hit ratio · plan limits · extra usage · activity heatmap.</sub>

<br><br>

<img src="assets/screenshots/reports.png" alt="Pulse Reports & Insights — A-F cache health grade, model routing, inflection timeline, cost spikes" width="900">

<sub><b>Reports & Insights</b> — provider-capable cache health · rule-based recommendations · cost inflection detection · one-click fix prompts for Claude Code or Codex.</sub>

<br><br>

<table>
  <tr>
    <td align="center" width="50%">
      <img src="assets/screenshots/discord-rich-presence.png" alt="Pulse Discord Rich Presence on a profile — Claude Code activity with model, reasoning effort, tokens, cost, plan usage" width="420"><br>
      <sub><b>Claude Code Rich Presence</b><br>Live model · reasoning effort · project · branch · tokens · cost · 5 h / 7 d / Extra Usage.</sub>
    </td>
    <td align="center" width="50%">
      <img src="assets/screenshots/codex-discord-rich-presence.png" alt="Pulse Discord Rich Presence on a profile — ChatGPT App activity with GPT model, reasoning, cost, tokens, context, and quota windows" width="420"><br>
      <sub><b>Codex / ChatGPT App Rich Presence</b><br>Selectable desktop identity · GPT-5.6 family · reasoning · speed · cache · cost · context · quota windows.</sub>
    </td>
  </tr>
</table>

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
| **Provider-owned cost math** | Per session · day · model. Claude uses its published model/cache/Fast rules and authoritative statusline totals. Codex uses the vendored factual catalog, exposes completeness, and never invents unpublished Fast or cache-write economics. |
| **A – F cache health grade** | Trend-weighted hit ratio — cchubber-style. See your direction, not just your current number. |
| **Model routing insights** | Opus / Sonnet / Haiku split + *"you could save $X by rerouting N sessions to Sonnet"* estimate. |
| **Inflection detection** | Any session ≥ 2 × baseline cost-per-session gets flagged with context. |
| **Recommendations engine** | Every finding has a **Copy Fix Prompt** action routed to the active provider: Claude Code or Codex. |
| **Plan usage limits** | 5-hour window · weekly all-models · Sonnet-only · Extra Usage monthly spend. Auto-detects Pro / Max / Teams. Sound alert on Extra Usage spikes. |
| **Heatmap · sparklines · charts** | All-local Chart.js. Zero network. |
| **Reports export** | Branded HTML + Markdown. One click. |
| **OpenAI Codex support** | Canonical GPT catalog with GPT-5.6 Sol / Terra / Luna, sourced API rates and Codex credits, exact/partial/unavailable cost status, 372K raw / 353.4K usable context, reasoning effort, and independent Standard/Fast display. One-click provider switch. |
| **Codex RP upstream sync** | Codex Discord Rich Presence logic is mirrored from [Codex-Discord-Rich-Presence](https://github.com/xt0n1-t3ch/Codex-Discord-Rich-Presence) with scripts and CI checks, instead of drifting as a private fork. |

### Discord Rich Presence

| | |
| :--- | :--- |
| **Live fields** | Project · git branch · model · reasoning effort · activity status. |
| **Session timer** | Elapsed since start. Persists through Discord restarts. |
| **Reasoning tiers** | Low · Medium · High · Extra High · Max, plus Ultra where the selected Codex model exposes it. |
| **Asset resolver** | Multi-tier — portal keys → Media Proxy → plain URLs → fallback. |
| **Presets and privacy** | Minimal · Standard · Full, plus independent project, branch, model, activity, tokens, cost, limits, context, and systems controls. |

<!--
Keywords: Claude Code analytics, Claude Code cost tracker, Claude Code usage dashboard, OpenAI Codex analytics, ChatGPT App analytics,
Claude Fable 5 pricing, Claude Mythos 5 pricing, Claude Opus 4.8 fast mode pricing, Anthropic token cost, cache hit ratio, prompt caching,
1M context window usage, model routing, GPT-5.6 pricing, Discord Rich Presence,
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
| Provider-aware *Fix with Claude Code* / *Fix with Codex* prompts | ✓ | — |
| Zero-config — reads JSONL transcripts directly | ✓ | setup required |
| Discord Rich Presence | ✓ | — |
| Native desktop (Tauri 2 + Rust, no Electron bloat) | ✓ | — |
| Open source | Apache-2.0 | varies |

<h2 id="usage"><img src="assets/icons/terminal.svg" alt="" width="28" align="center"> &nbsp;Usage</h2>

**First launch** → install Pulse → launch → it auto-detects local Claude Code and Codex state → choose a provider; sessions stream in live.

**Discord Rich Presence** → open the **Discord** tab → pick a preset (Minimal · Standard · Full) or customize fields → toggle on. Custom presence artwork in [`docs/discord-assets.md`](docs/discord-assets.md).

**Reports** → open the **Reports** tab → read the provider-capable analysis → click **Copy Fix Prompt** on any item → run it in the active provider. Export HTML / Markdown for your team.

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
<sub>Built with Rust, Tauri, and Svelte for Claude Code and Codex. &nbsp; · &nbsp; <a href="https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics">github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics</a></sub>
</div>


## Windows + WSL session roots

Pulse is Windows-native by default. On Windows it does **not** launch `wsl.exe` while polling sessions, because broken WSL installs can raise OS-level crash dialogs. If you intentionally keep Claude/Codex transcripts inside WSL and want Pulse to scan them, opt in before launch:

```powershell
$env:CC_PRESENCE_INCLUDE_WSL = "1"
```

Linux and macOS continue to use their native session paths; WSL path bridging remains available only through the explicit Windows opt-in.

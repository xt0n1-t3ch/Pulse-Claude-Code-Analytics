<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/pulse-logo-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/pulse-logo-light.svg">
  <img src="assets/pulse-logo-dark.svg" alt="Pulse — Claude Code Analytics" width="820">
</picture>

<br><br>

**A native, local-first analytics dashboard & Discord Rich Presence client for [Claude Code](https://claude.com/claude-code).**

Know exactly what each Claude Code session costs. Catch runaway spend before the invoice lands. Grade your cache health. Share your flow state on Discord. 100 % on your machine, zero telemetry.

<br>

[![CI](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/actions/workflows/ci.yml/badge.svg)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/actions/workflows/ci.yml)
[![Release](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/actions/workflows/release.yml/badge.svg)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/actions/workflows/release.yml)
[![Latest](https://img.shields.io/github/v/release/xt0n1-t3ch/Pulse-Claude-Code-Analytics?color=0a0a0a&label=latest&logo=github)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/xt0n1-t3ch/Pulse-Claude-Code-Analytics/total?color=0a0a0a&logo=github)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases)
[![License](https://img.shields.io/badge/license-MIT-0a0a0a.svg)](LICENSE)
[![Stars](https://img.shields.io/github/stars/xt0n1-t3ch/Pulse-Claude-Code-Analytics?style=flat&color=0a0a0a&logo=github)](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/stargazers)
[![Sponsor](https://img.shields.io/badge/sponsor-%E2%9D%A4-0a0a0a?logo=githubsponsors)](https://github.com/sponsors/xt0n1-t3ch)

<br>

<a href="#install"><b>Download</b></a>&nbsp;&nbsp;·&nbsp;&nbsp;<a href="#features"><b>Features</b></a>&nbsp;&nbsp;·&nbsp;&nbsp;<a href="#screenshots"><b>Screenshots</b></a>&nbsp;&nbsp;·&nbsp;&nbsp;<a href="docs/"><b>Docs</b></a>&nbsp;&nbsp;·&nbsp;&nbsp;<a href="https://github.com/sponsors/xt0n1-t3ch"><b>Sponsor</b></a>

</div>

<br>

---

<h2 id="screenshots"><img src="assets/icons/image.svg" alt="" width="22" align="center"> &nbsp;Screenshots</h2>

<div align="center">

<img src="assets/screenshots/dashboard.png" alt="Pulse dashboard — cost, tokens, cache health, plan limits, activity heatmap" width="920">

<sub><b>Dashboard</b> — at-a-glance cost · tokens · cache-hit ratio · plan limits · extra usage · activity heatmap.</sub>

<br><br>

<img src="assets/screenshots/reports.png" alt="Pulse Reports — letter-grade cache health, recommendations, inflection detection" width="920">

<sub><b>Reports & Insights</b> — letter-grade cache health · rule-based recommendations · cost inflection detection · one-click <i>Fix with Claude Code</i> prompts.</sub>

</div>

<br>

---

<h2 id="install"><img src="assets/icons/download.svg" alt="" width="22" align="center"> &nbsp;Install</h2>

### Windows

One-liner (PowerShell):

```powershell
irm https://raw.githubusercontent.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/main/scripts/install.ps1 | iex
```

Or pick an installer from the [latest release](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/releases/latest):

| Asset | Description |
| :---- | :---------- |
| `Pulse_x.y.z_x64-setup.exe` | NSIS installer (recommended) |
| `Pulse_x.y.z_x64_en-US.msi` | MSI installer |

### macOS

One-liner:

```bash
curl -fsSL https://raw.githubusercontent.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/main/scripts/install.sh | bash
```

| Asset | Architecture |
| :---- | :----------- |
| `Pulse_x.y.z_aarch64.dmg` | Apple Silicon (M1 / M2 / M3 / M4) |
| `Pulse_x.y.z_x64.dmg` | Intel |

### Linux

One-liner:

```bash
curl -fsSL https://raw.githubusercontent.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/main/scripts/install.sh | bash
```

| Asset | Distro |
| :---- | :----- |
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

<br>

---

<h2 id="features"><img src="assets/icons/sparkles.svg" alt="" width="22" align="center"> &nbsp;Features</h2>

### Analytics dashboard

| | |
| :-- | :-- |
| **Cost tracking** | Accurate cost per session, per day, per model. No guesswork. |
| **Cache-health grade** | A – F letter grade based on trend-weighted hit ratio, cchubber-style. |
| **Model routing** | Opus / Sonnet / Haiku split + potential-savings estimate. |
| **Inflection detection** | Flags ≥ 2× cost-per-session deviations before surprise bills. |
| **Recommendations engine** | Actionable fixes with **Copy Fix Prompt** — paste straight into Claude Code. |
| **Plan usage** | Current session · weekly · Sonnet-only · Extra Usage monthly limits. |
| **Heatmap · sparklines · charts** | All-local Chart.js — no network. |
| **Reports export** | HTML & Markdown, one click. |

### Discord Rich Presence

| | |
| :-- | :-- |
| **Live fields** | Project · git branch · model · reasoning effort · activity. |
| **Session timer** | Elapsed since session start. |
| **Reasoning tiers** | Five tiers (Low / Medium / High / Extra High / Max) — Opus 4.7 native. |
| **Asset resolver** | Multi-tier — portal keys, Media Proxy, plain URLs, fallback. |
| **Presets** | Minimal · Standard · Full. |

### Privacy & ownership

- 100 % local. No telemetry, no phone-home.
- SQLite DB at `~/.claude/pulse-analytics.db` — yours to inspect, export, or delete.
- MIT licensed.

<br>

---

<h2><img src="assets/icons/brain.svg" alt="" width="22" align="center"> &nbsp;What makes Pulse different</h2>

| | Pulse | Generic dashboards |
| :-- | :--: | :--: |
| Opus 4.7 tokenizer awareness (flags inflated counts) | ✓ | — |
| 1 M context GA pricing (Opus 4.6+ / Sonnet 4.6+) | ✓ | — |
| cchubber-style cache health grade (A – F, trend-weighted) | ✓ | — |
| *Fix with Claude Code* one-click prompts | ✓ | — |
| Zero-config (reads JSONL transcripts directly) | ✓ | setup required |
| Discord Rich Presence | ✓ | — |
| Native desktop (Tauri 2 + Rust, no Electron bloat) | ✓ | — |
| Open source | MIT | varies |

<br>

---

<h2 id="about"><img src="assets/icons/aperture.svg" alt="" width="22" align="center"> &nbsp;About</h2>

**Pulse** is built for developers who live in Claude Code and want full transparency on what each session costs, how efficient their context / cache usage is, and where the next model-routing win hides. It replaces the guesswork — *"did I just spend $40 on cache misses?"* — with a letter grade and a ready-to-paste fix prompt.

Written in **Rust 2024** + **Tauri 2** + **Svelte 5**, Pulse feels like a native app because it *is* one — ≈ 12 MB on Windows, ≈ 18 MB on macOS, cold-starts in under 200 ms. The data pipeline reads Claude Code's own JSONL transcripts (zero-config) and enriches them with the Anthropic Usage API when available.

<br>

---

<h2><img src="assets/icons/terminal.svg" alt="" width="22" align="center"> &nbsp;Usage</h2>

**First launch.** Install Pulse → launch → it auto-detects your `~/.claude/` data folder → use Claude Code normally; sessions stream in live.

**Discord Rich Presence.** Open the **Discord** tab → pick a preset (Minimal · Standard · Full) or customize fields → toggle on. See [`docs/discord-assets.md`](docs/discord-assets.md) for custom presence artwork.

**Reports.** Open the **Reports** tab → review your cache grade + recommendation list → click **Copy Fix Prompt** on any item → paste into Claude Code → done. Export via HTML / Markdown for your team.

<br>

---

<h2><img src="assets/icons/map.svg" alt="" width="22" align="center"> &nbsp;Roadmap</h2>

- Linux AppIndicator tray icon
- MCP server inventory view
- Team / workspace rollups (opt-in, still local-first)
- Custom budget alerts with desktop notifications
- Claude.ai (web) session import
- VS Code companion extension

Track progress on the [project board](https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/projects).

<br>

---

<h2><img src="assets/icons/git-pull-request.svg" alt="" width="22" align="center"> &nbsp;Contributing</h2>

Pull requests welcome. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for dev setup, coding style, and release process. Please read the [Code of Conduct](CODE_OF_CONDUCT.md) first.

<br>

---

<h2><img src="assets/icons/heart.svg" alt="" width="22" align="center"> &nbsp;Sponsor</h2>

If Pulse saves you money (or sanity), consider sponsoring its development:

[![Sponsor xt0n1-t3ch](https://img.shields.io/badge/GitHub_Sponsors-%E2%9D%A4-0a0a0a?style=for-the-badge&logo=githubsponsors)](https://github.com/sponsors/xt0n1-t3ch)

Every contribution goes toward faster releases, better analyzers, and keeping Pulse free & open source.

<br>

---

<h2><img src="assets/icons/shield.svg" alt="" width="22" align="center"> &nbsp;Security</h2>

See [`SECURITY.md`](SECURITY.md) for the responsible-disclosure policy.

<br>

---

<h2><img src="assets/icons/scale.svg" alt="" width="22" align="center"> &nbsp;License</h2>

[MIT](LICENSE) © xt0n1-t3ch

<br>

<div align="center">
<sub>Built with Rust, Tauri, and Claude Code. &nbsp;·&nbsp; <a href="https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics">github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics</a></sub>
</div>

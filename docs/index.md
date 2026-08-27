# Pulse & cc-discord-presence — Documentation

Pulse is the Tauri 2.0 analytics GUI for Claude Code and OpenAI Codex, paired with the `cc-discord-presence` daemon that pushes Rich Presence to Discord.

## Table of contents

| Doc | Purpose |
| --- | --- |
| [architecture.md](architecture.md) | High-level component map: daemon -> Tauri -> SQLite -> Svelte |
| [architecture/dev-bridge.md](architecture/dev-bridge.md) | Debug-only loopback bridge for real Rust data in the Vite browser |
| [architecture/adaptive-access.md](architecture/adaptive-access.md) | Versioned provider/Discord fixture envelope, access-route DTO, and Browser/Tauri/Discord proof boundaries |
| [discord-assets.md](discord-assets.md) | Upload assets to the Developer Portal so the RP logo actually renders; in-app preview art |
| [plan-detection.md](plan-detection.md) | Claude/Codex plan detection, manual override persistence, Codex service tier + surface |
| [fable-5.md](fable-5.md) | Claude Fable 5 + Mythos 5 pricing, 1M context, cache TTL note, Rich Presence labels |
| [sonnet-5.md](sonnet-5.md) | Claude Sonnet 5 native support: time-boxed introductory pricing (the date-driven badge system), official cache multipliers, inflated-tokenizer warning, the 1M-context bug it fixed |
| [context-tracking.md](context-tracking.md) | Current context fill vs. all-time peak: why they're separate fields, the compaction-boundary bug this fixed, and the Dashboard-vs-Costs aggregation-scope question |
| [opus-4-7-variants.md](opus-4-7-variants.md) | Reasoning-effort tiers (Low / Medium / High / Extra High / Max) + tokenizer note |
| [opus-4-8.md](opus-4-8.md) | Opus 4.8 — fast mode (priority speed) + billing impact |
| [analyzers.md](analyzers.md) | How the cchubber-style analyzers work + how to add new recommendations |
| [cost-calculation.md](cost-calculation.md) | Pricing tiers, cache math, 1M-context GA/beta handling + fast-mode rules |
| [codex-model-catalog.md](codex-model-catalog.md) | GPT-5.6 labels, aliases, reasoning, context, pricing, cache policy, provenance, and completeness rules |
| [codex-rich-presence-upstream.md](codex-rich-presence-upstream.md) | Codex Rich Presence source-of-truth repo, sync scripts, CI freshness gate, compatibility overlay |
| [update-checks.md](update-checks.md) | Backend GitHub Release checks, popup behavior, skip controls, signed-updater note |
| [notifications.md](notifications.md) | Durable provider-health, quota, and Discord notification lifecycle, deduplication, and Tauri events |
| [ui-design-system.md](ui-design-system.md) | Ultra-dark operational shell, semantic colors, one-fact/one-owner hierarchy, sensitive-content disclosure, responsive states, autosave and updater affordances |
| [troubleshooting.md](troubleshooting.md) | Diagnostics: doctor, RUST_LOG, data sources, common failures + fixes |

## v1.7.9 docs refresh

Pulse v1.7.9 ships the operator-console overhaul: real local Discord identity in the
Broadcast preview, canonical Codex limit naming (5-hour / Weekly), a subscription-real
cost cockpit, de-nested Costs surfaces, Reports bundle caching with loading skeletons,
`llms.txt` for repository discovery, and the promoted codex-presence-core v1.10.3 pin.

## v1.7.8 docs refresh

- Made updater availability platform-truthful: signed preflight owns the
  **Update** action and unsupported or incomplete channels open the release.
- Recorded native Windows, macOS, and Linux x64/ARM64 packaging and the refined
  centered provider workspace.

## v1.7.5 docs refresh

- Documented the dynamic Codex account-quota contract: only windows reported by
  the effective global response reach shared Pulse surfaces; fresh reads remove
  missing windows, absence is not `100% remaining`, and plans remain separate
  from credits.
- Documented duration-specific identities for multi-window provider scopes and
  the durable notification migration that dismisses pre-fix collision rows
  while preserving audit history and future genuine resets.
- Documented the snapshot trust boundary: Discord config failure is isolated
  from analytics, poisoned shared state fails closed, same-provider last-good
  Discord data is visibly degraded, and backend capture time owns freshness.
- Documented the graphite UI refinement and the Reports privacy boundary:
  prompt excerpts are backend-bounded and remain absent from the DOM until an
  explicit reveal.

## v1.7.2 docs refresh

- Promoted the exact immutable Codex Presence Core `v1.10.2` revision with
  native `gpt-daybreak-blue-latest` and `gpt-5.6-cyber` recognition.
- Documented and tested provenance-correct monetary DTOs, bounded incremental
  JSONL reads, active-session checkpoints, request coalescing, and non-blank
  route transitions across all seven Pulse views.
- Hardened local Windows publication so stale installers cannot enter a release;
  exact-version NSIS/MSI assets, SPDX metadata, checksums, and downloaded hashes
  must agree before the GitHub draft becomes public.

## v1.7.0 docs refresh

- Documented provider-neutral authenticated access routes, freshness-aware quota
  presentation, and proof/provenance boundaries across Pulse and Discord.
- Separated stable access-source identity from provider-scoped analytics:
  `codex`, `claude`, `openai`, and `anthropic` are exact scopes, while `all` is
  the only cross-provider aggregate; API lanes never borrow subscription
  sessions or cost.
- Split provider proof from local-history capability: an expired Claude
  subscription remains visible and selectable when SQLite contains Claude
  sessions, but it cannot supply allowance cards, quota notifications, plan
  claims, or Discord authority until authentication succeeds again.
- Replaced the unavailable-cost dead end with the Subscription Value Ledger.
  Token totals, token mix, daily usage trend, sessions, cache reuse, and priced
  coverage remain useful when money is not reported; partial monetary value keeps
  its denominator and provenance, while unavailable billing never renders as `$0`.
- Locked provider-scoped plan catalogs: Claude `free`, `pro`, `team`,
  `enterprise`, `max_5x`, `max_20x`; Codex `free`, `go`, `plus`, `business`,
  `enterprise`, `pro_5x`, `pro_20x`. Ambiguous Max and bare Pro telemetry stay
  Unknown.
- Recorded the typed `codex-presence-core` 2.0.0 integration against the local
  upstream v1.9.0 candidate; publication and immutable revision proof remain
  pending upstream push.
- Kept native quota alerts proof-driven: only fresh authenticated Codex/Claude
  reset transitions notify (`remaining < 100 -> 100` or `used > 0 -> 0`), while
  timestamp drift, stale/cache/unproved samples, thresholds, health, and
  Discord diagnostics remain silent. Legacy timestamp-only reset rows are
  preserved and dismissed once during migration.
- Added the second legacy-reset dismissal marker so false rows written by an
  older concurrent producer after the first migration are preserved but hidden;
  genuine reset transitions created after v2 stay visible.
- Exposed canonical Codex `rateLimitResetCredits` separately from spend
  `credits`, preserving the provider's structured count and optional detail
  records without a second Pulse parser. Claude cache-only reads remain an
  honest unauthenticated/unavailable route.
- Registered Tauri's single-instance plugin before background-poller setup so a
  second Pulse launch focuses the existing window instead of starting another
  poller; the Discord `PublisherLease` remains an independent publisher guard.
- Kept provider-health notifications edge-triggered: unavailable baselines and
  unproofed cache/session fallbacks stay diagnostic-only, while only authenticated
  health transitions can create native alerts.
- Kept the release contract explicit: Pulse v1.7.0 is a candidate until the
  upstream revision, version surfaces, and final release gates are re-verified.
- Hardened the Windows Codex account probe for unpackaged Pulse installs: the
  resolver prefers the user-local Codex CLI before the AppX package path, whose
  ACL can reject `CreateProcess` from Pulse.
- Hardened native Tauri CDP validation: the runner selects the `pulse` binary,
  isolates WebView2 user data from installed Pulse, and runs the real
  `chromium.connectOverCDP` Playwright seam with identity-checked cleanup.
- Kept Windows Pulse unit-test harnesses on the same Common-Controls v6
  manifest as the packaged Tauri app, avoiding a pre-test `TaskDialogIndirect`
  loader failure against legacy `system32/comctl32.dll`. Cargo currently does
  not apply build-script `rustc-link-arg-tests` to the Windows lib harness, so
  the checked-in manifest is intentionally shared and retains Tauri's exact
  `asInvoker`/`uiAccess=false` trust policy; production RT_MANIFEST readback is
  a release gate, not implied by the compile-time receipt.
- Reworked every Pulse view into a shared edge-to-edge responsive shell. Home
  now follows the approved dense provider/work composition, while Sessions,
  Context, Costs, Reports, Discord, and Settings share bounded-width,
  mobile-stack, and consumer-owned horizontal-scroll contracts.
- Unified cost provenance across live history, Summary, Projects, hourly/model
  aggregates, Reports timelines, analyzers, and offline exports: only
  `known_cost` contributes to provenance-aware monetary value, averages divide by priced sessions, and
  partial/unavailable coverage stays explicit instead of promoting raw
  estimates to exact totals.
- Recorded the same-viewport reference comparison and four-breakpoint visual
  result in [`../design-qa.md`](../design-qa.md). Live preview remains
  proof-gated: an unauthenticated provider route renders an honest empty state
  instead of example allowance data.
- Re-ran the final consumer audit at `1488 x 1058` and `390 x 844` across all
  seven routes, including Claude local-history selection, the Codex
  unavailable-cost state, and the durable notification center.

## v1.6.5 docs refresh

- Added the authenticated Codex account-quota path and its freshness contract: 30-second API cache, 15-minute JSONL fallback ceiling, and unavailable instead of stale.
- Documented the active-only Context owner, responsive multi-instance Dashboard, exact Context Window fractions, local-time heatmap, and matte surface contract.
- Preserved provider backend semantics, Discord autosave, update approval/install/relaunch, configuration schema 13, and database schema 5.

## v1.6.2 docs refresh

- Documented the Pulse UI contract: monochrome application chrome, provider-scoped identity color (Codex blue), semantic-only status color, and one visible owner per fact.
- Recorded the new session-focus Dashboard hierarchy, flat metric strips, coherent loading/empty states, Discord autosave feedback, and one-action signed update/relaunch flow.
- Recorded that the reasoning-effort tier is read from the top-level `effort` field Claude Code writes on each `assistant` transcript line, replacing the legacy system-reminder scrape that made every session read `Medium`.
- Documented usage provenance: Pulse authenticates to the usage endpoint with an OAuth bearer token from `~/.claude/.credentials.json`, never an API key, and the footer now names the observed handshake and plan or reports a cache hit.
- Noted Claude presence config schema **v6**, which persists the master Rich Presence switch, and the move to atomic config writes shared with the Codex writer.
- Kept the release as a SemVer patch because v1.6.2 corrects detection, persistence, and presentation without changing Tauri command signatures or the analytics database schema.

## v1.6.1 docs refresh

- Recorded the Claude Opus 5 pricing correction: a single-segment family version now parses, so `claude-opus-5` bills at the official $5/$25/$6.25/$0.50 rates with 1M GA context and no long-context surcharge.
- Documented window-accurate aggregation for Cost Analysis and Reports, which now total the full selected window in SQL instead of the newest page of sessions.
- Added the signed in-app updater flow, replacing the browser hand-off to the Releases page.
- Kept the release as a SemVer patch because v1.6.1 corrects pricing, aggregation, and presentation without changing Tauri command signatures or database schema.

## v1.6.0 docs refresh

- Documented the versioned snapshot/event transport that keeps the frontend, Discord publisher, and SQLite persistence on one semantic state.
- Added semantic Codex quota scopes, Credits, absolute local reset timestamps, and the exact canonical `codex-presence-core` v1.8.0 Git revision.
- Updated the responsive Dark/Light UI, Discord field-order contract, config-schema 13 migration, analytics-schema 5 migration, and performance evidence.
- Hardened releases with exact version surfaces, protected-main ancestry, Windows SPDX SBOM validation, complete platform assets, and immutable publication.
- Kept the release as a SemVer minor because v1.6.0 adds public snapshot semantics and substantial UI capabilities without removing an existing command.

## v1.5.3 docs refresh

- Pinned the Codex mirror to the immutable `v1.7.6` release and recorded its exact commit plus all canonical source/target hashes.
- Routed both periodic Git branch probes through the shared Windows `CREATE_NO_WINDOW` launcher.
- Added a vendoring regression that fails on the old pin, a raw Git launcher, or an incomplete detached-HEAD probe migration.
- Kept the release as a SemVer patch because v1.5.3 corrects Windows polling behavior without changing Tauri commands, database schema, or user configuration.

## v1.5.2 docs refresh

- Added the GPT-5.6 Sol / Terra / Luna catalog contract with official API rates, Codex credit rates, cache policy, local App context metadata, and explicit completeness/provenance states.
- Documented the immutable Codex Discord Rich Presence v1.7.5 pin, schema-12 enablement control, and the presentation contract shared by the live broadcaster and Pulse preview.
- Replaced the stale GPT-5.4/GPT-5.5 Fast multiplier prose with observed Standard/Fast display and fail-closed cost semantics.
- Documented SQLite v4 provenance, session-derived daily analytics, full Discord privacy controls, and the dual Claude Code + Codex product identity.
- Kept the release as a SemVer patch because v1.5.2 corrects and hardens existing Codex/Pulse behavior without removing a public command.

## v1.5.1 docs refresh

- Kept the mirrored Codex runtime aligned with standalone Codex Discord Rich Presence v1.7.1 WSL behavior: Windows WSL transcript roots are opt-in and use hidden subprocess launchers when enabled.
- Added sync-script safeguards so future Codex mirror pulls keep `wsl.exe` behind `CC_PRESENCE_INCLUDE_WSL=1` / `CODEX_PRESENCE_INCLUDE_WSL=1` instead of reintroducing visible console windows.
- Added an upstream contract test covering hidden WSL launchers and native-default session discovery.
- Kept the release as a patch bump: v1.5.1 ships a Windows runtime safety fix and Codex mirror freshness update without changing the public app contract.

## v1.4.2 docs refresh

- Finished the Discord Rich Presence settings contract: the Live Preview and broadcaster now read the same backend payload, and the Git branch toggle persists through the Tauri IPC argument names the backend actually receives.
- Removed the stale plain-thinking workflow label from user-facing systems copy. The Systems field is reserved for safe, generic signals such as `ULTRACODE`, `Tool active`, and `1 agent`.
- Corrected the Sonnet 5 release copy to use Anthropic's published introductory cache prices: $2.50 / MTok for 5-minute writes, $4.00 / MTok for 1-hour writes, and $0.20 / MTok for cache reads.
- Kept v1.4.1 immutable after publication; v1.4.2 is the patch release for docs, release metadata, and final Discord contract cleanup.

## v1.4.1 docs refresh

- Added [context-tracking.md](context-tracking.md): the `max_turn_api_input` lifetime peak vs. `current_context_tokens` point-in-time fill split, the compaction-boundary parsing that makes current fill correct, and why Dashboard's and Costs' cost totals differ by aggregation scope.
- Fixed a live-confirmed bug where every UI surface claiming to show current context fill read a monotonically increasing all-time peak field that never decreased across compactions.
- Dashboard and Costs monetary-value KPIs carry explicit scope and provenance labels instead of presenting API-equivalent estimates as billed spend.
- Kept the release a patch bump: v1.4.1 is a correctness fix with no public API removed.

## v1.4.0 docs refresh

- Added [sonnet-5.md](sonnet-5.md): Claude Sonnet 5 native support, including the generic introductory-pricing mechanism (`cost::active_intro_pricing`, clock-injected `cost::model_pricing_at`) that automatically reverts to standard pricing once the August 31, 2026 window closes.
- Fixed a pre-existing bug where `cost::is_ga_1m_context("claude-sonnet-5")` returned `false` due to the id's single version segment not fitting the generic two-segment Sonnet/Opus parser.
- Extended `cost::has_inflated_tokenizer()` to Sonnet 5 and generalized the Sessions/Dashboard tokenizer-warning tooltip.
- Sessions and Dashboard live-session cards show an "Intro Pricing" badge sourced entirely from backend `SessionInfo.intro_pricing`.
- Kept the release a minor SemVer bump: v1.4.0 adds model support, a pricing-correctness fix, and a UI capability without removing public API.

## v1.3.0 docs refresh

- Added [plan-detection.md](plan-detection.md): Claude/Codex plan resolution, the canonical plan-key contract behind the Settings override, where the manual override is persisted, and fresh-from-disk auto-detect.
- Documented the Codex service-tier source moving to `~/.codex/config.toml` `service_tier`, with the legacy global-state key kept as a fallback.
- Expanded [discord-assets.md](discord-assets.md) with the two Codex Discord applications, their `codex-logo` / `codex-app` uploads, and the in-app Live Preview art that bundles real Rich Presence images locally.
- Kept the release as a minor SemVer bump: v1.3.0 adds the faithful preview plus canonical plan mapping and fixes detection/override without removing public API.

## v1.2.0 docs refresh

- Added Claude Fable 5 and Claude Mythos 5 support notes: 1M context by default, 128k max output, $10 / $50 MTok input/output, 5-minute and 1-hour cache-write rates.
- Documented that runtime JSONL cost math models 5-minute cache writes because Claude Code transcripts do not expose cache TTL.
- Updated Context Window coverage for the new multi-session top-card flow.
- Added Pulse release-check coverage and Codex Rich Presence upstream-sync coverage.
- Kept the release lane as a minor SemVer bump because v1.2.0 adds model support and a UI capability without removing public API.

## Quick links

- **Install**: see [README](../README.md#install)
- **Architecture** (full component map): [architecture.md](architecture.md)
- **Adaptive access contract**: [architecture/adaptive-access.md](architecture/adaptive-access.md)
- **Contributing + local dev**: [../CONTRIBUTING.md](../CONTRIBUTING.md)
- **Test suite**: [../tests/index.md](../tests/index.md)
- **Bug / feature requests**: https://github.com/xt0n1-t3ch/Pulse-Claude-Code-Analytics/issues

## Version

- Released app: **v1.7.8**
- Unreleased target: **not selected**
- Schema: **Claude config v6 / Codex config v13 / Pulse analytics DB v5**
- Last docs refresh: 2026-08-25 (provider isolation, dynamic quotas,
  durable notification identity, responsive UI overhaul, and route-split
  performance evidence)
- Windows WSL transcript roots are opt-in with `CC_PRESENCE_INCLUDE_WSL=1`; default Windows polling stays native and does not spawn `wsl.exe`.

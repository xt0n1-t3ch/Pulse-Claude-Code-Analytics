# Pulse v1.7.0 design QA

Historical evidence retained after repository cleanup. These screenshots are not a current release acceptance claim. Paths below are repository-relative.

## Audit scope

- Product: Pulse desktop analytics workspace.
- Flow: Home, Sessions, Context, Costs, Reports, Discord, Settings, source
  selection, and the durable notification center.
- Approved source visual:
  `C:/Users/xt0n1/AppData/Local/Temp/codex-clipboard-dc43c1a6-527a-49cb-953d-23223540c697.png`.
- Reference file: `1487 x 1058`; the comparison pads one black pixel on the
  right to the required `1488 x 1058` canvas without scaling or cropping it.
- Wide CSS viewport: exactly `1488 x 1058`.
- Mobile CSS viewport: exactly `390 x 844`.
- Runtime: repository-owned Vite plus the authenticated debug Rust bridge,
  reading the current SQLite/provider state. No browser fixture supplied the
  accepted screenshots.

## Accepted evidence

| Evidence | Path | Pixels | Format proof |
| --- | --- | ---: | --- |
| Initial source/Home comparison | `assets/evidence/visual-qa/final-v170/00-reference-vs-home-1488x1058.png` | `2976 x 1058` | PNG `89504e470d0a1a0a` |
| Final source/Home comparison | `assets/evidence/visual-qa/final-v170/22-reference-vs-home-final.png` | `2976 x 1058` | PNG `89504e470d0a1a0a` |
| Final Home | `assets/evidence/visual-qa/final-v170/21-home-final-1488x1058.jpg` | `1488 x 1058` | JPEG `ffd8ff` |
| Sessions | `assets/evidence/visual-qa/final-v170/02-sessions-1488x1058.jpg` | `1488 x 1058` | JPEG `ffd8ff` |
| Context | `assets/evidence/visual-qa/final-v170/03-context-1488x1058.jpg` | `1488 x 1058` | JPEG `ffd8ff` |
| Costs, Codex unavailable | `assets/evidence/visual-qa/final-v170/04c-costs-codex-final-1488x1058.jpg` | `1488 x 1058` | JPEG `ffd8ff` |
| Reports | `assets/evidence/visual-qa/final-v170/05-reports-1488x1058.jpg` | `1488 x 1058` | JPEG `ffd8ff` |
| Discord | `assets/evidence/visual-qa/final-v170/06-discord-1488x1058.jpg` | `1488 x 1058` | JPEG `ffd8ff` |
| Settings | `assets/evidence/visual-qa/final-v170/07-settings-1488x1058.jpg` | `1488 x 1058` | JPEG `ffd8ff` |
| Seven-route wide contact sheet | `assets/evidence/visual-qa/final-v170/08-wide-contact-sheet.png` | `1520 x 2180` | PNG `89504e470d0a1a0a` |
| Seven-route mobile contact sheet | `assets/evidence/visual-qa/final-v170/18-mobile-contact-sheet.png` | `1624 x 1720` | PNG `89504e470d0a1a0a` |
| Home mobile | `assets/evidence/visual-qa/final-v170/11-home-390x844.jpg` | `390 x 844` | JPEG `ffd8ff` |
| Sessions mobile | `assets/evidence/visual-qa/final-v170/12-sessions-390x844.jpg` | `390 x 844` | JPEG `ffd8ff` |
| Context mobile | `assets/evidence/visual-qa/final-v170/13-context-390x844.jpg` | `390 x 844` | JPEG `ffd8ff` |
| Costs mobile | `assets/evidence/visual-qa/final-v170/14-costs-390x844.jpg` | `390 x 844` | JPEG `ffd8ff` |
| Reports mobile | `assets/evidence/visual-qa/final-v170/15-reports-390x844.jpg` | `390 x 844` | JPEG `ffd8ff` |
| Discord mobile | `assets/evidence/visual-qa/final-v170/16-discord-390x844.jpg` | `390 x 844` | JPEG `ffd8ff` |
| Settings mobile | `assets/evidence/visual-qa/final-v170/17-settings-390x844.jpg` | `390 x 844` | JPEG `ffd8ff` |
| Notification center mobile | `assets/evidence/visual-qa/final-v170/19-notifications-390x844.jpg` | `390 x 844` | JPEG `ffd8ff` |
| Claude local-history Costs | `assets/evidence/visual-qa/final-v170/20-costs-claude-local-390x844.jpg` | `390 x 844` | JPEG `ffd8ff` |

The browser capture API emitted JPEG bytes, so accepted browser screenshots use
the matching `.jpg` extension. ImageMagick produced the PNG comparison/contact
sheet artifacts, whose magic bytes and pixel dimensions were read back after
creation.

## Source and runtime truth

The selected implementation direction is the Subscription Value Ledger.

- Claude is visible because SQLite contains 300 Claude sessions.
- Claude remains local-history-only because the live credential probe reports
  `token expired`; it supplies no authenticated allowance card or Discord
  authority.
- Codex is authenticated and supplies the only live provider allowance rail.
- `all` is an explicit cross-provider analytics aggregate, not an authenticated
  quota source.
- Codex Costs renders 47 sessions, 1.2B observed tokens, 96% cache reuse, and
  `0 / 47` cost coverage. It does not render a monetary chart or `$0`.
- Claude Costs renders 300 sessions, 10.1B observed tokens, 98% cache reuse,
  and `299 / 300` partial legacy-calculated coverage with the denominator and
  provenance visible.

The approved reference illustrates three healthy providers and exact monetary
values. The live runtime cannot reproduce those facts honestly. The accepted
implementation preserves the reference's dense edge-to-edge hierarchy while
rendering the current provider boundaries instead of copying example data.

## Visual comparison

The final same-input comparison confirms:

- one edge-to-edge workspace with no centered-dashboard gutter;
- a provider allowance rail next to the current-work surface;
- stable matte-black surfaces, neutral borders, white hierarchy, Codex blue,
  Claude coral, and semantic green/amber status color;
- consistent navigation, source cards, title rhythm, metric strips, table
  density, and responsive stacking across all seven routes;
- legible token/cost boundaries without fake zeroes or empty monetary charts;
- native assets for Pulse, Codex, Claude, and Discord preview art.

The implementation does not copy the reference's unimplemented activity
timeline, IDE action, or fabricated multi-provider billing cards. Those are
product-scope differences, not visual regressions in the accepted Pulse v1.7
route set.

## Interaction and responsive proof

| Seam | Fresh result |
| --- | --- |
| Seven route navigation controls | PASS; each route became active and exposed its expected headings |
| Analytics source selection | PASS; `all`, Codex, and Claude local history changed the intended provider scope |
| Discord provider mutation boundary | PASS; selecting Claude local history did not change the active Discord provider |
| Codex unavailable cost state | PASS; token ledger remained populated and money stayed `Not reported` |
| Claude partial cost state | PASS; known spend remained partial with `299 / 300` coverage and provenance |
| Mobile page overflow | PASS; every route reported `documentElement.scrollWidth = clientWidth = 390` |
| Mobile main overflow | PASS; every route reported `main.scrollWidth = main.clientWidth = 380` |
| Notification center | PASS; dialog opened within the `390 x 844` viewport and rendered the durable `All clear` state |
| Browser console | PASS; zero warnings and zero errors after wide, mobile, source-selection, and notification flows |
| Final Home viewport | PASS; `clientWidth=1488`, `clientHeight=1058`, `scrollWidth=1488` |

Screenshots and DOM inspection do not prove complete WCAG conformance. Screen
reader announcements, OS high-contrast behavior, and every keyboard sequence
remain outside this visual gate.

## Comparison history and findings

1. Initial live Home captured at `1488 x 1058` and compared side by side with
   the approved reference.
2. All seven routes captured wide and inspected in one contact sheet.
3. All seven routes captured at `390 x 844`; no page or main-region horizontal
   overflow was observed.
4. P2 found: billion-scale totals rendered as four-digit millions
   (`11330.8M`, `1245.4M`), which weakened scanability.
5. P2 fixed: the canonical token formatter now uses `B` at one billion or
   greater; focused formatter and Costs tests passed.
6. Revised Codex and Claude cost states captured and inspected.
7. Final Home captured again and compared against the source in
   `22-reference-vs-home-final.png`.

- P0: 0 known.
- P1: 0 known.
- P2: 0 known after the formatter correction.
- P3: 0 known.

final result: passed

## Signal Ledger v2 overhaul (2026-08-03)

Premium evolution of the accepted Signal Ledger direction (no visual reset):
stronger hierarchy, token-driven elevation, and provider color (Codex blue,
Claude coral) promoted to a structural accent. Reviewed against the same live
Vite + authenticated Rust bridge, reading current SQLite/provider state.

### System foundations

- New tokens in `global.css`: `--codex`/`--claude` + `--provider-accent(-dim)`
  resolved from `html[data-provider]`; elevation scale `--elev-0/1/2`; activated
  `--panel-sheen`/`--panel-sheen-strong` in dark; `.panel`/`.panel--raised`/
  `.panel--hero` contract; provider-accent `.kicker`. Focus ring and selected
  source ring now track the active provider.
- Shared components: `StatusPill` (live/stale/waiting/expired/paused),
  `SegmentedControl`, `PanelHeader`, `MetricStrip`.

### Structural fixes (13 review comments)

- Access bar: source cards show a visible `StatusPill`; selected uses the
  provider-accent ring; "Source health" is an icon + status indicator.
- Killed tinted/gray gradient panel fills that broke "matte is the only
  app-panel fill": Costs value ledger, Reports cache-health hero + trace card,
  Settings rail. All now matte with token sheen/elevation.
- Reports window control uses the shared `SegmentedControl`; grade glow reduced.
- Dashboard "Work now" and provider-accent selection accents unified across
  Dashboard/Context.

### Functional fixes

- **Claude "no limits" (expired):** backend now emits a classified
  `unavailable_reason` (`expired`/`not_configured`/`probe_failed`/`no_data`).
  The allowance rail names an expired Claude session and keeps its 300-session
  local analytics instead of a generic "waiting for proof" empty state.
- **Missing avatar:** `get_discord_user` now returns `avatar_default_url`
  (always-resolvable). The Discord preview swaps to it on the img error event
  (stale hash → CDN 404), then to `PulseMark`. Live proof: rendered avatar
  `naturalWidth = 256`.

### Fresh gate results

| Seam | Result |
| --- | --- |
| Frontend unit/component tests | PASS; vitest 218 passed (30 files) incl. new StatusPill, SegmentedControl, AllowanceRail expired, avatar fallback |
| Type check | PASS; `svelte-check` 0 errors, 0 warnings |
| Backend | PASS; `cargo test --workspace`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` all clean |
| Live 7-route review (dark) | PASS; provider-accent nav, expired/live source pills, matte heroes, zero console errors |
| Light theme | PASS; readable contrast, provider color intact |
| Mobile 390x844 | PASS; `documentElement.scrollWidth = clientWidth = 390`, zero console errors |
| Discord avatar | PASS; renders with `naturalWidth = 256` via the fallback path |

- P0: 0 known. P1: 0 known. P2: 0 known. P3: 0 known.

Not proven here: full WCAG conformance, screen-reader semantics, and the
production Tauri bundle. Reviewed in the loopback dev bridge only.

final result: passed

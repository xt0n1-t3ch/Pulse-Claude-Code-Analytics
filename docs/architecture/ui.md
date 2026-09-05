# Pulse UI design system

Pulse uses an ultra-dark operational shell with a true-black canvas. The canonical tokens and reusable layout patterns live in `frontend/src/styles/global.css`; views consume those owners rather than redefining a second palette.

## Color contract

- Dark and light themes keep navigation, surfaces, borders, controls, and ordinary copy neutral. Provider and status signals use deliberate high-contrast color without tinting the canvas.
- Semantic values may use `--success`, `--warning`, `--danger`, or `--info` when the color communicates positive, neutral/caution, negative, or informational state.
- Provider identity is scoped in `frontend/src/lib/provider.ts`: Claude is coral and Codex is blue. Provider color does not become the application accent.
- Empty data is neutral. A zero or missing reading must not look successful or dangerous unless the backend supplies that meaning.

## Information hierarchy

- Every primary view has one concise title and one control group. Kicker/description copy appears only when it adds information rather than repeating navigation.
- Repeated KPIs use the shared flat `.metric-strip`; individual `StatCard` cells do not render as unrelated gray cards.
- Loading and empty states use `.state-panel` and explain the next observable state without fake analytics or decorative glyphs.
- One fact has one visible owner. The Dashboard switcher owns live-session selection, Context owns active context-window detail, account quota owns provider usage, and the session-status rail is reserved for the selected session's live signals.
- Provider limits form a content-driven horizontal ledger above the live
  workspace. They never reserve a full-height sidebar after the reported
  windows end.
- Dark panels use `--surface-panel` and `.surface-matte`; view-local sheens or gray/navy elevated fills are not alternate themes.
- Tables and detailed surfaces keep their existing backend/API owners. The redesign changes presentation, not analytics semantics.
- Reports prompt excerpts are sensitive content. The backend normalizes and bounds each preview, while the UI keeps it out of the DOM until the user explicitly reveals that session.

## Responsive and accessibility contract

- Desktop navigation is labeled; compact widths collapse to the existing icon rail.
- Persistent controls remain keyboard reachable with visible focus.
- Tables may scroll inline rather than hiding columns or expanding the page past its minimum window.
- Discord's field grid stacks at 1180px, before the content column can clip controls; live-instance selectors collapse from four columns to two and then one.
- The shared provider strip fills the operational width as one compact ledger.
  Its health control occupies the final grid cell instead of floating over
  empty chrome. At narrow widths the provider cells scroll inline without
  changing analytics or Discord provider ownership.
- Reduced-motion preferences disable nonessential broadcast animation, and loading motion never carries information by itself.

## Loading and bundle contract

- Home remains in the entry chunk so the operational workspace paints without
  a route waterfall.
- Sessions, Context, Costs, Reports, Discord, and Settings load on demand and
  expose a compact live region while their chunk is resolving.
- Route requests are generation-guarded: a slower prior import cannot replace
  the view selected most recently.

## Persistence and updates

- Discord settings save through the existing Tauri commands. The UI exposes `Saving changes...` and `Saved automatically`; a failed write still rolls back to backend truth.
- Update discovery runs automatically. `New Update Available` exposes **Update** only after a signed platform updater passes preflight; otherwise it offers **Open release**. A successful signed install relaunches Pulse.

## Focused validators

```bash
npm --prefix frontend run test -- tests/unit/view-router.test.ts tests/components/DesignSystem.test.ts tests/components/Dashboard.test.ts tests/components/Discord.test.ts tests/components/UpdateBanner.test.ts
PULSE_VISUAL_URL=http://127.0.0.1:1420 node tests/visual/verify-responsive.mjs
cargo test --test codex_account_usage --jobs 2
npm --prefix frontend run check
```

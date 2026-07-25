# Pulse UI design system

Pulse uses a Vercel/Geist-style monochrome shell. The canonical tokens and reusable layout patterns live in `frontend/src/styles/global.css`; views consume those owners rather than redefining a second palette.

## Color contract

- Dark and light themes keep navigation, surfaces, borders, controls, focus, and ordinary copy monochrome.
- Semantic values may use `--success`, `--warning`, `--danger`, or `--info` when the color communicates positive, neutral/caution, negative, or informational state.
- Provider identity is scoped in `frontend/src/lib/provider.ts`: Claude is coral and Codex is blue. Provider color does not become the application accent.
- Empty data is neutral. A zero or missing reading must not look successful or dangerous unless the backend supplies that meaning.

## Information hierarchy

- Every primary view has one concise title and one control group. Kicker/description copy appears only when it adds information rather than repeating navigation.
- Repeated KPIs use the shared flat `.metric-strip`; individual `StatCard` cells do not render as unrelated gray cards.
- Loading and empty states use `.state-panel` and explain the next observable state without fake analytics or decorative glyphs.
- One fact has one visible owner. The Dashboard switcher owns live-session selection, Context owns active context-window detail, account quota owns provider usage, and the session-status rail is reserved for the selected session's live signals.
- Dark panels use `--surface-panel` and `.surface-matte`; view-local sheens or gray/navy elevated fills are not alternate themes.
- Tables and detailed surfaces keep their existing backend/API owners. The redesign changes presentation, not analytics semantics.

## Responsive and accessibility contract

- Desktop navigation is labeled; compact widths collapse to the existing icon rail.
- Persistent controls remain keyboard reachable with visible focus.
- Tables may scroll inline rather than hiding columns or expanding the page past its minimum window.
- Discord's field grid stacks at 1180px, before the content column can clip controls; live-instance selectors collapse from four columns to two and then one.
- Reduced-motion preferences disable nonessential broadcast animation, and loading motion never carries information by itself.

## Persistence and updates

- Discord settings save through the existing Tauri commands. The UI exposes `Saving changes...` and `Saved automatically`; a failed write still rolls back to backend truth.
- Update discovery runs automatically. `New Update Available` exposes one explicit **Update** action; after the signed updater successfully downloads and installs, Pulse relaunches itself.

## Focused validators

```bash
npm --prefix frontend run test -- tests/components/DesignSystem.test.ts tests/components/Dashboard.test.ts tests/components/Discord.test.ts tests/components/UpdateBanner.test.ts
cargo test --test codex_account_usage --jobs 2
npm --prefix frontend run check
```

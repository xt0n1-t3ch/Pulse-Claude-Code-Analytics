# Pulse v1.6.5 design QA

## Source truth

- Layout reference: `C:/Users/xt0n1/.codex/generated_images/019f9981-f1a0-7b33-8a6e-09a7fc5cc54d/exec-1f126610-eb3a-4acb-be61-7bace4f2c744.png`.
- Authoritative correction: Pulse keeps its Vercel/Geist monochrome shell. Provider identity and semantic values may use color; Codex identity is blue. The reference's amber accent is not part of the product contract.
- Canonical UI owners: `frontend/src/styles/global.css`, shared shell components, and each existing Svelte view.
- Data constraint: runtime facts come from the Rust/Tauri owners. Reference values are composition examples, never seed data.

## Comparison input

- Combined source and runtime image: `C:/Users/xt0n1/.codex/visualizations/2026/07/25/019f9981-f1a0-7b33-8a6e-09a7fc5cc54d/pulse-v165-reference-runtime-comparison.png`.
- Runtime Dashboard: `C:/Users/xt0n1/.codex/visualizations/2026/07/25/019f9981-f1a0-7b33-8a6e-09a7fc5cc54d/pulse-v165-dashboard-1084x912-final.png`.
- State: live Codex backend, four simultaneous instances, dark theme, 1084x912.
- Judgment boundary: the implementation matches the selected information hierarchy while preserving Pulse's real provider, session, analytics, quota, updater, and Discord behaviors.

## Surface review

| Surface | Fresh observation | Result |
| --- | --- | --- |
| Shell and provider | Neutral shell in dark and light; Codex blue remains provider-scoped; compact icon rail activates on narrow windows. | PASS |
| Dashboard | Four live instances form a readable 4-column row at 1280 and a 2x2 selector at 1084/900/720. Selected-session detail owns exact context and live token composition. | PASS |
| Account quota | Authenticated Codex account API is the primary owner. Live QA reported 85% used / 15% available at 10:24 local time; the UI no longer reuses the stale 48% JSONL value. | PASS |
| Activity by hour | Labels and peak use full AM/PM and display `Local time · America/Bogota`. | PASS |
| Sessions | Live work, KPIs, costly-session ledger, and durable history have distinct owners on matte surfaces. | PASS |
| Context | Exactly four active windows appeared; no historical `.session-pill` or `.usage-row` owner rendered. The selected live window alone owns the detailed breakdown. | PASS |
| Costs | Budget cockpit, cost type/model split, project chart, and session ledger remain backend-driven with no duplicate page heading. | PASS |
| Reports | The selected saved-session window is explicit; no-action recommendation duplicates are omitted; timeline and export controls remain functional. | PASS |
| Discord | Controls and preview are side by side at 1084 and stack without clipping at 720. A toggle round trip returned `Saved automatically` and restored the original value. | PASS |
| Settings and updater | v1.6.5 is visible in the top bar and Settings; update checks run from the signed updater seam; source and storage owners remain visible. | PASS |
| Light theme | Dashboard and Settings use white neutral surfaces, retained borders, semantic values, and provider identity without amber shell accents. | PASS |
| Responsive | No document-level horizontal overflow at 1280x860, 1084x912, 900x700, or 720x560. | PASS |

## Evidence

- Dashboard dark: `pulse-v165-dashboard-1280x860.png`, `pulse-v165-dashboard-1084x912-final.png`, `pulse-v165-dashboard-720x560.png`.
- Dashboard light: `pulse-v165-dashboard-light-1084x912.png`.
- Sessions: `pulse-v165-sessions-1084x912.png`.
- Context: `pulse-v165-context-1084x912.png`.
- Costs: `pulse-v165-costs-1084x912.png`.
- Reports: `pulse-v165-reports-1084x912.png`.
- Discord: `pulse-v165-discord-1084x912.png`.
- Settings light: `pulse-v165-settings-light-1084x912.png`.

## Validation

- Rust workspace: 511 tests passed across unit and integration binaries.
- Frontend: 20 files / 135 tests passed.
- `svelte-check`: 0 errors and 0 warnings.
- Production frontend build: passed.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.
- Live processes used for QA: repo-owned Vite PID 7020 on 1420 and repo-owned Pulse v1.6.5 PID 33192 on 1421.

## Findings

- P0: 0
- P1: 0
- P2: 0
- P3: 0

Final result: PASS.

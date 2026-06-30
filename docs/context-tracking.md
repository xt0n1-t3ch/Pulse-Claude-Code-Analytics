# Context window tracking — current fill vs. all-time peak

Pulse tracks two distinct, intentionally separate numbers for a Claude Code session's context
usage. Conflating them was a real bug (fixed in v1.4.1) — this page documents why they have to
stay separate.

## The two fields

| Field | Meaning | Behavior |
|---|---|---|
| `max_turn_api_input` | The largest total API input (`input_tokens + cache_creation_input_tokens + cache_read_input_tokens`) any single turn has ever had in this session. | Monotonically increasing. Never decreases, including across compactions. |
| `current_context_tokens` | Total API input as of the most recent turn since the session's last compaction (or since session start, if it has never compacted). | Resets to the real post-compaction size at every `compact_boundary` event, then tracks forward turn by turn. |

`max_turn_api_input` answers a **lifetime capability question**: has this session ever needed
the 1M extended-context tier? Once true, it stays true for the rest of the session's life — that
matches how Claude Code itself behaves (a session that has gone into extended-context territory
doesn't shrink back to the 200K tier mid-session). This is the field behind:

- `is_ga_1m_context(...) || model_id.contains("[1m]") || max_turn_api_input > 200_000` — the
  1M-vs-200K window-size decision (`src-tauri/src/commands.rs`, `build_claude_session_infos` and
  `claude_context_window`).

`current_context_tokens` answers a **point-in-time question**: how full is the context window
*right now*? This is the field behind every UI surface that claims to show current state:

- `SessionInfo.context_used_tokens` (the Sessions/Dashboard "ctx-1m" badge's percentage).
- `ContextBreakdown.used_tokens` (the Context Window view's header card and the "Critical /
  Warning" recommendation engine).
- `SessionContextUsage.used_tokens` (the "Per-session utilization" panel), which derives from
  `ContextBreakdown` and so is fixed by the same change.

## The bug this prevents

Before v1.4.1, every one of those "current state" surfaces read `max_turn_api_input` — the
all-time peak — instead of a true point-in-time value. A session that hit a high-water mark
right before an auto-compaction would show that historical peak **forever after**, even hours
later, even after the compaction emptied the context back down to a few thousand tokens. The
Context Window view would show "100% full — CRITICAL, will auto-compact soon" for a session that
had, in reality, already compacted and was nowhere near full.

This was confirmed live against a real, currently-running session: `get_context_breakdown`
returned `used_tokens: 999486` (a pre-compaction peak from 2.5 hours earlier) when the session's
own JSONL transcript recorded a `compact_boundary` event with `compactMetadata.postTokens: 25500`
shortly after that peak — i.e. the true current fill was roughly 25,500 tokens plus whatever had
accumulated since, not 999,486.

## Where the post-compaction number comes from

Claude Code writes an explicit system event into the JSONL transcript on every compaction:

```json
{
  "type": "system",
  "subtype": "compact_boundary",
  "compactMetadata": {
    "trigger": "manual",
    "preTokens": 959012,
    "postTokens": 25500,
    "durationMs": 108397
  }
}
```

`src/session.rs`'s JSONL accumulator detects `type: "system"` lines with
`subtype: "compact_boundary"` and resets `current_context_tokens` to `compactMetadata.postTokens`
the instant it sees one — covering the gap between "just compacted" and "the next turn arrives"
that a pure "track the latest turn" approach would miss. A `compact_boundary` with missing or
malformed `compactMetadata` is a no-op (the field keeps its prior value) rather than a panic or a
silent reset to zero.

`max_turn_api_input` is untouched by compaction detection — it keeps accumulating via `.max()`
exactly as before.

## Aggregation scope (Dashboard vs. Costs)

A related but separate question: why does the Dashboard's "Total Cost" sometimes differ from the
Costs view's "Total Spent"? They answer different questions on purpose:

- Dashboard's **Total Cost (Live)** sums only the sessions Pulse currently considers live
  (`get_metrics()` → `current_live_session_infos()`).
- Costs view's **Total Spent (30d)** sums the persisted historical database over a rolling
  30-day window, merged with live sessions (`getSessionHistory(30, ...)`).

Both numbers are real and correctly computed; neither is fake or stale. The labels make the
scope explicit so the difference reads as "two different real questions" rather than "an
inconsistency."

## Source

Investigated live via the Chrome DevTools Protocol against a running v1.4.0 build (WebView2
remote debugging: `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>`, then
`window.__TAURI_INTERNALS__.invoke(...)` through `Runtime.evaluate`), and against the real JSONL
transcript on disk. See the `pulse-context-staleness-and-architecture-sdd.md` plan for the full
diagnosis ledger.

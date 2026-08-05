# Pulse notifications

## Introduction

This reference documents the durable notification center used by Pulse's Tauri
backend. It is for frontend and runtime maintainers who need to consume provider
health, quota, and Discord connectivity events without reimplementing dedupe or
read-state rules.

## Contract

`pulse::notifications::NotificationStore` owns the SQLite tables
`pulse_notifications` and `pulse_notification_state` in the canonical
`pulse-analytics.db`. `NotificationKind` has four values:

- `provider_health`
- `quota_threshold`
- `quota_reset`
- `discord_connectivity`

Each record carries a stable provider/window key, title, body, optional action,
creation time, read time, and dismissal time. `list()` excludes dismissed rows;
`list_all()` includes them. `unread_count()`, `mark_read()`,
`mark_all_read()`, `dismiss()`, and `undismiss()` are durable operations.

## Dedupe and triggers

The state table stores the last observed transition category and last native
delivery. The poller creates native records only for a genuine authenticated
quota reset on a fresh route:

- Codex: `remaining_percent < 100 -> 100`.
- Claude: `used_percent > 0 -> 0`.

The first sample establishes a silent baseline. Reset timestamps are descriptive
metadata, not the fingerprint, so timestamp drift, process restarts, repeated
cache samples, and stale reads cannot create false resets. Unproofed,
unauthenticated, stale, or unavailable routes never enter the reset observer.

The store still exposes provider-health, threshold, and Discord connectivity
observers for explicit callers and compatibility, but the Pulse background
poller does not promote those diagnostics to native alerts. The same applies to
threshold buckets: provider health, quota pressure, and Discord IPC state remain
visible in diagnostics without producing an automatic toast.

On schema initialization, legacy `quota_reset` rows are preserved for history
and marked dismissed once. Migration v2 deliberately supersedes the original
v1 marker because an older concurrent producer could append false reset rows
after v1 ran. The v2 sweep dismisses every row that existed before its own
commit, then records its marker in the same transaction; genuine transitions
inserted afterward remain visible. This prevents timestamp-only false resets
from resurfacing while keeping the complete audit trail.

A later one-time migration, `dismiss_spurious_poll_cadence_alerts_v1`, dismisses
every `provider_health`, `quota_threshold`, and `discord_connectivity` row and
clears their dedupe state. A prior build briefly promoted those poll-cadence
diagnostics to native alerts, which spammed duplicate, incoherent toasts because
the underlying signals legitimately flap (a pending account read, a percentage
hovering at a threshold, a reconnecting IPC). The migration lets the bell and
native delivery start clean; the observers stay callable but the poller does not
emit them.

Reset copy is human-readable: a clean, capitalized provider name, a friendly
window label (for example `5-hour`, `weekly`, `Sonnet`), and a local reset time
such as `Aug 07, 10:39 PM` — never an internal id or a raw RFC3339 timestamp.

## Tauri and tray

`tauri-plugin-notification` is registered by the desktop entrypoint and routes
native delivery to Windows, macOS, and Linux. A failed native delivery does not
delete the persisted row; the frontend can still present it in the notification
center. The command adapter emits `pulse://notification` after a new record.

The tray menu always includes an unread count. Tooltip updates are skipped on
Linux because Tauri documents that surface as unsupported; title updates are
skipped on Windows. Numeric badges are only claimed on macOS, where the window
badge API is available. Other platforms use the tray menu and any supported
title/tooltip surface instead of pretending a numeric badge exists.

## IPC commands

The Tauri command owner exposes:

- `get_notifications(limit?)`
- `get_unread_notification_count()`
- `mark_notification_read(id)`
- `mark_all_notifications_read()`
- `dismiss_notification(id)`

All mutating commands return an explicit success/count or a stringified storage
error; no command silently fabricates a notification when the database cannot
be opened.

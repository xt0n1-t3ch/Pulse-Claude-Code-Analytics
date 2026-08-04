use chrono::{TimeZone, Utc};
use pulse::notifications::{NotificationKind, NotificationSpec, NotificationStore};
use rusqlite::Connection;

fn store() -> (&'static Connection, NotificationStore<'static>) {
    let connection = Box::leak(Box::new(Connection::open_in_memory().expect("sqlite")));
    NotificationStore::initialize(connection).expect("notification schema");
    (connection, NotificationStore::new(connection))
}

fn spec(body: &str) -> NotificationSpec {
    NotificationSpec {
        kind: NotificationKind::ProviderHealth,
        provider: "codex".to_string(),
        key: "app-server".to_string(),
        title: "Codex provider".to_string(),
        body: body.to_string(),
        action: Some("Open Settings".to_string()),
    }
}

#[test]
fn transitions_are_recorded_once_and_identical_polls_are_deduped() {
    let (_connection, store) = store();
    let first = Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap();

    // An unavailable first observation is only a diagnostic baseline. Native
    // delivery starts on a real healthy -> unhealthy transition.
    assert!(
        store
            .observe(spec("Provider unavailable"), first)
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .observe(
                spec("Provider unavailable"),
                first + chrono::Duration::seconds(5)
            )
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .observe(
                spec("Provider recovered"),
                first + chrono::Duration::seconds(6)
            )
            .unwrap()
            .is_some()
    );
    assert_eq!(store.list(Some(20)).unwrap().len(), 2);
}

#[test]
fn same_state_repeats_after_cooldown_but_not_before() {
    let (_connection, store) = store();
    let first = Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap();

    assert!(
        store
            .observe(spec("Provider unavailable"), first)
            .unwrap()
            .is_some()
    );
    assert!(
        store
            .observe(
                spec("Provider unavailable"),
                first + chrono::Duration::hours(1)
            )
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .observe(
                spec("Provider unavailable"),
                first + chrono::Duration::hours(25)
            )
            .unwrap()
            .is_some()
    );
}

#[test]
fn provider_health_baseline_does_not_rearm_unchanged_failures() {
    let (_connection, store) = store();
    let first = Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap();

    // The first unavailable observation establishes a state for diagnostics,
    // but must not create a native/unread notification for an unconfigured
    // optional provider lane.
    assert!(
        store
            .observe_provider_health(
                "claude",
                "claude-subscription:default",
                false,
                Some("no credentials — check .credentials.json"),
                first,
            )
            .unwrap()
            .is_none()
    );

    // A 24-hour-plus unchanged failure remains quiet; provider health is
    // edge-triggered rather than a daily reminder.
    assert!(
        store
            .observe_provider_health(
                "claude",
                "claude-subscription:default",
                false,
                Some("no credentials — check .credentials.json"),
                first + chrono::Duration::hours(25),
            )
            .unwrap()
            .is_none()
    );

    // Recovery from that silent baseline stays quiet; only a subsequent
    // authenticated healthy baseline followed by an outage re-arms.
    assert!(
        store
            .observe_provider_health(
                "claude",
                "claude-subscription:default",
                true,
                None,
                first + chrono::Duration::hours(25) + chrono::Duration::seconds(1),
            )
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .observe_provider_health(
                "claude",
                "claude-subscription:default",
                true,
                None,
                first + chrono::Duration::hours(25) + chrono::Duration::seconds(2),
            )
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .observe_provider_health(
                "claude",
                "claude-subscription:default",
                false,
                Some("provider unavailable"),
                first + chrono::Duration::hours(25) + chrono::Duration::seconds(3),
            )
            .unwrap()
            .is_some()
    );
    assert_eq!(store.list_all(Some(20)).unwrap().len(), 1);
}

#[test]
fn unread_lifecycle_is_durable_and_dismissal_excludes_rows_from_list() {
    let (_connection, store) = store();
    let created = Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap();
    let first = store
        .observe(spec("Provider unavailable"), created)
        .unwrap()
        .unwrap();
    let second = store
        .observe(
            spec("Provider recovered"),
            created + chrono::Duration::seconds(1),
        )
        .unwrap()
        .unwrap();

    assert_eq!(store.unread_count().unwrap(), 2);
    store.mark_read(first.id).unwrap();
    assert_eq!(store.unread_count().unwrap(), 1);
    store.dismiss(second.id).unwrap();
    assert!(
        store
            .list(Some(20))
            .unwrap()
            .iter()
            .all(|item| item.id != second.id)
    );
    store.mark_all_read().unwrap();
    assert_eq!(store.unread_count().unwrap(), 0);
}

#[test]
fn quota_reset_only_notifies_on_a_genuine_provider_transition() {
    let (_connection, store) = store();
    let first = Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap();
    let reset = first + chrono::Duration::hours(2);

    // The first sample is a persisted baseline, never a notification.
    assert!(
        store
            .observe_quota_reset_transition("codex", "weekly", 10.0, 90.0, Some(reset), first)
            .unwrap()
            .is_none()
    );
    // A reset timestamp changing while the metric remains non-reset is drift,
    // not evidence that the provider actually reset the quota.
    assert!(
        store
            .observe_quota_reset_transition(
                "codex",
                "weekly",
                10.0,
                90.0,
                Some(reset + chrono::Duration::hours(1)),
                first + chrono::Duration::hours(48),
            )
            .unwrap()
            .is_none()
    );
    // Codex is genuine only when remaining reaches 100 after being below 100.
    assert!(
        store
            .observe_quota_reset_transition(
                "codex",
                "weekly",
                0.0,
                100.0,
                Some(reset),
                first + chrono::Duration::hours(48) + chrono::Duration::seconds(1),
            )
            .unwrap()
            .is_some()
    );
    // A delayed sample cannot roll the ledger backwards or create a second edge.
    assert!(
        store
            .observe_quota_reset_transition("codex", "weekly", 10.0, 90.0, Some(reset), first)
            .unwrap()
            .is_none()
    );

    // Claude is genuine only when used falls from a positive value to zero.
    assert!(
        store
            .observe_quota_reset_transition("claude", "weekly", 20.0, 80.0, Some(reset), first)
            .unwrap()
            .is_none()
    );
    assert!(
        store
            .observe_quota_reset_transition(
                "claude",
                "weekly",
                0.0,
                100.0,
                Some(reset + chrono::Duration::hours(1)),
                first + chrono::Duration::seconds(1),
            )
            .unwrap()
            .is_some()
    );
}

#[test]
fn legacy_quota_reset_rows_are_preserved_but_dismissed_once() {
    let connection = Connection::open_in_memory().expect("sqlite");
    connection
        .execute_batch(
            "CREATE TABLE pulse_notifications (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                kind TEXT NOT NULL,
                provider TEXT NOT NULL,
                notification_key TEXT NOT NULL,
                title TEXT NOT NULL,
                body TEXT NOT NULL,
                action TEXT,
                created_at TEXT NOT NULL,
                read_at TEXT,
                dismissed_at TEXT
            );
            INSERT INTO pulse_notifications
                (kind, provider, notification_key, title, body, created_at)
            VALUES ('quota_reset', 'claude', 'weekly', 'Legacy', 'false reset',
                    '2026-08-01T12:00:00Z');
            CREATE TABLE pulse_notification_migrations (
                migration TEXT PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            INSERT INTO pulse_notification_migrations (migration, applied_at)
            VALUES ('dismiss_legacy_quota_reset_rows_v1', '2026-08-01T12:01:00Z');
            INSERT INTO pulse_notifications
                (kind, provider, notification_key, title, body, created_at)
            VALUES ('quota_reset', 'codex', 'weekly', 'Legacy after v1',
                    'false reset from an older concurrent producer',
                    '2026-08-01T12:02:00Z');",
        )
        .expect("legacy notification schema");

    NotificationStore::initialize(&connection).expect("migrate legacy notifications");
    let first: (i64, i64, Option<String>) = connection
        .query_row(
            "SELECT COUNT(*), COUNT(dismissed_at), MAX(dismissed_at) FROM pulse_notifications
             WHERE kind = 'quota_reset'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read legacy notification");
    assert_eq!(first.0, 2);
    assert_eq!(first.1, 2);
    assert!(
        first.2.is_some(),
        "legacy and post-v1 rows remain but are dismissed"
    );

    connection
        .execute(
            "INSERT INTO pulse_notifications
                (kind, provider, notification_key, title, body, created_at)
             VALUES ('quota_reset', 'codex', 'weekly', 'Genuine after v2',
                     'real transition', '2026-08-01T12:03:00Z')",
            [],
        )
        .expect("insert post-migration reset");
    NotificationStore::initialize(&connection).expect("repeat migration");
    let repeated: (i64, i64, Option<String>) = connection
        .query_row(
            "SELECT COUNT(*), COUNT(dismissed_at), MAX(dismissed_at) FROM pulse_notifications
             WHERE kind = 'quota_reset'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("read repeated migration state");
    assert_eq!(repeated.0, 3);
    assert_eq!(repeated.1, 2);
    assert_eq!(repeated.2, first.2);
}

#[test]
fn provider_and_discord_diagnostics_are_single_line_and_bounded() {
    let (_connection, store) = store();
    let now = Utc.with_ymd_and_hms(2026, 8, 1, 12, 0, 0).unwrap();
    let detail = format!("\u{1b}[31m{}\nsecret", "x".repeat(400));

    // Provider health deliberately keeps an initial outage as a silent
    // diagnostic baseline; establish a healthy edge before exercising the
    // sanitized outage payload.
    store
        .observe_provider_health("claude\n\u{1b}[31m", "oauth\r\n", true, None, now)
        .unwrap();
    let provider = store
        .observe_provider_health(
            "claude\n\u{1b}[31m",
            "oauth\r\n",
            false,
            Some(&detail),
            now + chrono::Duration::seconds(1),
        )
        .unwrap()
        .unwrap();
    let discord = store
        .observe_discord_connectivity(
            "claude\n\u{1b}[31m",
            false,
            Some(&detail),
            now + chrono::Duration::seconds(1),
        )
        .unwrap()
        .unwrap();

    for record in [provider, discord] {
        assert!(!record.provider.chars().any(char::is_control));
        assert!(!record.key.chars().any(char::is_control));
        assert!(!record.title.chars().any(char::is_control));
        assert!(!record.body.chars().any(char::is_control));
        assert!(record.body.chars().count() <= 240);
    }
}

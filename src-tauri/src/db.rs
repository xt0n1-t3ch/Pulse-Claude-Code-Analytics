use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context, Result, bail};
use chrono::{Datelike, Utc};
use rusqlite::{Connection, Transaction, params};
use serde::Serialize;
use tracing::{debug, warn};

use cc_discord_presence::config;
use cc_discord_presence::cost;
use cc_discord_presence::provider::Provider;

static DB: OnceLock<Arc<Mutex<Connection>>> = OnceLock::new();

const SCHEMA_VERSION: i64 = 4;

fn context_label_tokens(label: &str) -> Option<i64> {
    let normalized = label
        .trim()
        .replace([',', '_', ' '], "")
        .to_ascii_lowercase();
    if normalized.is_empty() {
        return None;
    }

    let (number, multiplier) = if let Some(number) = normalized.strip_suffix('k') {
        (number, 1_000.0)
    } else if let Some(number) = normalized.strip_suffix('m') {
        (number, 1_000_000.0)
    } else {
        (normalized.as_str(), 1.0)
    };
    let value = number.parse::<f64>().ok()?;
    let tokens = (value * multiplier).round();
    if !tokens.is_finite() || tokens < 1.0 || tokens > i64::MAX as f64 {
        return None;
    }
    Some(tokens as i64)
}

fn session_window_tokens(s: &super::commands::SessionInfo) -> i64 {
    if s.context_window_tokens > 0 {
        return s.context_window_tokens.min(i64::MAX as u64) as i64;
    }
    if cost::is_ga_1m_context(&s.model_id) {
        return 1_000_000;
    }
    context_label_tokens(&s.context_window).unwrap_or(0)
}

fn session_used_tokens(s: &super::commands::SessionInfo) -> i64 {
    let window = session_window_tokens(s).max(0) as u64;
    s.context_used_tokens.min(window).min(i64::MAX as u64) as i64
}

fn db_path() -> PathBuf {
    config::claude_home().join("pulse-analytics.db")
}

fn active_provider() -> Provider {
    cc_discord_presence::provider::load_active_provider()
}

fn active_provider_slug() -> &'static str {
    active_provider().as_str()
}

fn storage_session_id(provider: &str, session_id: &str) -> String {
    format!("{provider}:{session_id}")
}

fn migration_backup_path(path: &Path) -> PathBuf {
    let mut file_name = path
        .file_name()
        .unwrap_or_else(|| OsStr::new("pulse-analytics.db"))
        .to_os_string();
    file_name.push(format!(".pre-v{SCHEMA_VERSION}.bak"));
    path.with_file_name(file_name)
}

fn schema_version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.query_row("PRAGMA user_version", [], |row| row.get(0))
}

fn has_user_schema(conn: &Connection) -> rusqlite::Result<bool> {
    conn.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
            WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
        )",
        [],
        |row| row.get(0),
    )
}

fn validate_database(path: &Path, expected_version: i64) -> Result<()> {
    let backup = Connection::open(path)
        .with_context(|| format!("failed to open migration backup {}", path.display()))?;
    let result: String = backup
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .with_context(|| format!("failed to validate migration backup {}", path.display()))?;
    if result != "ok" {
        bail!(
            "migration backup {} failed quick_check: {result}",
            path.display()
        );
    }
    let actual_version = schema_version(&backup)?;
    if actual_version != expected_version {
        bail!(
            "migration backup {} has schema {actual_version}, expected {expected_version}",
            path.display()
        );
    }
    Ok(())
}

fn create_migration_backup(conn: &Connection, path: &Path) -> Result<Option<PathBuf>> {
    let source_version = schema_version(conn)?;
    if source_version >= SCHEMA_VERSION || !has_user_schema(conn)? {
        return Ok(None);
    }

    let backup_path = migration_backup_path(path);
    if !backup_path.exists() {
        let backup_file = backup_path.to_str().with_context(|| {
            format!(
                "backup path is not valid Unicode: {}",
                backup_path.display()
            )
        })?;
        conn.execute("VACUUM INTO ?1", params![backup_file])
            .with_context(|| {
                format!(
                    "failed to create pre-v4 migration backup {}",
                    backup_path.display()
                )
            })?;
    }
    validate_database(&backup_path, source_version)?;
    Ok(Some(backup_path))
}

fn open_database(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create database directory {}", parent.display()))?;
    }
    let existed = path.exists();
    let conn = Connection::open(path)
        .with_context(|| format!("failed to open Pulse database {}", path.display()))?;
    if existed {
        create_migration_backup(&conn, path)?;
    }
    init_schema(&conn)?;
    Ok(conn)
}

fn db() -> &'static Arc<Mutex<Connection>> {
    DB.get_or_init(|| {
        let path = db_path();
        let conn = open_database(&path).expect("failed to initialize pulse-analytics.db");
        Arc::new(Mutex::new(conn))
    })
}

fn ensure_column(
    transaction: &Transaction<'_>,
    table: &str,
    column: &str,
    definition: &str,
) -> rusqlite::Result<()> {
    let sql = format!("SELECT EXISTS(SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1)");
    let exists: bool = transaction.query_row(&sql, params![column], |row| row.get(0))?;
    if !exists {
        transaction.execute_batch(&format!(
            "ALTER TABLE {table} ADD COLUMN {column} {definition}"
        ))?;
    }
    Ok(())
}

fn backfill_context_windows(transaction: &Transaction<'_>) -> rusqlite::Result<()> {
    let missing_windows = {
        let mut statement = transaction.prepare(
            "SELECT id, context_window FROM sessions
             WHERE window_tokens IS NULL OR window_tokens <= 0",
        )?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (id, label) in missing_windows {
        if let Some(tokens) = context_label_tokens(&label) {
            transaction.execute(
                "UPDATE sessions SET window_tokens = ?1 WHERE id = ?2",
                params![tokens, id],
            )?;
        }
    }
    Ok(())
}

fn migrate_schema(conn: &Connection) -> Result<()> {
    let previous_version = schema_version(conn)?;
    if previous_version > SCHEMA_VERSION {
        bail!(
            "Pulse database schema {previous_version} is newer than supported schema {SCHEMA_VERSION}"
        );
    }

    let transaction = conn.unchecked_transaction()?;
    transaction.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL DEFAULT 'claude',
            session_name TEXT DEFAULT NULL,
            project TEXT NOT NULL,
            model TEXT NOT NULL,
            model_id TEXT DEFAULT '',
            context_window TEXT DEFAULT 'Unknown',
            branch TEXT,
            effort TEXT DEFAULT 'Medium',
            speed TEXT NOT NULL DEFAULT 'unknown',
            speed_source TEXT NOT NULL DEFAULT 'unknown',
            speed_known INTEGER NOT NULL DEFAULT 0 CHECK (speed_known IN (0, 1)),
            started_at TEXT,
            created_at TEXT,
            ended_at TEXT,
            duration_secs INTEGER DEFAULT 0,
            total_cost REAL DEFAULT 0,
            cost_status TEXT NOT NULL DEFAULT 'unavailable'
                CHECK (cost_status IN ('exact', 'partial', 'unavailable')),
            cost_source TEXT NOT NULL DEFAULT 'unknown',
            known_cost REAL DEFAULT NULL CHECK (known_cost IS NULL OR known_cost >= 0),
            cached_input_savings REAL DEFAULT NULL
                CHECK (cached_input_savings IS NULL OR cached_input_savings >= 0),
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            cache_write_tokens INTEGER DEFAULT 0,
            cache_read_tokens INTEGER DEFAULT 0,
            total_tokens INTEGER DEFAULT 0,
            input_cost REAL DEFAULT 0,
            output_cost REAL DEFAULT 0,
            cache_write_cost REAL DEFAULT 0,
            cache_read_cost REAL DEFAULT 0,
            has_thinking INTEGER DEFAULT 0,
            subagent_count INTEGER DEFAULT 0,
            is_active INTEGER DEFAULT 1,
            updated_at TEXT NOT NULL,
            used_tokens INTEGER DEFAULT 0,
            window_tokens INTEGER DEFAULT 0,
            context_source TEXT NOT NULL DEFAULT 'unknown',
            context_raw_source TEXT NOT NULL DEFAULT 'unknown',
            raw_window_tokens INTEGER DEFAULT NULL
                CHECK (raw_window_tokens IS NULL OR raw_window_tokens >= 0),
            effective_context_percent INTEGER DEFAULT NULL
                CHECK (
                    effective_context_percent IS NULL
                    OR effective_context_percent BETWEEN 1 AND 100
                )
        );

        CREATE TABLE IF NOT EXISTS daily_stats (
            date TEXT NOT NULL,
            provider TEXT NOT NULL DEFAULT 'claude',
            project TEXT NOT NULL,
            model TEXT NOT NULL,
            session_count INTEGER DEFAULT 0,
            total_cost REAL DEFAULT 0,
            total_tokens INTEGER DEFAULT 0,
            input_tokens INTEGER DEFAULT 0,
            output_tokens INTEGER DEFAULT 0,
            cache_write_tokens INTEGER DEFAULT 0,
            cache_read_tokens INTEGER DEFAULT 0,
            PRIMARY KEY (date, provider, project, model)
        );

        CREATE TABLE IF NOT EXISTS budget_config (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            monthly_budget REAL DEFAULT 0,
            alert_threshold_pct REAL DEFAULT 80,
            updated_at TEXT NOT NULL DEFAULT '1970-01-01'
        );
        ",
    )?;

    for (column, definition) in [
        ("provider", "TEXT DEFAULT 'claude'"),
        ("session_name", "TEXT DEFAULT NULL"),
        ("created_at", "TEXT DEFAULT NULL"),
        ("used_tokens", "INTEGER DEFAULT 0"),
        ("window_tokens", "INTEGER DEFAULT 0"),
        ("speed", "TEXT NOT NULL DEFAULT 'unknown'"),
        ("speed_source", "TEXT NOT NULL DEFAULT 'unknown'"),
        (
            "speed_known",
            "INTEGER NOT NULL DEFAULT 0 CHECK (speed_known IN (0, 1))",
        ),
        (
            "cost_status",
            "TEXT NOT NULL DEFAULT 'unavailable' CHECK (cost_status IN ('exact', 'partial', 'unavailable'))",
        ),
        ("cost_source", "TEXT NOT NULL DEFAULT 'unknown'"),
        (
            "known_cost",
            "REAL DEFAULT NULL CHECK (known_cost IS NULL OR known_cost >= 0)",
        ),
        (
            "cached_input_savings",
            "REAL DEFAULT NULL CHECK (cached_input_savings IS NULL OR cached_input_savings >= 0)",
        ),
        ("context_source", "TEXT NOT NULL DEFAULT 'unknown'"),
        ("context_raw_source", "TEXT NOT NULL DEFAULT 'unknown'"),
        (
            "raw_window_tokens",
            "INTEGER DEFAULT NULL CHECK (raw_window_tokens IS NULL OR raw_window_tokens >= 0)",
        ),
        (
            "effective_context_percent",
            "INTEGER DEFAULT NULL CHECK (effective_context_percent IS NULL OR effective_context_percent BETWEEN 1 AND 100)",
        ),
    ] {
        ensure_column(&transaction, "sessions", column, definition)?;
    }
    ensure_column(
        &transaction,
        "daily_stats",
        "provider",
        "TEXT DEFAULT 'claude'",
    )?;

    backfill_context_windows(&transaction)?;
    if previous_version < SCHEMA_VERSION {
        transaction.execute(
            "UPDATE sessions
             SET provider = 'claude'
             WHERE provider IS NULL OR trim(provider) = ''",
            [],
        )?;
        transaction.execute(
            "UPDATE sessions
             SET input_tokens = MAX(total_tokens - output_tokens, 0)
             WHERE lower(provider) = 'codex'
               AND total_tokens > 0
               AND input_tokens > MAX(total_tokens - output_tokens, 0)",
            [],
        )?;
        transaction.execute(
            "UPDATE sessions
             SET created_at = COALESCE(created_at, started_at, updated_at)
             WHERE created_at IS NULL",
            [],
        )?;
        transaction.execute(
            "UPDATE sessions
             SET started_at = updated_at
             WHERE started_at IS NOT NULL
               AND instr(started_at, 'T') = 0
               AND updated_at IS NOT NULL",
            [],
        )?;
        transaction.execute(
            "UPDATE sessions
             SET started_at = COALESCE(started_at, created_at, updated_at)
             WHERE started_at IS NULL",
            [],
        )?;
        transaction.execute(
            "UPDATE sessions
             SET speed = 'unknown',
                 speed_source = 'legacy',
                 speed_known = 0,
                 cost_status = 'unavailable',
                 cost_source = 'legacy',
                 known_cost = NULL,
                 cached_input_savings = NULL,
                 context_source = 'legacy',
                 context_raw_source = 'unknown',
                 raw_window_tokens = NULL,
                 effective_context_percent = NULL
             WHERE lower(provider) = 'codex'",
            [],
        )?;
        transaction.execute(
            "UPDATE sessions
             SET cost_status = 'exact',
                 cost_source = 'legacy-calculated',
                 known_cost = MAX(total_cost, 0),
                 context_source = CASE
                     WHEN window_tokens > 0 THEN 'legacy'
                     ELSE context_source
                 END
             WHERE lower(provider) = 'claude'",
            [],
        )?;
    }
    transaction.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project);
        CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);
        CREATE INDEX IF NOT EXISTS idx_sessions_active ON sessions(is_active);
        CREATE INDEX IF NOT EXISTS idx_sessions_model ON sessions(model);
        CREATE INDEX IF NOT EXISTS idx_sessions_provider ON sessions(provider);
        CREATE INDEX IF NOT EXISTS idx_sessions_provider_active ON sessions(provider, is_active);
        CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at);
        CREATE INDEX IF NOT EXISTS idx_sessions_created ON sessions(created_at);
        CREATE INDEX IF NOT EXISTS idx_sessions_history_ts
            ON sessions(COALESCE(started_at, created_at, updated_at));
        CREATE UNIQUE INDEX IF NOT EXISTS idx_daily_stats_provider_key
            ON daily_stats(provider, date, project, model);

        CREATE VIRTUAL TABLE IF NOT EXISTS sessions_fts USING fts5(
            project, model, branch,
            content='sessions',
            content_rowid='rowid',
            tokenize='porter unicode61'
        );

        CREATE TRIGGER IF NOT EXISTS sessions_ai AFTER INSERT ON sessions BEGIN
            INSERT INTO sessions_fts(rowid, project, model, branch)
            VALUES (new.rowid, new.project, new.model, COALESCE(new.branch, ''));
        END;

        CREATE TRIGGER IF NOT EXISTS sessions_au AFTER UPDATE ON sessions BEGIN
            DELETE FROM sessions_fts WHERE rowid = old.rowid;
            INSERT INTO sessions_fts(rowid, project, model, branch)
            VALUES (new.rowid, new.project, new.model, COALESCE(new.branch, ''));
        END;

        ",
    )?;
    transaction.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    transaction.commit()?;
    Ok(())
}

fn init_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA cache_size = -8000;
        ",
    )
    .context("failed to configure pulse-analytics database")?;
    migrate_schema(conn)?;
    debug!("Pulse analytics DB initialized at {}", db_path().display());
    Ok(())
}

#[derive(Debug, Serialize, Clone)]
pub struct HistoricalSession {
    pub id: String,
    pub provider: String,
    pub session_name: Option<String>,
    pub project: String,
    pub model: String,
    pub model_id: String,
    pub context_window: String,
    pub branch: Option<String>,
    pub effort: String,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub duration_secs: i64,
    pub total_cost: f64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_read_tokens: i64,
    pub total_tokens: i64,
    pub input_cost: f64,
    pub output_cost: f64,
    pub cache_write_cost: f64,
    pub cache_read_cost: f64,
    pub has_thinking: bool,
    pub subagent_count: i64,
    pub is_active: bool,
    pub used_tokens: i64,
    pub window_tokens: i64,
}

fn history_timestamp_expr() -> &'static str {
    "COALESCE(started_at, created_at, updated_at)"
}

#[allow(clippy::too_many_arguments)]
fn query_sessions(
    conn: &Connection,
    days: Option<i64>,
    from_iso: Option<&str>,
    to_iso: Option<&str>,
    project: Option<&str>,
    model: Option<&str>,
    min_cost: Option<f64>,
    max_cost: Option<f64>,
    start_hour: Option<i64>,
    end_hour: Option<i64>,
    limit: Option<i64>,
) -> Vec<HistoricalSession> {
    let history_ts = history_timestamp_expr();
    let provider = active_provider_slug().to_string();
    let mut sql = String::from(
        "SELECT id, provider, session_name, project, model, model_id, context_window, branch, effort,
            started_at, ended_at, duration_secs, total_cost,
            input_tokens, output_tokens, cache_write_tokens, cache_read_tokens, total_tokens,
            input_cost, output_cost, cache_write_cost, cache_read_cost,
            has_thinking, subagent_count, is_active, used_tokens, window_tokens
         FROM sessions
         WHERE provider = ?1",
    );
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(provider)];
    let mut param_idx = 2;

    if let Some(d) = days {
        let cutoff = (Utc::now() - chrono::Duration::days(d)).to_rfc3339();
        sql.push_str(&format!(
            " AND COALESCE({history_ts}, datetime('now')) >= ?{param_idx}"
        ));
        params_vec.push(Box::new(cutoff));
        param_idx += 1;
    }

    if let Some(from_iso) = from_iso {
        sql.push_str(&format!(
            " AND COALESCE({history_ts}, datetime('now')) >= ?{param_idx}"
        ));
        params_vec.push(Box::new(from_iso.to_string()));
        param_idx += 1;
    }

    if let Some(to_iso) = to_iso {
        sql.push_str(&format!(
            " AND COALESCE({history_ts}, datetime('now')) <= ?{param_idx}"
        ));
        params_vec.push(Box::new(to_iso.to_string()));
        param_idx += 1;
    }

    if let Some(p) = project {
        sql.push_str(&format!(" AND project = ?{param_idx}"));
        params_vec.push(Box::new(p.to_string()));
        param_idx += 1;
    }

    if let Some(m) = model {
        sql.push_str(&format!(" AND model = ?{param_idx}"));
        params_vec.push(Box::new(m.to_string()));
        param_idx += 1;
    }

    if let Some(min_cost) = min_cost {
        sql.push_str(&format!(" AND total_cost >= ?{param_idx}"));
        params_vec.push(Box::new(min_cost));
        param_idx += 1;
    }

    if let Some(max_cost) = max_cost {
        sql.push_str(&format!(" AND total_cost <= ?{param_idx}"));
        params_vec.push(Box::new(max_cost));
        param_idx += 1;
    }

    if let Some(start_hour) = start_hour {
        sql.push_str(&format!(
            " AND CAST(substr(COALESCE({history_ts}, ''), 12, 2) AS INTEGER) >= ?{param_idx}"
        ));
        params_vec.push(Box::new(start_hour));
        param_idx += 1;
    }

    if let Some(end_hour) = end_hour {
        sql.push_str(&format!(
            " AND CAST(substr(COALESCE({history_ts}, ''), 12, 2) AS INTEGER) <= ?{param_idx}"
        ));
        params_vec.push(Box::new(end_hour));
        param_idx += 1;
    }

    sql.push_str(&format!(
        " ORDER BY COALESCE({history_ts}, datetime('now')) DESC, updated_at DESC"
    ));

    let lim = limit.unwrap_or(100);
    sql.push_str(&format!(" LIMIT ?{param_idx}"));
    params_vec.push(Box::new(lim));

    let refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|b| b.as_ref()).collect();
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            warn!("Failed to prepare history query: {e}");
            return vec![];
        }
    };

    let rows = stmt
        .query_map(refs.as_slice(), |row| {
            Ok(HistoricalSession {
                id: row.get(0)?,
                provider: row.get(1)?,
                session_name: row.get(2)?,
                project: row.get(3)?,
                model: row.get(4)?,
                model_id: row.get(5)?,
                context_window: row.get(6)?,
                branch: row.get(7)?,
                effort: row.get(8)?,
                started_at: row.get(9)?,
                ended_at: row.get(10)?,
                duration_secs: row.get(11)?,
                total_cost: row.get(12)?,
                input_tokens: row.get(13)?,
                output_tokens: row.get(14)?,
                cache_write_tokens: row.get(15)?,
                cache_read_tokens: row.get(16)?,
                total_tokens: row.get(17)?,
                input_cost: row.get(18)?,
                output_cost: row.get(19)?,
                cache_write_cost: row.get(20)?,
                cache_read_cost: row.get(21)?,
                has_thinking: row.get::<_, i32>(22)? != 0,
                subagent_count: row.get(23)?,
                is_active: row.get::<_, i32>(24)? != 0,
                used_tokens: row.get(25)?,
                window_tokens: row.get(26)?,
            })
        })
        .ok();

    rows.map(|r| r.filter_map(|x| x.ok()).collect())
        .unwrap_or_default()
}

#[derive(Debug, Serialize, Clone)]
pub struct DailyStat {
    pub date: String,
    pub project: String,
    pub model: String,
    pub session_count: i64,
    pub total_cost: f64,
    pub total_tokens: i64,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_write_tokens: i64,
    pub cache_read_tokens: i64,
}

#[derive(Debug, Serialize, Clone)]
pub struct ProjectStat {
    pub project: String,
    pub session_count: i64,
    pub total_cost: f64,
    pub total_tokens: i64,
    pub avg_session_cost: f64,
    pub avg_duration_secs: f64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub top_model: String,
}

#[derive(Debug, Serialize, Clone)]
pub struct HourlyActivity {
    pub hour: i64,
    pub session_count: i64,
    pub total_cost: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct CostForecast {
    pub spent_this_month: f64,
    pub days_elapsed: i64,
    pub days_in_month: i64,
    pub projected_monthly: f64,
    pub daily_average: f64,
}

#[derive(Debug, Serialize, Clone)]
pub struct BudgetStatus {
    pub monthly_budget: f64,
    pub alert_threshold_pct: f64,
    pub spent_this_month: f64,
    pub pct_used: f64,
    pub projected_monthly: f64,
    pub over_budget: bool,
}

fn bounded_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

fn nonnegative_finite(value: f64) -> f64 {
    if value.is_finite() && value >= 0.0 {
        value
    } else {
        0.0
    }
}

fn session_cost_provenance(
    session: &super::commands::SessionInfo,
) -> (&'static str, &'static str, Option<f64>) {
    let cost = nonnegative_finite(session.cost);
    if session.provider.eq_ignore_ascii_case("codex") {
        if cost > 0.0 {
            ("partial", "session-subtotal", Some(cost))
        } else {
            ("unavailable", "unknown", None)
        }
    } else {
        ("exact", "session-calculated", Some(cost))
    }
}

fn upsert_session_into(
    conn: &Connection,
    s: &super::commands::SessionInfo,
    updated_at: &str,
) -> rusqlite::Result<()> {
    let storage_id = storage_session_id(&s.provider, &s.session_id);
    let speed = s.speed.trim().to_ascii_lowercase();
    let speed_known = matches!(speed.as_str(), "standard" | "fast");
    let persisted_speed = if speed_known {
        speed.as_str()
    } else {
        "unknown"
    };
    let speed_source = if speed_known { "session" } else { "unknown" };
    let (cost_status, cost_source, known_cost) = session_cost_provenance(s);
    let window_tokens = session_window_tokens(s);
    let context_source = if window_tokens > 0 {
        "session"
    } else {
        "unknown"
    };
    conn.execute(
        "INSERT INTO sessions (id, provider, session_name, project, model, model_id, context_window, branch, effort,
            speed, speed_source, speed_known,
            started_at, duration_secs, total_cost, cost_status, cost_source, known_cost, cached_input_savings,
            input_tokens, output_tokens,
            cache_write_tokens, cache_read_tokens, total_tokens,
            input_cost, output_cost, cache_write_cost, cache_read_cost,
            has_thinking, subagent_count, is_active, updated_at, used_tokens, window_tokens,
            context_source, context_raw_source, raw_window_tokens, effective_context_percent)
        VALUES (
            ?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,NULL,
            ?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,1,?30,?31,?32,?33,'unknown',NULL,NULL
        )
        ON CONFLICT(id) DO UPDATE SET
            provider=?2,
            session_name=COALESCE(?3, session_name),
            project=?4, model=?5, model_id=?6, context_window=?7, branch=?8, effort=?9,
            speed=?10, speed_source=?11, speed_known=?12,
            started_at=CASE
                WHEN sessions.started_at IS NULL OR instr(sessions.started_at, 'T') = 0 THEN ?13
                ELSE sessions.started_at
            END,
            ended_at=NULL,
            duration_secs=?14, total_cost=?15,
            cost_status=?16, cost_source=?17, known_cost=?18, cached_input_savings=NULL,
            input_tokens=?19, output_tokens=?20,
            cache_write_tokens=?21, cache_read_tokens=?22, total_tokens=?23,
            input_cost=?24, output_cost=?25, cache_write_cost=?26, cache_read_cost=?27,
            has_thinking=?28, subagent_count=?29, is_active=1, updated_at=?30,
            used_tokens=?31, window_tokens=?32,
            context_source=?33, context_raw_source='unknown', raw_window_tokens=NULL,
            effective_context_percent=NULL",
        params![
            storage_id,
            s.provider,
            s.session_name,
            s.project,
            s.model,
            s.model_id,
            s.context_window,
            s.branch,
            s.effort,
            persisted_speed,
            speed_source,
            speed_known as i32,
            s.started_at,
            bounded_i64(s.duration_secs),
            nonnegative_finite(s.cost),
            cost_status,
            cost_source,
            known_cost,
            bounded_i64(s.input_tokens),
            bounded_i64(s.output_tokens),
            bounded_i64(s.cache_write_tokens),
            bounded_i64(s.cache_read_tokens),
            bounded_i64(s.tokens),
            nonnegative_finite(s.input_cost),
            nonnegative_finite(s.output_cost),
            nonnegative_finite(s.cache_write_cost),
            nonnegative_finite(s.cache_read_cost),
            s.has_thinking as i32,
            s.subagent_count.min(i32::MAX as usize) as i32,
            updated_at,
            session_used_tokens(s),
            window_tokens,
            context_source,
        ],
    )?;
    conn.execute(
        "UPDATE sessions
         SET created_at = COALESCE(created_at, started_at, updated_at),
             started_at = COALESCE(started_at, created_at, updated_at)
         WHERE id = ?1",
        params![storage_id],
    )?;
    Ok(())
}

pub fn upsert_session(s: &super::commands::SessionInfo) {
    let Ok(conn) = db().lock() else { return };
    if let Err(error) = upsert_session_into(&conn, s, &Utc::now().to_rfc3339()) {
        warn!("Failed to persist analytics session: {error}");
    }
}

pub fn mark_inactive(provider: &str, active_ids: &[String]) {
    let Ok(conn) = db().lock() else { return };
    let storage_ids: Vec<String> = active_ids
        .iter()
        .map(|id| storage_session_id(provider, id))
        .collect();
    if active_ids.is_empty() {
        let _ = conn.execute(
            "UPDATE sessions SET is_active = 0, ended_at = ?1 WHERE provider = ?2 AND is_active = 1",
            params![Utc::now().to_rfc3339(), provider],
        );
        return;
    }
    let placeholders: Vec<String> = storage_ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 3))
        .collect();
    let sql = format!(
        "UPDATE sessions SET is_active = 0, ended_at = ?1 WHERE provider = ?2 AND is_active = 1 AND id NOT IN ({})",
        placeholders.join(",")
    );
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return,
    };
    let now = Utc::now().to_rfc3339();
    let mut p: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(now), Box::new(provider.to_string())];
    for id in &storage_ids {
        p.push(Box::new(id.clone()));
    }
    let refs: Vec<&dyn rusqlite::types::ToSql> = p.iter().map(|b| b.as_ref()).collect();
    let _ = stmt.execute(refs.as_slice());
}

pub fn update_daily_stats(_session: &super::commands::SessionInfo) {}

pub fn get_session_history(
    days: Option<i64>,
    project: Option<&str>,
    limit: Option<i64>,
) -> Vec<HistoricalSession> {
    let Ok(conn) = db().lock() else {
        return vec![];
    };
    query_sessions(
        &conn, days, None, None, project, None, None, None, None, None, limit,
    )
}

pub fn get_session_history_filtered(
    from_iso: Option<&str>,
    to_iso: Option<&str>,
    project: Option<&str>,
    model: Option<&str>,
    min_cost: Option<f64>,
    max_cost: Option<f64>,
    limit: Option<i64>,
) -> Vec<HistoricalSession> {
    let Ok(conn) = db().lock() else {
        return vec![];
    };
    query_sessions(
        &conn, None, from_iso, to_iso, project, model, min_cost, max_cost, None, None, limit,
    )
}

pub fn get_sessions_by_hour_range(
    start_hour: i64,
    end_hour: i64,
    days: Option<i64>,
) -> Vec<HistoricalSession> {
    let Ok(conn) = db().lock() else {
        return vec![];
    };
    query_sessions(
        &conn,
        days,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(start_hour.clamp(0, 23)),
        Some(end_hour.clamp(0, 23)),
        Some(500),
    )
}

pub fn search_sessions(query: &str, limit: Option<i64>) -> Vec<HistoricalSession> {
    let Ok(conn) = db().lock() else {
        return vec![];
    };

    let lim = limit.unwrap_or(50);
    let provider = active_provider_slug().to_string();
    let sql = "SELECT s.id, s.provider, s.session_name, s.project, s.model, s.model_id, s.context_window, s.branch, s.effort,
            s.started_at, s.ended_at, s.duration_secs, s.total_cost,
            s.input_tokens, s.output_tokens, s.cache_write_tokens, s.cache_read_tokens, s.total_tokens,
            s.input_cost, s.output_cost, s.cache_write_cost, s.cache_read_cost,
            s.has_thinking, s.subagent_count, s.is_active, s.used_tokens, s.window_tokens
        FROM sessions_fts fts
        JOIN sessions s ON s.rowid = fts.rowid
        WHERE s.provider = ?1 AND sessions_fts MATCH ?2
        ORDER BY bm25(sessions_fts)
        LIMIT ?3";

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => {
            warn!("FTS search failed: {e}");
            return vec![];
        }
    };

    let rows = stmt
        .query_map(params![provider, query, lim], |row| {
            Ok(HistoricalSession {
                id: row.get(0)?,
                provider: row.get(1)?,
                session_name: row.get(2)?,
                project: row.get(3)?,
                model: row.get(4)?,
                model_id: row.get(5)?,
                context_window: row.get(6)?,
                branch: row.get(7)?,
                effort: row.get(8)?,
                started_at: row.get(9)?,
                ended_at: row.get(10)?,
                duration_secs: row.get(11)?,
                total_cost: row.get(12)?,
                input_tokens: row.get(13)?,
                output_tokens: row.get(14)?,
                cache_write_tokens: row.get(15)?,
                cache_read_tokens: row.get(16)?,
                total_tokens: row.get(17)?,
                input_cost: row.get(18)?,
                output_cost: row.get(19)?,
                cache_write_cost: row.get(20)?,
                cache_read_cost: row.get(21)?,
                has_thinking: row.get::<_, i32>(22)? != 0,
                subagent_count: row.get(23)?,
                is_active: row.get::<_, i32>(24)? != 0,
                used_tokens: row.get(25)?,
                window_tokens: row.get(26)?,
            })
        })
        .ok();

    rows.map(|r| r.filter_map(|x| x.ok()).collect())
        .unwrap_or_default()
}

fn query_daily_stats(conn: &Connection, provider: &str, cutoff: &str) -> Vec<DailyStat> {
    let history_timestamp = history_timestamp_expr();
    let sql = format!(
        "SELECT date({history_timestamp}) AS session_date,
                project,
                model,
                COUNT(*),
                COALESCE(SUM(known_cost), 0),
                COALESCE(SUM(total_tokens), 0),
                COALESCE(SUM(input_tokens), 0),
                COALESCE(SUM(output_tokens), 0),
                COALESCE(SUM(cache_write_tokens), 0),
                COALESCE(SUM(cache_read_tokens), 0)
         FROM sessions
         WHERE provider = ?1
           AND date({history_timestamp}) >= ?2
         GROUP BY session_date, project, model
         ORDER BY session_date DESC, project, model"
    );
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };

    let rows = stmt
        .query_map(params![provider, cutoff], |row| {
            Ok(DailyStat {
                date: row.get(0)?,
                project: row.get(1)?,
                model: row.get(2)?,
                session_count: row.get(3)?,
                total_cost: row.get(4)?,
                total_tokens: row.get(5)?,
                input_tokens: row.get(6)?,
                output_tokens: row.get(7)?,
                cache_write_tokens: row.get(8)?,
                cache_read_tokens: row.get(9)?,
            })
        })
        .ok();

    rows.map(|r| r.filter_map(|x| x.ok()).collect())
        .unwrap_or_default()
}

pub fn get_daily_stats(days: Option<i64>) -> Vec<DailyStat> {
    let Ok(conn) = db().lock() else {
        return vec![];
    };
    let provider = active_provider_slug().to_string();
    let cutoff = (Utc::now() - chrono::Duration::days(days.unwrap_or(30)))
        .format("%Y-%m-%d")
        .to_string();
    query_daily_stats(&conn, &provider, &cutoff)
}

pub fn get_analytics_summary() -> AnalyticsSummary {
    let Ok(conn) = db().lock() else {
        return AnalyticsSummary::default();
    };
    let provider = active_provider_slug().to_string();

    let total_sessions: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE provider = ?1",
            params![provider.clone()],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let total_cost: f64 = conn
        .query_row(
            "SELECT COALESCE(SUM(total_cost), 0) FROM sessions WHERE provider = ?1",
            params![provider.clone()],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let total_tokens: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(total_tokens), 0) FROM sessions WHERE provider = ?1",
            params![provider.clone()],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let total_cache_read: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(cache_read_tokens), 0) FROM sessions WHERE provider = ?1",
            params![provider.clone()],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let total_cache_write: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(cache_write_tokens), 0) FROM sessions WHERE provider = ?1",
            params![provider.clone()],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let avg_duration_secs: f64 = conn
        .query_row(
            "SELECT COALESCE(AVG(duration_secs), 0) FROM sessions WHERE provider = ?1 AND duration_secs > 0",
            params![provider.clone()],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let avg_tokens_per_session: f64 = if total_sessions > 0 {
        total_tokens as f64 / total_sessions as f64
    } else {
        0.0
    };

    let avg_cost_per_session: f64 = if total_sessions > 0 {
        total_cost / total_sessions as f64
    } else {
        0.0
    };

    let top_project: String = conn
        .query_row(
            "SELECT project FROM sessions WHERE provider = ?1 GROUP BY project ORDER BY SUM(total_cost) DESC LIMIT 1",
            params![provider.clone()],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "—".to_string());

    let top_model: String = conn
        .query_row(
            "SELECT model FROM sessions WHERE provider = ?1 GROUP BY model ORDER BY COUNT(*) DESC LIMIT 1",
            params![provider.clone()],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "—".to_string());

    let days_tracked: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT date({})) FROM sessions WHERE provider = ?1",
                history_timestamp_expr()
            ),
            params![provider],
            |r| r.get(0),
        )
        .unwrap_or(0);

    AnalyticsSummary {
        total_sessions,
        total_cost,
        total_tokens,
        total_cache_read,
        total_cache_write,
        avg_duration_secs,
        avg_tokens_per_session,
        avg_cost_per_session,
        top_project,
        top_model,
        days_tracked,
    }
}

#[derive(Debug, Serialize, Clone, Default)]
pub struct AnalyticsSummary {
    pub total_sessions: i64,
    pub total_cost: f64,
    pub total_tokens: i64,
    pub total_cache_read: i64,
    pub total_cache_write: i64,
    pub avg_duration_secs: f64,
    pub avg_tokens_per_session: f64,
    pub avg_cost_per_session: f64,
    pub top_project: String,
    pub top_model: String,
    pub days_tracked: i64,
}

pub fn get_project_stats(days: Option<i64>) -> Vec<ProjectStat> {
    let Ok(conn) = db().lock() else { return vec![] };
    let provider = active_provider_slug().to_string();
    let d = days.unwrap_or(30);
    let cutoff = (Utc::now() - chrono::Duration::days(d)).to_rfc3339();
    let sql = format!(
        "SELECT project,
            COUNT(*) as cnt,
            COALESCE(SUM(total_cost), 0),
            COALESCE(SUM(total_tokens), 0),
            COALESCE(AVG(total_cost), 0),
            COALESCE(AVG(duration_secs), 0),
            COALESCE(SUM(cache_read_tokens), 0),
            COALESCE(SUM(cache_write_tokens), 0),
            (SELECT model FROM sessions s2 WHERE s2.provider = sessions.provider AND s2.project = sessions.project
             GROUP BY model ORDER BY COUNT(*) DESC LIMIT 1)
        FROM sessions
        WHERE provider = ?1 AND COALESCE({}, datetime('now')) >= ?2
        GROUP BY project ORDER BY SUM(total_cost) DESC",
        history_timestamp_expr()
    );
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map(params![provider, cutoff], |row| {
        Ok(ProjectStat {
            project: row.get(0)?,
            session_count: row.get(1)?,
            total_cost: row.get(2)?,
            total_tokens: row.get(3)?,
            avg_session_cost: row.get(4)?,
            avg_duration_secs: row.get(5)?,
            cache_read_tokens: row.get(6)?,
            cache_write_tokens: row.get(7)?,
            top_model: row.get::<_, String>(8).unwrap_or_default(),
        })
    })
    .ok()
    .map(|r| r.filter_map(|x| x.ok()).collect())
    .unwrap_or_default()
}

pub fn get_hourly_activity(days: Option<i64>) -> Vec<HourlyActivity> {
    let Ok(conn) = db().lock() else { return vec![] };
    let provider = active_provider_slug().to_string();
    let d = days.unwrap_or(30);
    let cutoff = (Utc::now() - chrono::Duration::days(d)).to_rfc3339();
    let sql = format!(
        "SELECT CAST(substr(COALESCE({}, ''), 12, 2) AS INTEGER) as hour,
            COUNT(*), COALESCE(SUM(total_cost), 0)
        FROM sessions
        WHERE provider = ?1 AND COALESCE({}, datetime('now')) >= ?2
        GROUP BY hour ORDER BY hour",
        history_timestamp_expr(),
        history_timestamp_expr()
    );
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map(params![provider, cutoff], |row| {
        Ok(HourlyActivity {
            hour: row.get(0)?,
            session_count: row.get(1)?,
            total_cost: row.get(2)?,
        })
    })
    .ok()
    .map(|r| r.filter_map(|x| x.ok()).collect())
    .unwrap_or_default()
}

pub fn get_top_sessions(limit: Option<i64>, days: Option<i64>) -> Vec<HistoricalSession> {
    get_session_history(days, None, limit)
        .into_iter()
        .collect::<Vec<_>>()
        .into_iter()
        .take(limit.unwrap_or(25) as usize)
        .collect()
}

pub fn get_cost_forecast() -> CostForecast {
    let Ok(conn) = db().lock() else {
        return CostForecast {
            spent_this_month: 0.0,
            days_elapsed: 0,
            days_in_month: 30,
            projected_monthly: 0.0,
            daily_average: 0.0,
        };
    };
    let provider = active_provider_slug().to_string();
    let now = Utc::now();
    let month_start = now.format("%Y-%m-01T00:00:00+00:00").to_string();
    let days_elapsed = now.day() as i64;
    let days_in_month = {
        let (y, m) = (now.year(), now.month());
        if m == 12 {
            chrono::NaiveDate::from_ymd_opt(y + 1, 1, 1)
        } else {
            chrono::NaiveDate::from_ymd_opt(y, m + 1, 1)
        }
        .and_then(|d| d.pred_opt())
        .map(|d| d.day() as i64)
        .unwrap_or(30)
    };
    let spent: f64 = conn
        .query_row(
            &format!(
                "SELECT COALESCE(SUM(total_cost), 0) FROM sessions WHERE provider = ?1 AND COALESCE({}, datetime('now')) >= ?2",
                history_timestamp_expr()
            ),
            params![provider, month_start],
            |r| r.get(0),
        )
        .unwrap_or(0.0);
    let daily_avg = if days_elapsed > 0 {
        spent / days_elapsed as f64
    } else {
        0.0
    };
    CostForecast {
        spent_this_month: spent,
        days_elapsed,
        days_in_month,
        projected_monthly: daily_avg * days_in_month as f64,
        daily_average: daily_avg,
    }
}

pub fn get_budget_status() -> BudgetStatus {
    let forecast = get_cost_forecast();
    let Ok(conn) = db().lock() else {
        return BudgetStatus {
            monthly_budget: 0.0,
            alert_threshold_pct: 80.0,
            spent_this_month: forecast.spent_this_month,
            pct_used: 0.0,
            projected_monthly: forecast.projected_monthly,
            over_budget: false,
        };
    };
    let (budget, threshold): (f64, f64) = conn
        .query_row(
            "SELECT monthly_budget, alert_threshold_pct FROM budget_config WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0.0, 80.0));
    let pct = if budget > 0.0 {
        (forecast.spent_this_month / budget) * 100.0
    } else {
        0.0
    };
    BudgetStatus {
        monthly_budget: budget,
        alert_threshold_pct: threshold,
        spent_this_month: forecast.spent_this_month,
        pct_used: pct,
        projected_monthly: forecast.projected_monthly,
        over_budget: budget > 0.0 && forecast.projected_monthly > budget,
    }
}

pub fn set_budget(monthly_budget: f64, alert_threshold_pct: Option<f64>) {
    let Ok(conn) = db().lock() else { return };
    let threshold = alert_threshold_pct.unwrap_or(80.0);
    let now = Utc::now().to_rfc3339();
    let _ = conn.execute(
        "INSERT INTO budget_config (id, monthly_budget, alert_threshold_pct, updated_at)
         VALUES (1, ?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET monthly_budget=?1, alert_threshold_pct=?2, updated_at=?3",
        params![monthly_budget, threshold, now],
    );
}

pub fn get_model_distribution(days: Option<i64>) -> Vec<(String, i64, f64)> {
    let Ok(conn) = db().lock() else { return vec![] };
    let provider = active_provider_slug().to_string();
    let d = days.unwrap_or(30);
    let cutoff = (Utc::now() - chrono::Duration::days(d)).to_rfc3339();
    let sql = format!(
        "SELECT model, COUNT(*), COALESCE(SUM(total_cost), 0)
        FROM sessions
        WHERE provider = ?1 AND COALESCE({}, datetime('now')) >= ?2
        GROUP BY model ORDER BY COUNT(*) DESC",
        history_timestamp_expr()
    );
    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return vec![],
    };
    stmt.query_map(params![provider, cutoff], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, f64>(2)?,
        ))
    })
    .ok()
    .map(|r| r.filter_map(|x| x.ok()).collect())
    .unwrap_or_default()
}

pub fn export_all_data() -> serde_json::Value {
    let sessions = get_session_history(None, None, Some(10000));
    let daily = get_daily_stats(Some(365));
    let summary = get_analytics_summary();
    serde_json::json!({
        "exported_at": Utc::now().to_rfc3339(),
        "summary": summary,
        "sessions": sessions,
        "daily_stats": daily,
    })
}

pub fn clear_history() -> i64 {
    let Ok(conn) = db().lock() else { return 0 };
    let provider = active_provider_slug().to_string();
    let deleted: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sessions WHERE provider = ?1",
            params![provider.clone()],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let _ = conn.execute(
        "DELETE FROM sessions_fts
         WHERE rowid IN (SELECT rowid FROM sessions WHERE provider = ?1)",
        params![provider.clone()],
    );
    let _ = conn.execute(
        "DELETE FROM sessions WHERE provider = ?1",
        params![provider],
    );
    deleted
}

pub fn get_db_size_bytes() -> u64 {
    std::fs::metadata(db_path()).map(|m| m.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_database_path(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "pulse-{test_name}-{}-{nonce}.db",
            std::process::id()
        ))
    }

    fn remove_database_files(path: &Path) {
        for candidate in [
            path.to_path_buf(),
            migration_backup_path(path),
            PathBuf::from(format!("{}-wal", path.display())),
            PathBuf::from(format!("{}-shm", path.display())),
        ] {
            if candidate.exists() {
                std::fs::remove_file(candidate).expect("remove test database file");
            }
        }
    }

    fn create_v3_schema(conn: &Connection) {
        conn.execute_batch(
            "
            PRAGMA user_version = 3;

            CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                provider TEXT NOT NULL DEFAULT 'claude',
                session_name TEXT DEFAULT NULL,
                project TEXT NOT NULL,
                model TEXT NOT NULL,
                model_id TEXT DEFAULT '',
                context_window TEXT DEFAULT '200K',
                branch TEXT,
                effort TEXT DEFAULT 'Medium',
                started_at TEXT,
                created_at TEXT,
                ended_at TEXT,
                duration_secs INTEGER DEFAULT 0,
                total_cost REAL DEFAULT 0,
                input_tokens INTEGER DEFAULT 0,
                output_tokens INTEGER DEFAULT 0,
                cache_write_tokens INTEGER DEFAULT 0,
                cache_read_tokens INTEGER DEFAULT 0,
                total_tokens INTEGER DEFAULT 0,
                input_cost REAL DEFAULT 0,
                output_cost REAL DEFAULT 0,
                cache_write_cost REAL DEFAULT 0,
                cache_read_cost REAL DEFAULT 0,
                has_thinking INTEGER DEFAULT 0,
                subagent_count INTEGER DEFAULT 0,
                is_active INTEGER DEFAULT 1,
                updated_at TEXT NOT NULL,
                used_tokens INTEGER DEFAULT 0,
                window_tokens INTEGER DEFAULT 0
            );

            CREATE TABLE daily_stats (
                date TEXT NOT NULL,
                provider TEXT NOT NULL DEFAULT 'claude',
                project TEXT NOT NULL,
                model TEXT NOT NULL,
                session_count INTEGER DEFAULT 0,
                total_cost REAL DEFAULT 0,
                total_tokens INTEGER DEFAULT 0,
                input_tokens INTEGER DEFAULT 0,
                output_tokens INTEGER DEFAULT 0,
                cache_write_tokens INTEGER DEFAULT 0,
                cache_read_tokens INTEGER DEFAULT 0,
                PRIMARY KEY (date, provider, project, model)
            );

            CREATE VIRTUAL TABLE sessions_fts USING fts5(
                project, model, branch,
                content='sessions',
                content_rowid='rowid',
                tokenize='porter unicode61'
            );

            CREATE TRIGGER sessions_ai AFTER INSERT ON sessions BEGIN
                INSERT INTO sessions_fts(rowid, project, model, branch)
                VALUES (new.rowid, new.project, new.model, COALESCE(new.branch, ''));
            END;

            CREATE TRIGGER sessions_au AFTER UPDATE ON sessions BEGIN
                DELETE FROM sessions_fts WHERE rowid = old.rowid;
                INSERT INTO sessions_fts(rowid, project, model, branch)
                VALUES (new.rowid, new.project, new.model, COALESCE(new.branch, ''));
            END;

            CREATE TABLE budget_config (
                id INTEGER PRIMARY KEY CHECK (id = 1),
                monthly_budget REAL DEFAULT 0,
                alert_threshold_pct REAL DEFAULT 80,
                updated_at TEXT NOT NULL DEFAULT '1970-01-01'
            );
            ",
        )
        .expect("create v3 schema");
    }

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        init_schema(&conn).expect("initialize schema");
        conn
    }

    fn sample_session_info(
        context_window: &str,
        model_id: &str,
        input_tokens: u64,
        tokens: u64,
    ) -> super::super::commands::SessionInfo {
        super::super::commands::SessionInfo {
            provider: "claude".into(),
            app_name: None,
            session_id: "session".into(),
            session_name: None,
            project: "repo".into(),
            model: "Claude Opus".into(),
            model_id: model_id.into(),
            context_window: context_window.into(),
            cost: 0.0,
            tokens,
            input_tokens,
            output_tokens: 0,
            cache_write_tokens: 0,
            cache_read_tokens: 0,
            context_used_tokens: input_tokens,
            context_window_tokens: 0,
            branch: None,
            activity: "Idle".into(),
            activity_target: None,
            effort: "Medium".into(),
            effort_explicit: false,
            is_idle: false,
            started_at: None,
            duration_secs: 0,
            has_thinking: false,
            workflow_label: None,
            subagent_count: 0,
            subagents: Vec::new(),
            tokens_per_sec: 0.0,
            input_cost: 0.0,
            output_cost: 0.0,
            cache_write_cost: 0.0,
            cache_read_cost: 0.0,
            speed: "standard".into(),
            fast: false,
            service_tier: None,
            intro_pricing: None,
            has_inflated_tokenizer: false,
        }
    }

    #[test]
    fn session_window_tokens_reports_1m_for_1m_context() {
        let one_m = sample_session_info("1M", "claude-opus-4-8", 10, 10);
        assert_eq!(session_window_tokens(&one_m), 1_000_000);

        let ga_1m = sample_session_info("200K", "claude-opus-4-8", 10, 10);
        assert_eq!(session_window_tokens(&ga_1m), 1_000_000);

        let two_hundred_k = sample_session_info("200K", "claude-sonnet-4-5", 10, 10);
        assert_eq!(session_window_tokens(&two_hundred_k), 200_000);
    }

    #[test]
    fn session_used_tokens_uses_context_snapshot_not_lifetime_total() {
        let mut info = sample_session_info("1M", "claude-opus-4-8", 83_700, 8_580_000);
        info.context_used_tokens = 83_700;
        info.context_window_tokens = 1_000_000;
        assert_eq!(session_used_tokens(&info), 83_700);

        info.context_used_tokens = 1_250_000;
        assert_eq!(session_used_tokens(&info), 1_000_000);
    }

    #[test]
    fn history_query_uses_created_at_when_started_at_missing() {
        let conn = test_conn();
        let created_at = Utc::now().to_rfc3339();
        let provider = active_provider_slug().to_string();
        conn.execute(
            "INSERT INTO sessions (id, provider, project, model, created_at, updated_at, total_cost)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                "session-a",
                provider,
                "repo-a",
                "Claude Opus 4.7",
                created_at,
                created_at,
                12.5
            ],
        )
        .expect("insert session");

        let rows = query_sessions(
            &conn,
            Some(7),
            None,
            None,
            Some("repo-a"),
            None,
            None,
            None,
            None,
            None,
            Some(10),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "session-a");
    }

    #[test]
    fn hour_range_filter_uses_fallback_timestamp() {
        let conn = test_conn();
        let provider = active_provider_slug().to_string();
        conn.execute(
            "INSERT INTO sessions (id, provider, project, model, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "session-early",
                provider.clone(),
                "repo-a",
                "Claude Opus 4.7",
                "2026-04-18T03:15:00+00:00",
                "2026-04-18T03:15:00+00:00"
            ],
        )
        .expect("insert early");
        conn.execute(
            "INSERT INTO sessions (id, provider, project, model, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                "session-late",
                provider,
                "repo-a",
                "Claude Opus 4.7",
                "2026-04-18T15:45:00+00:00",
                "2026-04-18T15:45:00+00:00"
            ],
        )
        .expect("insert late");

        let rows = query_sessions(
            &conn,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(0),
            Some(6),
            Some(10),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "session-early");
    }

    #[test]
    fn init_schema_is_idempotent_and_preserves_rows() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        init_schema(&conn).expect("initialize schema");
        let provider = active_provider_slug().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO sessions (id, provider, project, model, created_at, updated_at, used_tokens, window_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                "session-keep",
                provider,
                "repo-a",
                "Claude Opus 4.7",
                now,
                now,
                123_456_i64,
                1_000_000_i64
            ],
        )
        .expect("insert session");

        init_schema(&conn).expect("repeat schema initialization");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))
            .expect("count rows");
        assert_eq!(count, 1);

        let rows = query_sessions(
            &conn,
            Some(7),
            None,
            None,
            Some("repo-a"),
            None,
            None,
            None,
            None,
            None,
            Some(10),
        );
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].used_tokens, 123_456);
        assert_eq!(rows[0].window_tokens, 1_000_000);
    }

    #[test]
    fn context_tokens_round_trip_through_query() {
        let conn = test_conn();
        let provider = active_provider_slug().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO sessions (id, provider, project, model, created_at, updated_at, used_tokens, window_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                "session-ctx",
                provider,
                "repo-ctx",
                "Claude Sonnet 4.6",
                now,
                now,
                90_000_i64,
                200_000_i64
            ],
        )
        .expect("insert session");

        let rows = query_sessions(
            &conn,
            Some(7),
            None,
            None,
            Some("repo-ctx"),
            None,
            None,
            None,
            None,
            None,
            Some(10),
        );

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].used_tokens, 90_000);
        assert_eq!(rows[0].window_tokens, 200_000);
    }

    #[test]
    fn window_backfill_maps_context_label_when_zero() {
        let conn = test_conn();
        let provider = active_provider_slug().to_string();
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO sessions (id, provider, project, model, context_window, created_at, updated_at, window_tokens)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![
                "session-1m",
                provider,
                "repo-1m",
                "Claude Opus 4.8",
                "1M",
                now,
                now
            ],
        )
        .expect("insert session");

        init_schema(&conn).expect("repeat schema initialization");

        let window: i64 = conn
            .query_row(
                "SELECT window_tokens FROM sessions WHERE id = 'session-1m'",
                [],
                |r| r.get(0),
            )
            .expect("read window");
        assert_eq!(window, 1_000_000);
    }

    #[test]
    fn migration_v3_to_v4_preserves_rollback_table_and_marks_legacy_codex_unknown() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        create_v3_schema(&conn);

        for (id, label) in [
            ("context-200k", "200K"),
            ("context-400k", "400K"),
            ("context-372k", "372K"),
            ("context-353-4k", "353.4K"),
        ] {
            conn.execute(
                "INSERT INTO sessions (
                    id, provider, project, model, context_window, total_cost, updated_at
                 ) VALUES (?1, 'codex', 'repo', 'GPT-5.6 Sol', ?2, 19.75, '2026-07-10T12:00:00+00:00')",
                params![id, label],
            )
            .expect("insert v3 session");
        }
        conn.execute(
            "INSERT INTO daily_stats (
                date, provider, project, model, session_count, total_cost, total_tokens
             ) VALUES ('2026-07-10', 'codex', 'rollback-sentinel', 'GPT-5.6 Sol', 4, 79.0, 4000)",
            [],
        )
        .expect("insert rollback sentinel");

        init_schema(&conn).expect("migrate schema");

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("read user_version");
        assert_eq!(version, 4);

        let columns = {
            let mut stmt = conn
                .prepare("SELECT name FROM pragma_table_info('sessions')")
                .expect("prepare schema query");
            stmt.query_map([], |row| row.get::<_, String>(0))
                .expect("query schema")
                .collect::<Result<Vec<_>, _>>()
                .expect("collect columns")
        };
        for expected in [
            "speed",
            "speed_source",
            "speed_known",
            "cost_status",
            "cost_source",
            "known_cost",
            "cached_input_savings",
            "context_source",
            "context_raw_source",
            "raw_window_tokens",
            "effective_context_percent",
        ] {
            assert!(
                columns.iter().any(|column| column == expected),
                "{expected}"
            );
        }

        let context_windows = {
            let mut stmt = conn
                .prepare("SELECT id, window_tokens FROM sessions ORDER BY id")
                .expect("prepare context query");
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .expect("query contexts")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect contexts")
        };
        assert_eq!(
            context_windows,
            vec![
                ("context-200k".into(), 200_000),
                ("context-353-4k".into(), 353_400),
                ("context-372k".into(), 372_000),
                ("context-400k".into(), 400_000),
            ]
        );

        let legacy_state: (i64, String, String, Option<f64>, String, String) = conn
            .query_row(
                "SELECT speed_known, speed_source, cost_status, known_cost,
                        context_source, context_raw_source
                 FROM sessions WHERE id = 'context-372k'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .expect("read legacy state");
        assert_eq!(
            legacy_state,
            (
                0,
                "legacy".into(),
                "unavailable".into(),
                None,
                "legacy".into(),
                "unknown".into(),
            )
        );

        let rollback_rows: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM daily_stats WHERE project = 'rollback-sentinel'",
                [],
                |row| row.get(0),
            )
            .expect("read rollback table");
        assert_eq!(rollback_rows, 1);
    }

    #[test]
    fn file_migration_creates_valid_v3_backup_once_before_schema_mutation() {
        let path = temporary_database_path("v3-backup");
        {
            let conn = Connection::open(&path).expect("open v3 database");
            create_v3_schema(&conn);
            conn.execute(
                "INSERT INTO sessions (
                    id, provider, project, model, context_window, updated_at
                 ) VALUES (
                    'backup-sentinel', 'codex', 'repo', 'GPT-5.6 Sol', '372K',
                    '2026-07-10T12:00:00+00:00'
                 )",
                [],
            )
            .expect("insert backup sentinel");
        }

        let migrated = open_database(&path).expect("migrate database");
        let backup_path = migration_backup_path(&path);
        assert!(backup_path.exists());

        let backup = Connection::open(&backup_path).expect("open migration backup");
        assert_eq!(schema_version(&backup).expect("backup schema version"), 3);
        let backup_rows: i64 = backup
            .query_row(
                "SELECT COUNT(*) FROM sessions WHERE id = 'backup-sentinel'",
                [],
                |row| row.get(0),
            )
            .expect("backup sentinel count");
        assert_eq!(backup_rows, 1);
        let backup_has_speed: bool = backup
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM pragma_table_info('sessions') WHERE name = 'speed'
                )",
                [],
                |row| row.get(0),
            )
            .expect("backup schema columns");
        assert!(!backup_has_speed);
        assert_eq!(schema_version(&migrated).expect("live schema version"), 4);

        drop(backup);
        drop(migrated);
        let first_backup = std::fs::read(&backup_path).expect("read first backup");
        drop(open_database(&path).expect("repeat database initialization"));
        let second_backup = std::fs::read(&backup_path).expect("read repeated backup");
        assert_eq!(second_backup, first_backup);

        remove_database_files(&path);
    }

    #[test]
    fn daily_stats_are_derived_from_idempotent_session_rows() {
        let conn = test_conn();
        conn.execute(
            "INSERT INTO daily_stats (
                date, provider, project, model, session_count, total_cost, total_tokens
             ) VALUES (
                '2026-07-10', 'codex', 'stale-table', 'GPT-5.6 Sol', 99, 999.0, 999999
             )",
            [],
        )
        .expect("insert stale daily aggregate");

        let upsert = "INSERT INTO sessions (
                id, provider, project, model, started_at, created_at, updated_at,
                total_cost, known_cost, total_tokens, input_tokens, output_tokens,
                cache_write_tokens, cache_read_tokens
             ) VALUES (
                ?1, 'codex', 'repo', 'GPT-5.6 Sol', ?2, ?2, ?2,
                ?3, ?3, ?4, ?5, ?6, ?7, ?8
             )
             ON CONFLICT(id) DO UPDATE SET
                total_cost = excluded.total_cost,
                known_cost = excluded.known_cost,
                total_tokens = excluded.total_tokens,
                input_tokens = excluded.input_tokens,
                output_tokens = excluded.output_tokens,
                cache_write_tokens = excluded.cache_write_tokens,
                cache_read_tokens = excluded.cache_read_tokens";
        conn.execute(
            upsert,
            params![
                "codex:one",
                "2026-07-10T10:00:00+00:00",
                1.0,
                1_000_i64,
                600_i64,
                100_i64,
                100_i64,
                200_i64
            ],
        )
        .expect("insert first session");
        conn.execute(
            upsert,
            params![
                "codex:one",
                "2026-07-10T10:00:00+00:00",
                1.5,
                1_500_i64,
                900_i64,
                150_i64,
                150_i64,
                300_i64
            ],
        )
        .expect("repeat first session upsert");
        conn.execute(
            upsert,
            params![
                "codex:two",
                "2026-07-10T11:00:00+00:00",
                2.5,
                2_500_i64,
                1_500_i64,
                250_i64,
                250_i64,
                500_i64
            ],
        )
        .expect("insert second session");

        let rows = query_daily_stats(&conn, "codex", "2026-07-01");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date, "2026-07-10");
        assert_eq!(rows[0].project, "repo");
        assert_eq!(rows[0].model, "GPT-5.6 Sol");
        assert_eq!(rows[0].session_count, 2);
        assert_eq!(rows[0].total_cost, 4.0);
        assert_eq!(rows[0].total_tokens, 4_000);
        assert_eq!(rows[0].input_tokens, 2_400);
        assert_eq!(rows[0].output_tokens, 400);
        assert_eq!(rows[0].cache_write_tokens, 400);
        assert_eq!(rows[0].cache_read_tokens, 800);

        let stale_rows: i64 = conn
            .query_row("SELECT COUNT(*) FROM daily_stats", [], |row| row.get(0))
            .expect("read rollback table");
        assert_eq!(stale_rows, 1);
    }

    #[test]
    fn session_upsert_persists_known_live_provenance_without_claiming_codex_exactness() {
        let conn = test_conn();
        let mut session = sample_session_info("353.4K", "gpt-5.6-sol", 1_000, 1_500);
        session.provider = "codex".into();
        session.model = "GPT-5.6 Sol".into();
        session.speed = "fast".into();
        session.fast = true;
        session.cost = 12.5;
        session.context_window_tokens = 353_400;

        upsert_session_into(&conn, &session, "2026-07-10T12:00:00+00:00")
            .expect("insert live session");
        session.cost = 13.0;
        upsert_session_into(&conn, &session, "2026-07-10T12:01:00+00:00")
            .expect("repeat live session upsert");

        type StoredProvenance = (
            i64,
            String,
            String,
            i64,
            String,
            String,
            Option<f64>,
            Option<f64>,
            String,
            String,
            Option<i64>,
            Option<i64>,
        );
        let stored: StoredProvenance = conn
            .query_row(
                "SELECT COUNT(*) OVER (), speed, speed_source, speed_known,
                        cost_status, cost_source, known_cost, cached_input_savings,
                        context_source, context_raw_source, raw_window_tokens,
                        effective_context_percent
                 FROM sessions WHERE id = 'codex:session'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                        row.get(7)?,
                        row.get(8)?,
                        row.get(9)?,
                        row.get(10)?,
                        row.get(11)?,
                    ))
                },
            )
            .expect("read persisted provenance");
        assert_eq!(
            stored,
            (
                1,
                "fast".into(),
                "session".into(),
                1,
                "partial".into(),
                "session-subtotal".into(),
                Some(13.0),
                None,
                "session".into(),
                "unknown".into(),
                None,
                None,
            )
        );

        session.session_id = "unpriced".into();
        session.cost = 0.0;
        upsert_session_into(&conn, &session, "2026-07-10T12:02:00+00:00")
            .expect("insert unpriced live session");
        let unpriced: (String, Option<f64>) = conn
            .query_row(
                "SELECT cost_status, known_cost FROM sessions WHERE id = 'codex:unpriced'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("read unpriced session");
        assert_eq!(unpriced, ("unavailable".into(), None));
    }
}

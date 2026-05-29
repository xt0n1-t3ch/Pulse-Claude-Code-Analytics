use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use chrono::{Datelike, Utc};
use rusqlite::{Connection, params};
use serde::Serialize;
use tracing::{debug, warn};

use cc_discord_presence::config;
use cc_discord_presence::cost;
use cc_discord_presence::provider::Provider;

static DB: OnceLock<Arc<Mutex<Connection>>> = OnceLock::new();

const WINDOW_TOKENS_1M: i64 = 1_000_000;
const WINDOW_TOKENS_DEFAULT: i64 = 200_000;
const CONTEXT_WINDOW_LABEL_1M: &str = "1M";

fn session_window_tokens(s: &super::commands::SessionInfo) -> i64 {
    let has_1m = s.context_window == CONTEXT_WINDOW_LABEL_1M || cost::is_ga_1m_context(&s.model_id);
    if has_1m {
        WINDOW_TOKENS_1M
    } else {
        WINDOW_TOKENS_DEFAULT
    }
}

fn session_used_tokens(s: &super::commands::SessionInfo) -> i64 {
    s.input_tokens.max(s.tokens) as i64
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

fn db() -> &'static Arc<Mutex<Connection>> {
    DB.get_or_init(|| {
        let path = db_path();
        let conn = Connection::open(&path).expect("failed to open pulse-analytics.db");
        init_schema(&conn);
        Arc::new(Mutex::new(conn))
    })
}

fn init_schema(conn: &Connection) {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;
        PRAGMA cache_size = -8000;

        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            provider TEXT NOT NULL DEFAULT 'claude',
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
            updated_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_sessions_project ON sessions(project);
        CREATE INDEX IF NOT EXISTS idx_sessions_started ON sessions(started_at);
        CREATE INDEX IF NOT EXISTS idx_sessions_active ON sessions(is_active);
        CREATE INDEX IF NOT EXISTS idx_sessions_model ON sessions(model);

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
    )
    .expect("failed to initialize pulse-analytics schema");
    debug!("Pulse analytics DB initialized at {}", db_path().display());

    let _ = conn.execute_batch("ALTER TABLE sessions ADD COLUMN provider TEXT DEFAULT 'claude';");
    let _ = conn.execute_batch("ALTER TABLE sessions ADD COLUMN session_name TEXT DEFAULT NULL;");
    let _ = conn.execute_batch("ALTER TABLE sessions ADD COLUMN created_at TEXT DEFAULT NULL;");
    let _ = conn.execute_batch("ALTER TABLE sessions ADD COLUMN used_tokens INTEGER DEFAULT 0;");
    let _ = conn.execute_batch("ALTER TABLE sessions ADD COLUMN window_tokens INTEGER DEFAULT 0;");
    let _ =
        conn.execute_batch("ALTER TABLE daily_stats ADD COLUMN provider TEXT DEFAULT 'claude';");
    let _ = conn.execute(
        "UPDATE sessions SET provider = 'claude' WHERE provider IS NULL OR trim(provider) = ''",
        [],
    );
    let _ = conn.execute(
        "UPDATE daily_stats SET provider = 'claude' WHERE provider IS NULL OR trim(provider) = ''",
        [],
    );
    let _ = conn.execute(
        "UPDATE sessions
         SET input_tokens = MAX(total_tokens - output_tokens, 0)
         WHERE provider = 'codex'
           AND total_tokens > 0
           AND input_tokens > MAX(total_tokens - output_tokens, 0)",
        [],
    );
    let _ = conn.execute(
        "UPDATE daily_stats
         SET input_tokens = MAX(total_tokens - output_tokens, 0)
         WHERE provider = 'codex'
           AND total_tokens > 0
           AND input_tokens > MAX(total_tokens - output_tokens, 0)",
        [],
    );
    let _ = conn.execute(
        "UPDATE sessions
         SET created_at = COALESCE(created_at, started_at, updated_at)
         WHERE created_at IS NULL",
        [],
    );
    let _ = conn.execute(
        "UPDATE sessions
         SET started_at = updated_at
         WHERE started_at IS NOT NULL
           AND instr(started_at, 'T') = 0
           AND updated_at IS NOT NULL",
        [],
    );
    let _ = conn.execute(
        "UPDATE sessions
         SET started_at = COALESCE(started_at, created_at, updated_at)
         WHERE started_at IS NULL",
        [],
    );
    let _ = conn.execute(
        "UPDATE sessions
         SET window_tokens = CASE WHEN context_window = '1M' THEN 1000000 ELSE 200000 END
         WHERE window_tokens IS NULL OR window_tokens = 0",
        [],
    );
    let _ = conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_sessions_provider ON sessions(provider);
        CREATE INDEX IF NOT EXISTS idx_sessions_provider_active ON sessions(provider, is_active);
        CREATE INDEX IF NOT EXISTS idx_sessions_updated ON sessions(updated_at);
        CREATE INDEX IF NOT EXISTS idx_sessions_created ON sessions(created_at);
        CREATE INDEX IF NOT EXISTS idx_sessions_history_ts
            ON sessions(COALESCE(started_at, created_at, updated_at));
        CREATE UNIQUE INDEX IF NOT EXISTS idx_daily_stats_provider_key
            ON daily_stats(provider, date, project, model);
        ",
    );
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
         WHERE provider = ?1"
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

pub fn upsert_session(s: &super::commands::SessionInfo) {
    let Ok(conn) = db().lock() else { return };
    let now = Utc::now().to_rfc3339();
    let storage_id = storage_session_id(&s.provider, &s.session_id);
    let _ = conn.execute(
        "INSERT INTO sessions (id, provider, session_name, project, model, model_id, context_window, branch, effort,
            started_at, duration_secs, total_cost, input_tokens, output_tokens,
            cache_write_tokens, cache_read_tokens, total_tokens,
            input_cost, output_cost, cache_write_cost, cache_read_cost,
            has_thinking, subagent_count, is_active, updated_at, used_tokens, window_tokens)
        VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,1,?24,?25,?26)
        ON CONFLICT(id) DO UPDATE SET
            provider=?2,
            session_name=COALESCE(?3, session_name),
            project=?4, model=?5, model_id=?6, context_window=?7, branch=?8, effort=?9,
            started_at=CASE
                WHEN sessions.started_at IS NULL OR instr(sessions.started_at, 'T') = 0 THEN ?10
                ELSE sessions.started_at
            END,
            ended_at=NULL,
            duration_secs=?11, total_cost=?12, input_tokens=?13, output_tokens=?14,
            cache_write_tokens=?15, cache_read_tokens=?16, total_tokens=?17,
            input_cost=?18, output_cost=?19, cache_write_cost=?20, cache_read_cost=?21,
            has_thinking=?22, subagent_count=?23, is_active=1, updated_at=?24,
            used_tokens=?25, window_tokens=?26",
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
            s.started_at,
            s.duration_secs as i64,
            s.cost,
            s.input_tokens as i64,
            s.output_tokens as i64,
            s.cache_write_tokens as i64,
            s.cache_read_tokens as i64,
            s.tokens as i64,
            s.input_cost,
            s.output_cost,
            s.cache_write_cost,
            s.cache_read_cost,
            s.has_thinking as i32,
            s.subagent_count as i32,
            now,
            session_used_tokens(s),
            session_window_tokens(s),
        ],
    );
    let _ = conn.execute(
        "UPDATE sessions
         SET created_at = COALESCE(created_at, started_at, updated_at),
             started_at = COALESCE(started_at, created_at, updated_at)
         WHERE id = ?1",
        params![storage_id],
    );
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

pub fn update_daily_stats(s: &super::commands::SessionInfo) {
    let Ok(conn) = db().lock() else { return };
    let date = Utc::now().format("%Y-%m-%d").to_string();
    let _ = conn.execute(
        "INSERT INTO daily_stats (date, provider, project, model, session_count, total_cost, total_tokens,
            input_tokens, output_tokens, cache_write_tokens, cache_read_tokens)
        VALUES (?1,?2,?3,?4,1,?5,?6,?7,?8,?9,?10)
        ON CONFLICT(date, provider, project, model) DO UPDATE SET
            session_count = MAX(session_count, 1),
            total_cost = ?5, total_tokens = ?6,
            input_tokens = ?7, output_tokens = ?8,
            cache_write_tokens = ?9, cache_read_tokens = ?10",
        params![
            date,
            s.provider,
            s.project,
            s.model,
            s.cost,
            s.tokens as i64,
            s.input_tokens as i64,
            s.output_tokens as i64,
            s.cache_write_tokens as i64,
            s.cache_read_tokens as i64,
        ],
    );
}

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

pub fn get_daily_stats(days: Option<i64>) -> Vec<DailyStat> {
    let Ok(conn) = db().lock() else {
        return vec![];
    };
    let provider = active_provider_slug().to_string();

    let d = days.unwrap_or(30);
    let cutoff = (Utc::now() - chrono::Duration::days(d))
        .format("%Y-%m-%d")
        .to_string();

    let mut stmt = match conn.prepare(
        "SELECT date, project, model, session_count, total_cost, total_tokens,
            input_tokens, output_tokens, cache_write_tokens, cache_read_tokens
        FROM daily_stats WHERE provider = ?1 AND date >= ?2
        ORDER BY date DESC",
    ) {
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
            "SELECT COUNT(DISTINCT date) FROM daily_stats WHERE provider = ?1",
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
        params![provider.clone()],
    );
    let _ = conn.execute(
        "DELETE FROM daily_stats WHERE provider = ?1",
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

    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        init_schema(&conn);
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
            branch: None,
            activity: "Idle".into(),
            activity_target: None,
            effort: "Medium".into(),
            effort_explicit: false,
            is_idle: false,
            started_at: None,
            duration_secs: 0,
            has_thinking: false,
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
    fn session_used_tokens_picks_larger_of_input_and_total() {
        let info = sample_session_info("200K", "claude-sonnet-4-5", 5_000, 7_500);
        assert_eq!(session_used_tokens(&info), 7_500);
        let info = sample_session_info("200K", "claude-sonnet-4-5", 9_000, 1_000);
        assert_eq!(session_used_tokens(&info), 9_000);
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
        init_schema(&conn);
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

        init_schema(&conn);

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

        init_schema(&conn);

        let window: i64 = conn
            .query_row(
                "SELECT window_tokens FROM sessions WHERE id = 'session-1m'",
                [],
                |r| r.get(0),
            )
            .expect("read window");
        assert_eq!(window, 1_000_000);
    }
}

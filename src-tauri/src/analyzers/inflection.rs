//! Inflection-point detector — surfaces days where the user's effective
//! cost-per-token or cost-per-session shifted by ≥2× versus the prior rolling
//! baseline. These points are the "something broke / something changed" flags.

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::db::HistoricalSession;

#[derive(Debug, Clone, Serialize)]
pub struct InflectionPoint {
    pub date: String,
    pub multiplier: f64,
    pub direction: &'static str,
    pub sessions_on_day: usize,
    pub cost_on_day: f64,
    pub baseline_cost: f64,
    pub note: String,
}

/// Detect cost-per-session inflections day-over-day. Returns points sorted
/// most-impactful first (largest multiplier).
pub fn detect(sessions: &[HistoricalSession]) -> Vec<InflectionPoint> {
    use std::collections::BTreeMap;

    let mut by_day: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    for s in sessions {
        let Some(started) = s.started_at.as_deref() else {
            continue;
        };
        let Ok(dt) = DateTime::parse_from_rfc3339(started) else {
            continue;
        };
        let day = dt.with_timezone(&Utc).format("%Y-%m-%d").to_string();
        let entry = by_day.entry(day).or_insert((0.0, 0));
        entry.0 += s.total_cost;
        entry.1 += 1;
    }

    if by_day.len() < 3 {
        return Vec::new();
    }

    let mut points = Vec::new();
    let days: Vec<(String, (f64, usize))> = by_day.into_iter().collect();

    // Rolling 3-day baseline (excluding the current day).
    for i in 3..days.len() {
        let (day, (cost_today, sessions_today)) = &days[i];
        if *sessions_today == 0 {
            continue;
        }
        let per_session_today = cost_today / *sessions_today as f64;
        let window = &days[i - 3..i];
        let mut baseline_cost = 0.0;
        let mut baseline_sessions = 0usize;
        for (_, (c, n)) in window {
            baseline_cost += c;
            baseline_sessions += n;
        }
        if baseline_sessions == 0 {
            continue;
        }
        let per_session_baseline = baseline_cost / baseline_sessions as f64;
        if per_session_baseline < 0.01 {
            continue;
        }
        let multiplier = per_session_today / per_session_baseline;
        let (direction, threshold_ok, note) = if multiplier >= 2.0 {
            (
                "spike",
                true,
                format!(
                    "Cost/session jumped {:.1}× versus the prior 3-day average — \
                    worth checking what changed (CLAUDE.md, model, or task complexity).",
                    multiplier
                ),
            )
        } else if multiplier <= 0.5 {
            (
                "drop",
                true,
                format!(
                    "Cost/session dropped to {:.1}× baseline — efficiency win. If this \
                    was intentional (e.g. CLAUDE.md trim), keep it.",
                    multiplier
                ),
            )
        } else {
            ("", false, String::new())
        };
        if threshold_ok {
            points.push(InflectionPoint {
                date: day.clone(),
                multiplier,
                direction,
                sessions_on_day: *sessions_today,
                cost_on_day: *cost_today,
                baseline_cost: per_session_baseline * *sessions_today as f64,
                note,
            });
        }
    }

    points.sort_by(|a, b| {
        let sig_a = (a.multiplier - 1.0).abs();
        let sig_b = (b.multiplier - 1.0).abs();
        sig_b
            .partial_cmp(&sig_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_returns_no_points() {
        assert!(detect(&[]).is_empty());
    }
}

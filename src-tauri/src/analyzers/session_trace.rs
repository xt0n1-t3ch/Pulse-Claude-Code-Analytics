use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Timelike, Utc};
use serde_json::Value;

use crate::db::HistoricalSession;

#[derive(Debug, Clone, Default)]
pub struct SessionTrace {
    pub session_id: String,
    pub first_prompt: Option<String>,
    pub user_messages: usize,
    pub assistant_messages: usize,
    pub total_tools: usize,
    pub tool_counts: HashMap<String, usize>,
    pub compact_commands: usize,
    pub mcp_tool_calls: usize,
    pub hour_distribution: [u32; 24],
}

impl SessionTrace {
    pub fn peak_overlap_pct(&self) -> u32 {
        let total: u32 = self.hour_distribution.iter().sum();
        if total == 0 {
            return 0;
        }
        let overlap: u32 = self.hour_distribution[12..=18].iter().sum();
        ((overlap as f64 / total as f64) * 100.0).round() as u32
    }
}

pub fn load_session_traces(sessions: &[HistoricalSession]) -> HashMap<String, SessionTrace> {
    let wanted: HashSet<String> = sessions.iter().map(|s| s.id.clone()).collect();
    if wanted.is_empty() {
        return HashMap::new();
    }

    let index = build_jsonl_index(&wanted);
    wanted
        .into_iter()
        .filter_map(|session_id| {
            let path = index.get(&session_id)?;
            Some((session_id.clone(), parse_session_trace(&session_id, path)))
        })
        .collect()
}

fn build_jsonl_index(wanted: &HashSet<String>) -> HashMap<String, PathBuf> {
    let mut remaining = wanted.clone();
    let mut found = HashMap::new();

    for root in cc_discord_presence::config::projects_paths() {
        if remaining.is_empty() || !root.exists() {
            continue;
        }
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            if remaining.is_empty() {
                break;
            }
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                if file_type.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                    continue;
                }
                let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                    continue;
                };
                if remaining.remove(stem) {
                    found.insert(stem.to_string(), path);
                }
            }
        }
    }

    found
}

fn parse_session_trace(session_id: &str, path: &Path) -> SessionTrace {
    let mut trace = SessionTrace {
        session_id: session_id.to_string(),
        ..Default::default()
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return trace;
    };

    for line in raw.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        let msg_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let timestamp = value
            .get("timestamp")
            .and_then(|v| v.as_str())
            .and_then(parse_timestamp_hour);
        if matches!(msg_type, "user" | "assistant")
            && let Some(hour) = timestamp
        {
            trace.hour_distribution[hour as usize] += 1;
        }

        let content = value.get("message").and_then(|m| m.get("content"));
        if msg_type == "user" {
            trace.user_messages += 1;
            let prompt_text = content.and_then(extract_text);
            if trace.first_prompt.is_none()
                && let Some(prompt) = prompt_text.clone()
            {
                trace.first_prompt = Some(truncate_prompt(&prompt));
            }
            if let Some(prompt) = prompt_text
                && prompt.contains("/compact")
            {
                trace.compact_commands += 1;
            }
        } else if msg_type == "assistant" {
            trace.assistant_messages += 1;
        }

        if let Some(content) = content {
            scan_tools(content, &mut trace);
        }
    }

    trace
}

fn parse_timestamp_hour(input: &str) -> Option<u32> {
    DateTime::parse_from_rfc3339(input)
        .ok()
        .map(|dt| dt.with_timezone(&Utc).hour())
}

fn extract_text(content: &Value) -> Option<String> {
    if let Some(s) = content.as_str() {
        return Some(s.trim().to_string()).filter(|s| !s.is_empty());
    }
    let arr = content.as_array()?;
    let mut parts = Vec::new();
    for item in arr {
        if let Some(text) = item.get("text").and_then(|v| v.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
        if let Some(text) = item.get("content").and_then(|v| v.as_str()) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn truncate_prompt(prompt: &str) -> String {
    const MAX: usize = 220;
    if prompt.len() <= MAX {
        return prompt.to_string();
    }
    let mut end = MAX;
    while end > 0 && !prompt.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &prompt[..end])
}

fn scan_tools(content: &Value, trace: &mut SessionTrace) {
    let items: Vec<&Value> = if let Some(arr) = content.as_array() {
        arr.iter().collect()
    } else if content.is_object() {
        vec![content]
    } else {
        Vec::new()
    };

    for item in items {
        if item.get("type").and_then(|v| v.as_str()) != Some("tool_use") {
            continue;
        }
        let name = item
            .get("name")
            .and_then(|v| v.as_str())
            .map(normalize_tool_name)
            .unwrap_or_else(|| "unknown".to_string());
        *trace.tool_counts.entry(name.clone()).or_insert(0) += 1;
        trace.total_tools += 1;
        if name.starts_with("mcp__") {
            trace.mcp_tool_calls += 1;
        }
    }
}

fn normalize_tool_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_text_joins_text_items() {
        let value = serde_json::json!([
            { "type": "text", "text": "Fix auth bug" },
            { "type": "text", "content": "line 42 in src/auth.rs" }
        ]);
        let text = extract_text(&value).unwrap();
        assert!(text.contains("Fix auth bug"));
        assert!(text.contains("line 42"));
    }

    #[test]
    fn peak_overlap_pct_counts_utc_window() {
        let mut trace = SessionTrace::default();
        trace.hour_distribution[12] = 3;
        trace.hour_distribution[8] = 3;
        assert_eq!(trace.peak_overlap_pct(), 50);
    }
}

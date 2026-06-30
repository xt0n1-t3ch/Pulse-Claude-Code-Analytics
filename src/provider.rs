use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    Claude,
    Codex,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Codex => "Codex",
        }
    }

    pub fn instruction_file_name(self) -> &'static str {
        match self {
            Self::Claude => "CLAUDE.md",
            Self::Codex => "AGENTS.md",
        }
    }

    pub fn home_dir_name(self) -> &'static str {
        match self {
            Self::Claude => ".claude",
            Self::Codex => ".codex",
        }
    }

    pub fn sessions_glob_label(self) -> &'static str {
        match self {
            Self::Claude => "~/.claude/projects/**/*.jsonl",
            Self::Codex => "~/.codex/sessions/**/*.jsonl",
        }
    }

    pub fn global_state_label(self) -> &'static str {
        match self {
            Self::Claude => "~/.claude/discord-presence-data.json + usage API",
            Self::Codex => "~/.codex/.codex-global-state.json + session telemetry",
        }
    }

    pub fn fix_action_label(self) -> &'static str {
        match self {
            Self::Claude => "Fix with Claude Code",
            Self::Codex => "Fix with Codex",
        }
    }

    pub fn fix_prompt_target(self) -> &'static str {
        self.display_name()
    }

    pub fn home_path(self) -> PathBuf {
        match self {
            Self::Claude => crate::config::claude_home(),
            Self::Codex => crate::codex::config::codex_home(),
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "claude" | "claude_code" | "claude-code" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProviderState {
    active_provider: Provider,
}

impl Default for ProviderState {
    fn default() -> Self {
        Self {
            active_provider: Provider::Claude,
        }
    }
}

fn state_path() -> PathBuf {
    crate::config::claude_home().join("pulse-provider.json")
}

pub fn load_active_provider() -> Provider {
    let path = state_path();
    let Ok(raw) = fs::read_to_string(path) else {
        return Provider::Claude;
    };
    serde_json::from_str::<ProviderState>(&raw)
        .map(|state| state.active_provider)
        .unwrap_or(Provider::Claude)
}

pub fn save_active_provider(provider: Provider) -> Result<()> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create provider config dir {}", parent.display())
        })?;
    }
    let payload = serde_json::to_string_pretty(&ProviderState {
        active_provider: provider,
    })?;
    fs::write(&path, payload)
        .with_context(|| format!("failed to write provider state {}", path.display()))?;
    Ok(())
}

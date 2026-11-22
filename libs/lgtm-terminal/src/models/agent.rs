//! Dynamic coding-agent identity and data-driven matcher models (no hardcoded agent enum).

use serde::{Deserialize, Serialize};

use crate::models::status::TerminalAgentStatus;

/// Dynamically detected coding agent running inside a terminal session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DetectedAgent {
    /// Dynamic machine-readable application ID (from OSC 7501 `app=` or config matcher).
    pub app_id: String,
    /// Human-friendly display label shown in `AgentsPane` and notifications.
    pub label: String,
    /// Optional status message reported by the agent (e.g., OSC 7501 `msg=`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Current lifecycle status of the agent.
    pub status: TerminalAgentStatus,
    /// Whether a `Done` transition has not yet been viewed by the user.
    pub unseen_done: bool,
}

/// User-extensible fallback matcher used to register CLI coding agents that do not yet emit OSC 7501.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AgentMatcherConfig {
    /// Stable identifier assigned when this matcher triggers (e.g., `"claude"`, `"codex"`).
    pub app_id: String,
    /// Display label rendered in `AgentsPane` (e.g., `"Claude Code"`).
    pub label: String,
    /// Case-insensitive command/argv substrings matched against the foreground process.
    #[serde(default)]
    pub command_patterns: Vec<String>,
    /// Case-insensitive substrings matched against the terminal OSC title.
    #[serde(default)]
    pub title_patterns: Vec<String>,
}

// Public ----------------------------------------------------------------------

/// Returns default data-driven agent matchers for common CLI coding agents that may not emit OSC 7501 yet.
pub fn default_agent_matchers() -> Vec<AgentMatcherConfig> {
    vec![
        AgentMatcherConfig {
            app_id: "claude".to_string(),
            label: "Claude".to_string(),
            command_patterns: vec!["claude".to_string(), "claude-code".to_string()],
            title_patterns: vec!["claude".to_string()],
        },
        AgentMatcherConfig {
            app_id: "codex".to_string(),
            label: "Codex".to_string(),
            command_patterns: vec!["codex".to_string()],
            title_patterns: vec!["codex".to_string()],
        },
        AgentMatcherConfig {
            app_id: "pi".to_string(),
            label: "Pi".to_string(),
            command_patterns: vec!["pi".to_string()],
            title_patterns: vec!["π -".to_string(), "pi -".to_string()],
        },
        AgentMatcherConfig {
            app_id: "agy".to_string(),
            label: "Antigravity".to_string(),
            command_patterns: vec!["agy".to_string(), "antigravity".to_string()],
            title_patterns: vec!["agy".to_string(), "antigravity".to_string()],
        },
        AgentMatcherConfig {
            app_id: "opencode".to_string(),
            label: "OpenCode".to_string(),
            command_patterns: vec!["opencode".to_string()],
            title_patterns: vec!["opencode".to_string()],
        },
    ]
}

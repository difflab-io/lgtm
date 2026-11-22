//! Terminal and agent lifecycle status representation.

use serde::{Deserialize, Serialize};

/// Herdr-style lifecycle status for a terminal session or detected coding agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum TerminalAgentStatus {
    /// Running in the background with no action needed (green dot).
    #[default]
    Idle,
    /// Actively executing a turn or command (yellow dot).
    Working,
    /// Waiting on user approval or interactive input (orange/red dot).
    Blocked,
    /// Completed and not yet viewed by the user (blue dot).
    Done,
}

impl TerminalAgentStatus {
    // Public ------------------------------------------------------------------

    /// Returns a numeric priority for sorting agents and aggregating worktree status dots (`Blocked > Working > Done > Idle`).
    pub fn priority(self) -> u8 {
        match self {
            Self::Blocked => 3,
            Self::Working => 2,
            Self::Done => 1,
            Self::Idle => 0,
        }
    }

    /// Maps an OSC 7501 `state` string into a [`TerminalAgentStatus`].
    pub fn from_osc7501_state(state: &str) -> Self {
        match state.trim().to_ascii_lowercase().as_str() {
            "working" | "running" | "busy" => Self::Working,
            "blocked" | "needs_attention" | "attention" | "permission" | "error" => Self::Blocked,
            "done" | "finished" | "completed" => Self::Done,
            _ => Self::Idle,
        }
    }
}

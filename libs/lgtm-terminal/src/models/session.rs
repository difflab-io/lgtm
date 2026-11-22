//! Domain models for persistent `herdr-core` terminal sessions and streamed IPC events.

use serde::{Deserialize, Serialize};

use crate::models::agent::DetectedAgent;
use crate::models::status::TerminalAgentStatus;

/// Snapshot of a persistent `herdr-core` terminal session owned by or attached to a workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSession {
    /// Stable `herdr-core` terminal address (e.g., `"t1"`).
    pub id: String,
    /// Canonical workspace checkout directory associated with this terminal.
    pub workspace_root: String,
    /// Display title for the terminal tab (shell name, OSC title, or agent label).
    pub title: String,
    /// Dynamically detected coding agent occupying this terminal, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<DetectedAgent>,
    /// Current lifecycle status dot for the terminal tab and worktree aggregation.
    pub status: TerminalAgentStatus,
    /// Monotonic counter incremented on every status or agent change for priority sorting.
    pub state_change_seq: u64,
    /// Whether the underlying `herdr-core` terminal process is still alive.
    pub alive: bool,
}

/// Push event emitted over an attached terminal stream (Tauri IPC `Channel`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TerminalStreamEvent {
    /// Bounded scrollback replay chunk sent immediately after attaching (`<= 256 KB`).
    Replay {
        /// Raw terminal bytes ready for `@wterm/ghostty` ingestion.
        data: Vec<u8>,
    },
    /// Coalesced live output chunk streamed from `herdr-core`.
    Output {
        /// Raw terminal bytes ready for `@wterm/ghostty` ingestion.
        data: Vec<u8>,
    },
    /// Updated session metadata (title, detected agent, or status dot change).
    SessionUpdated {
        /// Latest terminal session snapshot.
        session: TerminalSession,
    },
    /// Notification that the underlying terminal process exited.
    Exited {
        /// Process exit code if available.
        exit_code: Option<i32>,
    },
}

/// Host-level event emitted when a background or foreground terminal process exits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExitEvent {
    /// Stable `herdr-core` terminal address.
    pub session_id: String,
    /// Canonical workspace root associated with the exited terminal.
    pub workspace_root: String,
    /// Process exit code if available.
    pub exit_code: Option<i32>,
}

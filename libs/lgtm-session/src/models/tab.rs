//! Domain models representing candidate and persisted editor tabs in a workspace session.

use serde::{Deserialize, Serialize};

/// Candidate editor tab evaluated against the session retention policy (`edited || last_opened`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TabCandidate {
    /// Workspace-relative file path or config path (`~/.lgtm/config.json` / `.lgtm/config.json`).
    pub path: String,
    /// Display title shown in the tab bar.
    pub title: String,
    /// Whether the tab buffer was modified during this session.
    pub edited: bool,
    /// Whether the tab was the active focused tab when the workspace was saved or switched.
    pub last_opened: bool,
}

impl TabCandidate {
    // Public ------------------------------------------------------------------

    /// Returns whether this tab buffer was edited during the session.
    pub fn is_edited(&self) -> bool {
        self.edited
    }

    /// Returns whether this tab was the active focused tab when saved.
    pub fn is_last_opened(&self) -> bool {
        self.last_opened
    }

    /// Returns whether this tab candidate has a non-empty path and qualifies for session persistence (`edited || last_opened`).
    pub fn should_retain(&self) -> bool {
        !self.path.trim().is_empty() && (self.is_edited() || self.is_last_opened())
    }
}

/// Editor tab persisted in [`crate::WorkspaceSessionState`] after passing the retention filter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PersistedTab {
    /// Workspace-relative file path or config path.
    pub path: String,
    /// Display title shown in the tab bar.
    pub title: String,
    /// Whether the tab buffer was modified during the session.
    pub edited: bool,
    /// Whether the tab was the active focused tab when saved.
    pub last_opened: bool,
}

impl PersistedTab {
    // Public ------------------------------------------------------------------

    /// Returns whether this persisted tab was edited during the session.
    pub fn is_edited(&self) -> bool {
        self.edited
    }

    /// Returns whether this persisted tab was the active focused tab when saved.
    pub fn is_last_opened(&self) -> bool {
        self.last_opened
    }
}

impl From<TabCandidate> for PersistedTab {
    fn from(candidate: TabCandidate) -> Self {
        Self {
            path: candidate.path,
            title: candidate.title,
            edited: candidate.edited,
            last_opened: candidate.last_opened,
        }
    }
}

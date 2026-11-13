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

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // tests -------------------------------------------------------------------

    mod given_open_tab_candidates {
        use super::*;

        mod when_checking_retention_predicates {
            use super::*;

            #[test]
            fn should_mark_only_edited_or_last_opened_tabs_with_non_empty_paths_as_retainable() {
                // Arrange
                let unedited = TabCandidate {
                    path: "README.md".to_owned(),
                    title: "README.md".to_owned(),
                    edited: false,
                    last_opened: false,
                };
                let edited = TabCandidate {
                    path: "src/lib.rs".to_owned(),
                    title: "lib.rs".to_owned(),
                    edited: true,
                    last_opened: false,
                };
                let active = TabCandidate {
                    path: "src/app.rs".to_owned(),
                    title: "app.rs".to_owned(),
                    edited: false,
                    last_opened: true,
                };

                // Act & Assert
                assert!(!unedited.should_retain());
                assert!(edited.is_edited());
                assert!(edited.should_retain());
                assert!(active.is_last_opened());
                assert!(active.should_retain());
            }
        }
    }
}

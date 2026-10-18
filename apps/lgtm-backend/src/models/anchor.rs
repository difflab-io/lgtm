use crate::error::{Result, invalid};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Anchor {
    #[default]
    General,
    File {
        path: String,
    },
    Line {
        path: String,
        start: i64,
        end: i64,
        side: String,
        /// Git commit identity, or sha256:-prefixed content identity for local non-Git notes.
        commit: String,
    },
}

impl Anchor {
    /// Relocate local content anchors only when their selected lines match uniquely.
    /// The caller supplies current text and identity; original persisted coordinates remain unchanged.
    pub fn relocate(
        &self,
        content: &str,
        content_hash: &str,
        selected_text: Option<&str>,
    ) -> Option<Self> {
        let Self::Line {
            path, side, commit, ..
        } = self
        else {
            return None;
        };
        if !commit.starts_with("sha256:") {
            return None;
        }
        if commit == content_hash {
            return Some(self.clone());
        }
        let selected: Vec<_> = selected_text?.split('\n').collect();
        let lines: Vec<_> = content.lines().collect();
        let mut matches = lines
            .windows(selected.len())
            .enumerate()
            .filter(|(_, window)| *window == selected.as_slice())
            .map(|(line, _)| line);
        let line = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some(Self::Line {
            path: path.clone(),
            start: line as i64 + 1,
            end: (line + selected.len()) as i64,
            side: side.clone(),
            commit: content_hash.into(),
        })
    }

    pub fn validate(&self) -> Result<()> {
        let path = match self {
            Self::General => return Ok(()),
            Self::File { path } | Self::Line { path, .. } => path,
        };
        if path.is_empty()
            || Path::new(path).is_absolute()
            || Path::new(path)
                .components()
                .any(|p| matches!(p, std::path::Component::ParentDir))
        {
            return Err(invalid(
                "Anchor path must be a nonempty relative path without ..",
            ));
        }
        if let Self::Line {
            start,
            end,
            side,
            commit,
            ..
        } = self
            && (*start < 1
                || end < start
                || !["old", "new"].contains(&side.as_str())
                || commit.is_empty())
        {
            return Err(invalid(
                "Line anchor requires positive ordered lines, old/new side and reviewed commit",
            ));
        }
        Ok(())
    }
}

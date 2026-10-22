use crate::error::{Result, invalid};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;

/// Immutable identity of the saved bytes selected by a reviewer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AnchorSource {
    Commit {
        sha: String,
        content_hash: String,
    },
    IndexFile {
        blob_oid: String,
        content_hash: String,
    },
    WorkingFile {
        content_hash: String,
    },
    LocalFile {
        content_hash: String,
    },
}

impl AnchorSource {
    pub fn content_hash(&self) -> &str {
        match self {
            Self::Commit { content_hash, .. }
            | Self::IndexFile { content_hash, .. }
            | Self::WorkingFile { content_hash }
            | Self::LocalFile { content_hash } => content_hash,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let hash = self.content_hash();
        if !hash
            .strip_prefix("sha256:")
            .is_some_and(|value| value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        {
            return Err(invalid(
                "Source requires a SHA-256 identity of the exact saved bytes",
            ));
        }
        let oid = match self {
            Self::Commit { sha, .. } => Some(sha),
            Self::IndexFile { blob_oid, .. } => Some(blob_oid),
            _ => None,
        };
        if oid.is_some_and(|value| {
            ![40, 64].contains(&value.len()) || !value.bytes().all(|b| b.is_ascii_hexdigit())
        }) {
            return Err(invalid("Git source requires a full object identity"));
        }
        Ok(())
    }

    pub fn hash(bytes: &[u8]) -> String {
        format!("sha256:{:x}", Sha256::digest(bytes))
    }
}

/// Bounded immutable text evidence. This is not a full-file snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorEvidence {
    pub source: AnchorSource,
    pub selected_text: String,
    pub context_before: String,
    pub context_after: String,
}

impl AnchorEvidence {
    /// Find one whole-range match supported by the saved neighboring text.
    /// Added neighboring lines are allowed. Replaced or ambiguous evidence is not proof.
    pub fn get_matching_range(&self, content: &str) -> Option<(i64, i64)> {
        if self.selected_text.trim().is_empty()
            && self.context_before.trim().is_empty()
            && self.context_after.trim().is_empty()
        {
            return None;
        }
        let lines: Vec<_> = content.lines().collect();
        let selected: Vec<_> = self.selected_text.split('\n').collect();
        let before: Vec<_> = if self.context_before.is_empty() {
            vec![]
        } else {
            self.context_before.split('\n').collect()
        };
        let after: Vec<_> = if self.context_after.is_empty() {
            vec![]
        } else {
            self.context_after.split('\n').collect()
        };
        let contains_ordered = |window: &[&str], expected: &[&str]| {
            let mut found = 0;
            for line in window {
                if expected.get(found) == Some(line) {
                    found += 1;
                }
            }
            found == expected.len()
        };
        let mut candidates = lines
            .windows(selected.len())
            .enumerate()
            .filter(|(start, window)| {
                if *window != selected.as_slice() {
                    return false;
                }
                let end = *start + selected.len();
                contains_ordered(
                    &lines[start.saturating_sub(before.len() + 8)..*start],
                    &before,
                ) && contains_ordered(
                    &lines[end..(end + after.len() + 8).min(lines.len())],
                    &after,
                )
            })
            .map(|(start, _)| (start as i64 + 1, (start + selected.len()) as i64));
        let found = candidates.next()?;
        if candidates.next().is_some() {
            return None;
        }
        Some(found)
    }

    pub fn validate(&self) -> Result<()> {
        self.source.validate()?;
        if self.selected_text.len() > 65536
            || self.context_before.len() > 8192
            || self.context_after.len() > 8192
        {
            return Err(invalid("Anchor evidence exceeds the bounded excerpt limit"));
        }
        Ok(())
    }

    /// File-level identity has no fabricated line range or selected text.
    pub fn capture_file(source: AnchorSource, bytes: &[u8]) -> Result<Self> {
        source.validate()?;
        if source.content_hash() != AnchorSource::hash(bytes) {
            return Err(invalid("File evidence does not match its saved bytes"));
        }
        Ok(Self {
            source,
            selected_text: String::new(),
            context_before: String::new(),
            context_after: String::new(),
        })
    }

    /// Capture whole selected lines and at most three neighboring lines on either side.
    pub fn capture(source: AnchorSource, bytes: &[u8], start: i64, end: i64) -> Result<Self> {
        if source.content_hash() != AnchorSource::hash(bytes) {
            return Err(invalid("Anchor source does not match the supplied bytes"));
        }
        let text = std::str::from_utf8(bytes)
            .map_err(|_| invalid("Anchor evidence requires UTF-8 text"))?;
        let lines: Vec<_> = text.lines().collect();
        if start < 1 || end < start || end as usize > lines.len() {
            return Err(invalid("Selected range is outside the saved source"));
        }
        let start = start as usize - 1;
        let end = end as usize;
        let evidence = Self {
            source,
            selected_text: lines[start..end].join("\n"),
            context_before: lines[start.saturating_sub(3)..start].join("\n"),
            context_after: lines[end..(end + 3).min(lines.len())].join("\n"),
        };
        evidence.validate()?;
        Ok(evidence)
    }
}

/// A complete provider observation. Current coordinates never replace original evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeObservation {
    pub base: String,
    pub head: String,
    pub outdated: bool,
    pub current: Option<Anchor>,
}

/// A derived result for one displayed source. Only confirmed variants carry code coordinates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AnchorPlacement {
    Exact {
        location: Anchor,
        displayed_source: AnchorSource,
    },
    Relocated {
        location: Anchor,
        displayed_source: AnchorSource,
    },
    Outdated {
        reason: String,
    },
    Orphaned {
        reason: String,
    },
    Unverified {
        reason: String,
    },
}

/// The original discussion location: general, whole file, or a line range.
/// Exact byte identity and bounded relocation text belong to AnchorEvidence, not this location.
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
    },
}

impl Anchor {
    /// Resolve only against the caller's named, confirmed file association and displayed source.
    pub fn get_placement(
        &self,
        evidence: Option<&AnchorEvidence>,
        path: &str,
        displayed_source: Option<&AnchorSource>,
        content: Option<&str>,
        path_confirmed: bool,
    ) -> AnchorPlacement {
        if matches!(self, Self::General) {
            return AnchorPlacement::Unverified {
                reason: "Review-level feedback has no code location".into(),
            };
        }
        if !path_confirmed {
            return AnchorPlacement::Unverified {
                reason: "File association is not confirmed for this comparison".into(),
            };
        }
        let (displayed_source, content) = match (displayed_source, content) {
            (Some(source), Some(content)) => (source, content),
            (None, None) => {
                return AnchorPlacement::Orphaned {
                    reason: "File does not exist on the selected side".into(),
                };
            }
            _ => {
                return AnchorPlacement::Unverified {
                    reason: "Displayed source content is unavailable".into(),
                };
            }
        };
        if displayed_source.validate().is_err()
            || displayed_source.content_hash() != AnchorSource::hash(content.as_bytes())
        {
            return AnchorPlacement::Unverified {
                reason: "Displayed source identity does not match its bytes".into(),
            };
        }
        match self {
            Self::General => {
                return AnchorPlacement::Unverified {
                    reason: "Review-level feedback has no code location".into(),
                };
            }
            Self::File { .. } | Self::Line { .. } => {}
        }
        let Some(evidence) = evidence else {
            return AnchorPlacement::Unverified {
                reason: "Original source evidence is unavailable".into(),
            };
        };
        if evidence.validate().is_err() {
            return AnchorPlacement::Unverified {
                reason: "Original source evidence is invalid".into(),
            };
        }
        if matches!(evidence.source, AnchorSource::WorkingFile { .. })
            && evidence.source.content_hash() != displayed_source.content_hash()
        {
            return AnchorPlacement::Orphaned {
                reason: "Unstaged working source changed".into(),
            };
        }
        if let Self::File {
            path: original_path,
        } = self
        {
            let location = Self::File { path: path.into() };
            return if &evidence.source == displayed_source && original_path == path {
                AnchorPlacement::Exact {
                    location,
                    displayed_source: displayed_source.clone(),
                }
            } else {
                AnchorPlacement::Relocated {
                    location,
                    displayed_source: displayed_source.clone(),
                }
            };
        }
        let Self::Line {
            path: original_path,
            start,
            end,
            side,
            ..
        } = self
        else {
            unreachable!()
        };
        let at_original = evidence.source.content_hash() == displayed_source.content_hash()
            && AnchorEvidence::capture(displayed_source.clone(), content.as_bytes(), *start, *end)
                .is_ok_and(|actual| actual.selected_text == evidence.selected_text);
        let range = if at_original {
            Some((*start, *end))
        } else {
            evidence.get_matching_range(content)
        };
        let Some((start, end)) = range else {
            return AnchorPlacement::Outdated {
                reason: "Original selection was lost, changed, or became ambiguous".into(),
            };
        };
        let location = Self::Line {
            path: path.into(),
            start,
            end,
            side: side.clone(),
        };
        if at_original && original_path == path && evidence.source == *displayed_source {
            AnchorPlacement::Exact {
                location,
                displayed_source: displayed_source.clone(),
            }
        } else {
            AnchorPlacement::Relocated {
                location,
                displayed_source: displayed_source.clone(),
            }
        }
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
            start, end, side, ..
        } = self
            && (*start < 1 || end < start || !["old", "new"].contains(&side.as_str()))
        {
            return Err(invalid(
                "Line anchor requires positive ordered lines and old/new side",
            ));
        }
        Ok(())
    }
}

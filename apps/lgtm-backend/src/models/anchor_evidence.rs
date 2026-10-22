use super::AnchorSource;
use crate::error::{Result, invalid};
use crate::{models as m, proto::lgtm::v1 as p};
use connectrpc::ConnectError;
use serde::{Deserialize, Serialize};

/// Bounded immutable text evidence. This is not a full-file snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorEvidence {
    pub source: AnchorSource,
    pub selected_text: String,
    pub context_before: String,
    pub context_after: String,
}

impl AnchorEvidence {
    // Public ------------------------------------------------------------------

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

    // RPC conversion ----------------------------------------------------------

    /// Validate and convert the RPC input to the domain model.
    pub(crate) fn from_proto_anchor(
        v: &p::Anchor,
    ) -> std::result::Result<Option<m::AnchorEvidence>, ConnectError> {
        if v.source.is_set() != v.evidence.is_set() {
            return Err(ConnectError::invalid_argument(
                "p::Anchor evidence requires its source and excerpt fields",
            ));
        }
        if !v.source.is_set() {
            return Ok(None);
        }
        if !["line", "file"].contains(&v.kind.as_str()) {
            return Err(ConnectError::invalid_argument(
                "Source evidence requires a file or line anchor",
            ));
        }
        let evidence = m::AnchorEvidence {
            source: m::AnchorSource::from_proto(&v.source)?,
            selected_text: v.evidence.selected_text.clone(),
            context_before: v.evidence.context_before.clone(),
            context_after: v.evidence.context_after.clone(),
        };
        if v.kind == "file"
            && (!evidence.selected_text.is_empty()
                || !evidence.context_before.is_empty()
                || !evidence.context_after.is_empty())
        {
            return Err(ConnectError::invalid_argument(
                "File evidence has no line selection",
            ));
        }
        evidence.validate().map_err(ConnectError::from)?;
        Ok(Some(evidence))
    }
}

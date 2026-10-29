use super::AnchorSource;
use crate::error::{Result, invalid};
use serde::{Deserialize, Serialize};

/// Bounded immutable text evidence. This is not a full-file snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnchorEvidence {
    #[serde(default)]
    pub snapshot_id: Option<uuid::Uuid>,
    pub source: AnchorSource,
    pub selected_text: String,
    pub context_before: String,
    pub context_after: String,
}

impl AnchorEvidence {
    // Public ------------------------------------------------------------------

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
            snapshot_id: None,
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
            snapshot_id: None,
            source,
            selected_text: lines[start..end].join("\n"),
            context_before: lines[start.saturating_sub(3)..start].join("\n"),
            context_after: lines[end..(end + 3).min(lines.len())].join("\n"),
        };
        evidence.validate()?;
        Ok(evidence)
    }
}

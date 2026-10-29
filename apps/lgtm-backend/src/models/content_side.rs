//! Transient diff descriptors and selected-source data. No review ownership or persistence.
use super::AnchorSource;
use crate::{models as m, proto::lgtm::v1 as p};

#[derive(Clone, Debug)]
pub struct ContentSide {
    pub path: String,
    pub source: AnchorSource,
    pub content: String,
}

impl ContentSide {
    // Public ------------------------------------------------------------------

    /// Capture only from the named source; save operations independently verify these bytes.
    pub fn capture(&self, anchor: &super::Anchor) -> crate::error::Result<super::AnchorEvidence> {
        use super::{Anchor, AnchorEvidence};
        anchor.validate()?;
        if self.content.len() > 2 * 1024 * 1024 || self.content.contains('\0') {
            return Err(crate::error::invalid(
                "Anchor source requires bounded UTF-8 text",
            ));
        }
        let path = match anchor {
            Anchor::File { path } | Anchor::Line { path, .. } => path,
            Anchor::General => {
                return Err(crate::error::invalid(
                    "General feedback has no source evidence",
                ));
            }
        };
        if path != &self.path {
            return Err(crate::error::invalid(
                "Anchor differs from its saved source path",
            ));
        }
        match anchor {
            Anchor::Line { start, end, .. } => {
                AnchorEvidence::capture(self.source.clone(), self.content.as_bytes(), *start, *end)
            }
            _ => AnchorEvidence::capture_file(self.source.clone(), self.content.as_bytes()),
        }
    }

    // RPC conversion ----------------------------------------------------------

    /// Convert the domain value to its RPC representation.
    pub(crate) fn into_proto(self) -> p::ContentSide {
        let value = self;
        p::ContentSide {
            path: value.path,
            source: Some(m::AnchorSource::into_proto(value.source)).into(),
            content: value.content,
            ..Default::default()
        }
    }
}

// tests -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Anchor, AnchorSource};

    // tests -------------------------------------------------------------------
    #[test]
    fn should_capture_exact_crlf_source_bytes_and_reject_invalid_selection_or_identity() {
        let content = "before\r\nselected ü\r\nafter";
        let mut displayed = ContentSide {
            path: "file.md".into(),
            source: AnchorSource::IndexFile {
                blob_oid: "a".repeat(40),
                content_hash: AnchorSource::hash(content.as_bytes()),
            },
            content: content.into(),
        };
        let location = Anchor::Line {
            path: "file.md".into(),
            start: 2,
            end: 2,
            side: "new".into(),
        };
        let evidence = displayed.capture(&location).unwrap();
        assert_eq!(evidence.source, displayed.source);
        assert_eq!(evidence.selected_text, "selected ü");
        assert_eq!(evidence.context_before, "before");
        assert_eq!(evidence.context_after, "after");
        assert!(
            displayed
                .capture(&Anchor::Line {
                    path: "file.md".into(),
                    start: 4,
                    end: 4,
                    side: "new".into()
                })
                .is_err()
        );
        assert!(
            displayed
                .capture(&Anchor::File {
                    path: "other.md".into()
                })
                .is_err()
        );
        displayed.content.push_str("changed");
        assert!(displayed.capture(&location).is_err());
    }
}

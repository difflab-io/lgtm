use super::{Result, invalid};
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
        commit: String,
    },
}

impl Anchor {
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

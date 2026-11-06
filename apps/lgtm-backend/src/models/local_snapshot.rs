use crate::{
    error::{Result, invalid},
    proto::lgtm::v1 as p,
};
use serde::{Deserialize, Serialize};

/// Backend-owned receipt for exact source bytes retained outside the project checkout.
/// The slug identifies storage under LGTM_HOME/local; path is relative to its tree.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalSnapshot {
    pub project_slug: String,
    pub commit_oid: String,
    pub path: String,
}

impl LocalSnapshot {
    // Public ------------------------------------------------------------------

    pub fn validate(&self) -> Result<()> {
        if self.project_slug.is_empty()
            || !self
                .project_slug
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
            || self.commit_oid.len() != 40
            || !self.commit_oid.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(invalid("Invalid local snapshot receipt"));
        }
        super::Anchor::File {
            path: self.path.clone(),
        }
        .validate()
    }

    pub(crate) fn into_proto(self) -> p::LocalSnapshot {
        p::LocalSnapshot {
            project_slug: self.project_slug,
            commit_oid: self.commit_oid,
            path: self.path,
            ..Default::default()
        }
    }
}

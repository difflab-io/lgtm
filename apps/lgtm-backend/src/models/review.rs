//! Public review intent and write guards. Storage context/session IDs are resolved by services.
use super::{ComparisonKind, Session};
use crate::error::{Result, invalid};
use serde::{Deserialize, Serialize};

/// A location or forge address chosen by an API consumer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewTarget {
    LocalGit {
        path: String,
        branch: Option<String>,
    },
    Directory {
        path: String,
    },
    PullRequest {
        repository_url: String,
        number: i64,
    },
}

/// Immutable selection for one attempt. Mutable file sources are identified by each anchor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewComparison {
    Working {
        base_commit: String,
    },
    Staged {
        base_commit: String,
    },
    Branch {
        base_commit: String,
        head_commit: String,
    },
    Directory,
    PullRequest {
        base_commit: String,
        head_commit: String,
    },
}

impl ReviewComparison {
    pub fn kind(&self) -> ComparisonKind {
        match self {
            Self::Working { .. } => ComparisonKind::Working,
            Self::Staged { .. } => ComparisonKind::Staged,
            Self::Branch { .. } => ComparisonKind::Branch,
            Self::Directory => ComparisonKind::Directory,
            Self::PullRequest { .. } => ComparisonKind::PullRequest,
        }
    }

    pub fn validate(&self) -> Result<()> {
        let commits: Vec<&String> = match self {
            Self::Working { base_commit } | Self::Staged { base_commit } => vec![base_commit],
            Self::Branch {
                base_commit,
                head_commit,
            }
            | Self::PullRequest {
                base_commit,
                head_commit,
            } => vec![base_commit, head_commit],
            Self::Directory => vec![],
        };
        if commits.iter().any(|sha| {
            ![40, 64].contains(&sha.len()) || !sha.bytes().all(|b| b.is_ascii_hexdigit())
        }) {
            return Err(invalid("Comparison requires full commit identities"));
        }
        Ok(())
    }
}

/// One session-backed public attempt plus its current conversation version.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Review {
    pub target: ReviewTarget,
    pub comparison: Option<ReviewComparison>,
    /// Latest complete provider comparison; acceptance never changes the pinned comparison implicitly.
    pub observed_comparison: Option<ReviewComparison>,
    pub session: Session,
    pub version: i64,
    pub forge_state: Option<crate::database::types::ForgeState>,
}

/// Notes use a file location and do not require a visible review.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReviewScope {
    Review { review_id: uuid::Uuid },
    Notes { path: String },
}

/// Optimistic version and durable retry identity for a single normalized request.
#[derive(Clone, Debug)]
pub struct MutationGuard {
    pub expected_version: Option<i64>,
    pub request_key: String,
    pub(crate) payload_hash: String,
    /// Internal CAS version after an observation; excluded from the consumer request fingerprint.
    prepared_version: Option<i64>,
    pub(crate) expected_object: Option<ExpectedObject>,
    pub(crate) scope: Option<ReviewScope>,
}

impl MutationGuard {
    /// Bind retry identity to service-normalized input, including the supplied version.
    pub(crate) fn bind<T: Serialize>(&self, input: &T) -> Result<Self> {
        use sha2::{Digest, Sha256};
        Ok(Self {
            expected_version: self.expected_version,
            prepared_version: self.prepared_version,
            request_key: self.request_key.clone(),
            expected_object: self.expected_object.clone(),
            scope: self.scope.clone(),
            payload_hash: format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&(
                    self.expected_version,
                    &self.expected_object,
                    &self.scope,
                    input
                ))?)
            ),
        })
    }

    /// Bind hidden notes writers to the location scope, not to an internal session attempt.
    pub(crate) fn bind_attempt<T: Serialize>(
        &self,
        session_id: &uuid::Uuid,
        input: &T,
    ) -> Result<Self> {
        let attempt = if matches!(self.scope, Some(ReviewScope::Notes { .. })) {
            None
        } else {
            Some(session_id)
        };
        self.bind(&(attempt, input))
    }

    pub(crate) fn after_observation(mut self, version: i64) -> Self {
        self.prepared_version = Some(version);
        self
    }

    pub(crate) fn write_version(&self) -> Option<i64> {
        self.prepared_version.or(self.expected_version)
    }

    pub fn in_scope(mut self, scope: ReviewScope) -> Self {
        self.scope = Some(scope);
        self
    }

    pub(crate) fn with_expected_object(mut self, object: ExpectedObject) -> Self {
        self.expected_object = Some(object);
        self
    }

    pub(crate) fn validate_object<T: Serialize>(
        &self,
        kind: ObjectKind,
        id: uuid::Uuid,
        value: &T,
    ) -> Result<()> {
        if let Some(expected) = &self.expected_object
            && (expected.kind != kind
                || expected.id != id
                || crate::utils::object_hash(value)? != expected.hash)
        {
            return Err(crate::error::Error::Conflict(
                "The selected comment or thread changed; read it again before editing".into(),
            ));
        }
        Ok(())
    }

    pub fn new(expected_version: Option<i64>, request_key: String) -> Self {
        Self {
            expected_version,
            prepared_version: None,
            request_key,
            payload_hash: String::new(),
            expected_object: None,
            scope: None,
        }
    }
}

/// Confirmed client observations, validated against the selected scope and saved evidence.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceChange {
    /// A confirmed move of the selected saved-file scope or directory review root.
    LocationMoved {
        previous_path: String,
        current_path: String,
    },
    Moved {
        thread_id: uuid::Uuid,
        previous_path: String,
        current_path: String,
        source: super::AnchorSource,
    },
    Deleted {
        thread_id: uuid::Uuid,
        path: String,
    },
    WorkingSourceChanged {
        thread_id: uuid::Uuid,
        path: String,
        source: super::AnchorSource,
    },
    Restored {
        thread_id: uuid::Uuid,
        path: String,
        source: super::AnchorSource,
    },
}

/// Selected root notes and confirmed destination anchors. Replies remain at the source.
#[derive(Clone, Debug)]
pub struct NotePromotion {
    pub comment_ids: Vec<uuid::Uuid>,
    pub anchors: Vec<super::Anchor>,
    pub evidence: Vec<Option<super::AnchorEvidence>>,
    pub request_key: String,
    pub expected_source_version: Option<i64>,
    pub guard: Option<MutationGuard>,
}

/// Changing comparisons requires explicit acceptance of the selected prior attempt.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StartOptions {
    pub comparison_kind: ComparisonKind,
    pub replace_local: bool,
    pub accept_pr_comparison: bool,
    pub expected_active_id: Option<uuid::Uuid>,
}

impl From<ComparisonKind> for StartOptions {
    fn from(comparison_kind: ComparisonKind) -> Self {
        Self {
            comparison_kind,
            replace_local: false,
            accept_pr_comparison: false,
            expected_active_id: None,
        }
    }
}

/// Object tokens bind a scope revision to the exact comment or thread that a client read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectKind {
    Comment,
    Thread,
}

impl ObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Thread => "thread",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExpectedObject {
    pub kind: ObjectKind,
    pub id: uuid::Uuid,
    pub hash: String,
}

/// Final review intent, guarded by the comparison and conversation the consumer reviewed.
#[derive(Clone, Debug)]
pub struct ReviewSubmission {
    pub outcome: String,
    pub body: String,
    pub request_key: String,
    pub expected_head: String,
    pub expected_base: Option<String>,
    pub expected_version: Option<i64>,
}

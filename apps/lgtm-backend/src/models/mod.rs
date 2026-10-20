//! Domain models and command inputs, separate from SeaORM entity rows.
pub mod context_key;
pub use context_key::ContextKey;
pub mod actor;
pub use super::database::types::{ContextKind, PostingState, SessionState};
pub use actor::{Actor, ActorKind};
pub mod context;
pub use context::Context;
pub mod commit_context;
pub use commit_context::CommitContext;
pub mod session;
pub use session::{CompletedSession, Session, SessionStatus};
pub mod anchor;
pub use super::database::types::{ComparisonKind, OrphanReason};
pub use anchor::{Anchor, AnchorEvidence, AnchorPlacement, AnchorSource, ForgeObservation};
pub mod thread;
pub use thread::Thread;
pub mod comment;
pub use comment::Comment;
pub mod conversation;
pub use conversation::Conversation;
pub mod session_filter;
pub use session_filter::SessionFilter;
pub mod completion;
pub use completion::Completion;

pub mod posting_status;
pub use posting_status::PostingStatus;

pub mod local_location;
pub use local_location::LocalLocation;
pub mod forge_repository_url;
pub use forge_repository_url::ForgeRepositoryUrl;

pub mod review;
pub use review::{MutationGuard, Review, ReviewComparison, ReviewScope, ReviewTarget};

pub use review::SourceChange;

pub mod diff;
pub use diff::{ContentSide, DiffComparison, DiffContent, DiffFile};

pub use review::NotePromotion;

pub use review::StartOptions;

pub use review::{ExpectedObject, ObjectKind};

pub mod forge_change;
pub use forge_change::{ForgeChange, ForgeChangeInput, ForgeChangeObject};

pub use review::ReviewSubmission;

//! Typed discriminators stored as portable string-valued database columns.
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Discriminator for identity records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum IdentityKind {
    /// The human variant.
    #[sea_orm(string_value = "human")]
    Human,
    /// The agent variant.
    #[sea_orm(string_value = "agent")]
    Agent,
}
impl IdentityKind {
    /// Return the persisted discriminator.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Agent => "agent",
        }
    }
}
impl std::fmt::Display for IdentityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Discriminator for context records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum ContextKind {
    /// A local notes or branch context.
    #[sea_orm(string_value = "local")]
    Local,
    /// A PR/MR context backed by a forge repository URL.
    #[sea_orm(string_value = "forge_backed")]
    ForgeBacked,
}
impl ContextKind {
    /// Return the persisted discriminator.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::ForgeBacked => "forge_backed",
        }
    }
}
impl std::fmt::Display for ContextKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Discriminator for session records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// The active variant.
    #[sea_orm(string_value = "active")]
    Active,
    /// The completed variant.
    #[sea_orm(string_value = "completed")]
    Completed,
}
impl SessionState {
    /// Return the persisted discriminator.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Completed => "completed",
        }
    }
}
impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Discriminator for posting records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum PostingState {
    /// The unposted variant.
    #[sea_orm(string_value = "unposted")]
    Unposted,
    /// The sending variant.
    #[sea_orm(string_value = "sending")]
    Sending,
    /// The uncertain variant.
    #[sea_orm(string_value = "uncertain")]
    Uncertain,
    /// The confirmed variant.
    #[sea_orm(string_value = "confirmed")]
    Confirmed,
}
impl PostingState {
    /// Return the persisted discriminator.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unposted => "unposted",
            Self::Sending => "sending",
            Self::Uncertain => "uncertain",
            Self::Confirmed => "confirmed",
        }
    }
}
impl std::fmt::Display for PostingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The selected content comparison, independent of context identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum ComparisonKind {
    /// Internal notes writes have no visible review attempt.
    #[sea_orm(string_value = "notes")]
    Notes,
    #[sea_orm(string_value = "working")]
    Working,
    #[sea_orm(string_value = "staged")]
    Staged,
    #[sea_orm(string_value = "branch")]
    Branch,
    #[sea_orm(string_value = "directory")]
    Directory,
    #[sea_orm(string_value = "pull_request")]
    PullRequest,
}

/// The selected source of annotated bytes. Absence of evidence is represented by None.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    #[sea_orm(string_value = "commit")]
    Commit,
    #[sea_orm(string_value = "index_file")]
    IndexFile,
    #[sea_orm(string_value = "working_file")]
    WorkingFile,
    #[sea_orm(string_value = "local_file")]
    LocalFile,
}

/// Confirmed loss of a file or an ephemeral working source.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum OrphanReason {
    #[sea_orm(string_value = "file_deleted")]
    FileDeleted,
    #[sea_orm(string_value = "working_source_changed")]
    WorkingSourceChanged,
}

/// Last observed forge review lifecycle; local contexts never have this value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum ForgeState {
    #[sea_orm(string_value = "open")]
    Open,
    #[sea_orm(string_value = "closed")]
    Closed,
    #[sea_orm(string_value = "merged")]
    Merged,
}
impl ForgeState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::Merged => "merged",
        }
    }
}

//! Shared SeaORM enum discriminators stored as portable string columns.

use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Discriminator for human versus AI agent identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum IdentityKind {
    /// Human reviewer requiring an email address and optional forge login.
    #[sea_orm(string_value = "Human")]
    Human,
    /// AI agent requiring a model provider and model identifier without an email address.
    #[sea_orm(string_value = "Agent")]
    Agent,
}

impl IdentityKind {
    // Public ------------------------------------------------------------------

    /// Return the database string representation of this discriminator.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "Human",
            Self::Agent => "Agent",
        }
    }
}

impl std::fmt::Display for IdentityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Discriminator for local branch reviews versus remote pull request reviews.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum ReviewKind {
    /// Local branch review with no remote repository URL or pull request number.
    #[sea_orm(string_value = "Local")]
    Local,
    /// Remote forge-backed review bound to a repository URL and pull request number.
    #[sea_orm(string_value = "ForgeBacked")]
    ForgeBacked,
}

impl ReviewKind {
    // Public ------------------------------------------------------------------

    /// Return the database string representation of this discriminator.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "Local",
            Self::ForgeBacked => "ForgeBacked",
        }
    }
}

impl std::fmt::Display for ReviewKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Lifecycle state of a local or forge-backed review.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    /// Review is active and accepting draft sessions and comments.
    #[sea_orm(string_value = "Open")]
    Open,
    /// Review was closed without merging.
    #[sea_orm(string_value = "Closed")]
    Closed,
    /// Pull request or branch was merged.
    #[sea_orm(string_value = "Merged")]
    Merged,
}

impl ReviewStatus {
    // Public ------------------------------------------------------------------

    /// Return the database string representation of this status.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Closed => "Closed",
            Self::Merged => "Merged",
        }
    }
}

impl std::fmt::Display for ReviewStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Publication state of an author's review session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    /// Unpublished batch of comments visible only to its author.
    #[sea_orm(string_value = "Draft")]
    Draft,
    /// Published review submission with a required outcome and timestamp.
    #[sea_orm(string_value = "Published")]
    Published,
}

impl SessionState {
    // Public ------------------------------------------------------------------

    /// Return the database string representation of this session state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Published => "Published",
        }
    }
}

impl std::fmt::Display for SessionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Verdict recorded when publishing a review session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum, Serialize, Deserialize)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::None)")]
#[serde(rename_all = "snake_case")]
pub enum ReviewOutcome {
    /// General feedback without approval or rejection.
    #[sea_orm(string_value = "Comment")]
    Comment,
    /// Request changes or reject the reviewed diff (`--with reject`).
    #[sea_orm(string_value = "Reject")]
    Reject,
    /// Approve the reviewed diff (`--with approve`).
    #[sea_orm(string_value = "Approve")]
    Approve,
    /// Request a follow-up review (`--with request`).
    #[sea_orm(string_value = "Request")]
    Request,
}

impl ReviewOutcome {
    // Public ------------------------------------------------------------------

    /// Return the database string representation of this review outcome.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Comment => "Comment",
            Self::Reject => "Reject",
            Self::Approve => "Approve",
            Self::Request => "Request",
        }
    }
}

impl std::fmt::Display for ReviewOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

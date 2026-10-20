//! CLI review target and attribution arguments; no persistence or forge access.
use clap::{Args, ValueEnum};
use std::path::PathBuf;

/// Git content selection within the shared local branch conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum ReviewMode {
    Working,
    Staged,
    Branch,
}

#[derive(Args, Clone, Debug, Default)]
pub struct ContextArgs {
    #[arg(long,conflicts_with_all=["branch","notes"])]
    pub pr: Option<String>,
    #[arg(long, conflicts_with = "notes")]
    pub branch: Option<String>,
    /// Git comparison used by session start or machine-mode review.
    #[arg(long, value_enum, conflicts_with_all = ["pr", "notes"])]
    pub mode: Option<ReviewMode>,
    /// Base branch for --mode branch (defaults to main).
    #[arg(long, requires = "mode")]
    pub base: Option<String>,
    #[arg(long,num_args=0..=1,default_missing_value=".")]
    pub notes: Option<PathBuf>,
    #[arg(long)]
    pub path: Option<PathBuf>,
    #[arg(long)]
    pub repo: Option<String>,
    /// Forge hostname for --pr and shorthand repository names.
    #[arg(long, default_value = "github.com")]
    pub host: String,
}
#[derive(Args, Clone, Debug, Default)]
pub struct ActorArgs {
    #[arg(long = "as", conflicts_with = "identity")]
    pub as_name: Option<String>,
    #[arg(long, requires = "as_name")]
    pub model: Option<String>,
    #[arg(long, requires = "model")]
    pub provider: Option<String>,
    #[arg(long)]
    pub identity: Option<String>,
}

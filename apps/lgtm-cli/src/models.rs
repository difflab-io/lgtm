//! Shared arguments and outcomes for the future local-forge commands.
use clap::{Args, ValueEnum};

#[derive(Args, Debug)]
pub struct LocalArgs {
    /// Keep work within LGTM-managed storage (not implemented yet).
    #[arg(long)]
    pub local: bool,
}

#[derive(Args, Debug)]
pub struct ActorArgs {
    /// Attribute the operation to a named agent (not implemented yet).
    #[arg(long = "as", value_parser = parse_nonempty)]
    pub agent_name: Option<String>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ReviewOutcome {
    Comment,
    Reject,
    Approve,
    Request,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ReviewGroupBy {
    File,
    Session,
}

pub fn parse_nonempty(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() {
        Err("Expected nonempty text".into())
    } else {
        Ok(value.into())
    }
}

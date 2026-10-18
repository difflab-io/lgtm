//! GitHub transport and credential boundary; tokens never enter client contracts.
pub mod credentials;
pub mod github;

pub use github::{Github, Repository};

use super::Anchor;
use serde::{Deserialize, Serialize};

/// A complete provider observation. Current coordinates never replace original evidence.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgeObservation {
    pub base: String,
    pub head: String,
    pub outdated: bool,
    pub current: Option<Anchor>,
}

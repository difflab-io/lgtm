//! LGTM terminal client over bundled `herdr-core` (`hc`), Kitty inline image rewriter, and dynamic agent detection.
#![deny(missing_docs, unreachable_pub)]

mod client;
mod error;
mod models;
mod services;

pub use client::kitty::KittyInlineRewriter;
pub use client::osc7501::Osc7501Record;
pub use error::LgtmTerminalError;
pub use error::Result;
pub use models::agent::AgentMatcherConfig;
pub use models::agent::DetectedAgent;
pub use models::agent::default_agent_matchers;
pub use models::session::TerminalExitEvent;
pub use models::session::TerminalSession;
pub use models::session::TerminalStreamEvent;
pub use models::status::TerminalAgentStatus;
pub use services::terminal::TerminalEventCallback;
pub use services::terminal::TerminalHostEvent;
pub use services::terminal::TerminalService;
pub use services::terminal::TerminalStreamCallback;

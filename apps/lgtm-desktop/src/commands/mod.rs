//! Tauri IPC command modules grouped by domain (`workspace`, `lsp`, `config`, `session`, `project`, and `diff`).

pub mod config;
pub mod diff;
pub mod lsp;
pub mod project;
pub mod session;
pub mod workspace;

pub use config::*;
pub use diff::*;
pub use lsp::*;
pub use project::*;
pub use session::*;
pub use workspace::*;

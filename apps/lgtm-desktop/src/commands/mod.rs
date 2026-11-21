//! Tauri IPC command modules grouped by domain (`workspace`, `lsp`, `config`, `session`, and `project`).

pub mod config;
pub mod lsp;
pub mod project;
pub mod session;
pub mod workspace;

pub use config::*;
pub use lsp::*;
pub use project::*;
pub use session::*;
pub use workspace::*;

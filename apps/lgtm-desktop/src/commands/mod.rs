//! Tauri IPC command modules grouped by domain (`workspace`, `lsp`, `config`, `session`, `project`, and `terminal`).

pub mod config;
pub mod lsp;
pub mod project;
pub mod session;
pub mod terminal;
pub mod workspace;

pub use config::*;
pub use lsp::*;
pub use project::*;
pub use session::*;
pub use terminal::*;
pub use workspace::*;

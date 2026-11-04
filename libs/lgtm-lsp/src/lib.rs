//! Language Server Protocol (LSP) stdio client and workspace diagnostics orchestration.

pub mod error;
pub mod models;
pub mod services;

pub use error::{LspError, Result};
pub use models::lsp::{
    LspDiagnostic, LspDiagnosticSeverity, LspDiagnosticsBuffer, LspEvent, LspFileDiagnosticsEvent,
    LspHoverResult, LspLocation, LspServerStatus, LspServerStatusEvent, LspWorkspaceSnapshot,
    file_uri_to_path, offset_to_position, path_to_file_uri, position_to_offset,
    render_diagnostics_buffer,
};
pub use services::lsp::LspService;

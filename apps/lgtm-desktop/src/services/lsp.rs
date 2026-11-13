use std::path::Path;

use lgtm_config::ConfigService;
use lgtm_lsp::{
    LgtmLspManager, LspCompletionItem, LspDiagnosticsBuffer, LspEvent, LspHoverResult, LspLocation,
    LspServerStatusEvent, LspSignatureHelp, LspWorkspaceSnapshot,
};

use crate::error::{Result, error};

/// Desktop LSP orchestration service delegating workspace lifecycle and document operations to `LgtmLspManager`.
#[derive(Clone)]
pub struct LspService {
    manager: LgtmLspManager,
}

impl LspService {
    /// Creates an `LspService` backed by `config_service` that forwards `LspEvent` notifications to `on_event`.
    pub fn new<F>(config_service: ConfigService, on_event: F) -> Self
    where
        F: Fn(LspEvent) + Send + Sync + 'static,
    {
        Self {
            manager: LgtmLspManager::new(config_service, on_event),
        }
    }

    /// Creates a no-op `LspService` for unit tests where no language servers are spawned.
    pub fn noop(config_service: ConfigService) -> Self {
        Self {
            manager: LgtmLspManager::noop(config_service),
        }
    }

    // Public ------------------------------------------------------------------

    /// Shuts down language servers for `previous_root` when switching workspaces and warms up `next_root`.
    pub fn switch_workspace(&self, previous_root: Option<&Path>, next_root: &Path) {
        if let Some(prev) = previous_root
            && prev != next_root
        {
            self.manager.shutdown_workspace(prev);
        }
        let _ = self.manager.prepare_workspace(next_root);
    }

    /// Reloads the merged workspace configuration and re-synchronizes `.lgtm/mise.toml` after a config save.
    pub fn reload_workspace_config(&self, workspace_root: &Path) {
        let _ = self.manager.prepare_workspace(workspace_root);
    }

    /// Synchronizes an opened file with its workspace language server and returns its language ID and status.
    pub fn open_or_sync_file(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        content: &str,
    ) -> (Option<String>, Option<LspServerStatusEvent>) {
        self.manager
            .open_or_sync_file(workspace_root, file_path, content)
    }

    /// Propagates in-memory buffer edits (`textDocument/didChange`) to the active language server.
    pub fn did_change(&self, workspace_root: &Path, file_path: &Path, content: &str) -> Result<()> {
        self.manager
            .did_change(workspace_root, file_path, content)
            .map_err(|e| error("lsp", e.to_string()))
    }

    /// Propagates a saved document (`textDocument/didSave`) to the active language server.
    pub fn did_save(&self, workspace_root: &Path, file_path: &Path, content: &str) -> Result<()> {
        self.manager
            .did_save(workspace_root, file_path, content)
            .map_err(|e| error("lsp", e.to_string()))
    }

    /// Closes a document (`textDocument/didClose`) in the active language server.
    pub fn did_close(&self, workspace_root: &Path, file_path: &Path) -> Result<()> {
        self.manager
            .did_close(workspace_root, file_path)
            .map_err(|e| error("lsp", e.to_string()))
    }

    /// Requests hover documentation (`textDocument/hover`) at `offset` in `file_path`.
    pub fn get_hover(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        offset: u32,
    ) -> Result<Option<LspHoverResult>> {
        self.manager
            .get_hover(workspace_root, file_path, offset)
            .map_err(|e| error("lsp", e.to_string()))
    }

    /// Requests symbol definition locations (`textDocument/definition`) at `offset` in `file_path`.
    pub fn get_definition(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        offset: u32,
    ) -> Result<Vec<LspLocation>> {
        self.manager
            .get_definition(workspace_root, file_path, offset)
            .map_err(|e| error("lsp", e.to_string()))
    }

    /// Requests code completion candidates (`textDocument/completion`) at `offset` in `file_path`.
    pub fn get_completions(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        offset: u32,
    ) -> Result<Vec<LspCompletionItem>> {
        self.manager
            .get_completions(workspace_root, file_path, offset)
            .map_err(|e| error("lsp", e.to_string()))
    }

    /// Requests callable signature help (`textDocument/signatureHelp`) at `offset` in `file_path`.
    pub fn get_signature_help(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        offset: u32,
    ) -> Result<Option<LspSignatureHelp>> {
        self.manager
            .get_signature_help(workspace_root, file_path, offset)
            .map_err(|e| error("lsp", e.to_string()))
    }

    /// Returns the current workspace LSP server statuses and diagnostics snapshot.
    pub fn get_workspace_snapshot(&self, workspace_root: &Path) -> Result<LspWorkspaceSnapshot> {
        self.manager
            .get_workspace_snapshot(workspace_root)
            .map_err(|e| error("lsp", e.to_string()))
    }

    /// Renders a read-only diagnostics buffer for `workspace_root`, optionally filtered by `language_id`.
    pub fn get_diagnostics_buffer(
        &self,
        workspace_root: &Path,
        language_id: Option<&str>,
    ) -> Result<LspDiagnosticsBuffer> {
        self.manager
            .get_diagnostics_buffer(workspace_root, language_id)
            .map_err(|e| error("lsp", e.to_string()))
    }

    /// Restarts the language server for `language_id` in `workspace_root`.
    pub fn restart_language_server(
        &self,
        workspace_root: &Path,
        language_id: &str,
    ) -> Result<LspServerStatusEvent> {
        self.manager
            .restart_language_server(workspace_root, language_id)
            .map_err(|e| error("lsp", e.to_string()))
    }

    /// Shuts down all language server child processes across all workspaces.
    pub fn shutdown_all(&self) {
        self.manager.shutdown_all();
    }
}

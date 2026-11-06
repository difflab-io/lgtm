use std::path::{Path, PathBuf};
use std::sync::Arc;

use lgtm_config::{ConfigService, ResolvedConfigSnapshot};
use lgtm_lsp::{
    LgtmLspManager, LspDiagnosticsBuffer, LspEvent, LspHoverResult, LspLocation,
    LspServerStatusEvent, LspWorkspaceSnapshot,
};
use tauri::Emitter;

use crate::error::{Result, error};
#[cfg(any(test, feature = "e2e-bridge"))]
use crate::services::BridgeEventForwarder;

/// Desktop-scoped LSP orchestration service managing Tauri event forwarding, workspace transitions, and cleanup.
#[derive(Clone)]
pub struct LspService {
    manager: LgtmLspManager,
    config_emitter: Arc<dyn Fn(&ResolvedConfigSnapshot) + Send + Sync>,
}

impl LspService {
    /// Creates a desktop `LspService` wired to emit `lsp-status`, `lsp-diagnostics`, and `config-updated` events on `app_handle`.
    pub fn from_app_handle<R: tauri::Runtime>(
        app_handle: &tauri::AppHandle<R>,
        home: PathBuf,
        #[cfg(any(test, feature = "e2e-bridge"))] event_forwarder: Option<BridgeEventForwarder>,
    ) -> Self {
        let config_service = ConfigService::from_home(home);
        let emit_handle = app_handle.clone();
        #[cfg(any(test, feature = "e2e-bridge"))]
        let config_forwarder = event_forwarder.clone();
        let config_handle = app_handle.clone();
        let manager = LgtmLspManager::new(config_service, move |event| match event {
            LspEvent::Status(payload) => {
                #[cfg(any(test, feature = "e2e-bridge"))]
                if let Some(cb) = &event_forwarder
                    && let Ok(val) = serde_json::to_value(&payload)
                {
                    cb("lsp-status", val);
                }
                let _ = emit_handle.emit("lsp-status", payload);
            }
            LspEvent::Diagnostics(payload) => {
                #[cfg(any(test, feature = "e2e-bridge"))]
                if let Some(cb) = &event_forwarder
                    && let Ok(val) = serde_json::to_value(&payload)
                {
                    cb("lsp-diagnostics", val);
                }
                let _ = emit_handle.emit("lsp-diagnostics", payload);
            }
        });
        let config_emitter = Arc::new(move |snapshot: &ResolvedConfigSnapshot| {
            #[cfg(any(test, feature = "e2e-bridge"))]
            if let Some(cb) = &config_forwarder
                && let Ok(val) = serde_json::to_value(snapshot)
            {
                cb("config-updated", val);
            }
            let _ = config_handle.emit("config-updated", snapshot.clone());
        });
        Self {
            manager,
            config_emitter,
        }
    }

    /// Creates a no-op `LspService` for unit tests where no language servers are registered.
    pub fn noop(home: PathBuf) -> Self {
        Self {
            manager: LgtmLspManager::noop(ConfigService::from_home(home)),
            config_emitter: Arc::new(|_| {}),
        }
    }

    // Public ------------------------------------------------------------------

    /// Emits a `config-updated` event with the latest resolved configuration snapshot.
    pub fn emit_config_updated(&self, snapshot: &ResolvedConfigSnapshot) {
        (self.config_emitter)(snapshot);
    }

    /// Shuts down language servers for `previous_root` when switching workspaces and warms up `next_root`.
    pub fn switch_workspace(&self, previous_root: Option<&Path>, next_root: &Path) {
        if let Some(prev) = previous_root
            && prev != next_root
        {
            self.manager.shutdown_workspace(prev);
        }
        let _ = self.manager.prepare_workspace(next_root);
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

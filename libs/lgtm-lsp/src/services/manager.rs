use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use lgtm_config::{ConfigFormat, ConfigService, LanguageConfig, UserSettingsPatch};
use lgtm_env::MiseService;
use lsp_types::{
    DiagnosticSeverity, GotoDefinitionResponse, Hover, HoverContents, MarkedString, NumberOrString,
    PublishDiagnosticsParams, Range,
};
use serde_json::{Value, json};

use crate::error::{LspError, Result};
use crate::models::cache::{LspCachedDocument, LspDocumentSyncAction, LspSessionKey};
use crate::models::lsp::{
    LspDiagnostic, LspDiagnosticSeverity, LspDiagnosticsBuffer, LspEvent, LspHoverResult,
    LspLocation, LspServerStatus, LspServerStatusEvent, LspWorkspaceSnapshot, file_uri_to_path,
    offset_to_position, path_to_file_uri, position_to_offset,
};
use crate::services::cache::LspCacheService;

type EventSink = Arc<dyn Fn(LspEvent) + Send + Sync>;

struct LspServerSession {
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    next_id: AtomicU64,
    pending: Arc<Mutex<BTreeMap<u64, mpsc::Sender<Result<Value>>>>>,
    timeout: Duration,
}

/// Handle to a running language server session managed by [`LgtmLspManager`].
#[derive(Clone)]
pub struct LspClientHandle {
    key: LspSessionKey,
    session: Arc<LspServerSession>,
}

impl LspClientHandle {
    fn new(key: LspSessionKey, session: Arc<LspServerSession>) -> Self {
        Self { key, session }
    }

    // Public ------------------------------------------------------------------

    /// Returns the composite session key `(workspace_root, language_id, project_root)` for this server.
    pub fn get_key(&self) -> &LspSessionKey {
        &self.key
    }

    /// Sends a JSON-RPC 2.0 request to the language server and waits for its response within the configured timeout.
    pub fn request(&self, method: &str, params: Value) -> Result<Value> {
        self.session.request(method, params)
    }

    /// Sends a one-way JSON-RPC 2.0 notification to the language server.
    pub fn notify(&self, method: &str, params: Value) -> Result<()> {
        self.session.notify(method, params)
    }

    /// Gracefully shuts down and terminates the underlying language server child process.
    pub fn shutdown(&self) {
        self.session.shutdown();
    }
}

/// Manages language server processes, handles, document synchronization, and workspace diagnostics over stdio JSON-RPC 2.0.
#[derive(Clone)]
pub struct LgtmLspManager {
    config_service: ConfigService,
    mise_service: MiseService,
    cache_service: LspCacheService,
    event_sink: EventSink,
    sessions: Arc<Mutex<BTreeMap<LspSessionKey, LspClientHandle>>>,
    init_locks: Arc<Mutex<BTreeMap<LspSessionKey, Arc<Mutex<()>>>>>,
    disabled: bool,
}

impl LgtmLspManager {
    /// Creates a new `LgtmLspManager` with a fresh [`LspCacheService`] and the given `LspEvent` callback.
    pub fn new<F>(config_service: ConfigService, event_sink: F) -> Self
    where
        F: Fn(LspEvent) + Send + Sync + 'static,
    {
        Self::with_cache(config_service, LspCacheService::new(), event_sink)
    }

    /// Creates a new `LgtmLspManager` backed by an explicit [`LspCacheService`] and `LspEvent` callback.
    pub fn with_cache<F>(
        config_service: ConfigService,
        cache_service: LspCacheService,
        event_sink: F,
    ) -> Self
    where
        F: Fn(LspEvent) + Send + Sync + 'static,
    {
        let mise_service = MiseService::new(config_service.clone());
        Self {
            config_service,
            mise_service,
            cache_service,
            event_sink: Arc::new(event_sink),
            sessions: Arc::new(Mutex::new(BTreeMap::new())),
            init_locks: Arc::new(Mutex::new(BTreeMap::new())),
            disabled: false,
        }
    }

    /// Creates a no-op `LgtmLspManager` that records no servers and never spawns child processes.
    pub fn noop(config_service: ConfigService) -> Self {
        let mut manager = Self::new(config_service, |_| {});
        manager.disabled = true;
        manager
    }

    // Public ------------------------------------------------------------------

    /// Returns a reference to the underlying [`LspCacheService`].
    pub fn get_cache_service(&self) -> &LspCacheService {
        &self.cache_service
    }

    /// Returns the active [`LspClientHandle`] for `key`, if a session is currently running.
    pub fn get_server(&self, key: &LspSessionKey) -> Option<LspClientHandle> {
        self.sessions.lock().unwrap().get(key).cloned()
    }

    /// Returns an existing or newly initialized [`LspClientHandle`] for `(workspace_root, language_id, project_root)`.
    pub fn get_or_start_server(
        &self,
        workspace_root: &Path,
        project_root: &Path,
        language_id: &str,
    ) -> Result<Option<LspClientHandle>> {
        if self.disabled {
            return Ok(None);
        }
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let project_root = Self::canonical_or_owned(project_root);
        let config = self
            .cache_service
            .get_or_load_workspace_config(&self.config_service, &workspace_root)?;
        if !config.lsp.enabled {
            return Ok(None);
        }
        let Some(language_config) = config.languages.get(language_id).cloned() else {
            return Ok(None);
        };
        if !language_config.enabled {
            return Ok(None);
        }
        Ok(self.ensure_session(
            &workspace_root,
            &project_root,
            language_id,
            &language_config,
            config.lsp.auto_install,
            Duration::from_millis(config.lsp.request_timeout_ms),
        ))
    }

    /// Discovers workspace languages, caches merged settings, synchronizes `.lgtm/mise.toml`, and warms up language servers.
    pub fn prepare_workspace(&self, workspace_root: &Path) -> Result<()> {
        if self.disabled {
            return Ok(());
        }
        let canonical = Self::canonical_or_owned(workspace_root);
        let config = self.config_service.get_config(&canonical)?;
        let detected = ConfigService::detect_workspace_languages(&config, &canonical);
        self.cache_service
            .set_workspace_config(&canonical, config.clone());
        if !detected.is_empty() || ConfigService::get_project_config_path(&canonical).is_file() {
            let _ = self.mise_service.sync_workspace_mise(&canonical)?;
            let _ = self
                .cache_service
                .write_discovery_cache(&canonical, &detected, &config);
        }
        if config.lsp.enabled && !detected.is_empty() {
            let timeout = Duration::from_millis(config.lsp.request_timeout_ms);
            let auto_install = config.lsp.auto_install;
            for language_id in detected {
                let Some(lang_cfg) = config.languages.get(&language_id).cloned() else {
                    continue;
                };
                if !lang_cfg.enabled {
                    continue;
                }
                let key = LspSessionKey::new(&canonical, &language_id, &canonical);
                let already_known = self.sessions.lock().unwrap().contains_key(&key)
                    || self.cache_service.get_status(&key).is_some();
                if !already_known {
                    let starting_event = self.cache_service.set_status(
                        key,
                        LspServerStatusEvent {
                            workspace_root: canonical.to_string_lossy().to_string(),
                            language_id: language_id.clone(),
                            project_root: canonical.to_string_lossy().to_string(),
                            status: LspServerStatus::Starting,
                            command: lang_cfg.command.clone(),
                            message: None,
                            error_count: 0,
                            warning_count: 0,
                        },
                    );
                    (self.event_sink)(LspEvent::Status(starting_event));
                }
                let manager = self.clone();
                let ws_root = canonical.clone();
                std::thread::spawn(move || {
                    manager.warmup_workspace_language(
                        &ws_root,
                        &language_id,
                        &lang_cfg,
                        auto_install,
                        timeout,
                    );
                });
            }
        }
        Ok(())
    }

    /// Opens or updates `file_path` in the appropriate language server and returns its language ID and server status.
    pub fn open_or_sync_file(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        content: &str,
    ) -> (Option<String>, Option<LspServerStatusEvent>) {
        if self.disabled {
            return (None, None);
        }
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let file_path = Self::canonical_or_owned(file_path);

        let config = self
            .cache_service
            .get_or_load_workspace_config(&self.config_service, &workspace_root)
            .unwrap_or_default();
        let Some((language_id, lang_cfg)) = config.detect_file_language(&file_path) else {
            return (None, None);
        };
        let language_config = lang_cfg.clone();

        if !config.lsp.enabled {
            return (Some(language_id), None);
        }

        let (project_root, _) =
            ConfigService::detect_project_root(&workspace_root, &file_path, &language_config);
        let project_root = Self::canonical_or_owned(&project_root);
        let key = LspSessionKey::new(&workspace_root, &language_id, &project_root);

        let is_config_doc = self.is_lgtm_config_file(&file_path);
        let handle = self.ensure_session(
            &workspace_root,
            &project_root,
            &language_id,
            &language_config,
            config.lsp.auto_install,
            Duration::from_millis(config.lsp.request_timeout_ms),
        );

        let sync_action = self.cache_service.sync_document(
            &workspace_root,
            &project_root,
            &file_path,
            &language_id,
            content,
        );

        if let Some(handle) = handle {
            if let Some(uri) = path_to_file_uri(&file_path) {
                match sync_action {
                    LspDocumentSyncAction::Opened(doc) => {
                        let _ = handle.notify(
                            "textDocument/didOpen",
                            json!({
                                "textDocument": {
                                    "uri": uri,
                                    "languageId": doc.language_id,
                                    "version": doc.version,
                                    "text": doc.text,
                                }
                            }),
                        );
                    }
                    LspDocumentSyncAction::Changed(doc) => {
                        let _ = handle.notify(
                            "textDocument/didChange",
                            json!({
                                "textDocument": {
                                    "uri": uri,
                                    "version": doc.version,
                                },
                                "contentChanges": [{ "text": doc.text }],
                            }),
                        );
                    }
                    LspDocumentSyncAction::Unchanged(_) => {}
                }
            }
        } else if is_config_doc {
            let ready_event = self.cache_service.set_status(
                key.clone(),
                LspServerStatusEvent {
                    workspace_root: workspace_root.to_string_lossy().to_string(),
                    language_id: language_id.clone(),
                    project_root: project_root.to_string_lossy().to_string(),
                    status: LspServerStatus::Ready,
                    command: language_config.command.clone(),
                    message: None,
                    error_count: 0,
                    warning_count: 0,
                },
            );
            (self.event_sink)(LspEvent::Status(ready_event));
        }

        if is_config_doc {
            self.publish_config_schema_diagnostics(&key, &file_path, content);
        } else if let Some(diag_event) = self
            .cache_service
            .get_file_diagnostics(&workspace_root, &file_path)
        {
            (self.event_sink)(LspEvent::Diagnostics(diag_event));
        }

        let status = self.cache_service.get_status(&key);
        (Some(language_id), status)
    }

    /// Sends a `textDocument/didChange` notification with updated buffer text for `file_path`.
    pub fn did_change(&self, workspace_root: &Path, file_path: &Path, content: &str) -> Result<()> {
        if self.disabled {
            return Ok(());
        }
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let file_path = Self::canonical_or_owned(file_path);

        if let Some(sync_action) = self.cache_service.update_open_document(&file_path, content) {
            match sync_action {
                LspDocumentSyncAction::Changed(doc) => {
                    let key = LspSessionKey::new(
                        &doc.workspace_root,
                        &doc.language_id,
                        &doc.project_root,
                    );
                    if let (Some(handle), Some(uri)) =
                        (self.get_server(&key), path_to_file_uri(&file_path))
                    {
                        handle.notify(
                            "textDocument/didChange",
                            json!({
                                "textDocument": {
                                    "uri": uri,
                                    "version": doc.version,
                                },
                                "contentChanges": [{ "text": content }],
                            }),
                        )?;
                    }
                    if self.is_lgtm_config_file(&file_path) {
                        self.publish_config_schema_diagnostics(&key, &file_path, content);
                    }
                }
                LspDocumentSyncAction::Unchanged(_) | LspDocumentSyncAction::Opened(_) => {}
            }
            return Ok(());
        }

        let _ = self.open_or_sync_file(&workspace_root, &file_path, content);
        Ok(())
    }

    /// Synchronizes buffer changes and sends a `textDocument/didSave` notification for `file_path`.
    pub fn did_save(&self, workspace_root: &Path, file_path: &Path, content: &str) -> Result<()> {
        if self.disabled {
            return Ok(());
        }
        self.did_change(workspace_root, file_path, content)?;
        let file_path = Self::canonical_or_owned(file_path);
        if let Some(doc) = self.cache_service.get_document(&file_path) {
            let key = LspSessionKey::new(&doc.workspace_root, &doc.language_id, &doc.project_root);
            if let (Some(handle), Some(uri)) = (self.get_server(&key), path_to_file_uri(&file_path))
            {
                handle.notify(
                    "textDocument/didSave",
                    json!({
                        "textDocument": { "uri": uri },
                        "text": content,
                    }),
                )?;
            }
        }
        Ok(())
    }

    /// Removes `file_path` from tracked open documents and sends `textDocument/didClose` to its language server.
    pub fn did_close(&self, _workspace_root: &Path, file_path: &Path) -> Result<()> {
        if self.disabled {
            return Ok(());
        }
        let file_path = Self::canonical_or_owned(file_path);
        if let Some(doc) = self.cache_service.remove_document(&file_path) {
            let key = LspSessionKey::new(&doc.workspace_root, &doc.language_id, &doc.project_root);
            if let (Some(handle), Some(uri)) = (self.get_server(&key), path_to_file_uri(&file_path))
            {
                let _ = handle.notify(
                    "textDocument/didClose",
                    json!({
                        "textDocument": { "uri": uri },
                    }),
                );
            }
        }
        Ok(())
    }

    /// Requests hover documentation (`textDocument/hover`) at the given UTF-16 `offset` in `file_path`.
    pub fn get_hover(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        offset: u32,
    ) -> Result<Option<LspHoverResult>> {
        if self.disabled {
            return Ok(None);
        }
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let file_path = Self::canonical_or_owned(file_path);
        if self.is_lgtm_config_file(&file_path) {
            let text = self.cache_service.get_document_or_disk_text(&file_path);
            if let Some(schema_hover) = Self::compute_config_schema_hover(&text, offset) {
                return Ok(Some(schema_hover));
            }
        }
        let Some((handle, text)) = self.resolve_file_session(&workspace_root, &file_path) else {
            return Ok(None);
        };
        let Some(uri) = path_to_file_uri(&file_path) else {
            return Ok(None);
        };
        let (line, character) = offset_to_position(&text, offset);
        let response = handle.request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
            }),
        )?;

        if response.is_null() {
            return Ok(None);
        }

        let hover: Hover = serde_json::from_value(response)?;
        let contents = Self::format_hover_contents(&hover.contents);
        if contents.trim().is_empty() {
            return Ok(None);
        }

        let (from, to) = match hover.range {
            Some(range) => {
                let (_, _, _, _, start_offset, end_offset) = Self::map_lsp_range(&text, &range);
                (Some(start_offset), Some(end_offset))
            }
            None => (None, None),
        };
        Ok(Some(LspHoverResult { contents, from, to }))
    }

    /// Requests symbol definition locations (`textDocument/definition`) at the given UTF-16 `offset` in `file_path`.
    pub fn get_definition(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        offset: u32,
    ) -> Result<Vec<LspLocation>> {
        if self.disabled {
            return Ok(Vec::new());
        }
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let file_path = Self::canonical_or_owned(file_path);
        let Some((handle, text)) = self.resolve_file_session(&workspace_root, &file_path) else {
            return Ok(Vec::new());
        };
        let Some(uri) = path_to_file_uri(&file_path) else {
            return Ok(Vec::new());
        };
        let (line, character) = offset_to_position(&text, offset);
        let response = handle.request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
            }),
        )?;

        if response.is_null() {
            return Ok(Vec::new());
        }

        let parsed: GotoDefinitionResponse = serde_json::from_value(response)?;
        Ok(self.parse_definition_response(&workspace_root, parsed))
    }

    /// Returns a snapshot of all tracked language servers and diagnostics for `workspace_root`.
    pub fn get_workspace_snapshot(&self, workspace_root: &Path) -> Result<LspWorkspaceSnapshot> {
        Ok(self.cache_service.get_workspace_snapshot(workspace_root))
    }

    /// Renders a read-only diagnostics buffer for `workspace_root`, optionally filtered to `language_id`.
    pub fn get_diagnostics_buffer(
        &self,
        workspace_root: &Path,
        language_id: Option<&str>,
    ) -> Result<LspDiagnosticsBuffer> {
        Ok(self
            .cache_service
            .get_diagnostics_buffer(workspace_root, language_id))
    }

    /// Restarts all sessions for `language_id` in `workspace_root` and re-synchronizes open documents.
    pub fn restart_language_server(
        &self,
        workspace_root: &Path,
        language_id: &str,
    ) -> Result<LspServerStatusEvent> {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let config = self.config_service.get_config(&workspace_root)?;
        self.cache_service
            .set_workspace_config(&workspace_root, config.clone());
        let Some(language_config) = config.languages.get(language_id).cloned() else {
            return Err(LspError::ServerUnavailable(format!(
                "unknown language `{language_id}`"
            )));
        };

        let status_keys = self
            .cache_service
            .remove_statuses_for_language(&workspace_root, language_id);
        let (old_handles, mut project_roots) = {
            let mut guard = self.sessions.lock().unwrap();
            let session_keys: Vec<LspSessionKey> = guard
                .keys()
                .filter(|k| k.workspace_root == workspace_root && k.language_id == language_id)
                .cloned()
                .collect();
            let mut roots = Vec::new();
            let mut removed = Vec::new();
            for key in session_keys.into_iter().chain(status_keys) {
                if !roots.contains(&key.project_root) {
                    roots.push(key.project_root.clone());
                }
                if let Some(handle) = guard.remove(&key) {
                    removed.push(handle);
                }
            }
            (removed, roots)
        };

        if project_roots.is_empty() {
            project_roots.push(workspace_root.clone());
        }

        for handle in old_handles {
            handle.shutdown();
        }

        let open_docs: Vec<(PathBuf, LspCachedDocument)> = self
            .cache_service
            .get_documents_for_language(&workspace_root, language_id);
        let timeout = Duration::from_millis(config.lsp.request_timeout_ms);
        let mut latest_status = None;

        for project_root in project_roots {
            let handle = self.ensure_session(
                &workspace_root,
                &project_root,
                language_id,
                &language_config,
                config.lsp.auto_install,
                timeout,
            );
            if let Some(handle) = handle {
                for (path, doc) in &open_docs {
                    if doc.project_root == project_root
                        && let Some(uri) = path_to_file_uri(path)
                    {
                        let _ = handle.notify(
                            "textDocument/didOpen",
                            json!({
                                "textDocument": {
                                    "uri": uri,
                                    "languageId": doc.language_id,
                                    "version": doc.version,
                                    "text": doc.text,
                                }
                            }),
                        );
                    }
                }
            }
            let key = LspSessionKey::new(&workspace_root, language_id, &project_root);
            latest_status = self.cache_service.get_status(&key);
        }

        latest_status.ok_or_else(|| {
            LspError::ServerUnavailable(format!(
                "failed to restart `{language_id}` language server"
            ))
        })
    }

    /// Shuts down all language server processes associated with `workspace_root`.
    pub fn shutdown_workspace(&self, workspace_root: &Path) {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        self.cache_service.clear_workspace(&workspace_root);
        let handles: Vec<LspClientHandle> = {
            let mut guard = self.sessions.lock().unwrap();
            let keys: Vec<LspSessionKey> = guard
                .keys()
                .filter(|key| key.workspace_root == workspace_root)
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|key| guard.remove(&key))
                .collect()
        };
        for handle in handles {
            handle.shutdown();
        }
    }

    /// Shuts down all active language server processes across all workspaces.
    pub fn shutdown_all(&self) {
        self.cache_service.clear_all();
        let handles: Vec<LspClientHandle> = {
            let mut guard = self.sessions.lock().unwrap();
            std::mem::take(&mut *guard).into_values().collect()
        };
        for handle in handles {
            handle.shutdown();
        }
    }

    // Private -----------------------------------------------------------------

    fn canonical_or_owned(path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn to_workspace_relative(workspace_root: &Path, path: &Path) -> String {
        path.strip_prefix(workspace_root)
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string_lossy().to_string())
    }

    fn warmup_workspace_language(
        &self,
        workspace_root: &Path,
        language_id: &str,
        language_config: &LanguageConfig,
        auto_install: bool,
        timeout: Duration,
    ) {
        let Some(_handle) = self.ensure_session(
            workspace_root,
            workspace_root,
            language_id,
            language_config,
            auto_install,
            timeout,
        ) else {
            return;
        };

        let mut candidates = Vec::new();
        Self::collect_language_files(
            workspace_root,
            &language_config.extensions,
            &mut candidates,
            1,
            8,
        );
        for file_path in candidates {
            if self.cache_service.get_document(&file_path).is_some() {
                continue;
            }
            if let Ok(content) = std::fs::read_to_string(&file_path) {
                let _ = self.open_or_sync_file(workspace_root, &file_path, &content);
            }
        }
    }

    fn collect_language_files(
        dir: &Path,
        extensions: &[String],
        out: &mut Vec<PathBuf>,
        remaining_depth: usize,
        limit: usize,
    ) {
        if out.len() >= limit {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if out.len() >= limit {
                break;
            }
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_ascii_lowercase();
                    if extensions.iter().any(|candidate| candidate == &ext_lower) {
                        out.push(Self::canonical_or_owned(&path));
                    }
                }
            } else if remaining_depth > 0 && path.is_dir() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str.starts_with('.') || name_str == "node_modules" || name_str == "target" {
                    continue;
                }
                Self::collect_language_files(&path, extensions, out, remaining_depth - 1, limit);
            }
        }
    }

    fn ensure_session(
        &self,
        workspace_root: &Path,
        project_root: &Path,
        language_id: &str,
        language_config: &LanguageConfig,
        auto_install: bool,
        timeout: Duration,
    ) -> Option<LspClientHandle> {
        let key = LspSessionKey::new(workspace_root, language_id, project_root);

        if let Some(existing) = self.get_server(&key) {
            return Some(existing);
        }

        let key_lock = {
            let mut locks = self.init_locks.lock().unwrap();
            locks
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _init_guard = key_lock.lock().unwrap();

        if let Some(existing) = self.get_server(&key) {
            return Some(existing);
        }
        if let Some(status) = self.cache_service.get_status(&key)
            && status.status == LspServerStatus::Unavailable
            && status.command == language_config.command
        {
            return None;
        }

        let starting_event = self.cache_service.set_status(
            key.clone(),
            LspServerStatusEvent {
                workspace_root: workspace_root.to_string_lossy().to_string(),
                language_id: language_id.to_string(),
                project_root: project_root.to_string_lossy().to_string(),
                status: LspServerStatus::Starting,
                command: language_config.command.clone(),
                message: None,
                error_count: 0,
                warning_count: 0,
            },
        );
        (self.event_sink)(LspEvent::Status(starting_event));

        let _ = self.mise_service.sync_workspace_mise(workspace_root);

        match self.spawn_and_initialize_session(
            workspace_root,
            project_root,
            language_id,
            language_config,
            auto_install,
            timeout,
        ) {
            Ok(session) => {
                let handle = LspClientHandle::new(key.clone(), session);
                self.sessions
                    .lock()
                    .unwrap()
                    .insert(key.clone(), handle.clone());
                let ready_event = self.cache_service.set_status(
                    key,
                    LspServerStatusEvent {
                        workspace_root: workspace_root.to_string_lossy().to_string(),
                        language_id: language_id.to_string(),
                        project_root: project_root.to_string_lossy().to_string(),
                        status: LspServerStatus::Ready,
                        command: language_config.command.clone(),
                        message: None,
                        error_count: 0,
                        warning_count: 0,
                    },
                );
                (self.event_sink)(LspEvent::Status(ready_event));
                Some(handle)
            }
            Err(err) => {
                let unavailable_event = self.cache_service.set_status(
                    key,
                    LspServerStatusEvent {
                        workspace_root: workspace_root.to_string_lossy().to_string(),
                        language_id: language_id.to_string(),
                        project_root: project_root.to_string_lossy().to_string(),
                        status: LspServerStatus::Unavailable,
                        command: language_config.command.clone(),
                        message: Some(err.to_string()),
                        error_count: 0,
                        warning_count: 0,
                    },
                );
                (self.event_sink)(LspEvent::Status(unavailable_event));
                None
            }
        }
    }

    fn spawn_and_initialize_session(
        &self,
        workspace_root: &Path,
        project_root: &Path,
        language_id: &str,
        language_config: &LanguageConfig,
        auto_install: bool,
        timeout: Duration,
    ) -> Result<Arc<LspServerSession>> {
        let prefer_direct =
            language_config.mise_tools.is_empty() || Path::new(&language_config.command).is_file();

        if prefer_direct {
            return self.try_spawn_session(
                workspace_root,
                project_root,
                language_id,
                language_config,
                timeout,
                false,
                false,
            );
        }

        if let Ok(session) = self.try_spawn_session(
            workspace_root,
            project_root,
            language_id,
            language_config,
            timeout,
            true,
            auto_install,
        ) {
            return Ok(session);
        }

        if auto_install {
            let tools: Vec<(&str, &str)> = language_config
                .mise_tools
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect();
            let _ = self.mise_service.install_tools(workspace_root, &tools);
            if let Ok(session) = self.try_spawn_session(
                workspace_root,
                project_root,
                language_id,
                language_config,
                timeout,
                true,
                true,
            ) {
                return Ok(session);
            }
        }

        self.try_spawn_session(
            workspace_root,
            project_root,
            language_id,
            language_config,
            timeout,
            false,
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn try_spawn_session(
        &self,
        workspace_root: &Path,
        project_root: &Path,
        language_id: &str,
        language_config: &LanguageConfig,
        timeout: Duration,
        use_mise_exec: bool,
        auto_install_mise: bool,
    ) -> Result<Arc<LspServerSession>> {
        let mut command = if use_mise_exec {
            self.mise_service
                .build_exec_command(
                    workspace_root,
                    project_root,
                    &language_config.mise_tools,
                    &language_config.command,
                    &language_config.args,
                    auto_install_mise,
                )
                .map_err(|err| LspError::ServerUnavailable(err.to_string()))?
        } else {
            let mut cmd = Command::new(&language_config.command);
            cmd.args(&language_config.args);
            cmd
        };

        command
            .current_dir(project_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        for (env_key, env_val) in &language_config.env {
            command.env(env_key, env_val);
        }

        let mut child = command.spawn().map_err(|err| {
            LspError::ServerUnavailable(format!(
                "failed to launch `{}`: {err}",
                language_config.command
            ))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| LspError::Protocol("missing child stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LspError::Protocol("missing child stdout".to_string()))?;

        let pending: Arc<Mutex<BTreeMap<u64, mpsc::Sender<Result<Value>>>>> =
            Arc::new(Mutex::new(BTreeMap::new()));

        let session = Arc::new(LspServerSession {
            stdin: Mutex::new(Some(stdin)),
            child: Mutex::new(Some(child)),
            next_id: AtomicU64::new(1),
            pending,
            timeout,
        });

        let session_key = LspSessionKey::new(workspace_root, language_id, project_root);
        Self::spawn_reader_thread(
            stdout,
            session.clone(),
            self.cache_service.clone(),
            self.event_sink.clone(),
            session_key,
        );

        let root_uri = path_to_file_uri(project_root);
        let init_result = session.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "rootUri": root_uri,
                "initializationOptions": language_config.initialization_options,
                "workspaceFolders": root_uri.as_ref().map(|uri| vec![json!({
                    "uri": uri,
                    "name": project_root
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| "workspace".to_string()),
                })]),
                "capabilities": {
                    "textDocument": {
                        "synchronization": {
                            "dynamicRegistration": false,
                            "willSave": false,
                            "didSave": true,
                        },
                        "hover": {
                            "contentFormat": ["markdown", "plaintext"],
                        },
                        "definition": {
                            "linkSupport": true,
                        },
                        "publishDiagnostics": {
                            "relatedInformation": true,
                        },
                    }
                }
            }),
        );

        if let Err(err) = init_result {
            session.shutdown();
            return Err(err);
        }

        session.notify("initialized", json!({}))?;
        Ok(session)
    }

    fn spawn_reader_thread(
        stdout: std::process::ChildStdout,
        session: Arc<LspServerSession>,
        cache_service: LspCacheService,
        event_sink: EventSink,
        session_key: LspSessionKey,
    ) {
        std::thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            while let Ok(Some(message)) = Self::read_rpc_message(&mut reader) {
                if let Some(id) = message.get("id") {
                    if message.get("method").is_some() {
                        let _ = session.send_raw(&json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": Value::Null,
                        }));
                        continue;
                    }
                    if let Some(id_num) = id.as_u64() {
                        let sender = session.pending.lock().unwrap().remove(&id_num);
                        if let Some(tx) = sender {
                            if let Some(err) = message.get("error") {
                                let msg = err
                                    .get("message")
                                    .and_then(Value::as_str)
                                    .unwrap_or("JSON-RPC error");
                                let _ = tx.send(Err(LspError::Protocol(msg.to_string())));
                            } else {
                                let result = message.get("result").cloned().unwrap_or(Value::Null);
                                let _ = tx.send(Ok(result));
                            }
                        }
                    }
                    continue;
                }

                if message.get("method").and_then(Value::as_str)
                    == Some("textDocument/publishDiagnostics")
                    && let Some(params) = message.get("params")
                {
                    Self::handle_publish_diagnostics(
                        params,
                        &cache_service,
                        &event_sink,
                        &session_key,
                    );
                }
            }

            let pending_map = {
                let mut guard = session.pending.lock().unwrap();
                std::mem::take(&mut *guard)
            };
            for (_, tx) in pending_map {
                let _ = tx.send(Err(LspError::ServerUnavailable(
                    "LSP server stream closed".to_string(),
                )));
            }
        });
    }

    fn read_rpc_message<R: BufRead>(reader: &mut R) -> Result<Option<Value>> {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let bytes = reader.read_line(&mut line)?;
            if bytes == 0 {
                return Ok(None);
            }
            let trimmed = line.trim();
            if trimmed.is_empty() {
                break;
            }
            if let Some(rest) = trimmed.strip_prefix("Content-Length:")
                && let Ok(len) = rest.trim().parse::<usize>()
            {
                content_length = Some(len);
            }
        }

        let Some(len) = content_length else {
            return Ok(None);
        };

        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;
        let value: Value = serde_json::from_slice(&buf)?;
        Ok(Some(value))
    }

    fn handle_publish_diagnostics(
        params: &Value,
        cache_service: &LspCacheService,
        event_sink: &EventSink,
        session_key: &LspSessionKey,
    ) {
        let Ok(published) = serde_json::from_value::<PublishDiagnosticsParams>(params.clone())
        else {
            return;
        };
        let Some(raw_file_path) = file_uri_to_path(published.uri.as_str()) else {
            return;
        };
        let file_path = Self::canonical_or_owned(&raw_file_path);
        let text = cache_service.get_document_or_disk_text(&file_path);

        let mut mapped: Vec<LspDiagnostic> = published
            .diagnostics
            .into_iter()
            .map(|item| {
                let (line, character, end_line, end_character, from, to) =
                    Self::map_lsp_range(&text, &item.range);
                let severity = match item.severity {
                    Some(DiagnosticSeverity::ERROR) | None => LspDiagnosticSeverity::Error,
                    Some(DiagnosticSeverity::WARNING) => LspDiagnosticSeverity::Warning,
                    Some(DiagnosticSeverity::INFORMATION) => LspDiagnosticSeverity::Info,
                    Some(DiagnosticSeverity::HINT) => LspDiagnosticSeverity::Hint,
                    Some(_) => LspDiagnosticSeverity::Error,
                };
                let code = item.code.map(|c| match c {
                    NumberOrString::Number(n) => n.to_string(),
                    NumberOrString::String(s) => s,
                });
                LspDiagnostic {
                    from,
                    to,
                    line,
                    character,
                    end_line,
                    end_character,
                    severity,
                    message: item.message,
                    source: item.source,
                    code,
                }
            })
            .collect();

        if Self::is_config_file_name(&file_path) {
            for diag in Self::compute_config_schema_diagnostics(&file_path, &text) {
                if !mapped
                    .iter()
                    .any(|existing| existing.line == diag.line && existing.message == diag.message)
                {
                    mapped.push(diag);
                }
            }
        }

        let (file_event, updated_status) =
            cache_service.set_file_diagnostics(session_key, &file_path, mapped);
        (event_sink)(LspEvent::Diagnostics(file_event));
        if let Some(status_event) = updated_status {
            (event_sink)(LspEvent::Status(status_event));
        }
    }

    fn resolve_file_session(
        &self,
        workspace_root: &Path,
        file_path: &Path,
    ) -> Option<(LspClientHandle, String)> {
        if let Some(doc) = self.cache_service.get_document(file_path) {
            let key = LspSessionKey::new(&doc.workspace_root, &doc.language_id, &doc.project_root);
            if let Some(handle) = self.get_server(&key) {
                return Some((handle, doc.text));
            }
        }
        let content = std::fs::read_to_string(file_path).ok()?;
        let _ = self.open_or_sync_file(workspace_root, file_path, &content);
        let doc = self.cache_service.get_document(file_path)?;
        let key = LspSessionKey::new(&doc.workspace_root, &doc.language_id, &doc.project_root);
        let handle = self.get_server(&key)?;
        Some((handle, doc.text))
    }

    fn format_hover_contents(contents: &HoverContents) -> String {
        match contents {
            HoverContents::Scalar(marked) => Self::format_marked_string(marked),
            HoverContents::Array(items) => items
                .iter()
                .map(Self::format_marked_string)
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n"),
            HoverContents::Markup(markup) => markup.value.clone(),
        }
    }

    fn format_marked_string(marked: &MarkedString) -> String {
        match marked {
            MarkedString::String(s) => s.clone(),
            MarkedString::LanguageString(ls) => format!("```{}\n{}\n```", ls.language, ls.value),
        }
    }

    fn map_lsp_range(text: &str, range: &Range) -> (u32, u32, u32, u32, u32, u32) {
        let line = range.start.line;
        let character = range.start.character;
        let end_line = range.end.line;
        let end_character = range.end.character;
        let from = position_to_offset(text, line, character);
        let to = position_to_offset(text, end_line, end_character).max(from);
        (line, character, end_line, end_character, from, to)
    }

    fn parse_definition_response(
        &self,
        workspace_root: &Path,
        response: GotoDefinitionResponse,
    ) -> Vec<LspLocation> {
        match response {
            GotoDefinitionResponse::Scalar(loc) => self
                .map_location(workspace_root, loc.uri.as_str(), &loc.range)
                .into_iter()
                .collect(),
            GotoDefinitionResponse::Array(locs) => locs
                .into_iter()
                .filter_map(|loc| self.map_location(workspace_root, loc.uri.as_str(), &loc.range))
                .collect(),
            GotoDefinitionResponse::Link(links) => links
                .into_iter()
                .filter_map(|link| {
                    self.map_location(
                        workspace_root,
                        link.target_uri.as_str(),
                        &link.target_selection_range,
                    )
                })
                .collect(),
        }
    }

    fn map_location(&self, workspace_root: &Path, uri: &str, range: &Range) -> Option<LspLocation> {
        let raw_target_path = file_uri_to_path(uri)?;
        let target_path = Self::canonical_or_owned(&raw_target_path);
        let target_text = self.cache_service.get_document_or_disk_text(&target_path);
        let (line, character, _, _, offset, end_offset) = Self::map_lsp_range(&target_text, range);
        Some(LspLocation {
            path: Self::to_workspace_relative(workspace_root, &target_path),
            offset,
            end_offset,
            line,
            character,
        })
    }

    fn is_lgtm_config_file(&self, file_path: &Path) -> bool {
        if Self::is_config_file_name(file_path) {
            return true;
        }
        let global = Self::canonical_or_owned(&self.config_service.get_global_config_path());
        file_path == global
    }

    fn is_config_file_name(file_path: &Path) -> bool {
        let Some(name) = file_path.file_name().and_then(|n| n.to_str()) else {
            return false;
        };
        if !matches!(name, "config.json" | "config.yaml" | "config.yml") {
            return false;
        }
        file_path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .is_some_and(|parent| parent == ".lgtm")
    }

    fn compute_config_schema_diagnostics(file_path: &Path, text: &str) -> Vec<LspDiagnostic> {
        let format = ConfigFormat::from_path(file_path);
        UserSettingsPatch::validate_document_text(format, text)
            .into_iter()
            .map(|issue| {
                let from = position_to_offset(text, issue.line, issue.character);
                let to = position_to_offset(text, issue.line, issue.end_character).max(from + 1);
                LspDiagnostic {
                    from,
                    to,
                    line: issue.line,
                    character: issue.character,
                    end_line: issue.line,
                    end_character: issue.end_character,
                    severity: LspDiagnosticSeverity::Error,
                    message: issue.message,
                    source: Some("lgtm-config".to_string()),
                    code: Some(issue.code),
                }
            })
            .collect()
    }

    fn publish_config_schema_diagnostics(
        &self,
        key: &LspSessionKey,
        file_path: &Path,
        content: &str,
    ) {
        let schema_diags = Self::compute_config_schema_diagnostics(file_path, content);
        let (file_event, updated_status) =
            self.cache_service
                .set_file_diagnostics(key, file_path, schema_diags);
        (self.event_sink)(LspEvent::Diagnostics(file_event));
        if let Some(status_event) = updated_status {
            (self.event_sink)(LspEvent::Status(status_event));
        }
    }

    fn compute_config_schema_hover(text: &str, offset: u32) -> Option<LspHoverResult> {
        let units: Vec<(u32, char)> = {
            let mut pos = 0u32;
            let mut out = Vec::new();
            for ch in text.chars() {
                out.push((pos, ch));
                pos += ch.len_utf16() as u32;
            }
            out
        };
        if units.is_empty() {
            return None;
        }
        let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '$' || c == '_';
        let mut idx = units
            .iter()
            .position(|(pos, _)| *pos >= offset)
            .unwrap_or(units.len() - 1);
        if units[idx].1 == '"' && idx + 1 < units.len() && is_ident(units[idx + 1].1) {
            idx += 1;
        } else if !is_ident(units[idx].1) && idx > 0 && is_ident(units[idx - 1].1) {
            idx -= 1;
        }
        if !is_ident(units[idx].1) {
            return None;
        }
        let mut start = idx;
        while start > 0 && is_ident(units[start - 1].1) {
            start -= 1;
        }
        let mut end = idx;
        while end + 1 < units.len() && is_ident(units[end + 1].1) {
            end += 1;
        }
        let token: String = units[start..=end].iter().map(|(_, c)| *c).collect();
        let doc = UserSettingsPatch::hover_documentation_for_key(&token)?;
        let from = units[start].0;
        let to = units[end].0 + (units[end].1.len_utf16() as u32);
        Some(LspHoverResult {
            contents: doc.to_string(),
            from: Some(from),
            to: Some(to),
        })
    }
}

impl LspServerSession {
    // Private -----------------------------------------------------------------

    fn request(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        {
            self.pending.lock().unwrap().insert(id, tx);
        }
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        if let Err(err) = self.send_raw(&payload) {
            self.pending.lock().unwrap().remove(&id);
            return Err(err);
        }
        match rx.recv_timeout(self.timeout) {
            Ok(result) => result,
            Err(_) => {
                self.pending.lock().unwrap().remove(&id);
                Err(LspError::Timeout(format!(
                    "request `{method}` timed out after {}ms",
                    self.timeout.as_millis()
                )))
            }
        }
    }

    fn notify(&self, method: &str, params: Value) -> Result<()> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        self.send_raw(&payload)
    }

    fn shutdown(&self) {
        let _ = self.send_raw(&json!({
            "jsonrpc": "2.0",
            "id": 999_999,
            "method": "shutdown",
            "params": Value::Null,
        }));
        let _ = self.notify("exit", Value::Null);
        let _ = self.stdin.lock().unwrap().take();
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn send_raw(&self, payload: &Value) -> Result<()> {
        let body = serde_json::to_vec(payload)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        let mut guard = self.stdin.lock().unwrap();
        let Some(stdin) = guard.as_mut() else {
            return Err(LspError::ServerUnavailable("stdin closed".to_string()));
        };
        stdin.write_all(header.as_bytes())?;
        stdin.write_all(&body)?;
        stdin.flush()?;
        Ok(())
    }
}

impl Drop for LspServerSession {
    fn drop(&mut self) {
        self.shutdown();
    }
}

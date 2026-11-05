use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lgtm_config::{ConfigService, LgtmConfig};

use crate::error::Result;
use crate::models::cache::{
    LspCachedDocument, LspDiscoveryCache, LspDocumentSyncAction, LspSessionKey,
};
use crate::models::lsp::{
    LspDiagnostic, LspDiagnosticSeverity, LspDiagnosticsBuffer, LspFileDiagnosticsEvent,
    LspServerStatusEvent, LspWorkspaceSnapshot,
};

#[derive(Default)]
struct LspCacheState {
    workspace_configs: BTreeMap<PathBuf, LgtmConfig>,
    documents: BTreeMap<PathBuf, LspCachedDocument>,
    diagnostics: BTreeMap<PathBuf, (PathBuf, String, Vec<LspDiagnostic>)>,
    statuses: BTreeMap<LspSessionKey, LspServerStatusEvent>,
}

/// Manages in-memory and on-disk caching for workspace LSP discovery, open documents, server status, and file diagnostics.
#[derive(Clone, Default)]
pub struct LspCacheService {
    state: Arc<Mutex<LspCacheState>>,
}

impl LspCacheService {
    /// Creates an empty `LspCacheService`.
    pub fn new() -> Self {
        Self::default()
    }

    // Public ------------------------------------------------------------------

    /// Caches `config` in memory for `workspace_root`.
    pub fn set_workspace_config(&self, workspace_root: &Path, config: LgtmConfig) {
        let canonical = Self::canonical_or_owned(workspace_root);
        let mut guard = self.state.lock().unwrap();
        guard.workspace_configs.insert(canonical, config);
    }

    /// Returns the cached `LgtmConfig` for `workspace_root`, or loads and caches it from `config_service`.
    pub fn get_or_load_workspace_config(
        &self,
        config_service: &ConfigService,
        workspace_root: &Path,
    ) -> Result<LgtmConfig> {
        let canonical = Self::canonical_or_owned(workspace_root);
        {
            let guard = self.state.lock().unwrap();
            if let Some(cached) = guard.workspace_configs.get(&canonical) {
                return Ok(cached.clone());
            }
        }
        let loaded = config_service.get_config(&canonical)?;
        let mut guard = self.state.lock().unwrap();
        guard.workspace_configs.insert(canonical, loaded.clone());
        Ok(loaded)
    }

    /// Persists workspace language discovery and merged configuration to `.lgtm/cache/lsp-discovery.json`.
    pub fn write_discovery_cache(
        &self,
        workspace_root: &Path,
        detected_languages: &[String],
        config: &LgtmConfig,
    ) -> Result<LspDiscoveryCache> {
        let canonical = Self::canonical_or_owned(workspace_root);
        self.set_workspace_config(&canonical, config.clone());
        let cache_path = ConfigService::get_workspace_cache_path(&canonical, "lsp-discovery.json");
        if let Some(parent) = cache_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = LspDiscoveryCache {
            workspace_root: canonical.to_string_lossy().to_string(),
            detected_languages: detected_languages.to_vec(),
            config: config.clone(),
        };
        let serialized = serde_json::to_vec_pretty(&payload)?;
        std::fs::write(cache_path, serialized)?;
        Ok(payload)
    }

    /// Reads the persisted `.lgtm/cache/lsp-discovery.json` record for `workspace_root`, if present.
    pub fn get_discovery_cache(&self, workspace_root: &Path) -> Result<Option<LspDiscoveryCache>> {
        let canonical = Self::canonical_or_owned(workspace_root);
        let cache_path = ConfigService::get_workspace_cache_path(&canonical, "lsp-discovery.json");
        if !cache_path.is_file() {
            return Ok(None);
        }
        let bytes = std::fs::read(cache_path)?;
        let decoded: LspDiscoveryCache = serde_json::from_slice(&bytes)?;
        Ok(Some(decoded))
    }

    /// Inserts or updates an open document in the cache, incrementing its version only when text changes.
    pub fn sync_document(
        &self,
        workspace_root: &Path,
        project_root: &Path,
        file_path: &Path,
        language_id: &str,
        content: &str,
    ) -> LspDocumentSyncAction {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let project_root = Self::canonical_or_owned(project_root);
        let file_path = Self::canonical_or_owned(file_path);

        let mut guard = self.state.lock().unwrap();
        if let Some(existing) = guard.documents.get_mut(&file_path) {
            if existing.text == content {
                return LspDocumentSyncAction::Unchanged(existing.clone());
            }
            existing.version += 1;
            existing.text = content.to_string();
            return LspDocumentSyncAction::Changed(existing.clone());
        }

        let created = LspCachedDocument {
            workspace_root,
            project_root,
            language_id: language_id.to_string(),
            version: 1,
            text: content.to_string(),
        };
        guard.documents.insert(file_path, created.clone());
        LspDocumentSyncAction::Opened(created)
    }

    /// Updates an already-open document's text and increments its version if the text changed.
    pub fn update_open_document(
        &self,
        file_path: &Path,
        content: &str,
    ) -> Option<LspDocumentSyncAction> {
        let file_path = Self::canonical_or_owned(file_path);
        let mut guard = self.state.lock().unwrap();
        let doc = guard.documents.get_mut(&file_path)?;
        if doc.text == content {
            return Some(LspDocumentSyncAction::Unchanged(doc.clone()));
        }
        doc.version += 1;
        doc.text = content.to_string();
        Some(LspDocumentSyncAction::Changed(doc.clone()))
    }

    /// Returns the cached open document for `file_path`, if tracked.
    pub fn get_document(&self, file_path: &Path) -> Option<LspCachedDocument> {
        let file_path = Self::canonical_or_owned(file_path);
        self.state
            .lock()
            .unwrap()
            .documents
            .get(&file_path)
            .cloned()
    }

    /// Returns the cached in-memory buffer text for `file_path`, falling back to reading from disk.
    pub fn get_document_or_disk_text(&self, file_path: &Path) -> String {
        let file_path = Self::canonical_or_owned(file_path);
        self.state
            .lock()
            .unwrap()
            .documents
            .get(&file_path)
            .map(|doc| doc.text.clone())
            .or_else(|| std::fs::read_to_string(&file_path).ok())
            .unwrap_or_default()
    }

    /// Returns all tracked open documents for `workspace_root` and `language_id`.
    pub fn get_documents_for_language(
        &self,
        workspace_root: &Path,
        language_id: &str,
    ) -> Vec<(PathBuf, LspCachedDocument)> {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        self.state
            .lock()
            .unwrap()
            .documents
            .iter()
            .filter(|(_, doc)| {
                doc.workspace_root == workspace_root && doc.language_id == language_id
            })
            .map(|(path, doc)| (path.clone(), doc.clone()))
            .collect()
    }

    /// Removes and returns the cached open document for `file_path`.
    pub fn remove_document(&self, file_path: &Path) -> Option<LspCachedDocument> {
        let file_path = Self::canonical_or_owned(file_path);
        self.state.lock().unwrap().documents.remove(&file_path)
    }

    /// Returns the cached server status for `key`, if present.
    pub fn get_status(&self, key: &LspSessionKey) -> Option<LspServerStatusEvent> {
        self.state.lock().unwrap().statuses.get(key).cloned()
    }

    /// Records `status` for `key`, recomputing aggregate workspace error and warning counts.
    pub fn set_status(
        &self,
        key: LspSessionKey,
        mut status: LspServerStatusEvent,
    ) -> LspServerStatusEvent {
        let mut guard = self.state.lock().unwrap();
        let (error_count, warning_count) =
            Self::count_workspace_diagnostics(&guard, &key.workspace_root, &key.language_id);
        status.error_count = error_count;
        status.warning_count = warning_count;
        guard.statuses.insert(key, status.clone());
        status
    }

    /// Removes all server status entries for `workspace_root` and `language_id`, returning their keys.
    pub fn remove_statuses_for_language(
        &self,
        workspace_root: &Path,
        language_id: &str,
    ) -> Vec<LspSessionKey> {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let mut guard = self.state.lock().unwrap();
        let keys: Vec<LspSessionKey> = guard
            .statuses
            .keys()
            .filter(|key| key.workspace_root == workspace_root && key.language_id == language_id)
            .cloned()
            .collect();
        for key in &keys {
            guard.statuses.remove(key);
        }
        keys
    }

    /// Stores published diagnostics for `file_path` and returns the emitted file event plus updated server status.
    pub fn set_file_diagnostics(
        &self,
        key: &LspSessionKey,
        file_path: &Path,
        diagnostics: Vec<LspDiagnostic>,
    ) -> (LspFileDiagnosticsEvent, Option<LspServerStatusEvent>) {
        let workspace_root = Self::canonical_or_owned(&key.workspace_root);
        let file_path = Self::canonical_or_owned(file_path);

        let updated_status = {
            let mut guard = self.state.lock().unwrap();
            guard.diagnostics.insert(
                file_path.clone(),
                (
                    workspace_root.clone(),
                    key.language_id.clone(),
                    diagnostics.clone(),
                ),
            );
            let (total_errors, total_warnings) =
                Self::count_workspace_diagnostics(&guard, &workspace_root, &key.language_id);
            if let Some(status) = guard.statuses.get_mut(key) {
                status.error_count = total_errors;
                status.warning_count = total_warnings;
                Some(status.clone())
            } else {
                None
            }
        };

        let file_event = LspFileDiagnosticsEvent {
            workspace_root: workspace_root.to_string_lossy().to_string(),
            path: Self::to_workspace_relative(&workspace_root, &file_path),
            language_id: key.language_id.clone(),
            diagnostics,
        };

        (file_event, updated_status)
    }

    /// Returns the cached `LspFileDiagnosticsEvent` for `file_path`, if any diagnostics were recorded.
    pub fn get_file_diagnostics(
        &self,
        workspace_root: &Path,
        file_path: &Path,
    ) -> Option<LspFileDiagnosticsEvent> {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let file_path = Self::canonical_or_owned(file_path);
        let guard = self.state.lock().unwrap();
        guard
            .diagnostics
            .get(&file_path)
            .map(|(_, lang, diags)| LspFileDiagnosticsEvent {
                workspace_root: workspace_root.to_string_lossy().to_string(),
                path: Self::to_workspace_relative(&workspace_root, &file_path),
                language_id: lang.clone(),
                diagnostics: diags.clone(),
            })
    }

    /// Builds a complete `LspWorkspaceSnapshot` for `workspace_root`.
    pub fn get_workspace_snapshot(&self, workspace_root: &Path) -> LspWorkspaceSnapshot {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let guard = self.state.lock().unwrap();
        let servers = guard
            .statuses
            .iter()
            .filter(|(key, _)| key.workspace_root == workspace_root)
            .map(|(_, status)| status.clone())
            .collect();
        let diagnostics_by_path = guard
            .diagnostics
            .iter()
            .filter(|(_, (ws, _, diags))| ws == &workspace_root && !diags.is_empty())
            .map(|(path, (_, _, diags))| {
                (
                    Self::to_workspace_relative(&workspace_root, path),
                    diags.clone(),
                )
            })
            .collect();

        LspWorkspaceSnapshot {
            workspace_root: workspace_root.to_string_lossy().to_string(),
            servers,
            diagnostics_by_path,
        }
    }

    /// Renders a read-only `LspDiagnosticsBuffer` for `workspace_root`, optionally filtered by `language_id`.
    pub fn get_diagnostics_buffer(
        &self,
        workspace_root: &Path,
        language_id: Option<&str>,
    ) -> LspDiagnosticsBuffer {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let (servers, diags) = {
            let guard = self.state.lock().unwrap();
            let servers: Vec<LspServerStatusEvent> = guard
                .statuses
                .iter()
                .filter(|(key, _)| key.workspace_root == workspace_root)
                .map(|(_, status)| status.clone())
                .collect();
            let diags: BTreeMap<String, (String, Vec<LspDiagnostic>)> = guard
                .diagnostics
                .iter()
                .filter(|(_, (ws, _, _))| ws == &workspace_root)
                .map(|(path, (_, lang, list))| {
                    (
                        path.to_string_lossy().to_string(),
                        (lang.clone(), list.clone()),
                    )
                })
                .collect();
            (servers, diags)
        };

        LspDiagnosticsBuffer::render(&workspace_root, language_id, &servers, &diags)
    }

    /// Evicts cached configuration, open documents, statuses, and diagnostics for `workspace_root`.
    pub fn clear_workspace(&self, workspace_root: &Path) {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let mut guard = self.state.lock().unwrap();
        guard.workspace_configs.remove(&workspace_root);
        guard
            .documents
            .retain(|_, doc| doc.workspace_root != workspace_root);
        guard
            .diagnostics
            .retain(|_, (ws, _, _)| ws != &workspace_root);
        guard
            .statuses
            .retain(|key, _| key.workspace_root != workspace_root);
    }

    /// Clears all cached workspaces, documents, statuses, and diagnostics.
    pub fn clear_all(&self) {
        let mut guard = self.state.lock().unwrap();
        guard.workspace_configs.clear();
        guard.documents.clear();
        guard.diagnostics.clear();
        guard.statuses.clear();
    }

    // Private -----------------------------------------------------------------

    fn count_workspace_diagnostics(
        guard: &LspCacheState,
        workspace_root: &Path,
        language_id: &str,
    ) -> (usize, usize) {
        let (mut total_errors, mut total_warnings) = (0usize, 0usize);
        for (ws, lang, diags) in guard.diagnostics.values() {
            if ws == workspace_root && lang == language_id {
                for diag in diags {
                    match diag.severity {
                        LspDiagnosticSeverity::Error => total_errors += 1,
                        LspDiagnosticSeverity::Warning => total_warnings += 1,
                        _ => {}
                    }
                }
            }
        }
        (total_errors, total_warnings)
    }

    fn canonical_or_owned(path: &Path) -> PathBuf {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    }

    fn to_workspace_relative(workspace_root: &Path, path: &Path) -> String {
        path.strip_prefix(workspace_root)
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .unwrap_or_else(|_| path.to_string_lossy().to_string())
    }
}

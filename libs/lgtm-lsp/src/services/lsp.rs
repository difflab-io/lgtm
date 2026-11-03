use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use lgtm_config::{ConfigService, LanguageConfig, MiseService};
use serde_json::{Value, json};

use crate::error::{LspError, Result};
use crate::models::lsp::{
    LspDiagnostic, LspDiagnosticSeverity, LspDiagnosticsBuffer, LspEvent, LspFileDiagnosticsEvent,
    LspHoverResult, LspLocation, LspServerStatus, LspServerStatusEvent, LspWorkspaceSnapshot,
    file_uri_to_path, offset_to_position, path_to_file_uri, position_to_offset,
};

type SessionKey = (PathBuf, String, PathBuf);
type EventSink = Arc<dyn Fn(LspEvent) + Send + Sync>;

struct OpenDocument {
    workspace_root: PathBuf,
    project_root: PathBuf,
    language_id: String,
    version: i32,
    text: String,
}

#[derive(Default)]
struct LspServiceState {
    sessions: BTreeMap<SessionKey, Arc<LspServerSession>>,
    documents: BTreeMap<PathBuf, OpenDocument>,
    diagnostics: BTreeMap<PathBuf, (PathBuf, String, Vec<LspDiagnostic>)>,
    statuses: BTreeMap<SessionKey, LspServerStatusEvent>,
}

struct LspServerSession {
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
    next_id: AtomicU64,
    pending: Arc<Mutex<BTreeMap<u64, mpsc::Sender<Result<Value>>>>>,
    timeout: Duration,
}

#[derive(Clone)]
pub struct LspService {
    config_service: ConfigService,
    mise_service: MiseService,
    event_sink: EventSink,
    state: Arc<Mutex<LspServiceState>>,
    init_locks: Arc<Mutex<BTreeMap<SessionKey, Arc<Mutex<()>>>>>,
}

impl LspService {
    pub fn new<F>(config_service: ConfigService, event_sink: F) -> Self
    where
        F: Fn(LspEvent) + Send + Sync + 'static,
    {
        let mise_service = MiseService::new(config_service.clone());
        Self {
            config_service,
            mise_service,
            event_sink: Arc::new(event_sink),
            state: Arc::new(Mutex::new(LspServiceState::default())),
            init_locks: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    // Public ------------------------------------------------------------------

    pub fn prepare_workspace(&self, workspace_root: &Path) -> Result<()> {
        let canonical = Self::canonical_or_owned(workspace_root);
        let config = self.config_service.get_config(&canonical)?;
        let detected = ConfigService::detect_workspace_languages(&config, &canonical);
        if !detected.is_empty() || ConfigService::get_project_config_path(&canonical).is_file() {
            let _ = self.config_service.ensure_workspace_layout(&canonical)?;
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
                let key = (canonical.clone(), language_id.clone(), canonical.clone());
                let should_announce = {
                    let mut guard = self.state.lock().unwrap();
                    if guard.sessions.contains_key(&key) || guard.statuses.contains_key(&key) {
                        false
                    } else {
                        let starting_event = LspServerStatusEvent {
                            workspace_root: canonical.to_string_lossy().to_string(),
                            language_id: language_id.clone(),
                            project_root: canonical.to_string_lossy().to_string(),
                            status: LspServerStatus::Starting,
                            command: lang_cfg.command.clone(),
                            message: None,
                            error_count: 0,
                            warning_count: 0,
                        };
                        guard.statuses.insert(key, starting_event);
                        true
                    }
                };
                if should_announce {
                    (self.event_sink)(LspEvent::Status(LspServerStatusEvent {
                        workspace_root: canonical.to_string_lossy().to_string(),
                        language_id: language_id.clone(),
                        project_root: canonical.to_string_lossy().to_string(),
                        status: LspServerStatus::Starting,
                        command: lang_cfg.command.clone(),
                        message: None,
                        error_count: 0,
                        warning_count: 0,
                    }));
                }
                let service = self.clone();
                let ws_root = canonical.clone();
                std::thread::spawn(move || {
                    service.warmup_workspace_language(
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

    pub fn open_or_sync_file(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        content: &str,
    ) -> (Option<String>, Option<LspServerStatusEvent>) {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let file_path = Self::canonical_or_owned(file_path);

        let Ok(config) = self.config_service.get_config(&workspace_root) else {
            return (None, None);
        };
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
        let key = (
            workspace_root.clone(),
            language_id.clone(),
            project_root.clone(),
        );

        let session = self.ensure_session(
            &workspace_root,
            &project_root,
            &language_id,
            &language_config,
            config.lsp.auto_install,
            Duration::from_millis(config.lsp.request_timeout_ms),
        );

        if let Some(session) = session {
            let uri = path_to_file_uri(&file_path);
            let mut notification = None;
            {
                let mut guard = self.state.lock().unwrap();
                if let Some(existing) = guard.documents.get_mut(&file_path) {
                    if existing.text != content {
                        existing.version += 1;
                        existing.text = content.to_string();
                        if let Some(uri) = uri {
                            notification = Some((
                                "textDocument/didChange",
                                json!({
                                    "textDocument": {
                                        "uri": uri,
                                        "version": existing.version,
                                    },
                                    "contentChanges": [{ "text": content }],
                                }),
                            ));
                        }
                    }
                } else {
                    guard.documents.insert(
                        file_path.clone(),
                        OpenDocument {
                            workspace_root: workspace_root.clone(),
                            project_root: project_root.clone(),
                            language_id: language_id.clone(),
                            version: 1,
                            text: content.to_string(),
                        },
                    );
                    if let Some(uri) = uri {
                        notification = Some((
                            "textDocument/didOpen",
                            json!({
                                "textDocument": {
                                    "uri": uri,
                                    "languageId": language_id,
                                    "version": 1,
                                    "text": content,
                                }
                            }),
                        ));
                    }
                }
            }
            if let Some((method, params)) = notification {
                let _ = session.notify(method, params);
            }
        }

        let cached_diagnostics = {
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
        };
        if let Some(diag_event) = cached_diagnostics {
            (self.event_sink)(LspEvent::Diagnostics(diag_event));
        }

        let status = self.state.lock().unwrap().statuses.get(&key).cloned();
        (Some(language_id), status)
    }

    pub fn did_change(&self, workspace_root: &Path, file_path: &Path, content: &str) -> Result<()> {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let file_path = Self::canonical_or_owned(file_path);

        let session_and_version = {
            let mut guard = self.state.lock().unwrap();
            if let Some(doc) = guard.documents.get_mut(&file_path) {
                if doc.text == content {
                    return Ok(());
                }
                doc.version += 1;
                doc.text = content.to_string();
                let key = (
                    doc.workspace_root.clone(),
                    doc.language_id.clone(),
                    doc.project_root.clone(),
                );
                let version = doc.version;
                let session = guard.sessions.get(&key).cloned();
                Some((session, version))
            } else {
                None
            }
        };

        if let Some((Some(session), version)) = session_and_version {
            if let Some(uri) = path_to_file_uri(&file_path) {
                session.notify(
                    "textDocument/didChange",
                    json!({
                        "textDocument": {
                            "uri": uri,
                            "version": version,
                        },
                        "contentChanges": [{ "text": content }],
                    }),
                )?;
            }
            return Ok(());
        }

        let _ = self.open_or_sync_file(&workspace_root, &file_path, content);
        Ok(())
    }

    pub fn did_save(&self, workspace_root: &Path, file_path: &Path, content: &str) -> Result<()> {
        self.did_change(workspace_root, file_path, content)?;
        let file_path = Self::canonical_or_owned(file_path);
        let session = {
            let guard = self.state.lock().unwrap();
            guard.documents.get(&file_path).and_then(|doc| {
                let key = (
                    doc.workspace_root.clone(),
                    doc.language_id.clone(),
                    doc.project_root.clone(),
                );
                guard.sessions.get(&key).cloned()
            })
        };
        if let (Some(session), Some(uri)) = (session, path_to_file_uri(&file_path)) {
            session.notify(
                "textDocument/didSave",
                json!({
                    "textDocument": { "uri": uri },
                    "text": content,
                }),
            )?;
        }
        Ok(())
    }

    pub fn did_close(&self, _workspace_root: &Path, file_path: &Path) -> Result<()> {
        let file_path = Self::canonical_or_owned(file_path);
        let session = {
            let mut guard = self.state.lock().unwrap();
            let doc = guard.documents.remove(&file_path);
            doc.and_then(|d| {
                let key = (d.workspace_root, d.language_id, d.project_root);
                guard.sessions.get(&key).cloned()
            })
        };
        if let (Some(session), Some(uri)) = (session, path_to_file_uri(&file_path)) {
            let _ = session.notify(
                "textDocument/didClose",
                json!({
                    "textDocument": { "uri": uri },
                }),
            );
        }
        Ok(())
    }

    pub fn get_hover(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        offset: u32,
    ) -> Result<Option<LspHoverResult>> {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let file_path = Self::canonical_or_owned(file_path);
        let Some((session, text)) = self.resolve_file_session(&workspace_root, &file_path) else {
            return Ok(None);
        };
        let Some(uri) = path_to_file_uri(&file_path) else {
            return Ok(None);
        };
        let (line, character) = offset_to_position(&text, offset);
        let response = session.request(
            "textDocument/hover",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
            }),
        )?;

        if response.is_null() {
            return Ok(None);
        }

        let contents = Self::extract_hover_contents(&response);
        if contents.trim().is_empty() {
            return Ok(None);
        }

        let (from, to) = Self::extract_range_offsets(&text, response.get("range"));
        Ok(Some(LspHoverResult { contents, from, to }))
    }

    pub fn get_definition(
        &self,
        workspace_root: &Path,
        file_path: &Path,
        offset: u32,
    ) -> Result<Vec<LspLocation>> {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let file_path = Self::canonical_or_owned(file_path);
        let Some((session, text)) = self.resolve_file_session(&workspace_root, &file_path) else {
            return Ok(Vec::new());
        };
        let Some(uri) = path_to_file_uri(&file_path) else {
            return Ok(Vec::new());
        };
        let (line, character) = offset_to_position(&text, offset);
        let response = session.request(
            "textDocument/definition",
            json!({
                "textDocument": { "uri": uri },
                "position": { "line": line, "character": character },
            }),
        )?;

        Ok(self.parse_locations(&workspace_root, &response))
    }

    pub fn get_workspace_snapshot(&self, workspace_root: &Path) -> Result<LspWorkspaceSnapshot> {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let guard = self.state.lock().unwrap();
        let servers = guard
            .statuses
            .iter()
            .filter(|((ws, _, _), _)| ws == &workspace_root)
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

        Ok(LspWorkspaceSnapshot {
            workspace_root: workspace_root.to_string_lossy().to_string(),
            servers,
            diagnostics_by_path,
        })
    }

    pub fn get_diagnostics_buffer(
        &self,
        workspace_root: &Path,
        language_id: Option<&str>,
    ) -> Result<LspDiagnosticsBuffer> {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let (servers, diags) = {
            let guard = self.state.lock().unwrap();
            let servers: Vec<LspServerStatusEvent> = guard
                .statuses
                .iter()
                .filter(|((ws, _, _), _)| ws == &workspace_root)
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

        Ok(LspDiagnosticsBuffer::render(
            &workspace_root,
            language_id,
            &servers,
            &diags,
        ))
    }

    pub fn shutdown_workspace(&self, workspace_root: &Path) {
        let workspace_root = Self::canonical_or_owned(workspace_root);
        let sessions: Vec<Arc<LspServerSession>> = {
            let mut guard = self.state.lock().unwrap();
            let keys: Vec<SessionKey> = guard
                .sessions
                .keys()
                .filter(|(ws, _, _)| ws == &workspace_root)
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|key| guard.sessions.remove(&key))
                .collect()
        };
        for session in sessions {
            session.shutdown();
        }
    }

    pub fn shutdown_all(&self) {
        let sessions: Vec<Arc<LspServerSession>> = {
            let mut guard = self.state.lock().unwrap();
            let keys: Vec<SessionKey> = guard.sessions.keys().cloned().collect();
            keys.into_iter()
                .filter_map(|key| guard.sessions.remove(&key))
                .collect()
        };
        for session in sessions {
            session.shutdown();
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
        let Some(_session) = self.ensure_session(
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
            let already_open = {
                let guard = self.state.lock().unwrap();
                guard.documents.contains_key(&file_path)
            };
            if already_open {
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
    ) -> Option<Arc<LspServerSession>> {
        let key = (
            workspace_root.to_path_buf(),
            language_id.to_string(),
            project_root.to_path_buf(),
        );

        {
            let guard = self.state.lock().unwrap();
            if let Some(existing) = guard.sessions.get(&key) {
                return Some(existing.clone());
            }
        }

        let key_lock = {
            let mut locks = self.init_locks.lock().unwrap();
            locks
                .entry(key.clone())
                .or_insert_with(|| Arc::new(Mutex::new(())))
                .clone()
        };
        let _init_guard = key_lock.lock().unwrap();

        {
            let guard = self.state.lock().unwrap();
            if let Some(existing) = guard.sessions.get(&key) {
                return Some(existing.clone());
            }
            if let Some(status) = guard.statuses.get(&key)
                && status.status == LspServerStatus::Unavailable
                && status.command == language_config.command
            {
                return None;
            }
        }

        let starting_event = LspServerStatusEvent {
            workspace_root: workspace_root.to_string_lossy().to_string(),
            language_id: language_id.to_string(),
            project_root: project_root.to_string_lossy().to_string(),
            status: LspServerStatus::Starting,
            command: language_config.command.clone(),
            message: None,
            error_count: 0,
            warning_count: 0,
        };
        {
            let mut guard = self.state.lock().unwrap();
            guard.statuses.insert(key.clone(), starting_event.clone());
        }
        (self.event_sink)(LspEvent::Status(starting_event));

        let _ = self.config_service.ensure_workspace_layout(workspace_root);

        match self.spawn_and_initialize_session(
            workspace_root,
            project_root,
            language_id,
            language_config,
            auto_install,
            timeout,
        ) {
            Ok(session) => {
                let ready_event = {
                    let mut guard = self.state.lock().unwrap();
                    let (error_count, warning_count) =
                        Self::count_workspace_diagnostics(&guard, workspace_root, language_id);
                    let evt = LspServerStatusEvent {
                        workspace_root: workspace_root.to_string_lossy().to_string(),
                        language_id: language_id.to_string(),
                        project_root: project_root.to_string_lossy().to_string(),
                        status: LspServerStatus::Ready,
                        command: language_config.command.clone(),
                        message: None,
                        error_count,
                        warning_count,
                    };
                    guard.sessions.insert(key.clone(), session.clone());
                    guard.statuses.insert(key, evt.clone());
                    evt
                };
                (self.event_sink)(LspEvent::Status(ready_event));
                Some(session)
            }
            Err(err) => {
                let unavailable_event = LspServerStatusEvent {
                    workspace_root: workspace_root.to_string_lossy().to_string(),
                    language_id: language_id.to_string(),
                    project_root: project_root.to_string_lossy().to_string(),
                    status: LspServerStatus::Unavailable,
                    command: language_config.command.clone(),
                    message: Some(err.to_string()),
                    error_count: 0,
                    warning_count: 0,
                };
                {
                    let mut guard = self.state.lock().unwrap();
                    guard.statuses.insert(key, unavailable_event.clone());
                }
                (self.event_sink)(LspEvent::Status(unavailable_event));
                None
            }
        }
    }

    fn count_workspace_diagnostics(
        guard: &LspServiceState,
        workspace_root: &Path,
        language_id: &str,
    ) -> (usize, usize) {
        let (mut total_errors, mut total_warnings) = (0usize, 0usize);
        for (ws, lang, diags) in guard.diagnostics.values() {
            if ws == workspace_root && lang == language_id {
                for d in diags {
                    match d.severity {
                        LspDiagnosticSeverity::Error => total_errors += 1,
                        LspDiagnosticSeverity::Warning => total_warnings += 1,
                        _ => {}
                    }
                }
            }
        }
        (total_errors, total_warnings)
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
            );
        }

        if let Ok(session) = self.try_spawn_session(
            workspace_root,
            project_root,
            language_id,
            language_config,
            timeout,
            true,
        ) {
            return Ok(session);
        }

        if let Ok(session) = self.try_spawn_session(
            workspace_root,
            project_root,
            language_id,
            language_config,
            timeout,
            false,
        ) {
            return Ok(session);
        }

        if auto_install && !language_config.mise_tools.is_empty() {
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
        )
    }

    fn try_spawn_session(
        &self,
        workspace_root: &Path,
        project_root: &Path,
        language_id: &str,
        language_config: &LanguageConfig,
        timeout: Duration,
        use_mise_exec: bool,
    ) -> Result<Arc<LspServerSession>> {
        let mise_file = ConfigService::get_managed_mise_path(workspace_root);
        let mut command = if use_mise_exec {
            let mut cmd = Command::new("mise");
            cmd.arg("exec").arg("--cd").arg(project_root);
            for (tool, version) in &language_config.mise_tools {
                cmd.arg(format!("{tool}@{version}"));
            }
            cmd.arg("--")
                .arg(&language_config.command)
                .args(&language_config.args);
            cmd.env("MISE_CONFIG_FILE", &mise_file)
                .env("MISE_GLOBAL_CONFIG_FILE", &mise_file)
                .env("MISE_TRUSTED_CONFIG_PATHS", &mise_file)
                .env("MISE_NOT_FOUND_AUTO_INSTALL", "false")
                .env("MISE_EXEC_AUTO_INSTALL", "false");
            cmd
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
            pending: pending.clone(),
            timeout,
        });

        Self::spawn_reader_thread(
            stdout,
            session.clone(),
            self.state.clone(),
            self.event_sink.clone(),
            workspace_root.to_path_buf(),
            project_root.to_path_buf(),
            language_id.to_string(),
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
        state: Arc<Mutex<LspServiceState>>,
        event_sink: EventSink,
        workspace_root: PathBuf,
        project_root: PathBuf,
        language_id: String,
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
                        &state,
                        &event_sink,
                        &workspace_root,
                        &project_root,
                        &language_id,
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
        state: &Arc<Mutex<LspServiceState>>,
        event_sink: &EventSink,
        workspace_root: &Path,
        project_root: &Path,
        language_id: &str,
    ) {
        let Some(uri) = params.get("uri").and_then(Value::as_str) else {
            return;
        };
        let Some(raw_file_path) = file_uri_to_path(uri) else {
            return;
        };
        let file_path = Self::canonical_or_owned(&raw_file_path);
        let raw_diags = params
            .get("diagnostics")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let text = Self::document_or_disk_text(state, &file_path);

        let mut mapped = Vec::with_capacity(raw_diags.len());
        for item in raw_diags {
            let (line, character, end_line, end_character, from, to) =
                Self::parse_lsp_range(&text, item.get("range"));
            let severity = match item.get("severity").and_then(Value::as_u64) {
                Some(1) => LspDiagnosticSeverity::Error,
                Some(2) => LspDiagnosticSeverity::Warning,
                Some(3) => LspDiagnosticSeverity::Info,
                Some(4) => LspDiagnosticSeverity::Hint,
                _ => LspDiagnosticSeverity::Error,
            };
            let message = item
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let source = item
                .get("source")
                .and_then(Value::as_str)
                .map(str::to_string);
            let code = match item.get("code") {
                Some(Value::String(s)) => Some(s.clone()),
                Some(Value::Number(n)) => Some(n.to_string()),
                _ => None,
            };

            mapped.push(LspDiagnostic {
                from,
                to,
                line,
                character,
                end_line,
                end_character,
                severity,
                message,
                source,
                code,
            });
        }

        let key = (
            workspace_root.to_path_buf(),
            language_id.to_string(),
            project_root.to_path_buf(),
        );

        let updated_status = {
            let mut guard = state.lock().unwrap();
            guard.diagnostics.insert(
                file_path.clone(),
                (
                    workspace_root.to_path_buf(),
                    language_id.to_string(),
                    mapped.clone(),
                ),
            );

            let (mut total_errors, mut total_warnings) = (0usize, 0usize);
            for (ws, lang, diags) in guard.diagnostics.values() {
                if ws == workspace_root && lang == language_id {
                    for d in diags {
                        match d.severity {
                            LspDiagnosticSeverity::Error => total_errors += 1,
                            LspDiagnosticSeverity::Warning => total_warnings += 1,
                            _ => {}
                        }
                    }
                }
            }

            if let Some(status) = guard.statuses.get_mut(&key) {
                status.error_count = total_errors;
                status.warning_count = total_warnings;
                Some(status.clone())
            } else {
                None
            }
        };

        (event_sink)(LspEvent::Diagnostics(LspFileDiagnosticsEvent {
            workspace_root: workspace_root.to_string_lossy().to_string(),
            path: Self::to_workspace_relative(workspace_root, &file_path),
            language_id: language_id.to_string(),
            diagnostics: mapped,
        }));

        if let Some(status_event) = updated_status {
            (event_sink)(LspEvent::Status(status_event));
        }
    }

    fn resolve_file_session(
        &self,
        workspace_root: &Path,
        file_path: &Path,
    ) -> Option<(Arc<LspServerSession>, String)> {
        {
            let guard = self.state.lock().unwrap();
            if let Some(doc) = guard.documents.get(file_path) {
                let key = (
                    doc.workspace_root.clone(),
                    doc.language_id.clone(),
                    doc.project_root.clone(),
                );
                if let Some(session) = guard.sessions.get(&key) {
                    return Some((session.clone(), doc.text.clone()));
                }
            }
        }
        let content = std::fs::read_to_string(file_path).ok()?;
        let _ = self.open_or_sync_file(workspace_root, file_path, &content);
        let guard = self.state.lock().unwrap();
        let doc = guard.documents.get(file_path)?;
        let key = (
            doc.workspace_root.clone(),
            doc.language_id.clone(),
            doc.project_root.clone(),
        );
        let session = guard.sessions.get(&key)?.clone();
        Some((session, doc.text.clone()))
    }

    fn extract_hover_contents(hover: &Value) -> String {
        let Some(contents) = hover.get("contents") else {
            return String::new();
        };
        Self::format_hover_value(contents)
    }

    fn format_hover_value(value: &Value) -> String {
        match value {
            Value::String(s) => s.clone(),
            Value::Object(map) => {
                if let Some(Value::String(val)) = map.get("value") {
                    if let Some(Value::String(lang)) = map.get("language") {
                        return format!("```{lang}\n{val}\n```");
                    }
                    return val.clone();
                }
                String::new()
            }
            Value::Array(items) => items
                .iter()
                .map(Self::format_hover_value)
                .filter(|part| !part.trim().is_empty())
                .collect::<Vec<_>>()
                .join("\n\n"),
            _ => String::new(),
        }
    }

    fn document_or_disk_text(state: &Arc<Mutex<LspServiceState>>, file_path: &Path) -> String {
        let guard = state.lock().unwrap();
        guard
            .documents
            .get(file_path)
            .map(|doc| doc.text.clone())
            .or_else(|| std::fs::read_to_string(file_path).ok())
            .unwrap_or_default()
    }

    fn parse_lsp_range(text: &str, range: Option<&Value>) -> (u32, u32, u32, u32, u32, u32) {
        let start = range.and_then(|r| r.get("start"));
        let end = range.and_then(|r| r.get("end"));
        let line = start
            .and_then(|s| s.get("line"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let character = start
            .and_then(|s| s.get("character"))
            .and_then(Value::as_u64)
            .unwrap_or(0) as u32;
        let end_line = end
            .and_then(|e| e.get("line"))
            .and_then(Value::as_u64)
            .unwrap_or(line as u64) as u32;
        let end_character = end
            .and_then(|e| e.get("character"))
            .and_then(Value::as_u64)
            .unwrap_or(character as u64) as u32;
        let from = position_to_offset(text, line, character);
        let to = position_to_offset(text, end_line, end_character).max(from);
        (line, character, end_line, end_character, from, to)
    }

    fn extract_range_offsets(text: &str, range: Option<&Value>) -> (Option<u32>, Option<u32>) {
        let Some(range) = range else {
            return (None, None);
        };
        if range.get("start").is_none() || range.get("end").is_none() {
            return (None, None);
        }
        let (_, _, _, _, from, to) = Self::parse_lsp_range(text, Some(range));
        (Some(from), Some(to))
    }

    fn parse_locations(&self, workspace_root: &Path, value: &Value) -> Vec<LspLocation> {
        let items = match value {
            Value::Null => Vec::new(),
            Value::Array(arr) => arr.clone(),
            Value::Object(_) => vec![value.clone()],
            _ => Vec::new(),
        };

        let mut locations = Vec::new();
        for item in items {
            let uri = item
                .get("uri")
                .or_else(|| item.get("targetUri"))
                .and_then(Value::as_str);
            let range = item
                .get("targetSelectionRange")
                .or_else(|| item.get("targetRange"))
                .or_else(|| item.get("range"));

            let (Some(uri), Some(range)) = (uri, range) else {
                continue;
            };
            let Some(raw_target_path) = file_uri_to_path(uri) else {
                continue;
            };
            let target_path = Self::canonical_or_owned(&raw_target_path);
            let target_text = Self::document_or_disk_text(&self.state, &target_path);
            let (line, character, _, _, offset, end_offset) =
                Self::parse_lsp_range(&target_text, Some(range));

            locations.push(LspLocation {
                path: Self::to_workspace_relative(workspace_root, &target_path),
                offset,
                end_offset,
                line,
                character,
            });
        }

        locations
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

// tests -----------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // fixtures ----------------------------------------------------------------

    fn write_mock_lsp_server(dir: &Path) -> PathBuf {
        let script_path = dir.join("mock-lsp.mjs");
        let script = r#"import { Buffer } from 'node:buffer';

let buffer = Buffer.alloc(0);

function send(msg) {
  const body = Buffer.from(JSON.stringify(msg), 'utf8');
  process.stdout.write(`Content-Length: ${body.length}\r\n\r\n`);
  process.stdout.write(body);
}

function handleMessage(msg) {
  if (msg.method === 'initialize') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        capabilities: {
          textDocumentSync: 1,
          hoverProvider: true,
          definitionProvider: true,
        },
      },
    });
    return;
  }
  if (msg.method === 'textDocument/didOpen' || msg.method === 'textDocument/didChange') {
    const uri = msg.params.textDocument.uri;
    const text = msg.method === 'textDocument/didOpen'
      ? msg.params.textDocument.text
      : msg.params.contentChanges[0].text;
    const diagnostics = [];
    const lines = text.split('\n');
    for (let i = 0; i < lines.length; i++) {
      const idx = lines[i].indexOf('BROKEN');
      if (idx !== -1) {
        diagnostics.push({
          range: {
            start: { line: i, character: idx },
            end: { line: i, character: idx + 6 },
          },
          severity: 1,
          source: 'mock-lsp',
          code: 'E001',
          message: 'Unresolved symbol BROKEN',
        });
      }
    }
    send({
      jsonrpc: '2.0',
      method: 'textDocument/publishDiagnostics',
      params: { uri, diagnostics },
    });
    return;
  }
  if (msg.method === 'textDocument/hover') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: {
        contents: {
          kind: 'markdown',
          value: '`fn helper() -> usize`',
        },
        range: {
          start: { line: msg.params.position.line, character: 0 },
          end: { line: msg.params.position.line, character: 6 },
        },
      },
    });
    return;
  }
  if (msg.method === 'textDocument/definition') {
    send({
      jsonrpc: '2.0',
      id: msg.id,
      result: [
        {
          uri: msg.params.textDocument.uri,
          range: {
            start: { line: 0, character: 3 },
            end: { line: 0, character: 9 },
          },
        },
      ],
    });
    return;
  }
  if (msg.method === 'shutdown') {
    send({ jsonrpc: '2.0', id: msg.id, result: null });
    return;
  }
  if (msg.method === 'exit') {
    process.exit(0);
  }
}

process.stdin.on('data', (chunk) => {
  buffer = Buffer.concat([buffer, chunk]);
  while (true) {
    const headerEnd = buffer.indexOf('\r\n\r\n');
    if (headerEnd === -1) break;
    const header = buffer.slice(0, headerEnd).toString('utf8');
    const match = header.match(/Content-Length:\s*(\d+)/i);
    if (!match) break;
    const length = Number(match[1]);
    const total = headerEnd + 4 + length;
    if (buffer.length < total) break;
    const body = buffer.slice(headerEnd + 4, total).toString('utf8');
    buffer = buffer.slice(total);
    handleMessage(JSON.parse(body));
  }
});
"#;
        std::fs::write(&script_path, script).unwrap();
        script_path
    }

    // tests -------------------------------------------------------------------

    mod given_a_workspace_with_a_configured_mock_lsp_server {
        use super::*;

        mod when_opening_changing_hovering_and_resolving_definitions {
            use super::*;

            #[test]
            fn should_publish_diagnostics_resolve_hover_and_definition_and_render_buffer() {
                // Arrange
                let temp = tempfile::tempdir().unwrap();
                let workspace_root = std::fs::canonicalize(temp.path()).unwrap();
                let script_path = write_mock_lsp_server(&workspace_root);
                let global_cfg = workspace_root.join("global.toml");
                std::fs::create_dir_all(workspace_root.join(".lgtm")).unwrap();
                std::fs::write(
                    workspace_root.join("Cargo.toml"),
                    "[package]\nname = \"demo\"\n",
                )
                .unwrap();
                std::fs::write(
                    workspace_root.join(".lgtm/config.toml"),
                    format!(
                        "[languages.rust]\nenabled = true\nextensions = [\"rs\"]\nroot_markers = [\"Cargo.toml\"]\nmise_tools = {{}}\ncommand = \"node\"\nargs = [\"{}\"]\n",
                        script_path.display()
                    ),
                )
                .unwrap();

                let src_file = workspace_root.join("lib.rs");
                let initial_source = "fn helper() {}\nfn main() {\n    BROKEN();\n}\n";
                std::fs::write(&src_file, initial_source).unwrap();

                let events: Arc<Mutex<Vec<LspEvent>>> = Arc::new(Mutex::new(Vec::new()));
                let events_clone = events.clone();
                let service = LspService::new(
                    ConfigService::with_global_config_path(global_cfg),
                    move |evt| {
                        events_clone.lock().unwrap().push(evt);
                    },
                );

                // Act
                let (lang, status) =
                    service.open_or_sync_file(&workspace_root, &src_file, initial_source);

                let deadline = std::time::Instant::now() + Duration::from_secs(5);
                while std::time::Instant::now() < deadline {
                    let snapshot = service.get_workspace_snapshot(&workspace_root).unwrap();
                    if !snapshot.diagnostics_by_path.is_empty() {
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }

                let hover = service
                    .get_hover(&workspace_root, &src_file, 4)
                    .unwrap()
                    .unwrap();
                let definitions = service
                    .get_definition(&workspace_root, &src_file, 31)
                    .unwrap();
                let snapshot = service.get_workspace_snapshot(&workspace_root).unwrap();
                let buffer = service
                    .get_diagnostics_buffer(&workspace_root, Some("rust"))
                    .unwrap();
                service.shutdown_all();

                // Assert
                assert_eq!(lang, Some("rust".to_string()));
                assert_eq!(status.unwrap().status, LspServerStatus::Ready);
                assert!(hover.contents.contains("fn helper() -> usize"));
                assert_eq!(definitions.len(), 1);
                assert_eq!(definitions[0].offset, 3);
                assert_eq!(definitions[0].end_offset, 9);
                assert_eq!(snapshot.servers[0].error_count, 1);
                assert!(buffer.content.contains("Unresolved symbol BROKEN"));
            }
        }
    }
}

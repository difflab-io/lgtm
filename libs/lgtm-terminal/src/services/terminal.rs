//! High-level terminal session manager backed by bundled `herdr-core` (`hc`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use lgtm_env::HcBinaryService;
use serde_json::Value;

use crate::client::agent_detector::{AgentDetector, PendingIdleConfirmation};
use crate::client::herdr_core::{
    ChannelFrame, HcForeground, HcStatus, HcTerminal, HerdrCoreClient,
};
use crate::client::kitty::KittyInlineRewriter;
use crate::client::osc7501::Osc7501Record;
use crate::error::{LgtmTerminalError, Result};
use crate::models::agent::{AgentMatcherConfig, default_agent_matchers};
use crate::models::session::{TerminalExitEvent, TerminalSession, TerminalStreamEvent};
use crate::models::status::TerminalAgentStatus;

const WORKSPACE_LABEL_KEY: &str = "lgtm_workspace";
const MAX_REPLAY_BYTES: usize = 256 * 1024;
const DEFAULT_HISTORY_LINES: usize = 600;
const COALESCE_INTERVAL: Duration = Duration::from_millis(12);
const MIN_COLS: u16 = 20;
const MIN_ROWS: u16 = 4;

/// Callback invoked for streamed output, session updates, and exit notices on an attached terminal.
pub type TerminalStreamCallback = Arc<dyn Fn(TerminalStreamEvent) + Send + Sync>;

/// Callback invoked for host-level background session updates and terminal exits.
pub type TerminalEventCallback = Arc<dyn Fn(TerminalHostEvent) + Send + Sync>;

/// Host-level domain event emitted by [`TerminalService`] for background badges and notifications.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalHostEvent {
    /// A session's agent identity, title, or status dot changed.
    SessionUpdated(TerminalSession),
    /// A terminal process exited.
    Exited(TerminalExitEvent),
}

struct SessionTracking {
    session: TerminalSession,
    last_term: HcTerminal,
    idle_guard: PendingIdleConfirmation,
    observer_channel: Option<u32>,
}

struct ActiveAttachment {
    session_id: String,
    channel: u32,
    stop_flag: Arc<AtomicBool>,
}

struct ServiceInner {
    hc_binary_override: Option<PathBuf>,
    hc_session_name: Option<String>,
    client: Option<Arc<HerdrCoreClient>>,
    detector: AgentDetector,
    tracked: HashMap<String, SessionTracking>,
    active_attachment: Option<ActiveAttachment>,
    event_callback: Option<TerminalEventCallback>,
    state_seq: AtomicU64,
}

/// Manages persistent `herdr-core` terminal sessions, active framed attachment with coalesced streaming, Kitty inline rewriting, and dynamic agent tracking.
#[derive(Clone)]
pub struct TerminalService {
    inner: Arc<Mutex<ServiceInner>>,
}

impl TerminalService {
    /// Creates a [`TerminalService`] using the default bundled/staged `hc` binary and default `AgentMatcherConfig` rules.
    pub fn new(
        hc_binary_override: Option<PathBuf>,
        hc_session_name: Option<String>,
        agent_matchers: Option<Vec<AgentMatcherConfig>>,
        event_callback: Option<TerminalEventCallback>,
    ) -> Self {
        let matchers = agent_matchers.unwrap_or_else(default_agent_matchers);
        Self {
            inner: Arc::new(Mutex::new(ServiceInner {
                hc_binary_override,
                hc_session_name,
                client: None,
                detector: AgentDetector::new(matchers),
                tracked: HashMap::new(),
                active_attachment: None,
                event_callback,
                state_seq: AtomicU64::new(1),
            })),
        }
    }

    // Public ------------------------------------------------------------------

    /// Updates the data-driven agent matcher list (e.g., after `LgtmConfig` changes).
    pub fn set_agent_matchers(&self, matchers: Vec<AgentMatcherConfig>) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.detector = AgentDetector::new(matchers);
        }
    }

    /// Lists live `herdr-core` sessions, optionally filtered to a single `workspace_root`.
    pub fn get_sessions(&self, workspace_root: Option<&Path>) -> Result<Vec<TerminalSession>> {
        let client = self.ensure_connected()?;
        let terminals = client.terminal_list(false)?;
        let filter_root = workspace_root.map(Self::canonical_root_string);

        let mut guard = self
            .inner
            .lock()
            .map_err(|_| LgtmTerminalError::Protocol("terminal state poisoned".to_string()))?;

        let mut results = Vec::new();
        let mut to_observe = Vec::new();
        for term in terminals {
            let session = Self::reconcile_terminal(&mut guard, &term);
            if session.alive
                && guard
                    .tracked
                    .get(&session.id)
                    .is_some_and(|t| t.observer_channel.is_none())
            {
                to_observe.push(session.id.clone());
            }
            if let Some(ref wanted) = filter_root {
                if &session.workspace_root == wanted {
                    results.push(session);
                }
            } else {
                results.push(session);
            }
        }
        drop(guard);
        for sid in to_observe {
            self.start_osc_observer(&client, &sid);
        }
        results.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(results)
    }

    /// Spawns a new persistent shell session inside `workspace_root`.
    pub fn create_session(
        &self,
        workspace_root: &Path,
        cols: u16,
        rows: u16,
    ) -> Result<TerminalSession> {
        if !workspace_root.is_dir() {
            return Err(LgtmTerminalError::InvalidWorkspace(
                workspace_root.display().to_string(),
            ));
        }

        let canonical_root = Self::canonical_root_string(workspace_root);
        let client = self.ensure_connected()?;

        let mut labels = HashMap::new();
        labels.insert(WORKSPACE_LABEL_KEY.to_string(), canonical_root.clone());

        let term = client.terminal_run(
            &canonical_root,
            cols.max(MIN_COLS),
            rows.max(MIN_ROWS),
            labels,
        )?;

        let mut guard = self
            .inner
            .lock()
            .map_err(|_| LgtmTerminalError::Protocol("terminal state poisoned".to_string()))?;
        let session = Self::reconcile_terminal(&mut guard, &term);
        let callback = guard.event_callback.clone();
        drop(guard);

        self.start_osc_observer(&client, &session.id);

        if let Some(cb) = callback {
            cb(TerminalHostEvent::SessionUpdated(session.clone()));
        }
        Ok(session)
    }

    /// Attaches a single active UI stream to `session_id`, detaching any prior attachment, replaying a bounded tail (`<= 256 KB`), and marking unseen completions seen.
    pub fn attach(
        &self,
        session_id: &str,
        cols: u16,
        rows: u16,
        sink: TerminalStreamCallback,
    ) -> Result<TerminalSession> {
        let client = self.ensure_connected()?;

        // Detach any previously attached session so only one surface is streaming at a time.
        let previous = self
            .inner
            .lock()
            .ok()
            .and_then(|mut guard| guard.active_attachment.take());
        if let Some(prev) = previous {
            prev.stop_flag.store(true, Ordering::Relaxed);
            let _ = client.close_channel(prev.channel);
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let batch_buffer: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let first_chunk_sent = Arc::new(AtomicBool::new(false));

        let flusher_stop = Arc::clone(&stop_flag);
        let flusher_buf = Arc::clone(&batch_buffer);
        let flusher_first = Arc::clone(&first_chunk_sent);
        let flusher_sink = Arc::clone(&sink);

        // Coalescing flusher thread (12ms cadence) + Kitty local-file inline PNG rewriter.
        thread::Builder::new()
            .name(format!("lgtm-term-flush-{session_id}"))
            .spawn(move || {
                let mut rewriter = KittyInlineRewriter::new();
                while !flusher_stop.load(Ordering::Relaxed) {
                    thread::sleep(COALESCE_INTERVAL);
                    let raw = {
                        let Ok(mut buf) = flusher_buf.lock() else {
                            break;
                        };
                        if buf.is_empty() {
                            continue;
                        }
                        std::mem::take(&mut *buf)
                    };
                    let rewritten = rewriter.process(&raw);
                    if rewritten.is_empty() {
                        continue;
                    }
                    let is_replay = !flusher_first.swap(true, Ordering::Relaxed);
                    if is_replay {
                        let bounded = Self::bound_replay_tail(&rewritten);
                        flusher_sink(TerminalStreamEvent::Replay { data: bounded });
                    } else {
                        flusher_sink(TerminalStreamEvent::Output { data: rewritten });
                    }
                }
            })?;

        let writer_buf = Arc::clone(&batch_buffer);
        let writer_stop = Arc::clone(&stop_flag);
        let writer_sink = Arc::clone(&sink);

        let attach_res = client.terminal_attach(
            session_id,
            cols.max(MIN_COLS),
            rows.max(MIN_ROWS),
            DEFAULT_HISTORY_LINES,
            Box::new(move |frame| match frame {
                ChannelFrame::Data(bytes) => {
                    if let Ok(mut buf) = writer_buf.lock() {
                        buf.extend_from_slice(&bytes);
                    }
                }
                ChannelFrame::Closed => {
                    writer_stop.store(true, Ordering::Relaxed);
                    writer_sink(TerminalStreamEvent::Exited { exit_code: None });
                }
            }),
        )?;

        let mut guard = self
            .inner
            .lock()
            .map_err(|_| LgtmTerminalError::Protocol("terminal state poisoned".to_string()))?;
        guard.active_attachment = Some(ActiveAttachment {
            session_id: session_id.to_string(),
            channel: attach_res.channel,
            stop_flag,
        });
        let mut session = Self::reconcile_terminal(&mut guard, &attach_res.terminal);
        Self::apply_mark_seen(&mut guard, &mut session);
        Ok(session)
    }

    /// Detaches the active UI stream from `session_id` while keeping the `herdr-core` session alive in the background.
    pub fn detach(&self, session_id: &str) -> Result<()> {
        let (client, channel_to_close) = {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| LgtmTerminalError::Protocol("terminal state poisoned".to_string()))?;
            let removed = guard
                .active_attachment
                .take_if(|att| att.session_id == session_id);
            let channel = removed.map(|att| {
                att.stop_flag.store(true, Ordering::Relaxed);
                att.channel
            });
            (guard.client.clone(), channel)
        };

        if let (Some(client), Some(channel)) = (client, channel_to_close) {
            let _ = client.close_channel(channel);
        }
        Ok(())
    }

    /// Marks any unseen `Done` completion on `session_id` as seen, clearing its blue dot back to green (`Idle`).
    pub fn mark_seen(&self, session_id: &str) -> Result<Option<TerminalSession>> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_| LgtmTerminalError::Protocol("terminal state poisoned".to_string()))?;
        let Some(tracking) = guard.tracked.get_mut(session_id) else {
            return Ok(None);
        };
        let mut session = tracking.session.clone();
        let changed = Self::apply_mark_seen(&mut guard, &mut session);
        let callback = if changed {
            guard.event_callback.clone()
        } else {
            None
        };
        drop(guard);

        if let Some(cb) = callback {
            cb(TerminalHostEvent::SessionUpdated(session.clone()));
        }
        Ok(Some(session))
    }

    /// Writes keyboard/paste bytes to the currently attached `session_id`.
    pub fn write_input(&self, session_id: &str, data: &[u8]) -> Result<()> {
        let (client, channel) = self.require_attached_channel(session_id)?;
        client.send_data(channel, data)
    }

    /// Resizes the currently attached `session_id` (`cols >= 20`, `rows >= 4`).
    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        let (client, channel) = self.require_attached_channel(session_id)?;
        client.send_resize(channel, cols.max(MIN_COLS), rows.max(MIN_ROWS))
    }

    /// Terminates `session_id` inside `herdr-core` and detaches any active UI stream.
    pub fn close_session(&self, session_id: &str) -> Result<()> {
        let _ = self.detach(session_id);
        let client = self.ensure_connected()?;
        client.terminal_kill(session_id)?;
        if let Ok(mut guard) = self.inner.lock() {
            guard.tracked.remove(session_id);
        }
        Ok(())
    }

    /// Detaches any active UI stream without killing background `herdr-core` sessions (called on desktop app quit).
    pub fn detach_all(&self) {
        let (client, channel) = {
            let Ok(mut guard) = self.inner.lock() else {
                return;
            };
            let removed = guard.active_attachment.take();
            let ch = removed.map(|att| {
                att.stop_flag.store(true, Ordering::Relaxed);
                att.channel
            });
            (guard.client.clone(), ch)
        };
        if let (Some(client), Some(channel)) = (client, channel) {
            let _ = client.close_channel(channel);
        }
    }

    /// Stops the connected `herdr-core` daemon (intended only for isolated test sessions).
    pub fn stop_daemon(&self) -> Result<()> {
        self.detach_all();
        let client = {
            let guard = self
                .inner
                .lock()
                .map_err(|_| LgtmTerminalError::Protocol("terminal state poisoned".to_string()))?;
            guard.client.clone()
        };
        if let Some(client) = client {
            client.server_stop()?;
        }
        Ok(())
    }

    // Private -----------------------------------------------------------------

    fn ensure_connected(&self) -> Result<Arc<HerdrCoreClient>> {
        {
            let guard = self
                .inner
                .lock()
                .map_err(|_| LgtmTerminalError::Protocol("terminal state poisoned".to_string()))?;
            if let Some(client) = &guard.client {
                return Ok(Arc::clone(client));
            }
        }

        let (hc_override, session_name) = {
            let guard = self
                .inner
                .lock()
                .map_err(|_| LgtmTerminalError::Protocol("terminal state poisoned".to_string()))?;
            (
                guard.hc_binary_override.clone(),
                guard.hc_session_name.clone(),
            )
        };

        let hc_binary = match hc_override {
            Some(path) => path,
            None => HcBinaryService::new().get_hc_binary_path().ok_or_else(|| {
                LgtmTerminalError::BinaryNotFound(
                    "run `mise run //apps/lgtm-desktop:hc:stage` or set LGTM_HC_BIN".to_string(),
                )
            })?,
        };

        let client = Arc::new(HerdrCoreClient::connect(hc_binary, session_name)?);
        let weak_inner = Arc::downgrade(&self.inner);
        let _ = client.events_subscribe(Box::new(move |frame| {
            let ChannelFrame::Data(bytes) = frame else {
                return;
            };
            let Some(inner) = weak_inner.upgrade() else {
                return;
            };
            Self::handle_hc_event_lines(&inner, &bytes);
        }));

        let mut guard = self
            .inner
            .lock()
            .map_err(|_| LgtmTerminalError::Protocol("terminal state poisoned".to_string()))?;
        if let Some(existing) = &guard.client {
            return Ok(Arc::clone(existing));
        }
        guard.client = Some(Arc::clone(&client));
        Ok(client)
    }

    fn require_attached_channel(&self, session_id: &str) -> Result<(Arc<HerdrCoreClient>, u32)> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| LgtmTerminalError::Protocol("terminal state poisoned".to_string()))?;
        let client = guard
            .client
            .as_ref()
            .cloned()
            .ok_or_else(|| LgtmTerminalError::NotFound(session_id.to_string()))?;
        let att = guard
            .active_attachment
            .as_ref()
            .filter(|att| att.session_id == session_id)
            .ok_or_else(|| LgtmTerminalError::NotFound(session_id.to_string()))?;
        Ok((client, att.channel))
    }

    fn start_osc_observer(&self, client: &Arc<HerdrCoreClient>, session_id: &str) {
        let already_observing = self.inner.lock().ok().is_some_and(|guard| {
            guard
                .tracked
                .get(session_id)
                .is_some_and(|t| t.observer_channel.is_some())
        });
        if already_observing {
            return;
        }
        let inner = Arc::clone(&self.inner);
        let sid = session_id.to_string();
        let Ok(channel) = client.terminal_observe(
            session_id,
            Box::new(move |frame| {
                if let ChannelFrame::Data(bytes) = frame {
                    let records = Osc7501Record::extract_all(&bytes);
                    if records.is_empty() {
                        return;
                    }
                    let latest = records.into_iter().last().unwrap();
                    let event_to_emit = {
                        let Ok(mut guard) = inner.lock() else {
                            return;
                        };
                        let Some(mut term) = guard.tracked.get(&sid).map(|t| t.last_term.clone())
                        else {
                            return;
                        };
                        if latest.clear {
                            term.status = None;
                        } else {
                            let state_str = match latest.status {
                                TerminalAgentStatus::Idle => "idle",
                                TerminalAgentStatus::Working => "working",
                                TerminalAgentStatus::Blocked => "blocked",
                                TerminalAgentStatus::Done => "done",
                            };
                            let prev_app = term.status.as_ref().and_then(|s| s.app.clone());
                            term.status = Some(HcStatus {
                                state: state_str.to_string(),
                                app: latest.app.or(prev_app),
                                msg: latest.msg,
                                title: latest.title,
                                kind: None,
                                progress: None,
                            });
                        }
                        let session = Self::reconcile_terminal(&mut guard, &term);
                        guard
                            .event_callback
                            .clone()
                            .map(|cb| (cb, TerminalHostEvent::SessionUpdated(session)))
                    };
                    if let Some((cb, evt)) = event_to_emit {
                        cb(evt);
                    }
                }
            }),
        ) else {
            return;
        };
        if let Ok(mut guard) = self.inner.lock()
            && let Some(tracking) = guard.tracked.get_mut(session_id)
        {
            tracking.observer_channel = Some(channel);
        }
    }

    fn handle_hc_event_lines(inner: &Arc<Mutex<ServiceInner>>, bytes: &[u8]) {
        let text = String::from_utf8_lossy(bytes);
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
                continue;
            };
            let params = value.get("params").unwrap_or(&value);
            let event_type = params.get("type").and_then(Value::as_str).unwrap_or("");
            let data = params.get("data");

            let term_opt: Option<HcTerminal> = if event_type == "created" {
                data.and_then(|d| serde_json::from_value::<HcTerminal>(d.clone()).ok())
            } else if let Some(tid) = params.get("terminal").and_then(Value::as_str) {
                inner.lock().ok().and_then(|guard| {
                    let mut term = guard.tracked.get(tid)?.last_term.clone();
                    match event_type {
                        "foreground" => {
                            if let Some(fg) = data.and_then(|d| {
                                serde_json::from_value::<HcForeground>(d.clone()).ok()
                            }) {
                                term.foreground = Some(fg);
                            }
                        }
                        "title" => {
                            if let Some(title) =
                                data.and_then(|d| d.get("title")).and_then(Value::as_str)
                            {
                                term.title = title.to_string();
                            }
                        }
                        "status" => {
                            if let Some(st) = data
                                .and_then(|d| serde_json::from_value::<HcStatus>(d.clone()).ok())
                            {
                                term.status = Some(st);
                            }
                        }
                        "finished" => {
                            term.state = "finished".to_string();
                            term.exit_code = data
                                .and_then(|d| d.get("exit_code"))
                                .and_then(Value::as_i64)
                                .map(|c| c as i32);
                        }
                        _ => return None,
                    }
                    Some(term)
                })
            } else {
                params
                    .get("terminal")
                    .and_then(|t| serde_json::from_value::<HcTerminal>(t.clone()).ok())
            };

            let Some(term) = term_opt else {
                continue;
            };

            let event_to_emit = {
                let Ok(mut guard) = inner.lock() else {
                    continue;
                };
                let cb = guard.event_callback.clone();
                let session = Self::reconcile_terminal(&mut guard, &term);
                if !session.alive {
                    cb.map(|c| {
                        (
                            c,
                            TerminalHostEvent::Exited(TerminalExitEvent {
                                session_id: session.id.clone(),
                                workspace_root: session.workspace_root.clone(),
                                exit_code: term.exit_code,
                            }),
                        )
                    })
                } else {
                    cb.map(|c| (c, TerminalHostEvent::SessionUpdated(session)))
                }
            };

            if let Some((cb, evt)) = event_to_emit {
                cb(evt);
            }
        }
    }

    fn reconcile_terminal(inner: &mut ServiceInner, term: &HcTerminal) -> TerminalSession {
        let workspace_root = term
            .labels
            .get(WORKSPACE_LABEL_KEY)
            .cloned()
            .unwrap_or_else(|| term.cwd.clone());
        let alive = term.state != "finished";

        let (prev_status, prev_unseen_done, prev_seq) = inner
            .tracked
            .get(&term.address)
            .map(|t| {
                (
                    t.session.status,
                    t.session.agent.as_ref().is_some_and(|a| a.unseen_done),
                    t.session.state_change_seq,
                )
            })
            .unwrap_or((TerminalAgentStatus::Idle, false, 0));

        let (mut agent, mut status, explicit_protocol) =
            inner.detector.evaluate(term, prev_unseen_done);

        let is_active = inner
            .active_attachment
            .as_ref()
            .is_some_and(|att| att.session_id == term.address);

        let now = Instant::now();
        let entry = inner
            .tracked
            .entry(term.address.clone())
            .or_insert_with(|| SessionTracking {
                session: TerminalSession {
                    id: term.address.clone(),
                    workspace_root: workspace_root.clone(),
                    title: Self::resolve_title(term, None),
                    agent: None,
                    status: TerminalAgentStatus::Idle,
                    state_change_seq: 0,
                    alive,
                },
                last_term: term.clone(),
                idle_guard: PendingIdleConfirmation::default(),
                observer_channel: None,
            });
        entry.last_term = term.clone();

        if entry
            .idle_guard
            .should_hold_working_to_idle(prev_status, status, explicit_protocol, now)
        {
            status = TerminalAgentStatus::Working;
        } else if prev_status == TerminalAgentStatus::Working
            && status == TerminalAgentStatus::Idle
            && !is_active
            && agent.is_some()
        {
            status = TerminalAgentStatus::Done;
        }

        if let Some(ref mut detected) = agent {
            detected.status = status;
            if status == TerminalAgentStatus::Done && !is_active {
                detected.unseen_done = true;
            } else if status == TerminalAgentStatus::Working
                || status == TerminalAgentStatus::Blocked
                || is_active
            {
                detected.unseen_done = false;
                if status == TerminalAgentStatus::Done && is_active {
                    status = TerminalAgentStatus::Idle;
                    detected.status = TerminalAgentStatus::Idle;
                }
            }
        }

        let title = Self::resolve_title(term, agent.as_ref().map(|a| a.label.as_str()));
        let seq = if status != prev_status || entry.session.agent != agent || prev_seq == 0 {
            inner.state_seq.fetch_add(1, Ordering::Relaxed)
        } else {
            prev_seq
        };

        let next = TerminalSession {
            id: term.address.clone(),
            workspace_root,
            title,
            agent,
            status,
            state_change_seq: seq,
            alive,
        };
        entry.session = next.clone();
        next
    }

    fn apply_mark_seen(inner: &mut ServiceInner, session: &mut TerminalSession) -> bool {
        let mut changed = false;
        if session.status == TerminalAgentStatus::Done {
            session.status = TerminalAgentStatus::Idle;
            changed = true;
        }
        if let Some(ref mut agent) = session.agent
            && agent.unseen_done
        {
            agent.unseen_done = false;
            if agent.status == TerminalAgentStatus::Done {
                agent.status = TerminalAgentStatus::Idle;
            }
            changed = true;
        }
        if changed {
            session.state_change_seq = inner.state_seq.fetch_add(1, Ordering::Relaxed);
            if let Some(tracking) = inner.tracked.get_mut(&session.id) {
                tracking.session = session.clone();
            }
        }
        changed
    }

    fn resolve_title(term: &HcTerminal, agent_label: Option<&str>) -> String {
        if let Some(label) = agent_label {
            return label.to_string();
        }
        if !term.title.trim().is_empty() {
            return term.title.trim().to_string();
        }
        if let Some(name) = term.name.as_deref().filter(|s| !s.trim().is_empty()) {
            return name.trim().to_string();
        }
        if let Some(fg) = &term.foreground
            && !fg.name.trim().is_empty()
        {
            return fg.name.trim().to_string();
        }
        "shell".to_string()
    }

    fn canonical_root_string(path: &Path) -> String {
        std::fs::canonicalize(path)
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string()
    }

    fn bound_replay_tail(bytes: &[u8]) -> Vec<u8> {
        if bytes.len() <= MAX_REPLAY_BYTES {
            bytes.to_vec()
        } else {
            bytes[bytes.len() - MAX_REPLAY_BYTES..].to_vec()
        }
    }
}

//! Binary framed OpenRPC client over `herdr-core` (`hc connect [--session <name>]`).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::error::{LgtmTerminalError, Result};

const FRAME_DATA: u8 = 0;
const FRAME_CREDIT: u8 = 1;
const FRAME_CLOSE: u8 = 2;
const FRAME_RESIZE: u8 = 3;
const CONTROL_CHANNEL: u32 = 0;
const MAX_FRAME_LEN: usize = 16 * 1024 * 1024;

/// Foreground process metadata reported by `herdr-core` on a live terminal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HcForeground {
    pub(crate) pid: i32,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) argv: Vec<String>,
    #[serde(default)]
    pub(crate) cwd: String,
}

/// Native OSC 7501 status record reported by `herdr-core`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HcStatus {
    pub(crate) state: String,
    #[serde(default)]
    pub(crate) app: Option<String>,
    #[serde(default)]
    pub(crate) msg: Option<String>,
    #[serde(default)]
    pub(crate) title: Option<String>,
    #[serde(default)]
    pub(crate) kind: Option<String>,
    #[serde(default)]
    pub(crate) progress: Option<u8>,
}

/// Terminal record returned by `herdr-core` (`terminal.run`, `terminal.list`, `terminal.get`, `terminal.attach`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HcTerminal {
    pub(crate) address: String,
    #[serde(default)]
    pub(crate) name: Option<String>,
    #[serde(default)]
    pub(crate) cwd: String,
    #[serde(default)]
    pub(crate) title: String,
    #[serde(default)]
    pub(crate) state: String,
    #[serde(default)]
    pub(crate) exit_code: Option<i32>,
    #[serde(default)]
    pub(crate) labels: HashMap<String, String>,
    #[serde(default)]
    pub(crate) foreground: Option<HcForeground>,
    #[serde(default)]
    pub(crate) status: Option<HcStatus>,
}

/// Reply payload for `terminal.list`.
#[derive(Debug, Clone, Deserialize)]
struct HcListResult {
    #[serde(default)]
    terminals: Vec<HcTerminal>,
}

/// Reply payload for `terminal.attach`.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HcAttachResult {
    pub(crate) channel: u32,
    pub(crate) terminal: HcTerminal,
}

/// Reply payload for `events.subscribe`.
#[derive(Debug, Clone, Deserialize)]
struct HcSubscribeResult {
    channel: Option<u32>,
}

/// Stream frame delivered to an attached terminal or event subscription consumer.
pub(crate) enum ChannelFrame {
    Data(Vec<u8>),
    Closed,
}

type PendingRpcMap = HashMap<u64, mpsc::Sender<Result<Value>>>;
type ChannelSinkMap = HashMap<u32, Box<dyn Fn(ChannelFrame) + Send + Sync>>;

/// Multiplexed framed-mode client connected to a `herdr-core` (`hc`) daemon via `hc connect`.
pub(crate) struct HerdrCoreClient {
    writer: Arc<Mutex<ChildStdin>>,
    pending_rpc: Arc<Mutex<PendingRpcMap>>,
    channels: Arc<Mutex<ChannelSinkMap>>,
    next_id: AtomicU64,
    _child: Mutex<Child>,
}

impl HerdrCoreClient {
    /// Connects to an existing or auto-started `herdr-core` daemon using `hc_binary` and optional `session_name`, switching immediately into binary framed mode.
    pub(crate) fn connect(hc_binary: PathBuf, session_name: Option<String>) -> Result<Self> {
        let mut cmd = Command::new(&hc_binary);
        cmd.arg("connect");
        if let Some(session) = &session_name {
            cmd.arg("--session").arg(session);
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|err| {
            LgtmTerminalError::Protocol(format!(
                "failed to spawn hc connect ({}): {err}",
                hc_binary.display()
            ))
        })?;

        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| LgtmTerminalError::Protocol("missing hc connect stdin".to_string()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| LgtmTerminalError::Protocol("missing hc connect stdout".to_string()))?;
        let mut stderr = child.stderr.take();

        // 1. Plain-mode JSON-RPC handshake switching to framed binary mode.
        let hello = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "hello",
            "params": {
                "framed": true,
                "generation": 1
            }
        });
        let hello_line = format!("{hello}\n");
        stdin.write_all(hello_line.as_bytes())?;
        stdin.flush()?;

        let mut reader = BufReader::new(stdout);
        let mut reply_line = String::new();
        let read_bytes = reader.read_line(&mut reply_line)?;
        if read_bytes == 0 {
            let mut err_text = String::new();
            if let Some(ref mut err_pipe) = stderr {
                let _ = err_pipe.read_to_string(&mut err_text);
            }
            let detail = err_text.trim();
            return Err(LgtmTerminalError::Protocol(if detail.is_empty() {
                "hc connect closed stdout during hello handshake".to_string()
            } else {
                format!("hc connect closed stdout during hello handshake: {detail}")
            }));
        }
        if let Some(mut err_pipe) = stderr {
            let _ = thread::Builder::new()
                .name("lgtm-hc-stderr-drain".to_string())
                .spawn(move || {
                    let mut sink = std::io::sink();
                    let _ = std::io::copy(&mut err_pipe, &mut sink);
                });
        }

        let reply_json: Value = serde_json::from_str(reply_line.trim()).map_err(|err| {
            LgtmTerminalError::Protocol(format!("invalid hello response JSON: {err}"))
        })?;
        if let Some(err_obj) = reply_json.get("error") {
            return Err(LgtmTerminalError::Protocol(format!(
                "hc hello rejected: {err_obj}"
            )));
        }

        let writer = Arc::new(Mutex::new(stdin));
        let pending_rpc: Arc<Mutex<PendingRpcMap>> = Arc::new(Mutex::new(HashMap::new()));
        let channels: Arc<Mutex<ChannelSinkMap>> = Arc::new(Mutex::new(HashMap::new()));

        let reader_writer = Arc::clone(&writer);
        let reader_pending = Arc::clone(&pending_rpc);
        let reader_channels = Arc::clone(&channels);

        thread::Builder::new()
            .name("lgtm-hc-framed-reader".to_string())
            .spawn(move || {
                Self::run_reader_loop(reader, reader_writer, reader_pending, reader_channels);
            })?;

        Ok(Self {
            writer,
            pending_rpc,
            channels,
            next_id: AtomicU64::new(2),
            _child: Mutex::new(child),
        })
    }

    // Public ------------------------------------------------------------------

    /// Lists live (and optionally finished) terminals from `herdr-core`.
    pub(crate) fn terminal_list(&self, include_finished: bool) -> Result<Vec<HcTerminal>> {
        let result = self.rpc_call("terminal.list", json!({ "all": include_finished }))?;
        let parsed: HcListResult = serde_json::from_value(result).map_err(|err| {
            LgtmTerminalError::Protocol(format!("invalid terminal.list payload: {err}"))
        })?;
        Ok(parsed.terminals)
    }

    /// Spawns a new persistent shell terminal in `cwd` tagged with `labels`.
    pub(crate) fn terminal_run(
        &self,
        cwd: &str,
        cols: u16,
        rows: u16,
        labels: HashMap<String, String>,
    ) -> Result<HcTerminal> {
        let result = self.rpc_call(
            "terminal.run",
            json!({
                "cwd": cwd,
                "size": { "cols": cols, "rows": rows },
                "labels": labels,
                "no_notify": true,
                "no_parent": true,
                "outlive_parent": true,
                "wait": false
            }),
        )?;
        serde_json::from_value(result).map_err(|err| {
            LgtmTerminalError::Protocol(format!("invalid terminal.run payload: {err}"))
        })
    }

    /// Attaches a framed stream channel to `terminal_id`, replaying up to `history_lines` of scrollback before live bytes.
    pub(crate) fn terminal_attach(
        &self,
        terminal_id: &str,
        cols: u16,
        rows: u16,
        history_lines: usize,
        sink: Box<dyn Fn(ChannelFrame) + Send + Sync>,
    ) -> Result<HcAttachResult> {
        // Hold channels lock only while inserting once we know the channel ID;
        // herdr-core starts streaming after returning AttachResult, and we also
        // buffer any immediate frames cleanly by registering before returning.
        let result = self.rpc_call(
            "terminal.attach",
            json!({
                "terminal": terminal_id,
                "cols": cols,
                "rows": rows,
                "history": history_lines,
                "observe": false
            }),
        )?;
        let attach: HcAttachResult = serde_json::from_value(result).map_err(|err| {
            LgtmTerminalError::Protocol(format!("invalid terminal.attach payload: {err}"))
        })?;
        if let Ok(mut map) = self.channels.lock() {
            map.insert(attach.channel, sink);
        }
        // Grant initial receive credit so herdr-core streams initial paint & live output immediately.
        let _ = self.send_credit(attach.channel, 256 * 1024);
        Ok(attach)
    }

    /// Opens a lightweight `observe: true` stream channel for `terminal_id` (no history replay, no PTY resize) to monitor OSC 7501 status updates in the background.
    pub(crate) fn terminal_observe(
        &self,
        terminal_id: &str,
        sink: Box<dyn Fn(ChannelFrame) + Send + Sync>,
    ) -> Result<u32> {
        let result = self.rpc_call(
            "terminal.attach",
            json!({
                "terminal": terminal_id,
                "cols": 80,
                "rows": 24,
                "history": 0,
                "observe": true
            }),
        )?;
        let attach: HcAttachResult = serde_json::from_value(result).map_err(|err| {
            LgtmTerminalError::Protocol(format!("invalid terminal.attach observe payload: {err}"))
        })?;
        if let Ok(mut map) = self.channels.lock() {
            map.insert(attach.channel, sink);
        }
        let _ = self.send_credit(attach.channel, 64 * 1024);
        Ok(attach.channel)
    }

    /// Subscribes to `herdr-core` lifecycle and status events on a framed channel.
    pub(crate) fn events_subscribe(
        &self,
        sink: Box<dyn Fn(ChannelFrame) + Send + Sync>,
    ) -> Result<Option<u32>> {
        let result = self.rpc_call("events.subscribe", json!({}))?;
        let sub: HcSubscribeResult = serde_json::from_value(result).map_err(|err| {
            LgtmTerminalError::Protocol(format!("invalid events.subscribe payload: {err}"))
        })?;
        if let Some(channel) = sub.channel {
            if let Ok(mut map) = self.channels.lock() {
                map.insert(channel, sink);
            }
            let _ = self.send_credit(channel, 64 * 1024);
        }
        Ok(sub.channel)
    }

    /// Sends raw keyboard/input bytes on an attached terminal channel.
    pub(crate) fn send_data(&self, channel: u32, bytes: &[u8]) -> Result<()> {
        self.write_frame(channel, FRAME_DATA, bytes)
    }

    /// Sends a `RESIZE` frame (`[cols: u16 LE][rows: u16 LE]`) on an attached terminal channel.
    pub(crate) fn send_resize(&self, channel: u32, cols: u16, rows: u16) -> Result<()> {
        let mut payload = [0u8; 4];
        payload[0..2].copy_from_slice(&cols.to_le_bytes());
        payload[2..4].copy_from_slice(&rows.to_le_bytes());
        self.write_frame(channel, FRAME_RESIZE, &payload)
    }

    /// Closes an attached stream channel without killing the underlying `herdr-core` terminal.
    pub(crate) fn close_channel(&self, channel: u32) -> Result<()> {
        if let Ok(mut map) = self.channels.lock() {
            map.remove(&channel);
        }
        self.write_frame(channel, FRAME_CLOSE, b"{}")
    }

    /// Terminates a `herdr-core` terminal session (`terminal.kill`).
    pub(crate) fn terminal_kill(&self, terminal_id: &str) -> Result<()> {
        let _ = self.rpc_call("terminal.kill", json!({ "terminal": terminal_id }))?;
        Ok(())
    }

    /// Stops the connected `herdr-core` daemon (used by isolated integration test fixtures on teardown).
    pub(crate) fn server_stop(&self) -> Result<()> {
        let _ = self.rpc_call("server.stop", json!({}))?;
        Ok(())
    }

    // Private -----------------------------------------------------------------

    fn rpc_call(&self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        {
            let mut map = self
                .pending_rpc
                .lock()
                .map_err(|_| LgtmTerminalError::Protocol("RPC map poisoned".to_string()))?;
            map.insert(id, tx);
        }

        let payload = serde_json::to_vec(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        }))
        .map_err(|err| LgtmTerminalError::Protocol(format!("serialize RPC error: {err}")))?;

        if let Err(err) = self.write_frame(CONTROL_CHANNEL, FRAME_DATA, &payload) {
            if let Ok(mut map) = self.pending_rpc.lock() {
                map.remove(&id);
            }
            return Err(err);
        }

        rx.recv().map_err(|_| {
            LgtmTerminalError::Protocol(format!("hc connection closed while awaiting {method}"))
        })?
    }

    fn send_credit(&self, channel: u32, bytes: u32) -> Result<()> {
        self.write_frame(channel, FRAME_CREDIT, &bytes.to_le_bytes())
    }

    fn write_frame(&self, channel: u32, kind: u8, payload: &[u8]) -> Result<()> {
        Self::write_frame_with_writer(&self.writer, channel, kind, payload)
    }

    fn write_frame_with_writer(
        writer: &Arc<Mutex<ChildStdin>>,
        channel: u32,
        kind: u8,
        payload: &[u8],
    ) -> Result<()> {
        let mut header = [0u8; 9];
        header[0..4].copy_from_slice(&channel.to_le_bytes());
        header[4] = kind;
        header[5..9].copy_from_slice(&(payload.len() as u32).to_le_bytes());

        let mut guard = writer
            .lock()
            .map_err(|_| LgtmTerminalError::Protocol("hc writer mutex poisoned".to_string()))?;
        guard.write_all(&header)?;
        if !payload.is_empty() {
            guard.write_all(payload)?;
        }
        guard.flush()?;
        Ok(())
    }

    fn run_reader_loop(
        mut reader: BufReader<std::process::ChildStdout>,
        writer: Arc<Mutex<ChildStdin>>,
        pending_rpc: Arc<Mutex<PendingRpcMap>>,
        channels: Arc<Mutex<ChannelSinkMap>>,
    ) {
        let mut header = [0u8; 9];
        loop {
            if reader.read_exact(&mut header).is_err() {
                break;
            }
            let channel = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
            let kind = header[4];
            let len = u32::from_le_bytes([header[5], header[6], header[7], header[8]]) as usize;
            if len > MAX_FRAME_LEN {
                tracing::warn!(channel, len, "dropping oversized herdr-core frame");
                break;
            }

            let mut payload = vec![0u8; len];
            if len > 0 && reader.read_exact(&mut payload).is_err() {
                break;
            }

            if channel == CONTROL_CHANNEL && kind == FRAME_DATA {
                Self::dispatch_control_frame(&pending_rpc, &payload);
                continue;
            }

            match kind {
                FRAME_DATA => {
                    if let Ok(map) = channels.lock()
                        && let Some(sink) = map.get(&channel)
                    {
                        sink(ChannelFrame::Data(payload));
                    }
                    if len > 0 {
                        let _ = Self::write_frame_with_writer(
                            &writer,
                            channel,
                            FRAME_CREDIT,
                            &(len as u32).to_le_bytes(),
                        );
                    }
                }
                FRAME_CLOSE => {
                    let sink = channels
                        .lock()
                        .ok()
                        .and_then(|mut map| map.remove(&channel));
                    if let Some(sink) = sink {
                        sink(ChannelFrame::Closed);
                    }
                }
                _ => {}
            }
        }

        // Fail any in-flight RPC callers on EOF.
        if let Ok(mut map) = pending_rpc.lock() {
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(LgtmTerminalError::Protocol(
                    "herdr-core connection closed".to_string(),
                )));
            }
        }
    }

    fn dispatch_control_frame(pending_rpc: &Arc<Mutex<PendingRpcMap>>, payload: &[u8]) {
        let Ok(value) = serde_json::from_slice::<Value>(payload) else {
            return;
        };
        let Some(id) = value.get("id").and_then(Value::as_u64) else {
            return;
        };
        let tx = pending_rpc.lock().ok().and_then(|mut map| map.remove(&id));
        let Some(tx) = tx else {
            return;
        };

        if let Some(err_obj) = value.get("error") {
            let kind = err_obj
                .get("data")
                .and_then(|d| d.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("protocol");
            let message = err_obj
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("herdr-core RPC error");
            let mapped = if kind.contains("not_found") || kind.contains("unknown") {
                LgtmTerminalError::NotFound(message.to_string())
            } else {
                LgtmTerminalError::Protocol(format!("{kind}: {message}"))
            };
            let _ = tx.send(Err(mapped));
            return;
        }

        let result = value.get("result").cloned().unwrap_or(Value::Null);
        let _ = tx.send(Ok(result));
    }
}

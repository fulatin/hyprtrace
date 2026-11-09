//! DeepSeek Harness AI runtime — the one and only AI engine of HyprTrace.
//!
//! The server keeps a `dsh --profile sdk --patch <overlay>` child process alive
//! and drives it over the harness SDK's newline-delimited JSON-RPC protocol
//! (`initialize`, `session/prompt`, `session/resume`, `session.event`,
//! `session.status`). A companion plugin loaded into that child
//! (`integrations/dsh/hyprtrace-bridge.mjs`, see [`super::bridge`]) supplies
//! what the SDK protocol does not: token-level stream frames, reasoning deltas,
//! cancellation, and the live model/tool inventory.
//!
//! ```text
//! web AIChat ──NDJSON──▶ routes::ai ──▶ AiService ──▶ dsh child (agent, tools, history)
//!                                          ▲  │              │
//!                                          │  └── SDK JSON-RPC
//!                                          └───── Unix socket ── bridge plugin
//! ```
//!
//! Chat, non-streaming requests, the weekly report and the proactive monitor
//! all run through this service; there is no in-process provider stack.
//!
//! Design notes:
//! - One child process, many conversations: `conversation_id` is used directly
//!   as the DSH session id, so sessions are isolated without an extra mapping.
//! - The child is (re)started lazily. If it dies, the next turn respawns it and
//!   resumes the conversation's persisted session.
//! - Turns are streamed through per-session channels: a dispatcher task routes
//!   SDK notifications and bridge frames by session id, so concurrent
//!   conversations cannot see each other's events.
//! - The MCP tool guard checks *every* `request/header`: a harness that boots
//!   without its HyprTrace tools would otherwise answer from guesswork.

use super::bridge::Bridge;
use crate::config::DshConfig;
use anyhow::{anyhow, bail, Context, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, watch, Mutex};
use tokio::time::{timeout, Instant};

/// Control-plane requests answer immediately; model work streams afterwards.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(30);

/// One event forwarded to the chat transport (the NDJSON wire contract).
#[derive(Debug, Clone, PartialEq)]
pub enum AiEvent {
    Text(String),
    Reasoning(String),
    ToolCall {
        id: String,
        name: String,
        args: Value,
    },
    ToolResult {
        id: String,
        name: String,
        ok: bool,
        result: Value,
    },
    Cancelled,
    Done(String),
    Error(String),
}

impl AiEvent {
    /// Wire shape parsed by `web/src/lib/transport.ts` (one JSON per line).
    pub fn to_json(&self) -> Value {
        match self {
            AiEvent::Text(delta) => json!({"type": "text", "delta": delta}),
            AiEvent::Reasoning(delta) => json!({"type": "reasoning", "delta": delta}),
            AiEvent::ToolCall { id, name, args } => {
                json!({"type": "tool_call", "id": id, "name": name, "args": args})
            }
            AiEvent::ToolResult {
                id,
                name,
                ok,
                result,
            } => json!({
                "type": "tool_result",
                "id": id,
                "name": name,
                "ok": ok,
                "result": result,
            }),
            AiEvent::Cancelled => json!({"type": "cancelled"}),
            AiEvent::Done(_) => json!({"type": "done"}),
            AiEvent::Error(message) => json!({"type": "error", "message": message}),
        }
    }
}

/// Public health snapshot (`GET /api/ai/status`, also embedded in models/config).
#[derive(Debug, Clone, Serialize, Default)]
pub struct AiStatus {
    pub enabled: bool,
    /// A harness child is alive and initialized.
    pub ready: bool,
    pub provider: String,
    pub model: String,
    pub bridge_connected: bool,
    pub tool_count: usize,
    pub error: Option<String>,
}

/// Raw item delivered to a running turn.
enum RawItem {
    Sdk(Value),
    Bridge(Value),
}

struct SessionSlot {
    tx: mpsc::Sender<RawItem>,
}

/// One live harness child process plus its JSON-RPC plumbing.
struct SdkConn {
    child: Mutex<Child>,
    stdin_tx: mpsc::Sender<String>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>,
    next_id: AtomicU64,
    alive: Arc<AtomicBool>,
    exit: watch::Sender<bool>,
    /// Incremented on every respawn; used to know when a conversation needs
    /// `session/resume` on the new process.
    generation: u64,
}

impl SdkConn {
    async fn spawn(
        cfg: &DshConfig,
        overlay: &Path,
        env: &[(String, String)],
        cwd: &Path,
        sessions: Arc<Mutex<HashMap<String, SessionSlot>>>,
        generation: u64,
    ) -> Result<Self> {
        let mut cmd = Command::new(&cfg.command);
        cmd.arg("--profile").arg(&cfg.profile);
        cmd.arg("--patch").arg(overlay);
        for arg in &cfg.extra_args {
            cmd.arg(arg);
        }
        cmd.current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for (key, value) in env {
            cmd.env(key, value);
        }
        let mut child = cmd.spawn().with_context(|| {
            format!(
                "failed to spawn `{}` (is the DeepSeek Harness CLI installed?)",
                cfg.command
            )
        })?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("dsh: stdin pipe unavailable"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("dsh: stdout pipe unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow!("dsh: stderr pipe unavailable"))?;

        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>> =
            Arc::default();
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<String>(64);
        let alive = Arc::new(AtomicBool::new(true));
        let (exit, _exit_rx) = watch::channel(false);

        // Writer: serialize frames onto the child's stdin.
        tokio::spawn(async move {
            let mut stdin = stdin;
            while let Some(line) = stdin_rx.recv().await {
                let ok = stdin.write_all(line.as_bytes()).await.is_ok()
                    && stdin.write_all(b"\n").await.is_ok()
                    && stdin.flush().await.is_ok();
                if !ok {
                    break;
                }
            }
        });

        // Harness logs go to stderr; stdout is reserved for JSON-RPC.
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                log::debug!("dsh: {line}");
            }
        });

        // Reader: demultiplex responses from session notifications and route
        // the latter to the owning conversation's channel.
        let pending_reader = pending.clone();
        let alive_reader = alive.clone();
        let exit_reader = exit.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            loop {
                match lines.next_line().await {
                    Ok(Some(line)) => {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        let Ok(frame) = serde_json::from_str::<Value>(line) else {
                            log::warn!("dsh: ignored non-JSON stdout line: {}", truncate(line, 200));
                            continue;
                        };
                        if frame.get("method").is_none() {
                            if let Some(id) = frame.get("id").and_then(Value::as_str) {
                                if let Some(tx) = pending_reader.lock().await.remove(id) {
                                    let outcome = match frame.get("error") {
                                        Some(error) => Err(error.to_string()),
                                        None => {
                                            Ok(frame.get("result").cloned().unwrap_or(Value::Null))
                                        }
                                    };
                                    let _ = tx.send(outcome);
                                }
                                continue;
                            }
                        }
                        route_sdk_frame(&sessions, frame).await;
                    }
                    Ok(None) => break,
                    Err(error) => {
                        log::warn!("dsh: stdout read failed: {error}");
                        break;
                    }
                }
            }
            alive_reader.store(false, Ordering::SeqCst);
            let _ = exit_reader.send(true);
            let mut pending = pending_reader.lock().await;
            for (_, tx) in pending.drain() {
                let _ = tx.send(Err("dsh process exited".to_string()));
            }
        });

        Ok(Self {
            child: Mutex::new(child),
            stdin_tx,
            pending,
            next_id: AtomicU64::new(1),
            alive,
            exit,
            generation,
        })
    }

    fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    fn exit_signal(&self) -> watch::Receiver<bool> {
        self.exit.subscribe()
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value> {
        if !self.is_alive() {
            bail!("dsh process is not running");
        }
        let id = format!("hz-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        let frame = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        let line = serde_json::to_string(&frame)?;
        if self.stdin_tx.send(line).await.is_err() {
            self.pending.lock().await.remove(&id);
            bail!("dsh stdin is closed");
        }
        match timeout(CONTROL_TIMEOUT, rx).await {
            Ok(Ok(Ok(result))) => Ok(result),
            Ok(Ok(Err(message))) => Err(anyhow!("{method} failed: {message}")),
            Ok(Err(_)) => Err(anyhow!("{method}: response channel closed")),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(anyhow!("{method}: timed out after {CONTROL_TIMEOUT:?}"))
            }
        }
    }

    async fn kill(&self) {
        let mut child = self.child.lock().await;
        let _ = child.start_kill();
        let _ = child.wait().await;
    }
}

/// Route one SDK notification to the conversation it belongs to.
async fn route_sdk_frame(sessions: &Mutex<HashMap<String, SessionSlot>>, frame: Value) {
    let Some(session_id) = frame
        .pointer("/params/sessionId")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    let sender = {
        let sessions = sessions.lock().await;
        sessions.get(&session_id).map(|slot| slot.tx.clone())
    };
    if let Some(sender) = sender {
        let _ = sender.send(RawItem::Sdk(frame)).await;
    }
}

/// Everything the AI runtime knows about.
pub struct AiService {
    cfg: std::sync::RwLock<DshConfig>,
    api_base: String,
    auth_token: Option<String>,
    data_dir: PathBuf,
    conn: Mutex<Option<Arc<SdkConn>>>,
    bridge: Mutex<Option<Arc<Bridge>>>,
    sessions: Arc<Mutex<HashMap<String, SessionSlot>>>,
    /// conversation id → child generation whose process has been told to
    /// resume that session.
    resumed: Mutex<HashMap<String, u64>>,
    start_lock: Mutex<()>,
    default_session: Mutex<String>,
    /// Model override requested by the latest turn (falls back to config).
    active_model: Mutex<Option<String>>,
    /// Monotonic child-process counter; bumped on every spawn so per-session
    /// resume bookkeeping knows a new process took over.
    child_generation: AtomicU64,
    last_error: Mutex<Option<String>>,
    tool_count: std::sync::atomic::AtomicUsize,
}

impl AiService {
    pub fn new(cfg: DshConfig, api_base: String, auth_token: Option<String>) -> Self {
        Self::with_data_dir(cfg, api_base, auth_token, dsh_data_dir())
    }

    fn with_data_dir(
        cfg: DshConfig,
        api_base: String,
        auth_token: Option<String>,
        data_dir: PathBuf,
    ) -> Self {
        let default_session = load_or_create_session(&data_dir);
        Self {
            cfg: std::sync::RwLock::new(cfg),
            api_base,
            auth_token,
            data_dir,
            conn: Mutex::new(None),
            bridge: Mutex::new(None),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            resumed: Mutex::new(HashMap::new()),
            start_lock: Mutex::new(()),
            default_session: Mutex::new(default_session),
            active_model: Mutex::new(None),
            child_generation: AtomicU64::new(0),
            last_error: Mutex::new(None),
            tool_count: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    pub fn enabled(&self) -> bool {
        self.config_snapshot().enabled
    }

    fn config_snapshot(&self) -> DshConfig {
        self.cfg
            .read()
            .map(|cfg| cfg.clone())
            .unwrap_or_default()
    }

    /// Health snapshot. Never starts the harness: a page load must not pay a
    /// multi-second child boot just to draw a status dot.
    pub async fn status(&self) -> AiStatus {
        let cfg = self.config_snapshot();
        let ready = {
            let conn = self.conn.lock().await;
            conn.as_ref().is_some_and(|conn| conn.is_alive())
        };
        let bridge_connected = self.bridge_connected().await;
        let active_model = self.active_model.lock().await.clone();
        AiStatus {
            enabled: cfg.enabled,
            ready,
            provider: cfg.provider.clone(),
            model: active_model.unwrap_or_else(|| cfg.model.clone()),
            bridge_connected,
            tool_count: self.tool_count.load(Ordering::Relaxed),
            error: self.last_error.lock().await.clone(),
        }
    }

    /// Discoverable models on the configured provider. Fast when the harness is
    /// not running yet (returns the configured model); `refresh` boots the
    /// harness when needed so the settings page can test connectivity.
    pub async fn models(self: &Arc<Self>, refresh: bool) -> Vec<String> {
        let cfg = self.config_snapshot();
        if !cfg.enabled {
            return Vec::new();
        }
        if refresh {
            if let Err(error) = self.ensure_started(None).await {
                log::warn!("DSH: model discovery failed: {error}");
                return vec![cfg.model.clone()];
            }
        }
        // Only query a *running* harness. A page load / post-turn status refresh
        // must never spawn `dsh` as a side effect (it used to: after a failed
        // turn the bridge plugin reconnects, `bridge_connected` is true while
        // the child is gone, and the next plain models GET booted it again).
        let running = {
            let conn = self.conn.lock().await;
            conn.as_ref().is_some_and(|conn| conn.is_alive())
        };
        if !running {
            return vec![cfg.model.clone()];
        }
        match self.bridge_command(json!({"cmd": "models", "providers": [cfg.provider]})).await {
            Ok(reply) => {
                let models = reply
                    .pointer(&format!("/models/{}", escape_json_pointer(&cfg.provider)))
                    .and_then(Value::as_array)
                    .map(|entries| {
                        entries
                            .iter()
                            .filter_map(|entry| entry.get("id").and_then(Value::as_str))
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if models.is_empty() {
                    vec![cfg.model.clone()]
                } else {
                    models
                }
            }
            Err(error) => {
                log::debug!("DSH: model inventory unavailable: {error}");
                vec![cfg.model.clone()]
            }
        }
    }

    /// Tool inventory (name + description) as the model actually sees it.
    pub async fn tools(self: &Arc<Self>, session_id: Option<&str>) -> Vec<Value> {
        let mut command = json!({"cmd": "tools"});
        if let Some(session_id) = session_id {
            command["sessionId"] = Value::String(session_id.to_string());
        }
        match self.bridge_command(command).await {
            Ok(reply) => {
                let tools = reply.get("tools").and_then(Value::as_array).cloned().unwrap_or_default();
                self.tool_count.store(tools.len(), Ordering::Relaxed);
                tools
            }
            Err(_) => Vec::new(),
        }
    }

    /// Apply a config change; the running harness is discarded so the next turn
    /// boots with the new route/overlay.
    pub async fn reconfigure(&self, cfg: DshConfig) {
        let changed = {
            let current = self.config_snapshot();
            current.enabled != cfg.enabled
                || current.command != cfg.command
                || current.profile != cfg.profile
                || current.provider != cfg.provider
                || current.model != cfg.model
                || current.api_key_env != cfg.api_key_env
                || current.base_url != cfg.base_url
                || current.api != cfg.api
                || current.cwd != cfg.cwd
                || current.tools_script != cfg.tools_script
                || current.overlay_template != cfg.overlay_template
                || current.bridge_script != cfg.bridge_script
                || current.extra_args != cfg.extra_args
        };
        if let Ok(mut current) = self.cfg.write() {
            *current = cfg;
        }
        if changed {
            self.drop_runtime().await;
        }
    }

    /// Cancel one conversation's active turn. Returns false when no turn runs.
    pub async fn cancel(&self, session_id: &str) -> bool {
        let active = self.sessions.lock().await.contains_key(session_id);
        if !active {
            return false;
        }
        match self
            .bridge_command(json!({"cmd": "cancel", "sessionId": session_id}))
            .await
        {
            // The plugin answers `cancelled: false` when the session has no live
            // agent (a turn may have just finished); only real failures error.
            Ok(reply) => reply
                .get("cancelled")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            Err(error) => {
                log::warn!("DSH: cancel failed: {error}");
                false
            }
        }
    }

    /// Cancel every in-flight conversation (used by "clear conversations").
    pub async fn cancel_all(&self) {
        let ids: Vec<String> = self.sessions.lock().await.keys().cloned().collect();
        for id in ids {
            self.cancel(&id).await;
        }
    }

    /// Default conversation id for requests from older clients.
    pub async fn default_session(&self) -> String {
        self.default_session.lock().await.clone()
    }

    /// Start a fresh default conversation (used when clearing chat history so
    /// clients that do not send a `conversation_id` do not inherit old memory).
    pub async fn reset_default_session(&self) -> String {
        let mut current = self.default_session.lock().await;
        let id = new_session_id(&self.data_dir);
        *current = id.clone();
        id
    }

    /// Run one chat turn, streaming [`AiEvent`]s onto `tx`.
    ///
    /// Errors that prevented the turn from starting are returned; failures
    /// after streaming began are delivered as [`AiEvent::Error`].
    pub async fn prompt(
        self: &Arc<Self>,
        session_id: &str,
        text: &str,
        model: Option<&str>,
        tx: mpsc::Sender<AiEvent>,
    ) -> Result<()> {
        self.ensure_started(model).await?;
        let conn = self.current_conn().await?;
        let (raw_tx, mut raw_rx) = mpsc::channel::<RawItem>(512);
        {
            let mut sessions = self.sessions.lock().await;
            if sessions.contains_key(session_id) {
                bail!("this conversation already has a running turn");
            }
            sessions.insert(session_id.to_string(), SessionSlot { tx: raw_tx });
        }

        // Make sure a respawned harness reattaches to this conversation's
        // persisted session before the prompt is queued.
        let needs_resume = {
            let resumed = self.resumed.lock().await;
            resumed.get(session_id).copied() != Some(conn.generation)
        };
        if needs_resume {
            let resume = conn
                .request("session/resume", json!({"sessionId": session_id}))
                .await;
            if let Err(error) = resume {
                log::debug!("DSH: session {session_id} not resumed ({error})");
            }
            {
                let mut resumed = self.resumed.lock().await;
                resumed.insert(session_id.to_string(), conn.generation);
                // Conversations are unbounded over a long server lifetime; keep
                // only the bookkeeping for the current harness generation.
                if resumed.len() > 512 {
                    let generation = conn.generation;
                    resumed.retain(|_, value| *value == generation);
                }
            }
        }

        let mut exit = conn.exit_signal();
        if *exit.borrow() {
            self.sessions.lock().await.remove(session_id);
            self.drop_conn().await;
            return Err(anyhow!("DSH: the harness process is not running"));
        }

        let prompt = json!({
            "sessionId": session_id,
            "contentBlocks": [{"type": "text", "text": text}],
        });
        if let Err(error) = conn.request("session/prompt", prompt).await {
            self.sessions.lock().await.remove(session_id);
            self.drop_conn().await;
            return Err(anyhow!("DSH: {error}"));
        }

        let cfg = self.config_snapshot();
        let budget = Duration::from_secs(cfg.request_timeout_seconds.max(30));
        let deadline = Instant::now() + budget;
        let mut idle_grace: Option<Instant> = None;
        let mut state = TurnState::default();
        let mut terminal: Option<Terminal> = None;

        loop {
            let effective_deadline = idle_grace.map_or(deadline, |grace| grace.min(deadline));
            let remaining = effective_deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                if state.saw_turn_end {
                    break;
                }
                terminal = Some(Terminal::Timeout(budget.as_secs()));
                break;
            }
            let item = tokio::select! {
                biased;
                changed = exit.changed() => {
                    if *exit.borrow() || changed.is_err() {
                        terminal = Some(Terminal::Exited);
                        break;
                    }
                    continue;
                }
                item = timeout(remaining, raw_rx.recv()) => match item {
                    Ok(Some(item)) => item,
                    Ok(None) => {
                        terminal = Some(Terminal::Exited);
                        break;
                    }
                    Err(_) => {
                        if state.saw_turn_end {
                            break;
                        }
                        terminal = Some(Terminal::Timeout(budget.as_secs()));
                        break;
                    }
                },
            };

            let events = match item {
                RawItem::Sdk(frame) => state.handle_sdk(&frame, &cfg.tools_prefix),
                RawItem::Bridge(frame) => state.handle_bridge(&frame),
            };
            for event in events {
                let _ = tx.send(event).await;
            }
            if let Some(abort) = state.abort_message.take() {
                // Surface the cause in /api/ai/status (the settings health
                // panel) as well as in the chat stream.
                *self.last_error.lock().await = Some(abort.clone());
                let _ = tx.send(AiEvent::Error(abort)).await;
                self.drop_conn().await;
                state.failed = true;
                break;
            }
            if state.finished {
                break;
            }
            if state.saw_turn_end {
                idle_grace = Some(Instant::now() + Duration::from_secs(5));
                if state.failed || state.cancelled {
                    break;
                }
            }
        }

        self.sessions.lock().await.remove(session_id);

        match terminal {
            Some(Terminal::Timeout(seconds)) => {
                log::warn!("DSH: turn exceeded the {seconds}s budget; discarding the harness");
                let _ = tx
                    .send(AiEvent::Error(format!(
                        "DSH: turn timed out after {seconds}s"
                    )))
                    .await;
                self.drop_conn().await;
            }
            Some(Terminal::Exited) => {
                log::warn!("DSH: harness process exited mid-turn");
                let _ = tx
                    .send(AiEvent::Error(
                        "DSH: the harness process exited mid-turn".to_string(),
                    ))
                    .await;
                self.drop_conn().await;
            }
            None => {
                if state.cancelled {
                    let _ = tx.send(AiEvent::Cancelled).await;
                } else if state.failed {
                    // The error itself was already streamed.
                } else {
                    *self.last_error.lock().await = None;
                    let _ = tx.send(AiEvent::Done("stop".to_string())).await;
                }
            }
        }
        Ok(())
    }

    /// Non-streaming helper for the endpoints that only need the final text.
    pub async fn complete(self: &Arc<Self>, session_id: &str, text: &str) -> Result<String> {
        let (tx, mut rx) = mpsc::channel::<AiEvent>(256);
        let prompt = text.to_string();
        let session = session_id.to_string();
        let service = Arc::clone(self);
        let work = async move { service.prompt(&session, &prompt, None, tx).await };
        tokio::pin!(work);
        let mut work_done = false;
        let mut output = String::new();
        let mut failure: Option<String> = None;
        loop {
            tokio::select! {
                result = &mut work, if !work_done => {
                    work_done = true;
                    if let Err(error) = result {
                        failure.get_or_insert_with(|| error.to_string());
                    }
                }
                event = rx.recv() => match event {
                    Some(AiEvent::Text(delta)) => output.push_str(&delta),
                    Some(AiEvent::Error(message)) => {
                        failure.get_or_insert(message);
                    }
                    Some(_) => {}
                    None => break,
                },
            }
        }
        match failure {
            Some(message) => Err(anyhow!(message)),
            None if output.trim().is_empty() => Err(anyhow!("DSH returned an empty answer")),
            None => Ok(output),
        }
    }

    // ---- lifecycle ----

    async fn bridge_connected(&self) -> bool {
        self.bridge
            .lock()
            .await
            .as_ref()
            .is_some_and(|bridge| bridge.is_connected())
    }

    async fn bridge_command(&self, command: Value) -> Result<Value> {
        let bridge = self.bridge.lock().await.clone();
        match bridge {
            Some(bridge) => bridge.command(command).await,
            None => bail!("the DSH bridge is not running"),
        }
    }

    async fn current_conn(&self) -> Result<Arc<SdkConn>> {
        match self.conn.lock().await.as_ref() {
            Some(conn) if conn.is_alive() => Ok(conn.clone()),
            _ => bail!("the DSH harness is not running"),
        }
    }

    /// Ensure a live, initialized child for `model` (config default when None).
    /// Changing the model restarts the child; the conversation session is
    /// resumed by [`Self::prompt`] on the new generation.
    async fn ensure_started(self: &Arc<Self>, model: Option<&str>) -> Result<()> {
        let cfg = self.config_snapshot();
        if !cfg.enabled {
            bail!("AI is disabled: set [ai.dsh] enabled = true in ~/.config/hyprtrace/config.toml");
        }
        let requested_model = model
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| cfg.model.clone());

        let _guard = self.start_lock.lock().await;

        // Already running with the right model?
        {
            let conn = self.conn.lock().await;
            let active = self.active_model.lock().await.clone();
            if let Some(existing) = conn.as_ref() {
                if existing.is_alive() && active.as_deref() == Some(requested_model.as_str()) {
                    return Ok(());
                }
            }
        }
        // A different model (or a dead child): restart.
        self.drop_conn().await;

        let bridge = self.ensure_bridge().await?;
        let overlay = self.write_overlay(&cfg)?;
        let cwd = resolve_cwd(&cfg)?;
        let env = vec![
            ("HYPRTRACE_API_BASE".to_string(), self.api_base.clone()),
            (
                "HYPRTRACE_AUTH_TOKEN".to_string(),
                self.auth_token.clone().unwrap_or_default(),
            ),
            (
                "HYPRTRACE_BRIDGE_SOCK".to_string(),
                bridge.path().to_string_lossy().to_string(),
            ),
        ];
        let generation = self.child_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let conn = match SdkConn::spawn(
            &cfg,
            &overlay,
            &env,
            &cwd,
            self.sessions.clone(),
            generation,
        )
        .await
        {
            Ok(conn) => Arc::new(conn),
            Err(error) => {
                *self.last_error.lock().await = Some(error.to_string());
                return Err(error);
            }
        };

        let init = json!({
            "cwd": cwd.to_string_lossy(),
            "provider": cfg.provider,
            "model": requested_model,
        });
        if let Err(error) = conn.request("initialize", init).await {
            conn.kill().await;
            *self.last_error.lock().await = Some(format!("initialize failed: {error}"));
            return Err(anyhow!("DSH initialize failed: {error}"));
        }
        *self.conn.lock().await = Some(conn);
        *self.active_model.lock().await = Some(requested_model.clone());
        *self.last_error.lock().await = None;
        log::info!(
            "dsh: harness ready (provider={}, model={})",
            cfg.provider,
            requested_model
        );
        // Populate the tool-count cache in the background; status() must stay
        // cheap and synchronous-ish for page loads.
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let _ = service.tools(None).await;
        });
        Ok(())
    }

    async fn ensure_bridge(self: &Arc<Self>) -> Result<Arc<Bridge>> {
        {
            let bridge = self.bridge.lock().await;
            if let Some(bridge) = bridge.as_ref() {
                return Ok(bridge.clone());
            }
        }
        let path = self.data_dir.join("bridge.sock");
        let bridge = Bridge::bind(path)?;
        *self.bridge.lock().await = Some(bridge.clone());

        // Router: bridge frames (stream/status/agent-error/hello) go to the
        // conversation named by the frame.
        let sessions = self.sessions.clone();
        let mut frames = bridge.subscribe();
        tokio::spawn(async move {
            loop {
                match frames.recv().await {
                    Ok(frame) => {
                        let Some(session_id) = frame
                            .get("sessionId")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                        else {
                            continue;
                        };
                        let sender = {
                            let sessions = sessions.lock().await;
                            sessions.get(&session_id).map(|slot| slot.tx.clone())
                        };
                        if let Some(sender) = sender {
                            let _ = sender.send(RawItem::Bridge(frame)).await;
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(skipped)) => {
                        log::warn!("dsh bridge: router lagged, skipped {skipped} frames");
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        });
        Ok(bridge)
    }

    async fn drop_conn(&self) {
        let conn = self.conn.lock().await.take();
        if let Some(conn) = conn {
            conn.kill().await;
        }
    }

    async fn drop_runtime(&self) {
        self.drop_conn().await;
        let bridge = self.bridge.lock().await.take();
        self.resumed.lock().await.clear();
        *self.active_model.lock().await = None;
        *self.last_error.lock().await = None;
        self.tool_count.store(0, Ordering::Relaxed);
        drop(bridge);
    }

    /// Render the profile patch and write it where `--patch` can read it.
    fn write_overlay(&self, cfg: &DshConfig) -> Result<PathBuf> {
        let template_path =
            resolve_asset(cfg.overlay_template.as_deref(), "overlay.template.yml")?;
        let tools_script = resolve_asset(cfg.tools_script.as_deref(), "hyprtrace-mcp.mjs")?;
        let bridge_script = resolve_asset(cfg.bridge_script.as_deref(), "hyprtrace-bridge.mjs")?;
        let template = std::fs::read_to_string(&template_path)
            .with_context(|| format!("failed to read {}", template_path.display()))?;
        let rendered = render_overlay(&template, cfg, &tools_script, &bridge_script);
        // Empty optional placeholders leave whitespace-only indentation behind.
        let rendered = rendered
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n");
        if rendered.contains("{{") {
            bail!(
                "DSH overlay template {} contains an unreplaced placeholder",
                template_path.display()
            );
        }
        std::fs::create_dir_all(&self.data_dir)
            .with_context(|| format!("failed to create {}", self.data_dir.display()))?;
        let path = self.data_dir.join("overlay.yml");
        std::fs::write(&path, rendered)
            .with_context(|| format!("failed to write {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(path)
    }
}

/// Upper bound on tool calls in a single turn. The DSH harness bounds the
/// *rounds* of an agent loop, not the number of tools a model may invoke, and
/// each tool call re-feeds its (potentially large) result into the context.
/// Twenty is far above any normal HyprTrace question while still stopping a
/// runaway loop before the context budget is exhausted (issue #18).
const MAX_TOOL_CALLS_PER_TURN: usize = 20;

/// Why the turn loop stopped without the harness finishing cleanly.
enum Terminal {
    Timeout(u64),
    Exited,
}

/// Per-turn mapping state.
#[derive(Default)]
struct TurnState {
    tool_names: HashMap<String, String>,
    /// (turn, step) pairs whose text/reasoning already streamed through the
    /// bridge; the settled `assistant/message` must not duplicate them.
    streamed: HashSet<(i64, i64)>,
    attempts: HashMap<String, (i64, i64)>,
    /// Number of `tool/call` events seen in this turn (budget enforcement).
    tool_calls: usize,
    saw_turn_end: bool,
    finished: bool,
    failed: bool,
    cancelled: bool,
    abort_message: Option<String>,
}

impl TurnState {
    fn handle_sdk(&mut self, frame: &Value, tools_prefix: &str) -> Vec<AiEvent> {
        let method = frame.get("method").and_then(Value::as_str).unwrap_or_default();
        match method {
            "session.event" => {
                let event = frame.pointer("/params/event").cloned().unwrap_or(Value::Null);
                self.handle_session_event(&event, tools_prefix)
            }
            "session.status" => {
                let idle = frame.pointer("/params/status").and_then(Value::as_str) == Some("idle");
                if idle && self.saw_turn_end {
                    self.finished = true;
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    fn handle_session_event(&mut self, event: &Value, tools_prefix: &str) -> Vec<AiEvent> {
        let event_type = event.get("type").and_then(Value::as_str).unwrap_or_default();
        let data = event.get("data").cloned().unwrap_or(Value::Null);
        match event_type {
            "request/header" => {
                // Check *every* header: the harness re-emits it whenever it
                // rebuilds the request envelope. A later tool-less header means
                // the MCP bridge went away mid-conversation and the model would
                // answer from guesswork.
                let header = data.get("header").cloned().unwrap_or(Value::Null);
                if !header_exposes_tools(&header, tools_prefix) {
                    self.failed = true;
                    self.abort_message = Some(format!(
                        "HyprTrace tools are missing inside the harness (no `{}*` tool \
                         was offered to the model). Check that `node` is installed, \
                         [ai.dsh].tools_script points at hyprtrace-mcp.mjs, and the \
                         bridge starts cleanly. Tools offered: {}.",
                        tools_prefix,
                        tool_names_preview(&header),
                    ));
                }
                Vec::new()
            }
            "assistant/message" => {
                let turn = data.get("turn").and_then(Value::as_i64).unwrap_or_default();
                let step = data.get("step").and_then(Value::as_i64).unwrap_or_default();
                if self.streamed.contains(&(turn, step)) {
                    return Vec::new();
                }
                let content = data
                    .pointer("/message/content")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let mut events = Vec::new();
                for block in content {
                    let block_type = block.get("type").and_then(Value::as_str).unwrap_or_default();
                    let text = block.get("text").and_then(Value::as_str).unwrap_or_default();
                    if text.is_empty() {
                        continue;
                    }
                    match block_type {
                        "text" => events.push(AiEvent::Text(text.to_string())),
                        "reasoning" => events.push(AiEvent::Reasoning(text.to_string())),
                        _ => {}
                    }
                }
                events
            }
            "tool/call" => {
                let Some(id) = data.get("callId").and_then(Value::as_str) else {
                    return Vec::new();
                };
                self.tool_calls += 1;
                if self.tool_calls > MAX_TOOL_CALLS_PER_TURN {
                    // Stop before executing the call: the turn loop turns
                    // `abort_message` into an `error` event and discards the
                    // harness (issue #18).
                    self.failed = true;
                    if self.abort_message.is_none() {
                        self.abort_message = Some(format!(
                            "Stopped after {MAX_TOOL_CALLS_PER_TURN} tool calls in one turn (tool-call budget reached). Ask a narrower question or retry."
                        ));
                    }
                    return Vec::new();
                }
                let name = data
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("tool")
                    .to_string();
                let args = match data.get("arguments") {
                    Some(Value::String(raw)) => serde_json::from_str(raw)
                        .unwrap_or_else(|_| Value::String(raw.clone())),
                    Some(other) => other.clone(),
                    None => Value::Null,
                };
                self.tool_names.insert(id.to_string(), name.clone());
                vec![AiEvent::ToolCall {
                    id: id.to_string(),
                    name,
                    args,
                }]
            }
            "tool/result" => {
                let message = data.get("message").cloned().unwrap_or(Value::Null);
                let id = message
                    .get("toolCallId")
                    .and_then(Value::as_str)
                    .or_else(|| data.get("callId").and_then(Value::as_str))
                    .unwrap_or_default()
                    .to_string();
                let ok = !message
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let content = text_of_content(message.get("content"));
                let result = if content.is_empty() {
                    data.get("meta")
                        .cloned()
                        .unwrap_or(Value::String(String::new()))
                } else {
                    serde_json::from_str::<Value>(&content)
                        .unwrap_or_else(|_| Value::String(content.clone()))
                };
                let name = self
                    .tool_names
                    .remove(&id)
                    .unwrap_or_else(|| "tool".to_string());
                vec![AiEvent::ToolResult {
                    id,
                    name,
                    ok,
                    result,
                }]
            }
            "turn/end" => {
                let reason = data.get("reason").cloned().unwrap_or(Value::Null);
                let kind = reason
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                self.saw_turn_end = true;
                match kind {
                    "error" => {
                        // The bridge may have reported the same failure first
                        // (`agent-error`); one error event is enough.
                        let already_reported = self.failed;
                        self.failed = true;
                        if already_reported {
                            return Vec::new();
                        }
                        let message = reason
                            .pointer("/error/message")
                            .and_then(Value::as_str)
                            .unwrap_or("AI error")
                            .to_string();
                        vec![AiEvent::Error(message)]
                    }
                    "max-tokens" | "length" => {
                        // DSH reports a token-limit stop as a normal
                        // `turn/end`, with `{kind: "max-tokens"}`. Without this
                        // arm the chat simply ends mid-sentence — or with a
                        // truncated tool call — and the user is never told
                        // (issue #18).
                        self.failed = true;
                        vec![AiEvent::Error(
                            "The model response was truncated (max tokens reached). \
                             Ask a shorter question or retry."
                                .to_string(),
                        )]
                    }
                    "aborted" | "abort" => {
                        self.cancelled = true;
                        Vec::new()
                    }
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        }
    }

    fn handle_bridge(&mut self, frame: &Value) -> Vec<AiEvent> {
        match frame.get("type").and_then(Value::as_str).unwrap_or_default() {
            "stream" => {
                let Some(body) = frame.get("frame") else {
                    return Vec::new();
                };
                match body.get("type").and_then(Value::as_str).unwrap_or_default() {
                    "start" => {
                        let attempt = body
                            .get("attemptId")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let turn = body.get("turn").and_then(Value::as_i64).unwrap_or_default();
                        let step = body.get("step").and_then(Value::as_i64).unwrap_or_default();
                        self.attempts.insert(attempt, (turn, step));
                        Vec::new()
                    }
                    "chunk" => {
                        let chunk = body.get("chunk").cloned().unwrap_or(Value::Null);
                        let chunk_type = chunk.get("type").and_then(Value::as_str).unwrap_or_default();
                        let text = chunk.get("text").and_then(Value::as_str).unwrap_or_default();
                        if text.is_empty() {
                            return Vec::new();
                        }
                        // Remember that this step's text streamed through the
                        // bridge so the settled assistant message is not
                        // emitted a second time.
                        if let Some(attempt) = body.get("attemptId").and_then(Value::as_str) {
                            if let Some(key) = self.attempts.get(attempt) {
                                self.streamed.insert(*key);
                            }
                        }
                        match chunk_type {
                            "text-delta" => vec![AiEvent::Text(text.to_string())],
                            "reasoning-delta" => vec![AiEvent::Reasoning(text.to_string())],
                            _ => Vec::new(),
                        }
                    }
                    _ => Vec::new(),
                }
            }
            "agent-error" => {
                if self.failed {
                    return Vec::new();
                }
                self.failed = true;
                let message = frame
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("AI error");
                vec![AiEvent::Error(message.to_string())]
            }
            _ => Vec::new(),
        }
    }
}

// ---- pure helpers (unit-tested) ----

/// Replace the overlay's dynamic values.
pub fn render_overlay(
    template: &str,
    cfg: &DshConfig,
    tools_script: &Path,
    bridge_script: &Path,
) -> String {
    let provider_options = provider_options_yaml(cfg);
    let api_key_line = if cfg.api_key_env.trim().is_empty() {
        String::new()
    } else {
        format!("apiKeyEnv: {}", yaml_scalar(cfg.api_key_env.trim()))
    };
    let script = tools_script.to_string_lossy().replace('\'', "''");
    let bridge = bridge_script.to_string_lossy().replace('\'', "''");
    template
        .replace("{{PROVIDER}}", &yaml_scalar(&cfg.provider))
        .replace("{{MODEL}}", &yaml_scalar(&cfg.model))
        .replace("{{PROVIDER_OPTIONS}}", &provider_options)
        .replace("{{API_KEY_ENV_LINE}}", &api_key_line)
        .replace("{{TOOLS_SCRIPT}}", &script)
        .replace("{{BRIDGE_SCRIPT}}", &bridge)
}

/// A request header that exposes at least one tool under `prefix`.
pub fn header_exposes_tools(header: &Value, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    header
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools.iter().any(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.starts_with(prefix))
            })
        })
        .unwrap_or(false)
}

fn tool_names_preview(header: &Value) -> String {
    let names = header
        .get("tools")
        .and_then(Value::as_array)
        .map(|tools| {
            tools
                .iter()
                .filter_map(|tool| tool.get("name").and_then(Value::as_str))
                .take(8)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

fn text_of_content(content: Option<&Value>) -> String {
    content
        .and_then(Value::as_array)
        .map(|blocks| {
            blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

fn provider_options_yaml(cfg: &DshConfig) -> String {
    // The template line already carries the 8-space indent for the first
    // option; continuation lines need their own indent. (Indenting every line
    // here would double-indent the first one and produce invalid YAML.)
    let mut lines = Vec::new();
    if let Some(base_url) = cfg.base_url.as_deref().filter(|s| !s.trim().is_empty()) {
        lines.push(format!("baseURL: {}", yaml_scalar(base_url.trim())));
    }
    if let Some(api) = cfg.api.as_deref().filter(|s| !s.trim().is_empty()) {
        lines.push(format!("api: {}", yaml_scalar(api.trim())));
    }
    lines.join("\n        ")
}

fn yaml_scalar(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '@'))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "''"))
    }
}

fn escape_json_pointer(segment: &str) -> String {
    segment.replace('~', "~0").replace('/', "~1")
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let short: String = text.chars().take(max).collect();
        format!("{short}…")
    }
}

fn dsh_data_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.data_dir().join("hyprtrace").join("dsh"))
        .unwrap_or_else(|| std::env::temp_dir().join("hyprtrace-dsh"))
}

fn new_session_id(dir: &Path) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    if std::fs::create_dir_all(dir).is_ok() {
        let _ = std::fs::write(dir.join("session-id"), format!("{id}\n"));
    }
    id
}

fn load_or_create_session(dir: &Path) -> String {
    if let Ok(raw) = std::fs::read_to_string(dir.join("session-id")) {
        let id = raw.trim();
        if !id.is_empty() {
            return id.to_string();
        }
    }
    new_session_id(dir)
}

/// Resolve one `integrations/dsh` asset. Search order: explicit config path,
/// `$HYPRTRACE_DSH_DIR`, the user data dir, the system data dir, the repository
/// checkout at build time, then the current directory.
fn resolve_asset(explicit: Option<&str>, file_name: &str) -> Result<PathBuf> {
    let mut tried: Vec<PathBuf> = Vec::new();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(path) = explicit.filter(|p| !p.trim().is_empty()) {
        let configured = hyprtrace_common::paths::expand_tilde(path.trim());
        if configured.is_file() {
            return Ok(configured.canonicalize().unwrap_or(configured));
        }
        bail!(
            "configured DSH asset path does not exist: {}",
            configured.display()
        );
    }
    if let Some(dir) = std::env::var_os("HYPRTRACE_DSH_DIR") {
        candidates.push(PathBuf::from(dir).join(file_name));
    }
    candidates.push(dsh_data_dir().join(file_name));
    candidates.push(PathBuf::from("/usr/share/hyprtrace/dsh").join(file_name));
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../integrations/dsh")
            .join(file_name),
    );
    candidates.push(PathBuf::from("integrations/dsh").join(file_name));
    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate.canonicalize().unwrap_or(candidate));
        }
        tried.push(candidate);
    }
    let tried = tried
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Err(anyhow!(
        "DSH asset `{file_name}` not found. Searched: {tried}. Copy integrations/dsh/ to \
         ~/.local/share/hyprtrace/dsh/ or set [ai.dsh].{}",
        if file_name.ends_with(".yml") {
            "overlay_template"
        } else if file_name.contains("bridge") {
            "bridge_script"
        } else {
            "tools_script"
        }
    ))
}

fn resolve_cwd(cfg: &DshConfig) -> Result<PathBuf> {
    if let Some(configured) = cfg.cwd.as_deref().filter(|value| !value.trim().is_empty()) {
        let path = hyprtrace_common::paths::expand_tilde(configured.trim());
        if path.is_dir() {
            return Ok(path);
        }
        bail!("[ai.dsh].cwd is not a directory: {}", path.display());
    }
    if let Some(home) = std::env::var_os("HOME") {
        let path = PathBuf::from(home);
        if path.is_dir() {
            return Ok(path);
        }
    }
    std::env::current_dir().context("failed to resolve a working directory for dsh")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> DshConfig {
        DshConfig {
            enabled: true,
            provider: "opencode-go".to_string(),
            model: "deepseek-v4.1-flash".to_string(),
            api_key_env: "OPENCODE_GO_API_KEY".to_string(),
            ..DshConfig::default()
        }
    }

    #[test]
    fn overlay_rendering_substitutes_every_placeholder() {
        let template = "\
providers:
  {{PROVIDER}}:
    {{PROVIDER_OPTIONS}}
    {{API_KEY_ENV_LINE}}
    models:
      - id: {{MODEL}}
tools:
  args: ['{{TOOLS_SCRIPT}}']
bridge:
  name: 'file://{{BRIDGE_SCRIPT}}'
";
        let rendered = render_overlay(
            template,
            &config(),
            Path::new("/opt/hypr trace/hyprtrace-mcp.mjs"),
            Path::new("/opt/hypr trace/hyprtrace-bridge.mjs"),
        );
        assert!(!rendered.contains("{{"), "no placeholder may survive: {rendered}");
        assert!(rendered.contains("apiKeyEnv: OPENCODE_GO_API_KEY"));
        assert!(rendered.contains("- id: deepseek-v4.1-flash"));
        assert!(rendered.contains("'/opt/hypr trace/hyprtrace-mcp.mjs'"));
        assert!(rendered.contains("'file:///opt/hypr trace/hyprtrace-bridge.mjs'"));
    }

    #[test]
    fn provider_options_keep_valid_yaml_indentation() {
        // The template line already indents the first option; continuation
        // lines must be indented by the renderer exactly once (a real bug:
        // every line was pre-indented, producing 16 spaces on the first line).
        let template = "providers:\n  p:\n        {{PROVIDER_OPTIONS}}\n";
        let mut cfg = config();
        cfg.base_url = Some("https://example.com/v1".to_string());
        cfg.api = Some("openai-completions".to_string());
        let rendered = render_overlay(
            template,
            &cfg,
            Path::new("/tmp/tools.mjs"),
            Path::new("/tmp/bridge.mjs"),
        );
        assert!(
            rendered.contains("        baseURL: https://example.com/v1\n"),
            "first option must sit at exactly 8 spaces: {rendered:?}"
        );
        assert!(
            rendered.contains("        api: openai-completions"),
            "continuation option must be indented once: {rendered:?}"
        );
        assert!(
            !rendered.contains("                baseURL"),
            "no double indent allowed: {rendered:?}"
        );
    }

    #[test]
    fn overlay_rendering_can_omit_optional_provider_lines() {
        let template = "{{PROVIDER}} {{PROVIDER_OPTIONS}} {{API_KEY_ENV_LINE}} {{MODEL}}";
        let mut cfg = config();
        cfg.api_key_env = String::new();
        let rendered = render_overlay(
            template,
            &cfg,
            Path::new("/tmp/tools.mjs"),
            Path::new("/tmp/bridge.mjs"),
        );
        assert_eq!(
            rendered.split_whitespace().collect::<Vec<_>>(),
            vec!["opencode-go", "deepseek-v4.1-flash"]
        );
    }

    #[test]
    fn header_tool_check_accepts_the_expected_bridge_tools() {
        let header = json!({
            "tools": [
                {"name": "bash"},
                {"name": "mcp__hyprtrace__today_summary"}
            ]
        });
        assert!(header_exposes_tools(&header, "mcp__hyprtrace__"));
        assert!(!header_exposes_tools(&header, "mcp__other__"));
        assert!(!header_exposes_tools(&json!({"tools": []}), "mcp__hyprtrace__"));
        assert!(header_exposes_tools(&json!({}), ""));
    }

    #[test]
    fn bridged_stream_deltas_emit_text_and_reasoning() {
        let mut state = TurnState::default();
        let start = json!({
            "type": "stream",
            "sessionId": "s",
            "frame": {"type": "start", "attemptId": "a1", "turn": 1, "step": 1}
        });
        assert!(state.handle_bridge(&start).is_empty());
        let text = json!({
            "type": "stream",
            "sessionId": "s",
            "frame": {"type": "chunk", "attemptId": "a1",
                      "chunk": {"type": "text-delta", "index": 1, "text": "hello"}}
        });
        assert_eq!(state.handle_bridge(&text), vec![AiEvent::Text("hello".to_string())]);
        let reasoning = json!({
            "type": "stream",
            "sessionId": "s",
            "frame": {"type": "chunk", "attemptId": "a1",
                      "chunk": {"type": "reasoning-delta", "index": 0, "text": "hmm"}}
        });
        assert_eq!(
            state.handle_bridge(&reasoning),
            vec![AiEvent::Reasoning("hmm".to_string())]
        );
        // The settled message for the streamed step is suppressed.
        let settled = json!({
            "type": "assistant/message",
            "data": {"turn": 1, "step": 1,
                     "message": {"content": [{"type": "text", "text": "hello"}]}}
        });
        assert!(state.handle_session_event(&settled, "").is_empty());
    }

    #[test]
    fn unstreamed_assistant_message_is_a_fallback_text() {
        let mut state = TurnState::default();
        let settled = json!({
            "type": "assistant/message",
            "data": {"turn": 2, "step": 1,
                     "message": {"content": [
                        {"type": "reasoning", "text": "think"},
                        {"type": "text", "text": "answer"}
                     ]}}
        });
        assert_eq!(
            state.handle_session_event(&settled, ""),
            vec![
                AiEvent::Reasoning("think".to_string()),
                AiEvent::Text("answer".to_string())
            ]
        );
    }

    #[test]
    fn tool_events_keep_names_and_structured_results() {
        let mut state = TurnState::default();
        let call = json!({
            "type": "tool/call",
            "data": {"callId": "c1", "name": "mcp__hyprtrace__today_summary",
                     "arguments": "{\"date\":\"2026-10-24\"}"}
        });
        assert_eq!(
            state.handle_session_event(&call, ""),
            vec![AiEvent::ToolCall {
                id: "c1".to_string(),
                name: "mcp__hyprtrace__today_summary".to_string(),
                args: json!({"date": "2026-10-24"}),
            }]
        );
        let result = json!({
            "type": "tool/result",
            "data": {"message": {"toolCallId": "c1", "isError": false,
                                 "content": [{"type": "text", "text": "{\"total_ms\":42}"}]}}
        });
        assert_eq!(
            state.handle_session_event(&result, ""),
            vec![AiEvent::ToolResult {
                id: "c1".to_string(),
                name: "mcp__hyprtrace__today_summary".to_string(),
                ok: true,
                result: json!({"total_ms": 42}),
            }]
        );
    }

    #[test]
    fn turn_end_kinds_map_to_error_cancel_or_done() {
        let mut state = TurnState::default();
        let completed = json!({"type": "turn/end", "data": {"reason": {"kind": "completed"}}});
        assert!(state.handle_session_event(&completed, "").is_empty());
        assert!(state.saw_turn_end && !state.failed && !state.cancelled);

        let mut state = TurnState::default();
        let aborted = json!({"type": "turn/end", "data": {"reason": {"kind": "aborted"}}});
        assert!(state.handle_session_event(&aborted, "").is_empty());
        assert!(state.cancelled && !state.failed);

        let mut state = TurnState::default();
        let error = json!({
            "type": "turn/end",
            "data": {"reason": {"kind": "error", "error": {"message": "Insufficient Balance"}}}
        });
        assert_eq!(
            state.handle_session_event(&error, ""),
            vec![AiEvent::Error("Insufficient Balance".to_string())]
        );
        assert!(state.failed);

        // A token-limit stop is a normal turn end for DSH; it must surface as
        // an error so a truncated answer is never presented as complete.
        let mut state = TurnState::default();
        let truncated = json!({
            "type": "turn/end",
            "data": {"reason": {"kind": "max-tokens"}}
        });
        let events = state.handle_session_event(&truncated, "");
        assert_eq!(events.len(), 1);
        assert!(matches!(&events[0], AiEvent::Error(message) if message.contains("truncated")));
        assert!(state.saw_turn_end && state.failed && !state.cancelled);
    }

    #[test]
    fn tool_call_budget_aborts_the_turn() {
        let mut state = TurnState::default();
        // One tool call below the cap is passed through to the client.
        for index in 0..MAX_TOOL_CALLS_PER_TURN {
            let call = json!({
                "type": "tool/call",
                "data": {"callId": format!("c{index}"), "name": "mcp__hyprtrace__today_summary",
                         "arguments": "{}"}
            });
            let events = state.handle_session_event(&call, "");
            assert_eq!(events.len(), 1, "call {index} must stream");
            assert!(state.abort_message.is_none());
        }
        // The next call exceeds the budget: no event, failed + actionable
        // message (the turn loop will emit it as an `error`).
        let over = json!({
            "type": "tool/call",
            "data": {"callId": "over", "name": "mcp__hyprtrace__today_summary",
                     "arguments": "{}"}
        });
        assert!(state.handle_session_event(&over, "").is_empty());
        assert!(state.failed);
        assert!(state
            .abort_message
            .as_deref()
            .is_some_and(|message| message.contains("tool-call budget")));
    }

    #[test]
    fn a_tool_less_header_aborts_the_turn() {
        let mut state = TurnState::default();
        let header = json!({
            "type": "request/header",
            "data": {"header": {"tools": [{"name": "bash"}]}}
        });
        assert!(state.handle_session_event(&header, "mcp__hyprtrace__").is_empty());
        assert!(state.failed);
        assert!(state
            .abort_message
            .as_deref()
            .is_some_and(|message| message.contains("tools are missing")));
    }

    #[test]
    fn events_serialize_to_the_frontend_contract() {
        assert_eq!(
            AiEvent::Text("hi".to_string()).to_json(),
            json!({"type": "text", "delta": "hi"})
        );
        assert_eq!(
            AiEvent::Reasoning("hmm".to_string()).to_json(),
            json!({"type": "reasoning", "delta": "hmm"})
        );
        assert_eq!(AiEvent::Cancelled.to_json(), json!({"type": "cancelled"}));
        assert_eq!(
            AiEvent::Error("nope".to_string()).to_json(),
            json!({"type": "error", "message": "nope"})
        );
        assert_eq!(
            AiEvent::Done("stop".to_string()).to_json(),
            json!({"type": "done"})
        );
    }

    #[test]
    fn explicit_asset_path_must_exist() {
        let error = resolve_asset(Some("/nonexistent/hyprtrace-mcp.mjs"), "hyprtrace-mcp.mjs")
            .expect_err("missing explicit path must be an error");
        assert!(error.to_string().contains("does not exist"), "{error}");
    }

    #[test]
    fn session_id_is_stable_until_reset() {
        let dir = tempfile::tempdir().unwrap();
        let first = load_or_create_session(dir.path());
        assert_eq!(first, load_or_create_session(dir.path()));
        let reset = new_session_id(dir.path());
        assert_ne!(first, reset);
        assert_eq!(load_or_create_session(dir.path()), reset);
    }

    #[tokio::test]
    async fn reset_default_session_rotates_the_persisted_id() {
        let dir = tempfile::tempdir().unwrap();
        let service = AiService::with_data_dir(
            config(),
            "http://127.0.0.1:1".to_string(),
            None,
            dir.path().to_path_buf(),
        );
        let first = service.default_session().await;
        let second = service.reset_default_session().await;
        assert_ne!(first, second);
        assert_eq!(service.default_session().await, second);
        // The on-disk id is rotated as well, so a restart keeps the new one.
        assert_eq!(load_or_create_session(dir.path()), second);
    }

    #[test]
    fn session_id_falls_back_when_the_directory_is_unwritable() {
        let id = load_or_create_session(Path::new("/proc/nonexistent-hyprtrace"));
        assert!(!id.trim().is_empty());
    }

    #[test]
    fn json_pointer_escaping_for_provider_names() {
        assert_eq!(escape_json_pointer("a/b~c"), "a~1b~0c");
    }
}

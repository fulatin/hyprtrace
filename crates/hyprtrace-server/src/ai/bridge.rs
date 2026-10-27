//! Unix-socket bridge to the DSH `hyprtrace-bridge` plugin.
//!
//! The DSH SDK protocol (`session.event` / `session.status`) only carries
//! *committed* facts: an assistant message appears once a step settles, and the
//! protocol has no cancel, model-listing or tool-listing methods. DSH itself
//! streams finer-grained runtime events and owns the cancellation path, so the
//! harness loads a small plugin (`integrations/dsh/hyprtrace-bridge.mjs`, wired
//! in by the profile overlay) that is our in-process peer:
//!
//! ```text
//! HyprTrace (Rust)                       dsh --profile sdk (+ plugin)
//!   Unix socket, NDJSON  <──────────────>  agent/assistant-stream  ──► stream frames
//!   {"cmd":"cancel"}     ──────────────►   agent.cancel({kind:'user'})
//!   {"cmd":"models"}     ──────────────►   llm.listModels(provider)
//!   {"cmd":"tools"}      ──────────────►   tools.schemas(agent.ctx)
//! ```
//!
//! This module owns only the socket plumbing: framing, command/response
//! correlation, connection tracking and the per-connection reader. Routing the
//! frames to chat turns belongs to [`crate::ai::service`].

use anyhow::{anyhow, bail, Context, Result};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, ReadHalf, WriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{broadcast, mpsc, oneshot, Mutex};

/// How long a command may wait for the plugin's reply. Model/tool inventory
/// calls the llm adapter, which can take a few seconds; cancel/ping answer
/// immediately.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

/// One live Unix socket exposed to the plugin.
pub struct Bridge {
    path: PathBuf,
    /// NDJSON lines waiting to be written to the current peer.
    outbox: mpsc::Sender<String>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>,
    events: broadcast::Sender<Value>,
    connected: Arc<AtomicBool>,
    next_id: AtomicU64,
}

impl Bridge {
    /// Bind the socket at `path` and start accepting peers.
    ///
    /// A stale socket file from a previous (crashed) server is removed first.
    /// The parent directory should already be private (0700).
    pub fn bind(path: PathBuf) -> Result<Arc<Self>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }
        let listener = UnixListener::bind(&path)
            .with_context(|| format!("failed to bind bridge socket {}", path.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }

        let (events, _rx) = broadcast::channel(4096);
        let (outbox, outbox_rx) = mpsc::channel::<String>(1024);
        let pending: Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>> =
            Arc::default();
        let connected = Arc::new(AtomicBool::new(false));

        // The currently connected peer's write half. A reconnect replaces it.
        let writer: Arc<Mutex<Option<WriteHalf<UnixStream>>>> = Arc::new(Mutex::new(None));

        // Writer task: only the newest peer receives outbound frames.
        let writer_slot = writer.clone();
        let mut outbox_rx = outbox_rx;
        tokio::spawn(async move {
            while let Some(line) = outbox_rx.recv().await {
                let mut slot = writer_slot.lock().await;
                if let Some(stream) = slot.as_mut() {
                    if stream.write_all(line.as_bytes()).await.is_err()
                        || stream.write_all(b"\n").await.is_err()
                        || stream.flush().await.is_err()
                    {
                        *slot = None;
                    }
                }
            }
        });

        // Accept task state must be cloned before the senders move into the
        // bridge struct.
        let accept_pending = pending.clone();
        let accept_events = events.clone();
        let accept_connected = connected.clone();
        let accept_writer = writer.clone();

        let bridge = Arc::new(Self {
            path,
            outbox,
            pending,
            events,
            connected,
            next_id: AtomicU64::new(1),
        });

        tokio::spawn(async move {
            loop {
                match listener.accept().await {
                    Ok((stream, _addr)) => {
                        log::debug!("dsh bridge: plugin connected");
                        let (read_half, write_half) = tokio::io::split(stream);
                        *accept_writer.lock().await = Some(write_half);
                        accept_connected.store(true, Ordering::SeqCst);
                        let pending = accept_pending.clone();
                        let events = accept_events.clone();
                        let connected = accept_connected.clone();
                        let writer = accept_writer.clone();
                        tokio::spawn(async move {
                            read_peer(read_half, pending, events, connected.clone()).await;
                            connected.store(false, Ordering::SeqCst);
                            *writer.lock().await = None;
                            log::debug!("dsh bridge: plugin disconnected");
                        });
                    }
                    Err(error) => {
                        log::warn!("dsh bridge: accept failed: {error}");
                        tokio::time::sleep(Duration::from_millis(200)).await;
                    }
                }
            }
        });

        Ok(bridge)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether a plugin peer is currently connected and has said hello.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// Frames pushed by the plugin (stream/status/error/session-created/hello).
    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.events.subscribe()
    }

    /// Send one command and await the matching `reply` frame.
    pub async fn command(&self, mut command: Value) -> Result<Value> {
        if !self.is_connected() {
            bail!("the DSH bridge plugin is not connected");
        }
        let id = format!("hz-{}", self.next_id.fetch_add(1, Ordering::Relaxed));
        command["id"] = Value::String(id.clone());
        let line = serde_json::to_string(&command)?;
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        if self.outbox.send(line).await.is_err() {
            self.pending.lock().await.remove(&id);
            bail!("the DSH bridge socket is closed");
        }
        match tokio::time::timeout(COMMAND_TIMEOUT, rx).await {
            Ok(Ok(Ok(reply))) => {
                if reply.get("ok").and_then(Value::as_bool) == Some(false) {
                    let message = reply
                        .get("error")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown bridge error");
                    Err(anyhow!("bridge command failed: {message}"))
                } else {
                    Ok(reply)
                }
            }
            Ok(Ok(Err(message))) => Err(anyhow!("bridge command failed: {message}")),
            Ok(Err(_)) => Err(anyhow!("bridge reply channel closed")),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(anyhow!("bridge command timed out after {COMMAND_TIMEOUT:?}"))
            }
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Read NDJSON frames from one plugin peer: replies resolve pending commands,
/// everything else is broadcast to the service's router.
async fn read_peer(
    read_half: ReadHalf<UnixStream>,
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>,
    events: broadcast::Sender<Value>,
    connected: Arc<AtomicBool>,
) {
    let mut lines = BufReader::new(read_half).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Ok(frame) = serde_json::from_str::<Value>(line) else {
                    log::warn!("dsh bridge: ignored non-JSON line: {}", truncate(line, 200));
                    continue;
                };
                if frame.get("type").and_then(Value::as_str) == Some("hello") {
                    log::info!(
                        "dsh bridge: plugin hello (pid={}, protocol={})",
                        frame.get("pid").and_then(Value::as_i64).unwrap_or_default(),
                        frame
                            .get("protocol")
                            .and_then(Value::as_i64)
                            .unwrap_or_default()
                    );
                    connected.store(true, Ordering::SeqCst);
                }
                if frame.get("type").and_then(Value::as_str) == Some("reply") {
                    if let Some(id) = frame.get("id").and_then(Value::as_str) {
                        if let Some(tx) = pending.lock().await.remove(id) {
                            let _ = tx.send(Ok(frame));
                            continue;
                        }
                    }
                }
                let _ = events.send(frame);
            }
            Ok(None) => break,
            Err(error) => {
                log::warn!("dsh bridge: read failed: {error}");
                break;
            }
        }
    }
    // Fail every command still waiting on this peer.
    let mut pending = pending.lock().await;
    for (_, tx) in pending.drain() {
        let _ = tx.send(Err("bridge peer disconnected".to_string()));
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let short: String = text.chars().take(max).collect();
        format!("{short}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::AsyncBufReadExt;

    /// A real socket round-trip: bind, connect a fake plugin, answer a command
    /// and push a stream frame that subscribers observe.
    #[tokio::test]
    async fn command_and_event_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sock");
        let bridge = Bridge::bind(path.clone()).unwrap();

        let peer = UnixStream::connect(&path).await.unwrap();
        let (read_half, mut write_half) = tokio::io::split(peer);
        let mut peer_lines = BufReader::new(read_half).lines();

        // Handshake.
        write_half
            .write_all(b"{\"type\":\"hello\",\"pid\":42,\"protocol\":1}\n")
            .await
            .unwrap();
        write_half.flush().await.unwrap();
        for _ in 0..50 {
            if bridge.is_connected() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(bridge.is_connected(), "hello must mark the peer connected");

        // Subscribe before the command so the reply broadcast is not missed.
        let mut events = bridge.subscribe();

        // The plugin's answering task.
        let answer = async {
            let line = peer_lines.next_line().await.unwrap().unwrap();
            let command: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(command["cmd"], "models");
            let id = command["id"].as_str().unwrap();
            let reply = json!({"type":"reply","id":id,"ok":true,"models":{"p":["m1","m2"]}});
            write_half
                .write_all(format!("{}\n", reply).as_bytes())
                .await
                .unwrap();
            write_half.flush().await.unwrap();
        };
        let command = bridge.command(json!({"cmd":"models","providers":["p"]}));
        let (reply, ()) = tokio::join!(command, answer);
        let reply = reply.unwrap();
        assert_eq!(reply["models"]["p"], json!(["m1", "m2"]));

        // Push an unsolicited frame; the service router consumes it.
        write_half
            .write_all(b"{\"type\":\"stream\",\"sessionId\":\"s1\",\"frame\":{}}\n")
            .await
            .unwrap();
        write_half.flush().await.unwrap();
        let frame = tokio::time::timeout(Duration::from_secs(2), events.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(frame["type"], "stream");
        assert_eq!(frame["sessionId"], "s1");
    }

    #[tokio::test]
    async fn commands_fail_fast_without_a_peer() {
        let dir = tempfile::tempdir().unwrap();
        let bridge = Bridge::bind(dir.path().join("bridge.sock")).unwrap();
        let error = bridge.command(json!({"cmd":"ping"})).await.unwrap_err();
        assert!(error.to_string().contains("not connected"), "{error}");
    }

    #[tokio::test]
    async fn a_disconnected_peer_fails_pending_commands() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bridge.sock");
        let bridge = Bridge::bind(path.clone()).unwrap();
        let peer = UnixStream::connect(&path).await.unwrap();
        let (read_half, mut write_half) = tokio::io::split(peer);
        let mut peer_lines = BufReader::new(read_half).lines();
        write_half
            .write_all(b"{\"type\":\"hello\",\"pid\":1,\"protocol\":1}\n")
            .await
            .unwrap();
        write_half.flush().await.unwrap();
        for _ in 0..50 {
            if bridge.is_connected() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // The command future is lazy: drive it in a task while the peer
        // consumes the frame, then hang up without replying.
        let bridge = bridge.clone();
        let command = tokio::spawn(async move { bridge.command(json!({"cmd":"ping"})).await });
        let line = peer_lines.next_line().await.unwrap().unwrap();
        assert!(line.contains("\"cmd\":\"ping\""));
        drop(write_half);
        drop(peer_lines);
        let error = command.await.unwrap().unwrap_err();
        assert!(
            error.to_string().contains("disconnected") || error.to_string().contains("closed"),
            "pending command must be failed on disconnect, got: {error}"
        );
    }
}

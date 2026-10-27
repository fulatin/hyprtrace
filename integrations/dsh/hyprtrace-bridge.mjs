#!/usr/bin/env node
// HyprTrace ↔ DSH bridge plugin (DMZ between the harness and the HyprTrace
// server). Loaded as a local ESM plugin by the `sdk` profile overlay:
//
//   - insert:
//       - id: hyprtrace-bridge
//         name: 'file:///abs/path/hyprtrace-bridge.mjs'
//
// This file runs INSIDE the `dsh` process, so it must never write to stdout
// (stdout is the SDK's JSON-RPC channel); everything here logs to stderr.
// Zero dependencies: node: built-ins only.
//
// Wire protocol (frozen in .dsh-diag/deep-fusion/CONTRACT.md §1) is NDJSON over
// the Unix socket named by HYPRTRACE_BRIDGE_SOCK, bidirectional:
//
// plugin → HyprTrace
//   {"type":"hello","pid":<int>,"protocol":1}
//   {"type":"stream","sessionId":"...","frame":{...}}
//   {"type":"status","sessionId":"...","status":"running"|"idle"}
//   {"type":"agent-error","sessionId":"...","message":"...","code":"..."}
//   {"type":"session-created","sessionId":"...","parentSession":"..."}
//   {"type":"reply","id":"...","ok":true,"models":{...}}
//   {"type":"reply","id":"...","ok":true,"tools":[{name,description}]}
//   {"type":"reply","id":"...","ok":true,"cancelled":true}
//   {"type":"reply","id":"...","ok":false,"error":"..."}
//
// HyprTrace → plugin
//   {"cmd":"cancel","id":"...","sessionId":"..."}
//   {"cmd":"models","id":"...","providers":["opencode-go"]}
//   {"cmd":"tools","id":"...","sessionId":"..."}         // sessionId optional
//   {"cmd":"ping","id":"..."}
//
// Lifecycle: connects lazily, reconnects every 500 ms after a lost socket, and
// re-sends `hello` on every connection. Without HYPRTRACE_BRIDGE_SOCK the
// plugin disables itself with a single stderr line and touches nothing.

import net from 'node:net';

export const name = 'hyprtrace-bridge';

const LOG_PREFIX = '[hyprtrace-bridge]';
const PROTOCOL = 1;
const RECONNECT_MS = 500;
/**
 * dsh activates plugins asynchronously: right after the bridge comes up the
 * MCP tool registry / pi-ai model routes may still be materializing (measured
 * at 0–250 ms). An empty inventory gets a couple of short retries before the
 * reply reports it as empty, so a caller hitting the bridge immediately after
 * `hello` does not see a boot-race empty list.
 */
const READY_RETRY_MS = [250, 500];
/** Cap the pre-connect outbound backlog so a wedged socket cannot grow memory. */
const MAX_QUEUED_FRAMES = 1024;
/** Cap the inbound partial-line buffer (a malformed peer must not grow memory). */
const MAX_COMMAND_BUFFER_BYTES = 1 << 20;

function log(...parts) {
  // stderr only: stdout belongs to the DSH SDK JSON-RPC channel.
  try {
    process.stderr.write(`${LOG_PREFIX} ${parts.join(' ')}\n`);
  } catch {
    /* ignore */
  }
}

function errorText(error) {
  return error instanceof Error ? error.message : String(error);
}

function sleep(ms) {
  return new Promise((resolvePromise) => {
    const timer = setTimeout(resolvePromise, ms);
    timer.unref?.();
  });
}

function apply(ctx) {
  const sockPath = (process.env.HYPRTRACE_BRIDGE_SOCK || '').trim();
  if (!sockPath) {
    log('HYPRTRACE_BRIDGE_SOCK not set; bridge disabled');
    return;
  }

  /** @type {import('node:net').Socket | null} */
  let socket = null;
  let connected = false;
  let stopping = false;
  let reconnectTimer = null;
  let buffer = '';
  /** Frames produced while not connected; flushed after hello on connect. */
  const queue = [];

  const sessionIdOf = (agent) => {
    const id = agent?.session?.id;
    return id === undefined || id === null ? '' : String(id);
  };

  const writeFrame = (frame) => {
    if (!socket || socket.destroyed) return;
    try {
      socket.write(JSON.stringify(frame) + '\n');
    } catch (error) {
      log('socket write failed:', errorText(error));
    }
  };

  /** Outbound event: sent now when connected, buffered otherwise. */
  const send = (frame) => {
    if (stopping) return;
    if (connected) {
      writeFrame(frame);
      return;
    }
    queue.push(frame);
    if (queue.length > MAX_QUEUED_FRAMES) queue.shift();
  };

  const reply = (id, fields) => {
    send({ type: 'reply', id, ...fields });
  };

  const findAgent = (sessionId) => {
    if (!sessionId) return undefined;
    const agents = ctx.get('agents');
    if (!agents) return undefined;
    return agents.list().find((agent) => sessionIdOf(agent) === sessionId);
  };

  // ── inbound commands ──────────────────────────────────────────────────────

  async function handleCancel(cmd) {
    const id = cmd.id ?? null;
    const sessionId = cmd.sessionId === undefined || cmd.sessionId === null ? '' : String(cmd.sessionId);
    if (!sessionId) {
      reply(id, { ok: false, error: 'cancel: sessionId is required' });
      return;
    }
    const agent = findAgent(sessionId);
    if (!agent) {
      // The turn already ended (or never started): a valid, non-cancelling
      // outcome rather than a bridge failure. HyprTrace maps this to
      // `{"status":"not_found"}`.
      reply(id, { ok: true, cancelled: false });
      return;
    }
    try {
      agent.cancel({ kind: 'user' }, { keepInbox: true });
      reply(id, { ok: true, cancelled: true });
    } catch (error) {
      reply(id, { ok: false, error: `cancel failed: ${errorText(error)}` });
    }
  }

  async function handleModels(cmd) {
    const id = cmd.id ?? null;
    const llm = ctx.get('llm');
    if (!llm) {
      reply(id, { ok: false, error: 'models: llm service unavailable' });
      return;
    }
    const providers = Array.isArray(cmd.providers)
      ? cmd.providers.map((provider) => String(provider)).filter(Boolean)
      : cmd.provider
        ? [String(cmd.provider)]
        : [];
    const models = {};
    for (const provider of providers) {
      let list = [];
      let failure = null;
      for (let attempt = 0; attempt <= READY_RETRY_MS.length; attempt += 1) {
        try {
          list = (await llm.listModels(provider)) || [];
          failure = null;
        } catch (error) {
          // A provider whose adapter is not mounted is not a bridge failure: it
          // simply advertises no models in this process lifetime.
          list = [];
          failure = error;
        }
        if (list.length > 0) break;
        const retryMs = READY_RETRY_MS[attempt];
        if (retryMs === undefined) break;
        await sleep(retryMs);
      }
      if (failure && list.length === 0) log(`models: provider ${provider} unavailable: ${errorText(failure)}`);
      models[provider] = list.map((model) => ({ id: String(model.id), name: String(model.name ?? model.id) }));
    }
    reply(id, { ok: true, models });
  }

  async function handleTools(cmd) {
    const id = cmd.id ?? null;
    const tools = ctx.get('tools');
    if (!tools) {
      reply(id, { ok: false, error: 'tools: tool registry unavailable' });
      return;
    }
    const sessionId = cmd.sessionId === undefined || cmd.sessionId === null ? '' : String(cmd.sessionId);
    // The session scope is best-effort: before the first turn there is no
    // agent yet, and the global tool view is the right answer for a picker.
    const agent = findAgent(sessionId);
    let schemas = tools.schemas(agent ? agent.ctx : undefined) || [];
    if (!agent && schemas.length === 0) {
      // See READY_RETRY_MS: tolerate the plugin-activation race right after boot.
      for (const retryMs of READY_RETRY_MS) {
        await sleep(retryMs);
        schemas = tools.schemas(undefined) || [];
        if (schemas.length > 0) break;
      }
    }
    reply(id, {
      ok: true,
      tools: schemas.map((schema) => ({ name: schema.name, description: schema.description ?? '' })),
    });
  }

  async function handleCommand(cmd) {
    if (!cmd || typeof cmd !== 'object' || Array.isArray(cmd)) return;
    switch (cmd.cmd) {
      case 'ping':
        reply(cmd.id ?? null, { ok: true });
        return;
      case 'cancel':
        await handleCancel(cmd);
        return;
      case 'models':
        await handleModels(cmd);
        return;
      case 'tools':
        await handleTools(cmd);
        return;
      default:
        reply(cmd.id ?? null, { ok: false, error: `unknown command: ${String(cmd.cmd)}` });
    }
  }

  // ── socket lifecycle ──────────────────────────────────────────────────────

  function scheduleReconnect() {
    if (stopping || reconnectTimer) return;
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, RECONNECT_MS);
    // Never keep the harness process alive just to retry the bridge.
    reconnectTimer.unref?.();
  }

  function onData(chunk) {
    buffer += chunk;
    if (buffer.length > MAX_COMMAND_BUFFER_BYTES) {
      log('inbound buffer over cap; dropping partial line');
      buffer = '';
      return;
    }
    let index;
    while ((index = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, index).trim();
      buffer = buffer.slice(index + 1);
      if (!line) continue;
      let command;
      try {
        command = JSON.parse(line);
      } catch {
        log('ignoring malformed command line');
        continue;
      }
      handleCommand(command).catch((error) => {
        log('command handler crashed:', errorText(error));
        const id = command && typeof command === 'object' ? command.id ?? null : null;
        reply(id, { ok: false, error: errorText(error) });
      });
    }
  }

  function connect() {
    if (stopping) return;
    const sock = net.connect(sockPath);
    socket = sock;
    sock.setEncoding('utf8');
    sock.on('connect', () => {
      if (socket !== sock) return;
      connected = true;
      buffer = '';
      log(`connected ${sockPath}`);
      // hello is always the first frame of a connection; queued boot events
      // follow it.
      writeFrame({ type: 'hello', pid: process.pid, protocol: PROTOCOL });
      while (queue.length > 0) writeFrame(queue.shift());
    });
    sock.on('data', onData);
    sock.on('error', (error) => {
      log(`socket error: ${error && error.code ? error.code : errorText(error)}`);
    });
    sock.on('close', () => {
      if (socket === sock) {
        socket = null;
        connected = false;
        buffer = '';
      }
      // Frames from the dead connection are stale: a reconnected HyprTrace
      // must not receive a replay of the previous stream.
      queue.length = 0;
      scheduleReconnect();
    });
  }

  // ── live agent events (forwarded verbatim / projected) ────────────────────

  const safeSend = (frame) => {
    try {
      send(frame);
    } catch (error) {
      log('event forward failed:', errorText(error));
    }
  };

  ctx.on('agent/assistant-stream', ({ agent, frame }) => {
    const sessionId = sessionIdOf(agent);
    if (!sessionId || !frame) return;
    safeSend({ type: 'stream', sessionId, frame });
  });

  ctx.on('agent/status', ({ agent, status }) => {
    const sessionId = sessionIdOf(agent);
    if (!sessionId) return;
    safeSend({ type: 'status', sessionId, status });
  });

  ctx.on('agent/error', ({ agent, error }) => {
    const sessionId = sessionIdOf(agent);
    if (!sessionId) return;
    const frame = {
      type: 'agent-error',
      sessionId,
      message: errorText(error),
    };
    const code = error && typeof error === 'object' ? error.code : undefined;
    if (code !== undefined) frame.code = String(code);
    safeSend(frame);
  });

  ctx.on('session/created', (session) => {
    const sessionId = session?.id === undefined || session?.id === null ? '' : String(session.id);
    if (!sessionId) return;
    const frame = { type: 'session-created', sessionId };
    const parentSession = session?.header?.parentSession;
    if (parentSession !== undefined && parentSession !== null) frame.parentSession = String(parentSession);
    safeSend(frame);
  });

  // ── activation / disposal ─────────────────────────────────────────────────
  // cordis runs the effect callback now and its returned function on disposal
  // (profile reload, harness shutdown): one bridge per plugin fiber.
  ctx.effect(() => {
    connect();
    log(`loaded pid=${process.pid} sock=${sockPath} protocol=${PROTOCOL}`);
    return () => {
      stopping = true;
      if (reconnectTimer) {
        clearTimeout(reconnectTimer);
        reconnectTimer = null;
      }
      const sock = socket;
      socket = null;
      connected = false;
      queue.length = 0;
      if (sock) {
        try {
          sock.destroy();
        } catch {
          /* ignore */
        }
      }
      log('disposed');
    };
  }, 'hyprtrace-bridge.connection');
}

export default apply;

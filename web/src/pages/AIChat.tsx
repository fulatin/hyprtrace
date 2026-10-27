import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router-dom";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  AlertTriangle,
  ArrowRight,
  Bot,
  MessageSquarePlus,
  RefreshCw,
  Sparkles,
  Square,
} from "lucide-react";
import { Streamdown } from "streamdown";
import "streamdown/styles.css";
import { useChat } from "@ai-sdk/react";
import { NdjsonChatTransport } from "../lib/transport";
import { api } from "../lib/api";
import { authHeaders } from "../lib/auth";
import type { AiMessage, AiStatus } from "../lib/types";
import { easeOut, spring } from "../lib/motion";
import ChatInput from "../components/ChatInput";
import ReasoningPanel from "../components/ReasoningPanel";
import ToolCallCard from "../components/ToolCallCard";
import { EmptyState, Skeleton } from "../components/ui/Feedback";
import { Reveal, RevealItem } from "../components/ui/Reveal";

const QUICK_QUESTIONS = [
  "Analyze today's usage data and give me insights",
  "Which apps did I use the most today?",
  "What window am I using right now?",
  "Analyze my efficiency this week",
];

const FOLLOW_UP_QUESTIONS: Record<string, string[]> = {
  default: [
    "How can I be more productive?",
    "What apps distract me most?",
    "Analyze my focus time",
    "Compare today with yesterday",
  ],
};

function extractText(message: any): string {
  return (
    message.parts
      ?.filter((p: any) => p.type === "text")
      .map((p: any) => p.text)
      .join("") ?? ""
  );
}

/**
 * Three-dot typing pulse shown while an assistant message is still streaming.
 * Purely decorative: it never touches the transport or the message stream.
 */
function TypingDots({ className = "" }: { className?: string }) {
  const reduced = useReducedMotion();
  return (
    <span
      className={`inline-flex items-center gap-1 ${className}`}
      aria-label="Assistant is typing"
    >
      {[0, 1, 2].map((i) => (
        <motion.span
          key={i}
          className="h-1.5 w-1.5 rounded-full bg-accent"
          animate={
            reduced
              ? { opacity: 0.65 }
              : { opacity: [0.25, 1, 0.25], y: [0, -2, 0] }
          }
          transition={
            reduced
              ? { duration: 0 }
              : {
                  duration: 1,
                  repeat: Infinity,
                  ease: "easeInOut",
                  delay: i * 0.15,
                }
          }
        />
      ))}
    </span>
  );
}

/** Loose view of an AI SDK UI message: parts are dispatched on their `type`. */
interface ChatPartLike {
  type?: string;
  text?: string;
  /** `streaming` while the part is still being written. */
  state?: string;
  toolCallId?: string;
}

interface ChatMessageLike {
  id: string;
  role: string;
  parts?: ChatPartLike[];
}

/** Streaming flags, only meaningful on the last bubble. */
interface BubbleTail {
  isLast: boolean;
  isLoading: boolean;
  /** This turn was cancelled (user stop, /api/ai/cancel, or server cancel). */
  stopped: boolean;
  /** The turn was loaded from history with `complete: false`. */
  interrupted: boolean;
}

/**
 * Constant tail for every bubble that is not last: a stable reference keeps
 * those bubbles prop-equal, so they are skipped while the answer streams.
 */
const NO_TAIL: BubbleTail = {
  isLast: false,
  isLoading: false,
  stopped: false,
  interrupted: false,
};

/**
 * One chat bubble.
 *
 * Memoized because a single streamed token replaces the last message and
 * re-renders the page: without this, every historical bubble re-rendered and
 * `streamdown` re-parsed the markdown of every finished answer on every token.
 * Only the last bubble gets a changing `tail` (and a changing message), so all
 * earlier ones bail out of rendering.
 */
const MessageBubble = memo(function MessageBubble({
  message,
  tail,
  reduced,
  onStop,
}: {
  message: ChatMessageLike;
  tail: BubbleTail;
  reduced: boolean;
  onStop: () => void;
}) {
  const { isLast, isLoading, stopped, interrupted } = tail;
  const text = extractText(message);
  const isUser = message.role === "user";
  const bubbleTransition = reduced ? { duration: 0 } : spring;

  return (
    <motion.div
      initial={reduced ? { opacity: 0 } : { opacity: 0, y: 12, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      exit={reduced ? { opacity: 0 } : { opacity: 0, scale: 0.98, y: -6 }}
      transition={bubbleTransition}
      className={isUser ? "flex justify-end" : "flex justify-start"}
    >
      <div
        className={
          isUser
            ? "max-w-[80%] rounded-2xl rounded-br-md border border-accent/25 bg-accent/10 px-4 py-3 text-sm text-fg"
            : "max-w-[80%] rounded-2xl rounded-bl-md border border-line bg-surface-2 px-4 py-3 text-sm text-fg"
        }
      >
        {isUser ? (
          <p className="whitespace-pre-wrap">{text}</p>
        ) : (
          <>
            {message.parts?.map((part, i) => {
              if (part.type === "text") {
                return (
                  <Streamdown
                    key={i}
                    isAnimating={isLoading && isLast}
                    className="text-fg"
                  >
                    {part.text ?? ""}
                  </Streamdown>
                );
              }
              // Reasoning trace: collapsed by default, expanded on demand.
              if (part.type === "reasoning") {
                return (
                  <ReasoningPanel
                    key={i}
                    text={part.text ?? ""}
                    streaming={part.state === "streaming"}
                  />
                );
              }
              if (
                typeof part.type === "string" &&
                part.type.startsWith("tool-")
              ) {
                return <ToolCallCard key={part.toolCallId ?? i} part={part} />;
              }
              return null;
            })}
            {isLast && isLoading && (
              <span className="mt-2 flex items-center gap-2">
                <TypingDots />
                <button
                  onClick={onStop}
                  title="Stop generating"
                  aria-label="Stop generating"
                  className="btn btn-ghost px-1.5 py-0.5"
                >
                  <Square size={11} />
                </button>
              </span>
            )}
          </>
        )}
        {isLast && stopped && !isLoading && (
          <div className="mt-2 flex items-center gap-1.5 text-xs text-fg-faint">
            <Square size={10} />
            Stopped
          </div>
        )}
        {isLast && interrupted && !isLoading && (
          <div className="mt-2 flex items-center gap-1.5 text-xs text-warn">
            <AlertTriangle size={12} />
            Interrupted before it finished — ask again to continue
          </div>
        )}
      </div>
    </motion.div>
  );
});

/**
 * Identifier for client-side message objects AND conversation ids.
 *
 * `crypto.randomUUID` only exists in secure contexts — https, or http on
 * localhost. The web UI is routinely opened over plain http on a LAN address,
 * where the method is simply absent and calling it throws, which would take
 * the whole history-loading effect down with it. `crypto.getRandomValues` has
 * no such restriction, so it is the preferred fallback; the timestamp-based
 * branch is only there to keep the function total.
 */
function newId(): string {
  const c: Crypto | undefined = globalThis.crypto;
  if (c && typeof c.randomUUID === "function") {
    return c.randomUUID();
  }
  if (c && typeof c.getRandomValues === "function") {
    const bytes = c.getRandomValues(new Uint8Array(16));
    bytes[6] = (bytes[6] & 0x0f) | 0x40; // version 4
    bytes[8] = (bytes[8] & 0x3f) | 0x80; // variant 10xx
    const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join(
      "",
    );
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
  }
  return `${Date.now().toString(16)}-${Math.random().toString(16).slice(2)}`;
}

// The selected model survives reloads; the conversation id does too, so a
// refresh continues the same DSH session instead of starting a new one.
const STORAGE_MODEL = "hyprtrace_ai_model";
const STORAGE_CONVERSATION = "hyprtrace_ai_conversation";

function loadStored(key: string): string {
  try {
    return localStorage.getItem(key) ?? "";
  } catch {
    return "";
  }
}

function saveStored(key: string, value: string) {
  try {
    if (value) {
      localStorage.setItem(key, value);
    } else {
      localStorage.removeItem(key);
    }
  } catch {
    // storage unavailable (private mode etc.) — silently skip
  }
}

export default function AIChat() {
  const [selectedModel, setSelectedModel] = useState(() => loadStored(STORAGE_MODEL));
  const [modelOptions, setModelOptions] = useState<string[]>([]);
  const [aiStatus, setAiStatus] = useState<AiStatus | null>(null);
  // Set when /api/ai/models itself fails (server down / endpoint missing).
  const [runtimeError, setRuntimeError] = useState("");
  const [runtimeLoading, setRuntimeLoading] = useState(false);

  const [conversationId, setConversationId] = useState(
    () => loadStored(STORAGE_CONVERSATION) || newId(),
  );
  const [historyLoaded, setHistoryLoaded] = useState(false);
  const [stopped, setStopped] = useState(false);
  const [historyInterrupted, setHistoryInterrupted] = useState(false);
  const [reportLoading, setReportLoading] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const reduced = useReducedMotion();

  // The conversation id routes turns to a DSH session: persist it so a reload
  // continues the same conversation.
  useEffect(() => {
    saveStored(STORAGE_CONVERSATION, conversationId);
  }, [conversationId]);

  /**
   * Load the DSH runtime state: available model routes, the current route and
   * health. This is the single source of truth for the "not ready → go to
   * Settings" banner and for the model dropdown.
   */
  const loadRuntime = useCallback(async () => {
    setRuntimeLoading(true);
    try {
      const res = await api.aiModels();
      setAiStatus(res.status ?? null);
      setRuntimeError("");
      const list = res.providers?.dsh ?? [];
      setModelOptions(list);

      const stored = loadStored(STORAGE_MODEL);
      const current = res.current?.model ?? "";
      const next =
        stored && list.includes(stored)
          ? stored
          : current && list.includes(current)
            ? current
            : (list[0] ?? "");
      setSelectedModel(next);
      if (next) saveStored(STORAGE_MODEL, next);
    } catch (e) {
      setRuntimeError(
        e instanceof Error ? e.message : "Failed to reach the AI runtime",
      );
    } finally {
      setRuntimeLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadRuntime();
  }, [loadRuntime]);

  const transport = useMemo(
    () =>
      new NdjsonChatTransport({
        api: "/api/ai/chat/agent",
        // The server also treats a dropped connection as a cancel; the
        // explicit event covers cancellation initiated server-side.
        onCancelled: () => setStopped(true),
        prepareSendMessagesRequest: ({ messages }) => {
          const parts = (messages as any[])
            .filter((m: any) => m.role === "user")
            .pop()?.parts;
          const text =
            parts
              ?.filter((p: any) => p.type === "text")
              .map((p: any) => p.text)
              .join("") ?? "";
          return {
            headers: authHeaders(),
            body: {
              message: text,
              conversation_id: conversationId,
              model: selectedModel || undefined,
            },
          };
        },
      }),
    [conversationId, selectedModel],
  );

  const { messages, setMessages, sendMessage, stop, status, error } = useChat({
    transport,
    onFinish: ({ isAbort }) => {
      // True when this turn was stopped locally (the fetch was aborted) — the
      // server then treats the disconnect as a cancel for this conversation.
      if (isAbort) setStopped(true);
      // The first successful turn may have booted a cold harness: refresh the
      // runtime badge/banner so it stops claiming "not ready".
      void loadRuntime();
    },
  });

  const isLoading = status === "submitted" || status === "streaming";

  // A cold harness (enabled, no error, not started yet) is NOT a blocker: the
  // first message boots it server-side. Only a disabled runtime or one that
  // already failed to start blocks the chat. `null` status (the status request
  // itself failed) also does not block: the send will surface what is broken.
  const runtimeDisabled = aiStatus !== null && !aiStatus.enabled;
  const runtimeFailed = aiStatus !== null && aiStatus.enabled && !!aiStatus.error;
  const runtimeCold =
    aiStatus !== null && aiStatus.enabled && !aiStatus.ready && !aiStatus.error;
  // Only a *disabled* runtime prevents sending. A failed start is reported by
  // the banner and by the chat itself, but retrying is allowed (the user may
  // have just fixed the config).
  const runtimeBlocked = runtimeDisabled;

  const handleSend = useCallback(
    (message: string) => {
      if (runtimeBlocked) return;
      setStopped(false);
      setHistoryInterrupted(false);
      sendMessage({ text: message });
    },
    [runtimeBlocked, sendMessage],
  );

  /**
   * Stop the current turn: abort the NDJSON fetch (the server cancels the
   * turn when the connection drops) and fire the explicit cancel as a backup.
   */
  const handleStop = useCallback(() => {
    setStopped(true);
    stop();
    void api.aiCancel(conversationId).catch(() => {});
  }, [conversationId, stop]);

  /**
   * Start a fresh conversation: clear the server-side transcript (that is what
   * "new conversation" means in this single-history UI) and hand the next turn
   * a new conversation id, which routes it to a new DSH session.
   */
  const handleNewConversation = useCallback(async () => {
    if (isLoading) stop();
    setStopped(false);
    setHistoryInterrupted(false);
    setMessages([]);
    setConversationId(newId());
    try {
      await api.clearConversations();
    } catch {
      // The local transcript is already cleared; a stale server history will
      // simply load again on the next refresh.
    }
  }, [isLoading, setMessages, stop]);

  const handleWeeklyReport = async () => {
    setReportLoading(true);
    try {
      const res = await api.weeklyReport(selectedModel || undefined);
      setStopped(false);
      setHistoryInterrupted(false);
      sendMessage({ text: res.report || "Weekly report generated" });
    } catch (e) {
      alert(
        "Weekly report failed: " +
          (e instanceof Error ? e.message : "Unknown error"),
      );
    } finally {
      setReportLoading(false);
    }
  };

  // Load the displayed transcript once. The conversation id itself is not
  // part of the history rows — this UI keeps one rolling transcript whose DSH
  // session is chosen by the id above.
  useEffect(() => {
    api
      .aiConversations()
      .then((convs: AiMessage[]) => {
        if (convs.length > 0) {
          const msgs = convs.map((c) => ({
            id: newId(),
            role: c.role as "user" | "assistant",
            parts: [{ type: "text" as const, text: c.content }],
          }));
          setMessages(msgs);

          // A turn that was cut off (page refresh, cancel, crash) is stored
          // with `complete: false`; flag it instead of pretending it finished.
          const last = convs[convs.length - 1];
          if (last.role === "assistant" && last.complete === false) {
            setHistoryInterrupted(true);
          }
        }
        setHistoryLoaded(true);
      })
      .catch(() => setHistoryLoaded(true));
  }, [setMessages]);

  // Note: opening an empty chat used to fire "Analyze today's usage data" on a
  // timer. Nothing runs on load any more — with a paid model route configured,
  // merely opening the page was spending money the user never authorised. The
  // same prompt is one click away in the quick-question row below.

  // Follow the newest content. `scrollTo` on the container keeps the smooth
  // animation even when the list is taller than the viewport.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTo({
      top: el.scrollHeight,
      behavior: reduced ? "auto" : "smooth",
    });
  }, [messages, status, reduced]);

  const handleModelChange = (m: string) => {
    setSelectedModel(m);
    saveStored(STORAGE_MODEL, m);
  };

  const lastAssistantText =
    messages.length > 0 && messages[messages.length - 1].role === "assistant"
      ? extractText(messages[messages.length - 1])
      : null;

  // `submitted` means the request is out but no assistant bubble exists yet, so
  // the typing pulse lives in a placeholder bubble; once the assistant message
  // arrives the pulse moves inside it (and never shows twice).
  const awaitingFirstChunk =
    isLoading && messages[messages.length - 1]?.role !== "assistant";

  const bubbleTransition = reduced ? { duration: 0 } : spring;

  // Model dropdown: prefer the DSH list; keep the current selection visible even
  // when the server advertises a stale list.
  const modelChoices = useMemo(() => {
    const list = [...modelOptions];
    if (selectedModel && !list.includes(selectedModel)) list.unshift(selectedModel);
    return list;
  }, [modelOptions, selectedModel]);

  // Why the runtime is unusable, for the banner. Only shown when blocked/failed.
  let bannerTitle = "";
  let bannerDetail = "";
  if (runtimeError) {
    bannerTitle = "Cannot reach the AI runtime";
    bannerDetail = runtimeError;
  } else if (error) {
    // A failed turn (bad harness start, missing tools, model error) explains
    // itself in the banner instead of leaving the cold-start hint up.
    bannerTitle = "The last turn failed";
    bannerDetail = error.message || String(error);
  } else if (aiStatus && !aiStatus.enabled) {
    bannerTitle = "DeepSeek Harness is disabled";
    bannerDetail =
      "Enable [ai.dsh] in Settings → AI, then reload the runtime here.";
  } else if (runtimeFailed) {
    bannerTitle = "DeepSeek Harness is not ready";
    bannerDetail =
      aiStatus?.error ||
      (aiStatus?.bridge_connected
        ? "The harness is connected but the model route is not ready yet."
        : "The bridge to the harness is not connected. Check that `dsh` is installed and that the command/profile in Settings → AI are correct.");
  } else if (runtimeCold) {
    bannerTitle = "DeepSeek Harness is idle";
    bannerDetail =
      "It starts on the first message (a cold start takes a second or two).";
  }
  const showRuntimeBanner =
    !!runtimeError || !!error || runtimeBlocked || runtimeFailed || runtimeCold;

  return (
    // Layout adds a 4rem top bar (h-16) and `p-6` around the routed page, so
    // the chat viewport is the window minus 7rem. The old `-3rem` (which
    // matches neither) left the composer hanging below the fold.
    <div className="flex h-[calc(100vh-7rem)] flex-col">
      <Reveal className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <RevealItem className="flex flex-wrap items-center gap-2">
          <h2 className="flex items-center gap-2 text-xl font-bold text-fg">
            <Bot size={20} className="text-accent" />
            AI Analysis
          </h2>

          {/* Compact runtime status: active provider (dsh) + ready/blocked. */}
          <span
            className={
              aiStatus === null
                ? "chip-neutral"
                : !aiStatus.enabled
                  ? "chip-neutral"
                  : aiStatus.ready
                    ? "chip-accent"
                    : "chip-bad"
            }
            title={
              aiStatus
                ? `DSH: ${aiStatus.enabled ? (aiStatus.ready ? "ready" : "not ready") : "disabled"} · ${aiStatus.provider}/${aiStatus.model} · ${aiStatus.bridge_connected ? "bridge connected" : "bridge disconnected"} · ${aiStatus.tool_count} tools`
                : runtimeLoading
                  ? "Checking the DSH runtime…"
                  : "DSH runtime status unknown"
            }
          >
            <span className="relative flex h-1.5 w-1.5">
              {isLoading && !reduced && (
                <motion.span
                  className="absolute inline-flex h-full w-full rounded-full bg-accent/70"
                  animate={{ scale: [1, 2.2], opacity: [0.6, 0] }}
                  transition={{ duration: 1.2, repeat: Infinity, ease: "easeOut" }}
                />
              )}
              <span
                className={`relative inline-flex h-1.5 w-1.5 rounded-full ${
                  aiStatus === null
                    ? "bg-fg-faint"
                    : aiStatus.ready
                      ? "bg-accent"
                      : aiStatus.enabled
                        ? "bg-bad"
                        : "bg-fg-faint"
                }`}
              />
            </span>
            {aiStatus === null
              ? "dsh"
              : !aiStatus.enabled
                ? "dsh disabled"
                : aiStatus.ready
                  ? "dsh ready"
                  : "dsh not ready"}
          </span>

          <AnimatePresence initial={false}>
            {isLoading && (
              <motion.span
                key="streaming-chip"
                className="chip-warn"
                initial={{ opacity: 0, scale: 0.9 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.9 }}
                transition={easeOut}
              >
                {stopped ? "stopping" : "streaming"}
              </motion.span>
            )}
          </AnimatePresence>
        </RevealItem>

        <RevealItem className="flex flex-wrap items-center gap-2">
          {modelChoices.length > 0 && (
            <select
              value={selectedModel}
              onChange={(e) => handleModelChange(e.target.value)}
              aria-label="Model"
              title="Model (restarts the harness for a different route)"
              className="input max-w-[220px] text-xs"
            >
              {modelChoices.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          )}

          <motion.button
            onClick={handleWeeklyReport}
            disabled={reportLoading || runtimeBlocked}
            whileHover={reduced ? undefined : { y: -1 }}
            whileTap={reduced ? undefined : { scale: 0.97 }}
            transition={spring}
            className="btn text-xs"
          >
            <motion.span
              className="text-accent"
              animate={reportLoading && !reduced ? { rotate: 360 } : { rotate: 0 }}
              transition={
                reportLoading
                  ? { duration: 1.4, repeat: Infinity, ease: "linear" }
                  : { duration: 0.2 }
              }
            >
              <Sparkles size={12} />
            </motion.span>
            {reportLoading ? "Generating..." : "Weekly Report"}
          </motion.button>

          <AnimatePresence initial={false}>
            {(messages.length > 0 || isLoading) && (
              <motion.button
                key="new"
                onClick={handleNewConversation}
                initial={{ opacity: 0, scale: 0.9 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.9 }}
                whileHover={reduced ? undefined : { y: -1 }}
                whileTap={reduced ? undefined : { scale: 0.97 }}
                transition={spring}
                className="btn text-xs"
                title="Start a new conversation"
              >
                <MessageSquarePlus size={12} />
                New chat
              </motion.button>
            )}
          </AnimatePresence>
        </RevealItem>
      </Reveal>

      <AnimatePresence initial={false}>
        {showRuntimeBanner && (
          <motion.div
            key="runtime-banner"
            initial={reduced ? { opacity: 0 } : { opacity: 0, y: -6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={reduced ? { opacity: 0 } : { opacity: 0, y: -6 }}
            transition={easeOut}
            className="mb-3 flex items-start gap-3 rounded-lg border border-warn/30 bg-warn/10 px-4 py-3"
          >
            <AlertTriangle size={15} className="mt-0.5 shrink-0 text-warn" />
            <div className="min-w-0 flex-1">
              <p className="text-xs font-medium text-warn">{bannerTitle}</p>
              <p className="mt-0.5 break-words text-xs text-fg-muted">
                {bannerDetail}
              </p>
              <div className="mt-2 flex flex-wrap items-center gap-3">
                <Link
                  to="/settings"
                  className="inline-flex items-center gap-1 text-xs text-accent hover:underline"
                >
                  Open Settings <ArrowRight size={11} />
                </Link>
                <button
                  type="button"
                  onClick={() => void loadRuntime()}
                  disabled={runtimeLoading}
                  className="inline-flex items-center gap-1 text-xs text-fg-muted transition-colors hover:text-fg disabled:opacity-50"
                >
                  <RefreshCw
                    size={11}
                    className={runtimeLoading ? "animate-spin" : ""}
                  />
                  Retry
                </button>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <div
        ref={scrollRef}
        className="min-h-0 flex-1 space-y-4 overflow-y-auto rounded-t-2xl border border-b-0 border-line bg-surface/40 p-4"
      >
        {!historyLoaded ? (
          <div className="space-y-3">
            <Skeleton className="h-12 w-1/2" />
            <Skeleton className="ml-auto h-16 w-2/3" />
            <Skeleton className="h-24 w-3/4" />
          </div>
        ) : messages.length === 0 ? (
          <div className="flex h-full items-center justify-center">
            <EmptyState
              className="w-full max-w-2xl"
              icon={
                <div className="relative">
                  <Bot size={44} className="text-accent/40" />
                  <motion.span
                    className="absolute -right-1 -top-1 text-accent"
                    animate={reduced ? { opacity: 0.8 } : { opacity: [0.45, 1, 0.45] }}
                    transition={
                      reduced
                        ? { duration: 0 }
                        : { duration: 2.2, repeat: Infinity, ease: "easeInOut" }
                    }
                  >
                    <Sparkles size={18} />
                  </motion.span>
                </div>
              }
              title="AI Analysis Assistant"
              hint="Ask anything about your usage data. The assistant runs on DeepSeek Harness and reads your data through read-only tools — nothing is sent until you ask."
              action={
                <Reveal className="mt-4 flex flex-wrap justify-center gap-2" stagger={0.06}>
                  {QUICK_QUESTIONS.map((q) => (
                    <RevealItem key={q}>
                      <motion.button
                        onClick={() => handleSend(q)}
                        disabled={runtimeBlocked}
                        whileHover={
                          runtimeBlocked || reduced ? undefined : { y: -3, scale: 1.02 }
                        }
                        whileTap={
                          runtimeBlocked || reduced ? undefined : { scale: 0.98 }
                        }
                        transition={spring}
                        className={`card-interactive px-4 py-2.5 text-sm text-fg-muted hover:text-fg ${
                          runtimeBlocked ? "cursor-not-allowed opacity-50" : ""
                        }`}
                      >
                        {q}
                      </motion.button>
                    </RevealItem>
                  ))}
                </Reveal>
              }
            />
          </div>
        ) : null}

        <AnimatePresence initial={false}>
          {messages.map((message, idx) => (
            <MessageBubble
              key={message.id}
              message={message as unknown as ChatMessageLike}
              tail={
                idx === messages.length - 1
                  ? {
                      isLast: true,
                      isLoading,
                      stopped: stopped && !isLoading,
                      interrupted: historyInterrupted && !isLoading,
                    }
                  : NO_TAIL
              }
              reduced={!!reduced}
              onStop={handleStop}
            />
          ))}
        </AnimatePresence>

        <AnimatePresence initial={false}>
          {awaitingFirstChunk && (
            <motion.div
              key="pending-bubble"
              initial={reduced ? { opacity: 0 } : { opacity: 0, y: 12, scale: 0.98 }}
              animate={{ opacity: 1, y: 0, scale: 1 }}
              exit={reduced ? { opacity: 0 } : { opacity: 0, scale: 0.98, y: -6 }}
              transition={bubbleTransition}
              className="flex justify-start"
            >
              <div className="flex items-center gap-2 rounded-2xl rounded-bl-md border border-line bg-surface-2/70 px-4 py-3 text-sm text-fg-muted">
                <TypingDots />
                <button
                  onClick={handleStop}
                  title="Stop generating"
                  aria-label="Stop generating"
                  className="btn btn-ghost ml-1 px-2 py-1"
                >
                  <Square size={12} />
                </button>
              </div>
            </motion.div>
          )}
        </AnimatePresence>

        <AnimatePresence initial={false}>
          {stopped &&
            !isLoading &&
            messages[messages.length - 1]?.role !== "assistant" && (
              <motion.div
                key="stopped-note"
                initial={{ opacity: 0, y: 6 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -6 }}
                transition={easeOut}
                className="flex items-center justify-center gap-1.5 text-xs text-fg-faint"
              >
                <Square size={10} />
                Stopped
              </motion.div>
            )}
        </AnimatePresence>

        <AnimatePresence initial={false}>
          {error && !isLoading && (
            <motion.div
              key="error"
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              transition={easeOut}
              className="rounded-lg border border-bad/30 bg-bad/10 px-4 py-3 text-center text-xs text-bad"
            >
              {error.message}
            </motion.div>
          )}
        </AnimatePresence>

        <AnimatePresence initial={false}>
          {!isLoading &&
            lastAssistantText &&
            messages[messages.length - 1]?.role === "assistant" && (
              <motion.div
                key="follow-ups"
                initial={{ opacity: 0, y: 10 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -6 }}
                transition={easeOut}
                className="flex flex-wrap gap-2 pt-2"
              >
                {FOLLOW_UP_QUESTIONS.default.map((q) => (
                  <motion.button
                    key={q}
                    onClick={() => handleSend(q)}
                    disabled={runtimeBlocked}
                    whileHover={
                      runtimeBlocked || reduced ? undefined : { y: -2 }
                    }
                    whileTap={
                      runtimeBlocked || reduced ? undefined : { scale: 0.97 }
                    }
                    transition={spring}
                    className={`btn btn-ghost text-xs hover:border-accent/30 hover:text-accent ${
                      runtimeBlocked ? "cursor-not-allowed opacity-50" : ""
                    }`}
                  >
                    {q}
                  </motion.button>
                ))}
              </motion.div>
            )}
        </AnimatePresence>
      </div>

      <ChatInput
        onSend={handleSend}
        disabled={isLoading || runtimeBlocked}
      />
    </div>
  );
}

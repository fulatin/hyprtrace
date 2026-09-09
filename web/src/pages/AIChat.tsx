import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  AlertTriangle,
  Bot,
  Loader2,
  Trash2,
  Square,
  Sparkles,
} from "lucide-react";
import { Streamdown } from "streamdown";
import "streamdown/styles.css";
import { useChat } from "@ai-sdk/react";
import { NdjsonChatTransport } from "../lib/transport";
import { api } from "../lib/api";
import { authHeaders } from "../lib/auth";
import type { AiMessage, AiModelsResponse } from "../lib/types";
import { easeOut, spring } from "../lib/motion";
import ChatInput from "../components/ChatInput";
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

/**
 * Identifier for client-side message objects.
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

const STORAGE_PROVIDER = "hyprtrace_ai_provider";
const STORAGE_MODEL = "hyprtrace_ai_model";

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
  const [selectedProvider, setSelectedProvider] = useState(
    () => loadStored(STORAGE_PROVIDER) || "ollama",
  );
  const [selectedModel, setSelectedModel] = useState(() => loadStored(STORAGE_MODEL));
  const [providers, setProviders] = useState<Record<string, string[]>>({});
  const [includeData, setIncludeData] = useState(true);
  const [historyLoaded, setHistoryLoaded] = useState(false);
  const [dateRange, setDateRange] = useState("today");
  const [incompleteId, setIncompleteId] = useState<number | null>(null);
  const [pollTimedOut, setPollTimedOut] = useState(false);
  const scrollRef = useRef<HTMLDivElement>(null);
  const dbIdToUiId = useRef(new Map<number, string>());
  const reduced = useReducedMotion();

  const models = providers[selectedProvider] ?? [];

  const transport = useMemo(
    () =>
      new NdjsonChatTransport({
        api: "/api/ai/chat/agent",
        prepareSendMessagesRequest: ({ messages, body }) => {
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
              provider: (body as any)?.provider ?? selectedProvider,
              model: (body as any)?.model ?? (selectedModel || undefined),
              include_data: (body as any)?.include_data ?? includeData,
              date_range: dateRange,
            },
          };
        },
      }),
    [selectedProvider, selectedModel, includeData, dateRange],
  );

  const { messages, setMessages, sendMessage, stop, status, error } = useChat({
    transport,
  });

  const isLoading = status === "submitted" || status === "streaming";

  const handleSend = useCallback(
    (message: string) => {
      sendMessage({ text: message });
    },
    [sendMessage],
  );

  const [reportLoading, setReportLoading] = useState(false);

  const handleWeeklyReport = async () => {
    setReportLoading(true);
    try {
      const res = await api.weeklyReport(selectedProvider, selectedModel || undefined);
      handleSend(res.report || 'Weekly report generated');
    } catch (e) {
      alert('Weekly report failed: ' + (e instanceof Error ? e.message : 'Unknown error'));
    } finally {
      setReportLoading(false);
    }
  };

  useEffect(() => {
    api
      .aiModels()
      .then((res: AiModelsResponse) => {
        setProviders(res.providers);

        // Prefer the user's previously stored provider if it still exists,
        // otherwise fall back to the server default.
        const storedProvider = loadStored(STORAGE_PROVIDER);
        const provider = res.providers[storedProvider]
          ? storedProvider
          : res.default;
        setSelectedProvider(provider);
        saveStored(STORAGE_PROVIDER, provider);

        // Prefer the stored model if it's still available for this provider.
        const storedModel = loadStored(STORAGE_MODEL);
        const models = res.providers[provider] ?? [];
        const model = storedModel && models.includes(storedModel)
          ? storedModel
          : (models[0] ?? "");
        setSelectedModel(model);
        saveStored(STORAGE_MODEL, model);
      })
      .catch(() => {});

    api
      .aiConversations()
      .then((convs: AiMessage[]) => {
        if (convs.length > 0) {
          const msgs = convs.map((c) => {
            const uiId = newId();
            dbIdToUiId.current.set(c.id, uiId);
            return {
              id: uiId,
              role: c.role as "user" | "assistant",
              parts: [{ type: "text" as const, text: c.content }],
            };
          });

          // If the last assistant message was still being generated when the
          // page was refreshed, the server continues streaming in the
          // background — poll until it completes.
          const last = convs[convs.length - 1];
          if (last.role === "assistant" && last.complete === false) {
            setIncompleteId(last.id);
            setPollTimedOut(false);
          }

          setMessages(msgs);
        }
        setHistoryLoaded(true);
      })
      .catch(() => setHistoryLoaded(true));
  }, []);

  // Note: opening an empty chat used to fire "Analyze today's usage data" on a
  // timer. Nothing runs on load any more — with a paid provider configured,
  // merely opening the page was spending money the user never authorised, and
  // with a local model it burned CPU on every visit. The same prompt is one
  // click away in the quick-question row below.

  // Poll for a message that was mid-generation when the page refreshed.
  // The server keeps streaming in the background and saves partial content,
  // so we live-update the bubble until it completes.
  useEffect(() => {
    if (incompleteId === null) return;
    let attempts = 0;
    const timer = setInterval(async () => {
      attempts++;
      try {
        const convs = await api.aiConversations();
        const target = convs.find((c) => c.id === incompleteId);
        if (target) {
          const uiId = dbIdToUiId.current.get(incompleteId);
          if (uiId) {
            setMessages((prev) =>
              prev.map((m) =>
                m.id === uiId
                  ? { ...m, parts: [{ type: "text" as const, text: target.content }] }
                  : m,
              ),
            );
          }
          if (target.complete) {
            setIncompleteId(null);
          }
        }
      } catch {
        // server unreachable — keep trying until the cap
      }
      if (attempts >= 150) {
        setIncompleteId(null);
        setPollTimedOut(true);
      }
    }, 2000);
    return () => clearInterval(timer);
  }, [incompleteId, setMessages]);

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

  const handleClearContext = async () => {
    setMessages([]);
    try {
      await api.clearConversations();
    } catch {}
  };

  const handleProviderChange = (p: string) => {
    setSelectedProvider(p);
    saveStored(STORAGE_PROVIDER, p);
    const first = providers[p]?.[0];
    setSelectedModel(first ?? "");
    saveStored(STORAGE_MODEL, first ?? "");
  };

  const handleModelChange = (m: string) => {
    setSelectedModel(m);
    saveStored(STORAGE_MODEL, m);
  };

  const lastAssistantText =
    messages.length > 0 && messages[messages.length - 1].role === "assistant"
      ? extractText(messages[messages.length - 1])
      : null;

  const stillGenerating = incompleteId !== null;

  // `submitted` means the request is out but no assistant bubble exists yet, so
  // the typing pulse lives in a placeholder bubble; once the assistant message
  // arrives the pulse moves inside it (and never shows twice).
  const awaitingFirstChunk =
    isLoading && messages[messages.length - 1]?.role !== "assistant";

  const bubbleTransition = reduced ? { duration: 0 } : spring;

  return (
    <div className="flex h-[calc(100vh-3rem)] flex-col">
      <Reveal className="mb-4 flex flex-wrap items-center justify-between gap-3">
        <RevealItem className="flex flex-wrap items-center gap-2">
          <h2 className="flex items-center gap-2 text-xl font-bold text-fg">
            <Bot size={20} className="text-accent" />
            AI Analysis
          </h2>

          {/* Compact status: active provider (+ live pulse while streaming). */}
          <span className="chip-accent" title={`Active provider: ${selectedProvider}`}>
            <span className="relative flex h-1.5 w-1.5">
              {isLoading && !reduced && (
                <motion.span
                  className="absolute inline-flex h-full w-full rounded-full bg-accent/70"
                  animate={{ scale: [1, 2.2], opacity: [0.6, 0] }}
                  transition={{ duration: 1.2, repeat: Infinity, ease: "easeOut" }}
                />
              )}
              <span className="relative inline-flex h-1.5 w-1.5 rounded-full bg-accent" />
            </span>
            {selectedProvider}
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
                streaming
              </motion.span>
            )}
          </AnimatePresence>
        </RevealItem>

        <RevealItem className="flex flex-wrap items-center gap-2">
          <select
            value={selectedProvider}
            onChange={(e) => handleProviderChange(e.target.value)}
            aria-label="Provider"
            title="Provider"
            className="input text-xs"
          >
            {Object.keys(providers).map((p) => (
              <option key={p} value={p}>
                {p}
              </option>
            ))}
          </select>

          {models.length > 0 && (
            <select
              value={selectedModel}
              onChange={(e) => handleModelChange(e.target.value)}
              aria-label="Model"
              title="Model"
              className="input max-w-[220px] text-xs"
            >
              {models.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          )}

          <select
            value={dateRange}
            onChange={(e) => setDateRange(e.target.value)}
            aria-label="Date range"
            title="Date range"
            className="input text-xs"
          >
            <option value="today">Today</option>
            <option value="week">This Week</option>
            <option value="month">This Month</option>
          </select>

          <motion.button
            onClick={handleWeeklyReport}
            disabled={reportLoading}
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
            {reportLoading ? 'Generating...' : 'Weekly Report'}
          </motion.button>

          <AnimatePresence initial={false}>
            {messages.length > 0 && (
              <motion.button
                key="clear"
                onClick={handleClearContext}
                initial={{ opacity: 0, scale: 0.9 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.9 }}
                whileHover={reduced ? undefined : { y: -1 }}
                whileTap={reduced ? undefined : { scale: 0.97 }}
                transition={spring}
                className="btn text-xs hover:border-bad/40 hover:text-bad"
              >
                <Trash2 size={12} />
                Clear
              </motion.button>
            )}
          </AnimatePresence>
        </RevealItem>
      </Reveal>

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
              hint="Ask anything about your usage data. Nothing is sent until you ask — every question is a request to the configured model."
              action={
                <Reveal className="mt-4 flex flex-wrap justify-center gap-2" stagger={0.06}>
                  {QUICK_QUESTIONS.map((q) => (
                    <RevealItem key={q}>
                      <motion.button
                        onClick={() => handleSend(q)}
                        whileHover={reduced ? undefined : { y: -3, scale: 1.02 }}
                        whileTap={reduced ? undefined : { scale: 0.98 }}
                        transition={spring}
                        className="card-interactive px-4 py-2.5 text-sm text-fg-muted hover:text-fg"
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
          {messages.map((message, idx) => {
            const text = extractText(message);
            const isLast = idx === messages.length - 1;
            const isUser = message.role === "user";
            return (
              <motion.div
                key={message.id}
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
                      {(message as any).parts?.map((part: any, i: number) => {
                        if (part.type === "text") {
                          return (
                            <Streamdown
                              key={i}
                              isAnimating={isLoading && isLast}
                              className="text-fg"
                            >
                              {part.text}
                            </Streamdown>
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
                            onClick={stop}
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
                  {isLast && stillGenerating && (
                    <div className="mt-2 flex items-center gap-1.5 text-xs text-accent">
                      <Loader2 size={12} className="animate-spin" />
                      Still generating server-side…
                    </div>
                  )}
                  {isLast && pollTimedOut && (
                    <div className="mt-2 flex items-center gap-1.5 text-xs text-warn">
                      <AlertTriangle size={12} />
                      Generation was interrupted — send a message to continue
                    </div>
                  )}
                </div>
              </motion.div>
            );
          })}
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
                  onClick={stop}
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
            !stillGenerating &&
            !pollTimedOut &&
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
                    whileHover={reduced ? undefined : { y: -2 }}
                    whileTap={reduced ? undefined : { scale: 0.97 }}
                    transition={spring}
                    className="btn btn-ghost text-xs hover:border-accent/30 hover:text-accent"
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
        disabled={isLoading}
        includeData={includeData}
        onToggleData={() => setIncludeData(!includeData)}
      />
    </div>
  );
}

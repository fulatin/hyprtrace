import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import type { AiMessage, AiModelsResponse } from "../lib/types";
import ChatInput from "../components/ChatInput";
import ToolCallCard from "../components/ToolCallCard";

const QUICK_QUESTIONS = [
  "What window am I using right now?",
  "Which apps did I use the most today?",
  "How many workspaces do I have and what's on them?",
  "Analyze my efficiency this week",
  "Show my Hyprland version and monitors",
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
  const [hasAutoAnalyzed, setHasAutoAnalyzed] = useState(false);
  const [incompleteId, setIncompleteId] = useState<number | null>(null);
  const [pollTimedOut, setPollTimedOut] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const sendCalledRef = useRef(false);
  const dbIdToUiId = useRef(new Map<number, string>());

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
      sendCalledRef.current = true;
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
            const uiId = crypto.randomUUID();
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
        } else {
          setHasAutoAnalyzed(false);
        }
        setHistoryLoaded(true);
      })
      .catch(() => setHistoryLoaded(true));
  }, []);

  useEffect(() => {
    if (
      historyLoaded &&
      messages.length === 0 &&
      !hasAutoAnalyzed &&
      !sendCalledRef.current
    ) {
      setHasAutoAnalyzed(true);
      const timer = setTimeout(() => {
        handleSend("Analyze today's usage data and give me insights");
      }, 500);
      return () => clearTimeout(timer);
    }
  }, [historyLoaded, messages.length, hasAutoAnalyzed, handleSend]);

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

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleClearContext = async () => {
    setMessages([]);
    // Keep auto-analysis suppressed after a manual clear — the user explicitly
    // dismissed the conversation, so don't fire the proactive prompt again.
    setHasAutoAnalyzed(true);
    sendCalledRef.current = false;
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

  return (
    <div className="flex flex-col h-[calc(100vh-3rem)] animate-fadeIn">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-xl font-bold flex items-center gap-2">
          <Bot size={20} className="text-cyan-400" />
          AI Analysis
        </h2>
        <div className="flex items-center gap-3">
          <button
            onClick={handleWeeklyReport}
            disabled={reportLoading}
            className="flex items-center gap-1.5 bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-xs text-gray-300 hover:bg-gray-700 transition-colors disabled:opacity-50"
          >
            <Sparkles size={12} className="text-cyan-400" />
            {reportLoading ? 'Generating...' : 'Weekly Report'}
          </button>
          <select
            value={dateRange}
            onChange={(e) => setDateRange(e.target.value)}
            className="bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-xs text-gray-300 focus:ring-cyan-500"
          >
            <option value="today">Today</option>
            <option value="week">This Week</option>
            <option value="month">This Month</option>
          </select>
          {messages.length > 0 && (
            <button
              onClick={handleClearContext}
              className="flex items-center gap-1.5 bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-xs text-gray-400 hover:text-red-400 hover:border-red-800 transition-colors"
            >
              <Trash2 size={12} />
              Clear
            </button>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-auto bg-gray-900 border border-gray-800 rounded-t-xl p-4 space-y-4">
        {!historyLoaded ? (
          <div className="text-center py-12">
            <Loader2
              size={24}
              className="animate-spin text-gray-500 mx-auto"
            />
          </div>
        ) : messages.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-full text-center py-12 animate-fadeInUp">
            <div className="relative mb-6">
              <Bot size={56} className="text-cyan-400/30 mx-auto" />
              <Sparkles
                size={20}
                className="text-cyan-400 absolute -top-1 -right-1 animate-pulse"
              />
            </div>
            <p className="text-gray-400 mb-2 text-lg">
              AI Analysis Assistant
            </p>
            <p className="text-gray-500 text-sm mb-8">
              Ask anything about your usage data
            </p>
            <div className="flex flex-wrap justify-center gap-2 max-w-lg">
              {QUICK_QUESTIONS.map((q, i) => (
                <button
                  key={q}
                  onClick={() => handleSend(q)}
                  className="bg-gray-800 border border-gray-700 rounded-xl px-4 py-2.5 text-sm text-gray-300 
                    hover:bg-gray-700 hover:border-cyan-500/30 hover:text-cyan-100 
                    transition-all duration-200"
                  style={{ animationDelay: `${i * 80}ms` }}
                >
                  {q}
                </button>
              ))}
            </div>
          </div>
        ) : null}

        {messages.map((message, idx) => {
          const text = extractText(message);
          const isLast = idx === messages.length - 1;
          return (
            <div
              key={message.id}
              className={
                message.role === "user"
                  ? "flex justify-end animate-fadeInUp"
                  : "flex justify-start animate-fadeInUp"
              }
              style={{ animationDelay: "0ms" }}
            >
              <div
                className={
                  message.role === "user"
                    ? "max-w-[80%] rounded-2xl px-4 py-3 text-sm bg-cyan-600/20 text-cyan-100 border border-cyan-500/30"
                    : "max-w-[80%] rounded-2xl px-4 py-3 text-sm bg-gray-800 text-gray-200 border border-gray-700"
                }
              >
                {message.role === "user" ? (
                  <p className="whitespace-pre-wrap">{text}</p>
                ) : (
                  <>
                    {(message as any).parts?.map((part: any, i: number) => {
                      if (part.type === "text") {
                        return (
                          <Streamdown key={i} isAnimating={isLoading && isLast}>
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
                      <span className="inline-block w-2 h-4 bg-cyan-400 ml-1 animate-blink" />
                    )}
                  </>
                )}
                {isLast && stillGenerating && (
                  <div className="mt-2 flex items-center gap-1.5 text-xs text-cyan-400/80">
                    <Loader2 size={12} className="animate-spin" />
                    Still generating server-side…
                  </div>
                )}
                {isLast && pollTimedOut && (
                  <div className="mt-2 flex items-center gap-1.5 text-xs text-amber-400">
                    <AlertTriangle size={12} />
                    Generation was interrupted — send a message to continue
                  </div>
                )}
              </div>
            </div>
          );
        })}

        {isLoading && (
          <div className="flex justify-start">
            <div className="bg-gray-800/50 border border-gray-700/50 rounded-2xl px-4 py-3 flex items-center gap-2 text-sm text-gray-400">
              <Loader2 size={14} className="animate-spin" />
              <button
                onClick={stop}
                className="ml-1 p-1 rounded hover:bg-gray-700 transition-colors"
                title="Stop generating"
              >
                <Square size={12} />
              </button>
            </div>
          </div>
        )}

        {error && !isLoading && (
          <div className="text-center text-red-400 text-xs mb-4 bg-red-900/20 border border-red-800/30 rounded-xl px-4 py-3">
            {error.message}
          </div>
        )}

        {!isLoading &&
          !stillGenerating &&
          !pollTimedOut &&
          lastAssistantText &&
          messages[messages.length - 1]?.role === "assistant" && (
            <div className="flex flex-wrap gap-2 pt-2 animate-fadeInUp">
              {FOLLOW_UP_QUESTIONS.default.map((q) => (
                <button
                  key={q}
                  onClick={() => handleSend(q)}
                  className="bg-gray-800/60 border border-gray-700/60 rounded-lg px-3 py-1.5 text-xs text-gray-400 
                    hover:bg-gray-700 hover:border-cyan-500/30 hover:text-cyan-100 
                    transition-all duration-200"
                >
                  {q}
                </button>
              ))}
            </div>
          )}

        <div ref={messagesEndRef} />
      </div>

      <ChatInput
        onSend={handleSend}
        disabled={isLoading}
        includeData={includeData}
        onToggleData={() => setIncludeData(!includeData)}
        selectedProvider={selectedProvider}
        onProviderChange={handleProviderChange}
        providers={providers}
        selectedModel={selectedModel}
        onModelChange={handleModelChange}
      />
    </div>
  );
}

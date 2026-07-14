import { useEffect, useMemo, useRef, useState } from "react";
import { Bot, Loader2, Trash2, Square } from "lucide-react";
import { Streamdown } from "streamdown";
import "streamdown/styles.css";
import { useChat } from "@ai-sdk/react";
import { TextStreamChatTransport } from "ai";
import { api } from "../lib/api";
import type { AiMessage, AiModelsResponse } from "../lib/types";
import ChatInput from "../components/ChatInput";

const QUICK_QUESTIONS = [
  "Which apps did I use the most today?",
  "Analyze my efficiency this week",
  "Help me identify time waste",
  "Give me a productivity summary",
];

export default function AIChat() {
  const [selectedProvider, setSelectedProvider] = useState("ollama");
  const [providers, setProviders] = useState<Record<string, string[]>>({});
  const [includeData, setIncludeData] = useState(true);
  const [historyLoaded, setHistoryLoaded] = useState(false);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  const transport = useMemo(
    () =>
      new TextStreamChatTransport({
        api: "/api/ai/chat/stream/text",
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
              include_data: (body as any)?.include_data ?? includeData,
              date_range: "today",
            },
          };
        },
      }),
    [selectedProvider, includeData],
  );

  const { messages, setMessages, sendMessage, stop, status, error } = useChat({
    transport,
  });

  useEffect(() => {
    api
      .aiModels()
      .then((res: AiModelsResponse) => {
        setProviders(res.providers);
        setSelectedProvider(res.default);
      })
      .catch(() => {});

    api
      .aiConversations()
      .then((convs: AiMessage[]) => {
        if (convs.length > 0) {
          setMessages(
            convs.map((c) => ({
              id: crypto.randomUUID(),
              role: c.role as "user" | "assistant",
              parts: [{ type: "text" as const, text: c.content }],
            })),
          );
        }
        setHistoryLoaded(true);
      })
      .catch(() => setHistoryLoaded(true));
  }, []);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleSend = (message: string) => {
    sendMessage({ text: message });
  };

  const handleClearContext = async () => {
    setMessages([]);
    try {
      await api.clearConversations();
    } catch {}
  };

  const isLoading = status === "submitted" || status === "streaming";

  return (
    <div className="flex flex-col h-[calc(100vh-3rem)]">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-xl font-bold flex items-center gap-2">
          <Bot size={20} className="text-cyan-400" />
          AI Analysis
        </h2>
        {messages.length > 0 && (
          <button
            onClick={handleClearContext}
            className="flex items-center gap-1.5 bg-gray-800 border border-gray-700 rounded-lg px-3 py-1.5 text-xs text-gray-400 hover:text-red-400 hover:border-red-800 transition-colors"
          >
            <Trash2 size={12} />
            Clear context
          </button>
        )}
      </div>

      <div className="flex-1 overflow-auto bg-gray-900 border border-gray-800 rounded-t-xl p-4">
        {!historyLoaded ? (
          <div className="text-center py-12">
            <Loader2 size={24} className="animate-spin text-gray-500 mx-auto" />
          </div>
        ) : messages.length === 0 ? (
          <div className="text-center py-12">
            <Bot size={48} className="text-gray-500 mx-auto mb-4" />
            <p className="text-gray-400 mb-6">
              Hi! I can help analyze your window usage data.
            </p>
            <div className="flex flex-wrap justify-center gap-2">
              {QUICK_QUESTIONS.map((q) => (
                <button
                  key={q}
                  onClick={() => handleSend(q)}
                  className="bg-gray-800 border border-gray-700 rounded-lg px-3 py-2 text-xs text-gray-300 hover:bg-gray-700 transition-colors"
                >
                  {q}
                </button>
              ))}
            </div>
          </div>
        ) : null}

        {messages.map((message) => (
          <div
            key={message.id}
            className={
              message.role === "user"
                ? "flex justify-end mb-4"
                : "flex justify-start mb-4"
            }
          >
            <div
              className={
                message.role === "user"
                  ? "max-w-[80%] rounded-xl px-4 py-3 text-sm bg-cyan-600/20 text-cyan-100 border border-cyan-500/30"
                  : "max-w-[80%] rounded-xl px-4 py-3 text-sm bg-gray-800 text-gray-200 border border-gray-700"
              }
            >
              {(() => {
                const text = (message as any).parts
                  ?.filter((p: any) => p.type === "text")
                  .map((p: any) => p.text)
                  .join("");
                return message.role === "user" ? (
                  <p className="whitespace-pre-wrap">{text}</p>
                ) : (
                  <Streamdown isAnimating={isLoading}>{text}</Streamdown>
                );
              })()}
            </div>
          </div>
        ))}

        {isLoading && (
          <div className="flex justify-start mb-4">
            <div className="bg-gray-800 border border-gray-700 rounded-xl px-4 py-3 flex items-center gap-2 text-sm text-gray-400">
              <Loader2 size={14} className="animate-spin" />
              <button
                onClick={stop}
                className="ml-2 p-1 rounded hover:bg-gray-700 transition-colors"
                title="Stop generating"
              >
                <Square size={12} />
              </button>
            </div>
          </div>
        )}

        {error && !isLoading && (
          <div className="text-center text-red-400 text-xs mb-4">
            {error.message}
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
        onProviderChange={setSelectedProvider}
        providers={providers}
      />
    </div>
  );
}

import { useEffect, useRef, useState } from 'react';
import { Bot, Loader2 } from 'lucide-react';
import { api } from '../lib/api';
import type { AiModelsResponse } from '../lib/types';
import ChatMessageComponent from '../components/ChatMessage';
import ChatInput from '../components/ChatInput';

interface Message {
  role: 'user' | 'assistant';
  content: string;
}

const QUICK_QUESTIONS = [
  'Which apps did I use the most today?',
  'Analyze my efficiency this week',
  'Help me identify time waste',
  'Give me a productivity summary',
];

export default function AIChat() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(false);
  const [includeData, setIncludeData] = useState(true);
  const [selectedProvider, setSelectedProvider] = useState('ollama');
  const [providers, setProviders] = useState<Record<string, string[]>>({});
  const [error, setError] = useState<string | null>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.aiModels().then((res: AiModelsResponse) => {
      setProviders(res.providers);
      setSelectedProvider(res.default);
    }).catch(() => {});
  }, []);

  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages]);

  const handleSend = async (message: string) => {
    setError(null);
    setMessages((prev) => [...prev, { role: 'user', content: message }]);
    setLoading(true);

    try {
      const res = await api.aiChat(selectedProvider, message, includeData, 'today');
      setMessages((prev) => [...prev, { role: 'assistant', content: res.reply }]);
    } catch (e) {
      const errMsg = e instanceof Error ? e.message : 'Unknown error';
      setError(errMsg);
      setMessages((prev) => [
        ...prev,
        { role: 'assistant', content: `Error: ${errMsg}` },
      ]);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="flex flex-col h-[calc(100vh-3rem)]">
      <div className="flex items-center justify-between mb-4">
        <h2 className="text-xl font-bold flex items-center gap-2">
          <Bot size={20} className="text-cyan-400" />
          AI Analysis
        </h2>
      </div>

      <div className="flex-1 overflow-auto bg-gray-900 border border-gray-800 rounded-t-xl p-4">
        {messages.length === 0 && (
          <div className="text-center py-12">
            <Bot size={48} className="text-gray-500 mx-auto mb-4" />
            <p className="text-gray-400 mb-6">Hi! I can help analyze your window usage data.</p>
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
        )}

        {messages.map((msg, i) => (
          <ChatMessageComponent key={i} role={msg.role} content={msg.content} />
        ))}

        {loading && (
          <div className="flex justify-start mb-4">
            <div className="bg-gray-800 border border-gray-700 rounded-xl px-4 py-3 flex items-center gap-2 text-sm text-gray-400">
              <Loader2 size={14} className="animate-spin" />
              Thinking...
            </div>
          </div>
        )}

        {error && !loading && (
          <div className="text-center text-red-400 text-xs mb-4">{error}</div>
        )}

        <div ref={messagesEndRef} />
      </div>

      <ChatInput
        onSend={handleSend}
        disabled={loading}
        includeData={includeData}
        onToggleData={() => setIncludeData(!includeData)}
        selectedProvider={selectedProvider}
        onProviderChange={setSelectedProvider}
        providers={providers}
      />
    </div>
  );
}
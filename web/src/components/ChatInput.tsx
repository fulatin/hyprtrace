import { useState, useRef, useCallback, useEffect } from 'react';
import { Send, Mic, MicOff } from 'lucide-react';

declare global {
  interface Window {
    SpeechRecognition?: any;
    webkitSpeechRecognition?: any;
  }
}

interface ChatInputProps {
  onSend: (message: string) => void;
  disabled?: boolean;
  includeData: boolean;
  onToggleData: () => void;
  selectedProvider: string;
  onProviderChange: (provider: string) => void;
  providers: Record<string, string[]>;
  selectedModel: string;
  onModelChange: (model: string) => void;
}

export default function ChatInput({
  onSend,
  disabled,
  includeData,
  onToggleData,
  selectedProvider,
  onProviderChange,
  providers,
  selectedModel,
  onModelChange,
}: ChatInputProps) {
  const [message, setMessage] = useState('');
  const [listening, setListening] = useState(false);
  const recognitionRef = useRef<any>(null);

  const models = providers[selectedProvider] ?? [];

  const speechSupported = typeof window !== 'undefined' && !!(
    window.SpeechRecognition || window.webkitSpeechRecognition
  );

  const stopListening = useCallback(() => {
    if (recognitionRef.current) {
      try { recognitionRef.current.stop(); } catch {}
    }
    recognitionRef.current = null;
    setListening(false);
  }, []);

  const toggleListening = () => {
    if (listening) {
      stopListening();
      return;
    }
    const SR = window.SpeechRecognition || window.webkitSpeechRecognition;
    if (!SR) return;
    const rec = new SR();
    recognitionRef.current = rec;
    rec.continuous = false;
    rec.interimResults = false;
    rec.lang = navigator.language || 'en-US';
    rec.onresult = (e: any) => {
      const text = e.results?.[0]?.[0]?.transcript ?? '';
      if (text) {
        setMessage((prev) => (prev.trim() ? prev + ' ' + text : text));
      }
    };
    rec.onend = () => {
      setListening(false);
      recognitionRef.current = null;
    };
    rec.onerror = () => {
      setListening(false);
      recognitionRef.current = null;
    };
    setListening(true);
    try { rec.start(); } catch {}
  };

  useEffect(() => {
    return () => stopListening();
  }, [stopListening]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (message.trim() && !disabled) {
      stopListening();
      onSend(message.trim());
      setMessage('');
    }
  };

  return (
    <form onSubmit={handleSubmit} className="border-t border-gray-800 p-4">
      <div className="flex items-center gap-3 mb-2 flex-wrap">
        <label className="flex items-center gap-2 text-xs text-gray-400 cursor-pointer">
          <input
            type="checkbox"
            checked={includeData}
            onChange={onToggleData}
            className="rounded border-gray-600 bg-gray-800 text-cyan-500 focus:ring-cyan-500"
          />
          Include usage data
        </label>

        <select
          value={selectedProvider}
          onChange={(e) => onProviderChange(e.target.value)}
          className="text-xs bg-gray-800 border border-gray-700 rounded px-2 py-1 text-gray-300 focus:ring-cyan-500 focus:border-cyan-500"
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
            onChange={(e) => onModelChange(e.target.value)}
            className="text-xs bg-gray-800 border border-gray-700 rounded px-2 py-1 text-gray-300 focus:ring-cyan-500 focus:border-cyan-500 max-w-[220px]"
            title="Model"
          >
            {models.map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </select>
        )}
      </div>

      <div className="flex gap-2">
        <input
          type="text"
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder="Ask about your usage data or live system state..."
          disabled={disabled}
          className="flex-1 bg-gray-800 border border-gray-700 rounded-md px-4 py-2 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500 disabled:opacity-50"
        />
        {speechSupported && (
          <button
            type="button"
            onClick={toggleListening}
            disabled={disabled}
            title={listening ? 'Stop listening' : 'Voice input'}
            className={`rounded-md px-3 py-2 text-sm transition-colors border ${
              listening
                ? 'bg-red-600/20 border-red-500/40 text-red-400 animate-pulse'
                : 'bg-gray-800 border-gray-700 text-gray-300 hover:bg-gray-700'
            } disabled:opacity-50`}
          >
            {listening ? <MicOff size={16} /> : <Mic size={16} />}
          </button>
        )}
        <button
          type="submit"
          disabled={disabled || !message.trim()}
          className="bg-cyan-600 hover:bg-cyan-500 disabled:bg-gray-700 disabled:opacity-50 text-white rounded-md px-4 py-2 text-sm transition-colors"
        >
          <Send size={16} />
        </button>
      </div>
    </form>
  );
}

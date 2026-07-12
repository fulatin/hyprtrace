import { useState } from 'react';
import { Send } from 'lucide-react';

interface ChatInputProps {
  onSend: (message: string) => void;
  disabled?: boolean;
  includeData: boolean;
  onToggleData: () => void;
  selectedProvider: string;
  onProviderChange: (provider: string) => void;
  providers: Record<string, string[]>;
}

export default function ChatInput({
  onSend,
  disabled,
  includeData,
  onToggleData,
  selectedProvider,
  onProviderChange,
  providers,
}: ChatInputProps) {
  const [message, setMessage] = useState('');

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (message.trim() && !disabled) {
      onSend(message.trim());
      setMessage('');
    }
  };

  return (
    <form onSubmit={handleSubmit} className="border-t border-gray-800 p-4">
      <div className="flex items-center gap-3 mb-2">
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
      </div>

      <div className="flex gap-2">
        <input
          type="text"
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          placeholder="Ask about your usage data..."
          disabled={disabled}
          className="flex-1 bg-gray-800 border border-gray-700 rounded-lg px-4 py-2 text-sm text-gray-200 placeholder-gray-500 focus:ring-cyan-500 focus:border-cyan-500 disabled:opacity-50"
        />
        <button
          type="submit"
          disabled={disabled || !message.trim()}
          className="bg-cyan-600 hover:bg-cyan-500 disabled:bg-gray-700 disabled:opacity-50 text-white rounded-lg px-4 py-2 text-sm transition-colors"
        >
          <Send size={16} />
        </button>
      </div>
    </form>
  );
}
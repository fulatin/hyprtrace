import { useState, useRef, useCallback, useEffect } from 'react';
import { AnimatePresence, motion, useReducedMotion } from 'framer-motion';
import { Mic, MicOff, Send } from 'lucide-react';
import { spring } from '../lib/motion';

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
}

export default function ChatInput({
  onSend,
  disabled,
  includeData,
  onToggleData,
}: ChatInputProps) {
  const [message, setMessage] = useState('');
  const [listening, setListening] = useState(false);
  const recognitionRef = useRef<any>(null);
  const reduced = useReducedMotion();

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

  const canSend = message.trim().length > 0 && !disabled;

  return (
    <form onSubmit={handleSubmit} className="border-t border-line bg-surface/60 p-4 backdrop-blur-xl">
      <div className="mb-2 flex flex-wrap items-center gap-3">
        <label className="flex cursor-pointer items-center gap-2 text-xs text-fg-muted transition-colors hover:text-fg">
          <input
            type="checkbox"
            checked={includeData}
            onChange={onToggleData}
            className="h-3.5 w-3.5 rounded border-line bg-surface-3 accent-accent focus:ring-2 focus:ring-accent/25"
          />
          Include usage data
        </label>

        <AnimatePresence initial={false}>
          {listening && (
            <motion.span
              key="listening"
              className="chip-bad"
              initial={{ opacity: 0, scale: 0.9 }}
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.9 }}
              transition={spring}
            >
              <motion.span
                className="h-1.5 w-1.5 rounded-full bg-bad"
                animate={reduced ? { opacity: 0.8 } : { opacity: [0.3, 1, 0.3] }}
                transition={reduced ? { duration: 0 } : { duration: 1.1, repeat: Infinity, ease: 'easeInOut' }}
              />
              Listening…
            </motion.span>
          )}
        </AnimatePresence>

        <span className="ml-auto hidden text-[11px] text-fg-faint sm:block">Enter to send</span>
      </div>

      <div className="flex items-stretch gap-2">
        {/* A <label> wrapper so a click anywhere in the field focuses the input. */}
        <label className="input flex min-w-0 flex-1 cursor-text items-center gap-2 px-3 py-2 focus-within:border-accent/60 focus-within:ring-2 focus-within:ring-accent/25">
          <input
            type="text"
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            placeholder="Ask about your usage data or live system state..."
            disabled={disabled}
            aria-label="Message"
            className="min-w-0 flex-1 bg-transparent text-sm text-fg placeholder:text-fg-faint focus:outline-none disabled:cursor-not-allowed disabled:opacity-50"
          />
        </label>

        {speechSupported && (
          <motion.button
            type="button"
            onClick={toggleListening}
            disabled={disabled}
            title={listening ? 'Stop listening' : 'Voice input'}
            aria-label={listening ? 'Stop listening' : 'Voice input'}
            whileTap={reduced ? undefined : { scale: 0.94 }}
            className={`btn relative px-3 py-2 ${
              listening ? 'border-bad/40 bg-bad/10 text-bad' : ''
            }`}
          >
            {listening && !reduced && (
              <motion.span
                className="pointer-events-none absolute inset-0 rounded-lg border border-bad/60"
                initial={{ opacity: 0.6, scale: 1 }}
                animate={{ opacity: 0, scale: 1.5 }}
                transition={{ duration: 1.4, repeat: Infinity, ease: 'easeOut' }}
              />
            )}
            <AnimatePresence mode="wait" initial={false}>
              <motion.span
                key={listening ? 'mic-off' : 'mic-on'}
                initial={{ opacity: 0, scale: 0.7 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.7 }}
                transition={reduced ? { duration: 0 } : spring}
              >
                {listening ? <MicOff size={16} /> : <Mic size={16} />}
              </motion.span>
            </AnimatePresence>
          </motion.button>
        )}

        <motion.button
          type="submit"
          disabled={!canSend}
          aria-label="Send message"
          whileHover={canSend && !reduced ? { y: -1 } : undefined}
          whileTap={canSend && !reduced ? { scale: 0.94 } : undefined}
          animate={{ scale: canSend ? 1 : 0.97 }}
          transition={reduced ? { duration: 0 } : spring}
          className={`btn px-4 py-2 ${canSend ? 'btn-accent glow-accent' : 'text-fg-faint'}`}
        >
          <Send size={16} />
        </motion.button>
      </div>
    </form>
  );
}

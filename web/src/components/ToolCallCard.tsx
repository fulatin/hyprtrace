import { useState } from 'react';
import { AlertTriangle, Check, ChevronDown, Loader2, Wrench } from 'lucide-react';

interface ToolCallCardProps {
  part: any; // AI SDK tool part: { type: `tool-${name}`, toolCallId, state, input, output?, errorText? }
}

function toolName(part: any): string {
  if (typeof part.type === 'string' && part.type.startsWith('tool-')) {
    return part.type.slice(5);
  }
  return part.toolName ?? 'tool';
}

export default function ToolCallCard({ part }: ToolCallCardProps) {
  const [open, setOpen] = useState(false);
  const name = toolName(part);
  const state: string = part.state ?? 'input-available';

  const running = state === 'input-streaming' || state === 'input-available';
  const done = state === 'output-available';
  const failed = state === 'output-error' || state === 'output-denied';

  return (
    <div className="my-2 overflow-hidden rounded-md border border-gray-700/80 bg-gray-800/50 text-xs animate-scaleIn">
      <button
        onClick={() => setOpen(!open)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-gray-800/80 transition-colors"
      >
        <Wrench size={12} className="shrink-0 text-cyan-400" />
        <span className="font-mono text-cyan-300">{name}</span>
        <span className="ml-auto flex items-center gap-1 text-gray-500">
          {running && (
            <>
              <Loader2 size={11} className="animate-spin" />
              calling…
            </>
          )}
          {done && (
            <>
              <Check size={11} className="text-emerald-400" />
              done
            </>
          )}
          {failed && (
            <>
              <AlertTriangle size={11} className="text-amber-400" />
              failed
            </>
          )}
        </span>
        <ChevronDown
          size={12}
          className={`shrink-0 text-gray-500 transition-transform ${open ? 'rotate-180' : ''}`}
        />
      </button>
      {open && (
        <pre className="max-h-64 overflow-auto border-t border-gray-700/80 px-3 py-2 text-[11px] leading-relaxed text-gray-400">
          {JSON.stringify(
            {
              ...(part.input !== undefined ? { input: part.input } : {}),
              ...(done ? { output: part.output } : {}),
              ...(failed ? { error: part.errorText } : {}),
            },
            null,
            2,
          )}
        </pre>
      )}
    </div>
  );
}

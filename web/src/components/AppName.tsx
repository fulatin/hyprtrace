import type { AppMetadata } from '../lib/types';

interface AppNameProps {
  cls: string;
  metadata?: AppMetadata | null;
}

// Deterministic color derived from a simple hash of the class, clamped to a
// palette that fits the dark UI.
function colorForClass(cls: string): string {
  const colors = [
    '#22d3ee', '#8b5cf6', '#10b981', '#f59e0b', '#ef4444',
    '#3b82f6', '#ec4899', '#14b8a6', '#f97316', '#6366f1',
  ];
  let h = 0;
  for (let i = 0; i < cls.length; i++) {
    h = (h * 31 + cls.charCodeAt(i)) >>> 0;
  }
  return colors[h % colors.length];
}

export default function AppName({ cls, metadata }: AppNameProps) {
  const displayName = metadata?.display_name && metadata.display_name.length > 0
    ? metadata.display_name
    : cls;
  const initial = displayName.charAt(0).toUpperCase();

  return (
    <span className="inline-flex items-center gap-2">
      <span
        className="inline-flex items-center justify-center w-5 h-5 rounded text-[11px] font-semibold shrink-0"
        style={{ backgroundColor: `${colorForClass(cls)}33`, color: colorForClass(cls) }}
      >
        {initial}
      </span>
      <span className="truncate">{displayName}</span>
    </span>
  );
}

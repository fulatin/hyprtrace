import type { AppMetadata } from '../lib/types';

interface AppNameProps {
  cls: string;
  metadata?: AppMetadata | null;
}

/** Token-backed badge palettes — picked deterministically so a class keeps its hue. */
const PALETTES = [
  { badge: 'bg-accent/15 text-accent', ring: 'ring-accent/25' },
  { badge: 'bg-accent-2/15 text-accent-2', ring: 'ring-accent-2/25' },
  { badge: 'bg-accent-3/15 text-accent-3', ring: 'ring-accent-3/25' },
];

function paletteForClass(cls: string) {
  let h = 0;
  for (let i = 0; i < cls.length; i++) {
    h = (h * 31 + cls.charCodeAt(i)) >>> 0;
  }
  return PALETTES[h % PALETTES.length];
}

export default function AppName({ cls, metadata }: AppNameProps) {
  const displayName = metadata?.display_name && metadata.display_name.length > 0
    ? metadata.display_name
    : cls;
  const initial = displayName.charAt(0).toUpperCase();
  const palette = paletteForClass(cls);

  return (
    <span className="inline-flex min-w-0 items-center gap-2">
      <span
        aria-hidden
        className={`inline-flex h-5 w-5 shrink-0 items-center justify-center rounded-md text-[11px] font-semibold ring-1 ring-inset ${palette.badge} ${palette.ring}`}
      >
        {initial}
      </span>
      <span className="min-w-0 truncate text-sm font-medium text-fg">{displayName}</span>
    </span>
  );
}

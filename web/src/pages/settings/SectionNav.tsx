import { motion } from 'framer-motion';
import { Cpu, Download, FileText, FolderKanban, Server, ShieldCheck, Tags, Target, Trash2 } from 'lucide-react';
import { spring } from '../../lib/motion';

/** The sections of the Settings page, in page order. */
export const SECTIONS = [
  { id: 'status', label: 'Server', icon: Server },
  { id: 'ai', label: 'AI config', icon: Cpu },
  { id: 'categories', label: 'Categories', icon: Tags },
  { id: 'goals', label: 'Goals', icon: Target },
  { id: 'projects', label: 'Projects', icon: FolderKanban },
  { id: 'report', label: 'Weekly report', icon: FileText },
  { id: 'export', label: 'Export', icon: Download },
  { id: 'privacy', label: 'Privacy', icon: ShieldCheck },
  { id: 'danger', label: 'Danger zone', icon: Trash2 },
] as const;

export type SectionId = (typeof SECTIONS)[number]['id'];

/** One rail entry: rendered twice (sticky tabs on narrow screens, rail on lg). */
export function SectionNav({
  active,
  onSelect,
  layoutId,
  className = '',
}: {
  active: SectionId;
  onSelect: (id: SectionId) => void;
  layoutId: string;
  className?: string;
}) {
  return (
    <nav aria-label="Settings sections" className={className}>
      <div className="flex gap-1 overflow-x-auto pb-0.5 lg:flex-col lg:overflow-visible lg:pb-0">
        {SECTIONS.map(({ id, label, icon: Icon }) => {
          const isActive = active === id;
          return (
            <button
              key={id}
              type="button"
              onClick={() => onSelect(id)}
              aria-current={isActive ? 'true' : undefined}
              className={`relative flex shrink-0 items-center gap-2 rounded-lg px-3 py-2 text-sm transition-colors ${
                isActive ? 'text-fg' : 'text-fg-muted hover:text-fg'
              }`}
            >
              {isActive && (
                <motion.span
                  layoutId={layoutId}
                  className="absolute inset-0 rounded-lg bg-accent/10 ring-1 ring-inset ring-accent/25"
                  transition={spring}
                />
              )}
              <Icon size={15} className={`relative z-10 shrink-0 ${isActive ? 'text-accent' : 'text-fg-faint'}`} />
              <span className="relative z-10 whitespace-nowrap">{label}</span>
            </button>
          );
        })}
      </div>
    </nav>
  );
}

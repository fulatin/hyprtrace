# HyprTrace Web — design system

Everything below already exists. Reuse it; do not invent parallel variants.

## Colour tokens (Tailwind classes)

All colours are CSS variables (see `src/index.css`), so **every class below works
in dark *and* light theme automatically**. Never hard-code `gray-*`, `cyan-*`,
`slate-*`, `white`, `black`, or `#hex` in page code — use these:

| Purpose | Classes |
|---|---|
| Page background | `bg-bg`, `bg-bg-soft` |
| Cards / panels | `bg-surface`, `bg-surface-2`, `bg-surface-3` |
| Borders | `border-line`, `border-line-strong` |
| Text | `text-fg`, `text-fg-muted`, `text-fg-faint` |
| Accent (cyan) | `text-accent`, `bg-accent`, `border-accent`, `bg-accent/10` … |
| Secondary accent (violet) | `text-accent-2`, `bg-accent-2/10` … |
| Semantic | `text-good`, `text-warn`, `text-bad` (+ `/10` washes) |

Opacity modifiers work everywhere (`bg-surface/60`, `border-accent/30`).

## Component classes (`@layer components`)

- `.card` — bordered surface with soft shadow. `.card-pad` = card + `p-5`.
- `.card-interactive` — adds hover border/glow (pair with `motion` `hoverLift`).
- `.btn`, `.btn-accent`, `.btn-ghost` — buttons.
- `.input` — text/date inputs and selects.
- `.chip`, `.chip-neutral`, `.chip-accent`, `.chip-good`, `.chip-bad`, `.chip-warn` — pills.
- `.panel-title`, `.panel-sub` — panel heading text styles.
- `.divider`, `.dot-grid`, `.glow-accent`, `.text-gradient`, `.tnum` (tabular numbers).
- `.skeleton` — shimmer placeholder (prefer `<Skeleton>`).

## React primitives

```tsx
import Card, { Panel } from '../components/ui/Card';        // Card, Panel (title/icon/hint/actions)
import StatCard from '../components/StatCard';              // metric tile, count-up + sparkline + delta
import CountUp from '../components/ui/CountUp';             // <CountUp value={ms} format={formatDuration} />
import SegmentedControl from '../components/ui/SegmentedControl'; // animated pill; needs unique layoutId
import { Reveal, RevealItem, RevealOnScroll } from '../components/ui/Reveal';
import { ProgressBar, EmptyState, Skeleton, SkeletonPanel, SkeletonStats } from '../components/ui/Feedback';
import Sparkline from '../components/ui/Sparkline';
import ThemeToggle from '../components/ui/ThemeToggle';
```

### StatCard

```tsx
<StatCard
  icon={<Clock size={15} />}
  label="Active time"
  value={summary.total_active_ms}          // number → animated count-up
  format={formatDuration}                   // required for numeric values
  subtext="today"
  accent="accent"                           // accent | accent-2 | good | warn | bad
  delta={{ value: deltaMs, format: formatDelta }}   // optional change chip
  spark={last14Days}                        // optional trend line
/>
```

### Motion presets (`src/lib/motion.ts`)

`spring`, `softSpring`, `easeOut`, `staggerContainer(stagger)`, `fadeUp`,
`fadeIn`, `scaleIn`, `pageTransition`, `listItem`, `expand`, `hoverLift`,
`barTransition`.

Rules of thumb:

1. Page-level entrance is already handled by `Layout` (route transition). Page
   content should stagger with `<Reveal>` / `<RevealItem>`, not re-animate the
   whole page.
2. Lists: wrap the list in `<Reveal>` and each row in `<RevealItem variants={listItem}>`.
3. Any width/height/position that changes with data must animate via `motion`
   (`initial`/`animate`), not a CSS class, so value updates are smooth.
4. Charts: `isAnimationActive` + `animationDuration={700}` + `animationEasing="ease-out"`.
5. Keep durations 0.2–0.45 s. Respect `useReducedMotion()` for anything large.
6. Never animate `box-shadow` on scroll — use `border`/`opacity`/`transform`.

## Formatting (`src/lib/format.ts`)

`formatDuration` (coarse), `formatDurationFine` (with seconds), `formatDelta`
(signed), `formatPercent`, `formatMemKb`, `formatHour`, `formatShortDate`,
`formatRelativeDate`.

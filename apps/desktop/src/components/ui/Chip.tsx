import type { ReactNode } from 'react'
import { cx } from '@/lib/format'
import type { Tone } from '@/lib/selectors'

// Colour is status only: a chip that is merely busy (catching up, sifting)
// must not read like one that wants something (warn). They shared ochre
// once, and "Catching up 1,240" looked like a warning on every first sync.
const tones: Record<Tone, string> = {
  neutral: 'bg-sand-2 text-ink-2',
  ok: 'bg-moss/15 text-moss',
  busy: 'bg-slate/15 text-slate',
  warn: 'bg-ochre/15 text-ochre',
  bad: 'bg-rust/15 text-rust',
}

export function Chip({ tone = 'neutral', children, title, className, mono }: { tone?: Tone; children: ReactNode; title?: string; className?: string; mono?: boolean }) {
  return (
    <span title={title} className={cx('inline-flex items-center gap-1 rounded px-1.5 py-px text-2xs font-medium leading-4 whitespace-nowrap', tones[tone], mono && 'mono', className)}>
      {children}
    </span>
  )
}

import type { ReactNode } from 'react'
import { cx } from '@/lib/format'
import type { Tone } from '@/lib/selectors'

const tones: Record<Tone, string> = {
  neutral: 'bg-sand-2 text-ink-2',
  ok: 'bg-moss/15 text-moss',
  busy: 'bg-ochre/15 text-ochre',
  warn: 'bg-ochre/15 text-ochre',
  bad: 'bg-rust/15 text-rust',
}

export function Chip({ tone = 'neutral', children, title, className, mono }: { tone?: Tone; children: ReactNode; title?: string; className?: string; mono?: boolean }) {
  return (
    <span title={title} className={cx('inline-flex items-center gap-1 rounded px-1.5 py-px text-[11px] font-medium leading-4 whitespace-nowrap', tones[tone], mono && 'mono', className)}>
      {children}
    </span>
  )
}

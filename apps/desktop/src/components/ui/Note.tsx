import type { ReactNode } from 'react'
import { cx } from '@/lib/format'
import type { Tone } from '@/lib/selectors'

const tones: Record<Tone, string> = {
  neutral: 'border-line bg-sand text-ink-2',
  ok: 'border-moss/30 bg-moss/10 text-ink',
  busy: 'border-slate/30 bg-slate/10 text-ink',
  warn: 'border-ochre/40 bg-ochre/10 text-ink',
  bad: 'border-rust/40 bg-rust/10 text-ink',
}

export function Note({ tone = 'neutral', title, children, actions, className }: { tone?: Tone; title?: string; children?: ReactNode; actions?: ReactNode; className?: string }) {
  return (
    <div className={cx('rounded-md border px-3 py-2 text-sm', tones[tone], className)} role={tone === 'bad' ? 'alert' : undefined}>
      {title ? <div className="font-medium">{title}</div> : null}
      {children ? <div className={cx(title && 'mt-0.5')}>{children}</div> : null}
      {actions ? <div className="mt-2 flex flex-wrap gap-2">{actions}</div> : null}
    </div>
  )
}

/**
 * Small pieces the account screens share: a radio group of described
 * options, which replaced bare selects whose labels had to carry the whole
 * explanation, and an avatar.
 */
import type { ReactNode } from 'react'
import { cx } from '@/lib/format'
import { avatarTone, type Option } from './model'

export function OptionCards<T extends string>({ label, options, value, onChange, columns = options.length, badge }: { label: string; options: Option<T>[]; value: T; onChange: (v: T) => void; columns?: number; badge?: (v: T) => ReactNode }) {
  return (
    <div role="radiogroup" aria-label={label} className="grid gap-2" style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>
      {options.map((o) => {
        const on = o.value === value
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={on}
            onClick={() => onChange(o.value)}
            className={cx('flex flex-col items-start gap-1 rounded-md border px-3 py-2 text-left transition-colors focus-visible:outline focus-visible:outline-2 focus-visible:outline-ochre', on ? 'border-ochre bg-ochre/5' : 'border-line hover:border-ink-2/40 hover:bg-sand')}
          >
            <span className="flex w-full items-center gap-2 text-sm font-medium text-ink">
              <span aria-hidden className={cx('grid size-3.5 shrink-0 place-items-center rounded-full border', on ? 'border-ochre' : 'border-clay')}>
                {on ? <span className="size-1.5 rounded-full bg-ochre" /> : null}
              </span>
              {o.label}
              {badge ? <span className="ml-auto">{badge(o.value)}</span> : null}
            </span>
            <span className="text-xs leading-snug text-ink-2">{o.detail}</span>
          </button>
        )
      })}
    </div>
  )
}

const TONES = ['bg-ochre/20 text-ochre', 'bg-moss/20 text-moss', 'bg-rust/15 text-rust', 'bg-sand-2 text-ink', 'bg-ink/10 text-ink-2']

export function Avatar({ id, name, size = 'md' }: { id: string; name: string; size?: 'md' | 'lg' }) {
  const initial = (name.trim()[0] ?? '?').toUpperCase()
  return (
    <span aria-hidden className={cx('grid shrink-0 place-items-center rounded-full font-semibold', TONES[avatarTone(id)], size === 'lg' ? 'size-10 text-base' : 'size-7 text-xs')}>
      {initial}
    </span>
  )
}

/** A labelled block inside the detail panel. */
export function Block({ title, hint, children, aside }: { title: string; hint?: ReactNode; children: ReactNode; aside?: ReactNode }) {
  return (
    <section className="border-t border-line py-4 first:border-t-0">
      <div className="mb-2 flex items-baseline gap-2">
        <h3 className="text-sm font-semibold">{title}</h3>
        {aside ? <span className="ml-auto">{aside}</span> : null}
      </div>
      {hint ? <p className="-mt-1 mb-2 max-w-2xl text-xs text-ink-2">{hint}</p> : null}
      {children}
    </section>
  )
}

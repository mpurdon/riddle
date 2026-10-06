/**
 * The keyboard-triggered pickers: a small overlay driven by j/k, Enter, the
 * digits and Escape, so `s`, `r` and `f` can offer a choice without the
 * hands leaving the keyboard. One picker for every list: Finds (snooze,
 * retype), Unresolved (which Find type) and the message lists (snooze).
 * The audit found `f` in Unresolved hard-coded Respond because it had no
 * picker of its own, and a second snooze picker had grown beside the
 * Inbox because this one lived under Finds.
 */
import { useEffect, useState } from 'react'
import type { FindType } from '@/bindings/FindType'
import { cx } from '@/lib/format'
import { findTypeLabel, FIND_TYPES } from '@/lib/selectors'
import { followUpOptions, snoozeOptions } from './Bits'

export interface OverlayItem {
  label: string
  onSelect: () => void
}

export function OverlayList({ title, items, onClose }: { title: string; items: OverlayItem[]; onClose: () => void }) {
  const [i, setI] = useState(0)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
      else if (e.key === 'j' || e.key === 'ArrowDown') setI((x) => Math.min(items.length - 1, x + 1))
      else if (e.key === 'k' || e.key === 'ArrowUp') setI((x) => Math.max(0, x - 1))
      else if (e.key === 'Enter') items[i]?.onSelect()
      else if (/^[1-9]$/.test(e.key)) items[Number(e.key) - 1]?.onSelect()
      else return
      e.preventDefault()
      e.stopPropagation()
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [i, items, onClose])
  return (
    <div className="absolute left-1/2 top-24 z-30 w-72 -translate-x-1/2 rounded-md border border-line bg-paper p-2 shadow-xl fade-in" role="dialog" aria-label={title}>
      <div className="px-2 py-1 text-xs font-semibold text-clay">{title}</div>
      {items.map((it, idx) => (
        <button key={it.label} type="button" className={cx('flex w-full items-center justify-between rounded px-2 py-1 text-left text-sm', idx === i ? 'bg-sand' : 'hover:bg-sand/60')} onClick={it.onSelect}>
          <span>{it.label}</span>
          <kbd>{idx + 1}</kbd>
        </button>
      ))}
      <div className="px-2 pt-1 text-2xs text-clay">Esc closes</div>
    </div>
  )
}

/** Snooze until: the same options as the menu, fixed when the picker opens. */
export function SnoozeOverlay({ onPick, onClose }: { onPick: (until: string) => void; onClose: () => void }) {
  const [options] = useState(() => snoozeOptions())
  return <OverlayList title="Snooze until" onClose={onClose} items={options.map((o) => ({ label: o.label, onSelect: () => onPick(o.until) }))} />
}

/** Follow up by: the same options as the menu, fixed when the picker opens. */
export function FollowUpOverlay({ onPick, onClose }: { onPick: (dueAt: string | null) => void; onClose: () => void }) {
  const [options] = useState(() => followUpOptions())
  return <OverlayList title="Follow up by" onClose={onClose} items={options.map((o) => ({ label: o.label, onSelect: () => onPick(o.dueAt) }))} />
}

/** A Find type; `current` (when retyping) is left out since it is no change. */
export function FindTypeOverlay({ title, current, onPick, onClose }: { title: string; current?: FindType; onPick: (t: FindType) => void; onClose: () => void }) {
  return <OverlayList title={title} onClose={onClose} items={FIND_TYPES.filter((t) => t !== current).map((t) => ({ label: findTypeLabel(t), onSelect: () => onPick(t) }))} />
}

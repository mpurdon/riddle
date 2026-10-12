/**
 * `g l`: go to any place by name, the keyboard way into the mail column
 * (and the only way to a folded or empty label without opening folds).
 * The same shape as the ⌘K palette: one input driving a listbox, the first
 * match preselected, Enter goes.
 */
import * as RD from '@radix-ui/react-dialog'
import { useEffect, useMemo, useRef, useState } from 'react'
import type { MailPlaces } from '@/bindings/MailPlaces'
import { cx, fmtInt } from '@/lib/format'
import { allPlaces, matchPlace } from './places'

const KIND: Record<string, string> = { mailbox: 'Mail', label: 'Label', topic: 'Topic' }

export function PlacePicker({ open, onOpenChange, places, onPlace }: { open: boolean; onOpenChange: (o: boolean) => void; places: MailPlaces | undefined; onPlace: (place: string) => void }) {
  const [q, setQ] = useState('')
  const [active, setActive] = useState(0)
  const listRef = useRef<HTMLDivElement>(null)
  const rows = useMemo(() => (places ? allPlaces(places).filter((r) => matchPlace(r, q)) : []), [places, q])
  const current = rows[Math.min(active, Math.max(0, rows.length - 1))]
  useEffect(() => {
    if (open) {
      setQ('')
      setActive(0)
    }
  }, [open])
  useEffect(() => setActive(0), [q])
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>('[data-active="true"]')?.scrollIntoView?.({ block: 'nearest' })
  }, [active])
  const go = (place: string) => {
    onOpenChange(false)
    onPlace(place)
  }
  return (
    <RD.Root open={open} onOpenChange={onOpenChange}>
      <RD.Portal>
        <RD.Overlay className="fixed inset-0 z-40 bg-ink/30 fade-in" />
        <RD.Content aria-label="Go to a label or Topic" className="fixed left-1/2 top-[12vh] z-50 w-[480px] max-w-[92vw] -translate-x-1/2 overflow-hidden rounded-lg border border-line bg-paper shadow-xl fade-in focus:outline-none">
          <RD.Title className="sr-only">Go to a label or Topic</RD.Title>
          <RD.Description className="sr-only">Type part of a name. Up and down move, Enter goes, Esc closes.</RD.Description>
          <div className="flex items-center gap-2 border-b border-line px-3">
            <input
              value={q}
              onChange={(e) => setQ(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'ArrowDown') {
                  e.preventDefault()
                  setActive((i) => Math.min(i + 1, rows.length - 1))
                } else if (e.key === 'ArrowUp') {
                  e.preventDefault()
                  setActive((i) => Math.max(i - 1, 0))
                } else if (e.key === 'Enter') {
                  e.preventDefault()
                  if (current) go(current.place)
                }
              }}
              role="combobox"
              aria-label="Label or Topic"
              aria-expanded="true"
              aria-controls="place-picker-list"
              aria-autocomplete="list"
              aria-activedescendant={current ? `place-${current.place}` : undefined}
              placeholder="Type a label or Topic…"
              spellCheck={false}
              autoComplete="off"
              className="h-10 w-full bg-transparent text-sm text-ink placeholder:text-clay focus:outline-none"
            />
            <kbd className="pointer-events-none shrink-0">Esc</kbd>
          </div>
          <div id="place-picker-list" ref={listRef} role="listbox" aria-label="Places" className="max-h-[50vh] overflow-y-auto p-1">
            {rows.map((r, i) => {
              const isActive = r === current
              return (
                <div
                  key={r.place}
                  id={`place-${r.place}`}
                  role="option"
                  aria-selected={isActive}
                  data-active={isActive || undefined}
                  className={cx('flex cursor-default select-none items-center gap-3 rounded px-2 py-1.5 text-sm', isActive ? 'bg-sand text-ink' : 'text-ink-2')}
                  onMouseMove={() => i !== active && setActive(i)}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => go(r.place)}
                >
                  <span className="min-w-0 flex-1 truncate">{r.name}</span>
                  <span className="shrink-0 text-2xs text-clay">{r.hasLabel ? 'Topic · label + Riddle' : KIND[r.kind]}</span>
                  <span className={cx('w-12 shrink-0 text-right text-2xs tabular-nums', r.unread ? 'font-semibold text-ink' : 'text-clay')}>{r.unread ? fmtInt(r.unread) : r.messages ? fmtInt(r.messages) : ''}</span>
                </div>
              )
            })}
            {!rows.length ? (
              <div className="px-2 py-6 text-center text-sm text-clay" role="status">
                {places ? 'No label or Topic has that name.' : 'Loading…'}
              </div>
            ) : null}
          </div>
        </RD.Content>
      </RD.Portal>
    </RD.Root>
  )
}

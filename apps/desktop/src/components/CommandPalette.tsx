/**
 * The ⌘K command palette (UX study, Flow 11, Option B). Every row comes
 * from the shortcut table through `useBoundCommands`, so it lists exactly
 * what is bound on the page that is on screen: a command that is not bound
 * here is absent, never greyed. Each row shows its key, because the palette
 * exists to teach the shortcut that replaces it; the hint line says so.
 *
 * The two synthetic rows follow the typed text. "Search mail" navigates to
 * the Inbox with the query; "Ask the Director" only prefills the composer
 * there and never sends, the same rule the search box learned when a
 * question was sent at once and cost a model call nobody asked for.
 *
 * It is a Radix Dialog rather than the shared Dialog: no title bar, a
 * top-centre panel, and a listbox driven from the one input. The command
 * runs after the dialog has closed, so focus that the command moves (a
 * composer, a picker) is not taken back by the dialog's focus return.
 */
import * as RD from '@radix-ui/react-dialog'
import { useEffect, useMemo, useRef, useState } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { cx } from '@/lib/format'
import { useDirector } from '@/lib/stores'
import { boundCommands, useBoundCommands, type BoundCommand } from '@/lib/useShortcuts'
import { Keys } from './ui/Kbd'

/** Globals with no place in a list: the palette itself, Esc, and `/` (the search row replaces it). */
const HIDDEN = new Set(['palette', 'escape', 'search'])

export interface PaletteRow {
  key: string
  label: string
  keys?: string
  also?: string
  run: () => void
}

export interface PaletteSection {
  title: string
  rows: PaletteRow[]
}

/** Every word typed must appear somewhere in the label, case-insensitively. */
export function matchesQuery(label: string, q: string): boolean {
  const words = q.toLowerCase().split(/\s+/).filter(Boolean)
  const hay = label.toLowerCase()
  return words.every((w) => hay.includes(w))
}

/**
 * The sections, in order: what this page binds, the `go_*` globals, the
 * other globals, then the two synthetic rows once there is text. Sections
 * with no matching row are dropped, so the first row overall is the first
 * match and is preselected.
 */
export function sectionsFor(commands: BoundCommand[], q: string, synthetic: { search: (text: string) => void; ask: (text: string) => void }): PaletteSection[] {
  const text = q.trim()
  const rows = (list: BoundCommand[]): PaletteRow[] =>
    list.filter((c) => matchesQuery(c.label, text)).map((c) => ({ key: `${c.scope}:${c.id}`, label: c.label, keys: c.keys, also: c.also, run: c.run }))
  const page = commands.filter((c) => c.scope !== 'global')
  const goTo = commands.filter((c) => c.scope === 'global' && c.id.startsWith('go_'))
  const also = commands.filter((c) => c.scope === 'global' && !c.id.startsWith('go_') && !HIDDEN.has(c.id))
  const out: PaletteSection[] = [
    { title: 'On this page', rows: rows(page) },
    { title: 'Go to', rows: rows(goTo) },
    { title: 'Also', rows: rows(also) },
  ]
  if (text) {
    out.push({
      title: 'Search and ask',
      rows: [
        { key: 'search', label: `Search mail for “${text}”`, run: () => synthetic.search(text) },
        { key: 'ask', label: `Ask the Director: “${text}”`, run: () => synthetic.ask(text) },
      ],
    })
  }
  return out.filter((s) => s.rows.length > 0)
}

export function CommandPalette({ open, onOpenChange }: { open: boolean; onOpenChange: (open: boolean) => void }) {
  const nav = useNavigate()
  const loc = useLocation()
  const bindings = useBoundCommands((s) => s.bindings)
  const [q, setQ] = useState('')
  const [active, setActive] = useState(0)
  const listRef = useRef<HTMLDivElement>(null)

  const sections = useMemo(
    () =>
      sectionsFor(boundCommands(bindings), q, {
        search: (text) => nav(`/inbox?q=${encodeURIComponent(text)}`),
        ask: (text) => {
          useDirector.getState().setPrefill(text)
          nav('/director')
        },
      }),
    // `open` is a dependency on purpose: the bound handlers are read when the palette opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [bindings, q, nav, open],
  )
  const flat = useMemo(() => sections.flatMap((s) => s.rows), [sections])
  const index = useMemo(() => new Map(flat.map((r, i) => [r.key, i])), [flat])
  const current = flat[Math.min(active, Math.max(0, flat.length - 1))]

  // A fresh palette every time: empty text, first match preselected.
  useEffect(() => {
    if (open) {
      setQ('')
      setActive(0)
    }
  }, [open])
  useEffect(() => setActive(0), [q])
  useEffect(() => {
    listRef.current?.querySelector<HTMLElement>('[data-active="true"]')?.scrollIntoView?.({ block: 'nearest' })
  }, [active, flat])

  // A global shortcut can navigate while the palette is open (⌘, is legal
  // in an input); the page under it changed, so the list would be stale.
  const lastKey = useRef(loc.key)
  useEffect(() => {
    if (lastKey.current !== loc.key) {
      lastKey.current = loc.key
      onOpenChange(false)
    }
  }, [loc.key, onOpenChange])

  const run = (row: PaletteRow) => {
    onOpenChange(false)
    window.setTimeout(row.run, 0)
  }

  return (
    <RD.Root open={open} onOpenChange={onOpenChange}>
      <RD.Portal>
        <RD.Overlay className="fixed inset-0 z-40 bg-ink/30 fade-in" />
        <RD.Content
          aria-label="Command palette"
          className="fixed left-1/2 top-[12vh] z-50 w-[560px] max-w-[92vw] -translate-x-1/2 overflow-hidden rounded-lg border border-line bg-paper shadow-xl fade-in focus:outline-none"
        >
          <RD.Title className="sr-only">Command palette</RD.Title>
          <RD.Description className="sr-only">Type to filter. Up and down move, Enter runs, Esc closes.</RD.Description>
          <div className="flex items-center gap-2 border-b border-line px-3">
            <input
              value={q}
              onChange={(e) => setQ(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'ArrowDown') {
                  e.preventDefault()
                  setActive((i) => Math.min(i + 1, flat.length - 1))
                } else if (e.key === 'ArrowUp') {
                  e.preventDefault()
                  setActive((i) => Math.max(i - 1, 0))
                } else if (e.key === 'Home' && flat.length) {
                  e.preventDefault()
                  setActive(0)
                } else if (e.key === 'End' && flat.length) {
                  e.preventDefault()
                  setActive(flat.length - 1)
                } else if (e.key === 'Enter') {
                  e.preventDefault()
                  if (current) run(current)
                }
              }}
              role="combobox"
              aria-label="Command"
              aria-expanded="true"
              aria-controls="command-palette-list"
              aria-autocomplete="list"
              aria-activedescendant={current ? `command-palette-${current.key}` : undefined}
              placeholder="Type a command, a search, or a question"
              spellCheck={false}
              autoComplete="off"
              className="h-10 w-full bg-transparent text-sm text-ink placeholder:text-clay focus:outline-none"
            />
            <kbd className="pointer-events-none shrink-0">Esc</kbd>
          </div>
          <div id="command-palette-list" ref={listRef} role="listbox" aria-label="Commands" className="max-h-[50vh] overflow-y-auto p-1">
            {sections.map((s) => (
              <div key={s.title} role="group" aria-label={s.title}>
                <div aria-hidden className="px-2 pb-0.5 pt-2 text-2xs font-semibold uppercase tracking-wide text-clay">
                  {s.title}
                </div>
                {s.rows.map((row) => {
                  const i = index.get(row.key) ?? -1
                  const isActive = row === current
                  return (
                    <div
                      key={row.key}
                      id={`command-palette-${row.key}`}
                      role="option"
                      aria-selected={isActive}
                      data-active={isActive || undefined}
                      className={cx('flex cursor-default select-none items-center justify-between gap-4 rounded px-2 py-1.5 text-sm', isActive ? 'bg-sand text-ink' : 'text-ink-2')}
                      onMouseMove={() => i >= 0 && i !== active && setActive(i)}
                      onMouseDown={(e) => e.preventDefault()}
                      onClick={() => run(row)}
                    >
                      <span className="truncate">{row.label}</span>
                      {row.keys ? <Keys keys={row.keys} also={row.also} /> : null}
                    </div>
                  )
                })}
              </div>
            ))}
            {flat.length === 0 ? (
              <div className="px-2 py-6 text-center text-sm text-clay" role="status">
                Nothing is bound here yet.
              </div>
            ) : null}
          </div>
          <div className="border-t border-line px-3 py-1.5 text-2xs text-clay">Every command shows its key, so you can skip the palette next time.</div>
        </RD.Content>
      </RD.Portal>
    </RD.Root>
  )
}

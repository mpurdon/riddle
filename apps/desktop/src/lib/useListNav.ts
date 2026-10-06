/**
 * Keyboard row navigation shared by every list: j/k (or the arrows) move,
 * Home/End jump, Enter opens, x selects. The selected row is kept in view.
 * `onCursor` fires when the cursor lands on a row, which is how a list
 * with a preview pane shows the row without opening it.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { useShortcuts } from './useShortcuts'

export function useListNav<T extends { id: string }>(
  items: T[],
  opts: { onOpen?: (item: T) => void; onCursor?: (item: T) => void; enabled?: boolean; multi?: boolean } = {},
) {
  const [cursor, setCursor] = useState(0)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const itemsRef = useRef(items)
  itemsRef.current = items
  const optsRef = useRef(opts)
  optsRef.current = opts
  const cursorRef = useRef(cursor)
  cursorRef.current = cursor
  useEffect(() => {
    if (cursor >= items.length) setCursor(Math.max(0, items.length - 1))
  }, [items.length, cursor])
  const current = items[cursor] ?? null
  const toggle = useCallback((id: string) => {
    setSelected((s) => {
      const n = new Set(s)
      if (n.has(id)) n.delete(id)
      else n.add(id)
      return n
    })
  }, [])
  const clear = useCallback(() => setSelected(new Set()), [])
  /** What a verb acts on: the checked rows, else the cursor row. Four pages had each written this. */
  const targets = (): T[] => (selected.size ? items.filter((i) => selected.has(i.id)) : current ? [current] : [])
  const moveTo = useCallback((next: number) => {
    const list = itemsRef.current
    const c = Math.max(0, Math.min(list.length - 1, next))
    setCursor(c)
    const it = list[c]
    if (it) optsRef.current.onCursor?.(it)
  }, [])
  useShortcuts(
    'list',
    {
      next: () => moveTo(cursorRef.current + 1),
      prev: () => moveTo(cursorRef.current - 1),
      first: () => moveTo(0),
      last: () => moveTo(itemsRef.current.length - 1),
      open: () => {
        const it = itemsRef.current[cursorRef.current]
        if (it) optsRef.current.onOpen?.(it)
      },
      select: () => {
        const it = itemsRef.current[cursorRef.current]
        if (it && optsRef.current.multi !== false) toggle(it.id)
      },
      select_all: () => {
        if (optsRef.current.multi === false) return
        setSelected((s) => (s.size === itemsRef.current.length ? new Set() : new Set(itemsRef.current.map((i) => i.id))))
      },
    },
    opts.enabled ?? true,
  )
  useEffect(() => {
    const el = document.querySelector<HTMLElement>(`[data-row-index="${cursor}"]`)
    el?.scrollIntoView({ block: 'nearest' })
  }, [cursor])
  return { cursor, setCursor, moveTo, current, selected, toggle, clear, setSelected, targets }
}

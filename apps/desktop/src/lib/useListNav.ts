/**
 * Keyboard row navigation shared by every list: j/k move, Enter opens,
 * x selects. The selected row is kept in view.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { useShortcuts } from './useShortcuts'

export function useListNav<T extends { id: string }>(items: T[], opts: { onOpen?: (item: T) => void; enabled?: boolean; multi?: boolean } = {}) {
  const [cursor, setCursor] = useState(0)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const itemsRef = useRef(items)
  itemsRef.current = items
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
  useShortcuts(
    'list',
    {
      next: () => setCursor((c) => Math.min(itemsRef.current.length - 1, c + 1)),
      prev: () => setCursor((c) => Math.max(0, c - 1)),
      open: () => {
        const it = itemsRef.current[cursor]
        if (it) opts.onOpen?.(it)
      },
      select: () => {
        const it = itemsRef.current[cursor]
        if (it && opts.multi !== false) toggle(it.id)
      },
      select_all: () => {
        if (opts.multi === false) return
        setSelected((s) => (s.size === itemsRef.current.length ? new Set() : new Set(itemsRef.current.map((i) => i.id))))
      },
    },
    opts.enabled ?? true,
  )
  useEffect(() => {
    const el = document.querySelector<HTMLElement>(`[data-row-index="${cursor}"]`)
    el?.scrollIntoView({ block: 'nearest' })
  }, [cursor])
  return { cursor, setCursor, current, selected, toggle, clear, setSelected }
}

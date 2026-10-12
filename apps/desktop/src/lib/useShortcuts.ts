/**
 * Binds handlers to the shortcut table for one scope. Chords ("g f") wait
 * up to a second for the second key. Handlers are looked up by shortcut id
 * so a key can only do what the table documents.
 *
 * Each mounted scope listens on its own, so one key reaches every listener.
 * While a chord is open its second key belongs to the chord: `g l` in the
 * Inbox also fired `l` (Add a Stratum) from the page's own listener, and the
 * App's listener, finding no `g l` of its own, closed the chord before the
 * page's listener could complete it. Now a key that completes a chord is
 * handled once, single keys wait while a chord is open, and the chord
 * closes only after every listener has seen the key.
 */
import { useEffect, useRef } from 'react'
import { create } from 'zustand'
import { eventKey, isEditableTarget, parseKeys, shortcutsFor, type Shortcut, type ShortcutScope } from './shortcuts'

export type Handlers = Partial<Record<string, (e: KeyboardEvent) => void>>

/**
 * What is bound right now, for the ⌘K palette: every mounted scope's
 * handlers by shortcut id, so the palette can list "Resolve · e" only while
 * a page that resolves is on screen, and run it. Generated from the same
 * table as the keys and the help sheet, never hand-written.
 */
export interface BoundCommand {
  id: string
  scope: ShortcutScope
  label: string
  keys: string
  also?: string
  run: () => void
}

/**
 * One mount's table and a getter for its current handlers. The handlers are
 * read when the palette opens, not when the page mounted: pages bind and
 * unbind ids between renders (a chooser that adds decide keys while it is
 * open), and a snapshot listed stale commands.
 */
interface Binding {
  table: Shortcut[]
  handlers: () => Handlers
}

interface CommandState {
  /** Keyed by a per-mount token so two pages binding the same id do not clash. */
  bindings: Record<string, Binding>
  set: (token: string, binding: Binding) => void
  remove: (token: string) => void
}

export const useBoundCommands = create<CommandState>((set) => ({
  bindings: {},
  set: (token, binding) => set((s) => ({ bindings: { ...s.bindings, [token]: binding } })),
  remove: (token) =>
    set((s) => {
      const next = { ...s.bindings }
      delete next[token]
      return { bindings: next }
    }),
}))

/** Every command bound at this moment, later mounts first, one entry per id. */
export function boundCommands(bindings: Record<string, Binding>): BoundCommand[] {
  const seen = new Set<string>()
  const out: BoundCommand[] = []
  for (const b of Object.values(bindings).reverse()) {
    const handlers = b.handlers()
    for (const sc of b.table) {
      const h = handlers[sc.id]
      if (!h) continue
      const key = `${sc.scope}:${sc.id}`
      if (seen.has(key)) continue
      seen.add(key)
      out.push({
        id: sc.id,
        scope: sc.scope,
        label: sc.label,
        keys: sc.keys,
        also: sc.also,
        run: () => b.handlers()[sc.id]?.(new KeyboardEvent('keydown', { key: sc.keys })),
      })
    }
  }
  return out
}

let mountSeq = 0

interface Pending {
  first: string
  at: number
  /** The key press that opened it, so other listeners of that same press do not read it as a second key. */
  event: KeyboardEvent
}

/** Presses some listener already handled; the others leave them alone. */
const handled = new WeakSet<KeyboardEvent>()
const isChord = (sc: Shortcut) => parseKeys(sc.keys).chord.length > 1

let pendingChord: Pending | null = null

/** The chord waiting for its second key, if `e` can be that key: opened by another press, under a second ago. */
const openChord = (e: KeyboardEvent): Pending | null => (pendingChord && pendingChord.event !== e && Date.now() - pendingChord.at < 1000 ? pendingChord : null)

function matchKeys(keys: string, e: KeyboardEvent, key: string): 'full' | 'first' | null {
  const p = parseKeys(keys)
  const isMac = /Mac|iPhone|iPad/.test(navigator.platform)
  const modDown = isMac ? e.metaKey : e.ctrlKey
  if (p.mod !== modDown) return null
  if (!p.mod && (e.metaKey || e.ctrlKey || e.altKey)) return null
  const first = p.chord[0]
  const isLetter = /^[a-z]$/.test(first)
  if (isLetter && p.shift !== e.shiftKey) return null
  if (p.chord.length === 1) return first === key ? 'full' : null
  if (openChord(e)?.first === first && p.chord[1] === key) return 'full'
  if (!openChord(e) && first === key) return 'first'
  return null
}

function matches(sc: Shortcut, e: KeyboardEvent, key: string): 'full' | 'first' | null {
  return matchKeys(sc.keys, e, key) ?? (sc.also ? matchKeys(sc.also, e, key) : null)
}

export function useShortcuts(scope: ShortcutScope | ShortcutScope[], handlers: Handlers, enabled = true): void {
  const ref = useRef(handlers)
  ref.current = handlers
  useEffect(() => {
    if (!enabled) return
    const scopes = Array.isArray(scope) ? scope : [scope]
    const table = scopes.flatMap(shortcutsFor)
    const token = `m${++mountSeq}`
    useBoundCommands.getState().set(token, { table, handlers: () => ref.current })
    const onKey = (e: KeyboardEvent) => {
      if (handled.has(e)) return
      const key = eventKey(e)
      if (isEditableTarget(e.target) && key !== 'escape' && !(e.metaKey || e.ctrlKey)) return
      const chordOpen = !!openChord(e)
      let startedChord = false
      for (const sc of table) {
        const h = ref.current[sc.id]
        if (!h) continue
        if (chordOpen && !isChord(sc)) continue
        const m = matches(sc, e, key)
        if (m === 'full') {
          e.preventDefault()
          handled.add(e)
          pendingChord = null
          h(e)
          return
        }
        if (m === 'first') startedChord = true
      }
      if (startedChord && !chordOpen) {
        pendingChord = { first: key, at: Date.now(), event: e }
        e.preventDefault()
      } else if (chordOpen) {
        // After every listener has had this key: no chord took it, so the chord is over.
        queueMicrotask(() => {
          if (!handled.has(e)) pendingChord = null
        })
      }
    }
    window.addEventListener('keydown', onKey)
    return () => {
      window.removeEventListener('keydown', onKey)
      useBoundCommands.getState().remove(token)
    }
  }, [scope, enabled]) // eslint-disable-line react-hooks/exhaustive-deps
}

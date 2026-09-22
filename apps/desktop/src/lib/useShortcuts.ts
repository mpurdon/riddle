/**
 * Binds handlers to the shortcut table for one scope. Chords ("g f") wait
 * up to a second for the second key. Handlers are looked up by shortcut id
 * so a key can only do what the table documents.
 */
import { useEffect, useRef } from 'react'
import { eventKey, isEditableTarget, parseKeys, shortcutsFor, type Shortcut, type ShortcutScope } from './shortcuts'

export type Handlers = Partial<Record<string, (e: KeyboardEvent) => void>>

interface Pending {
  first: string
  at: number
}

let pendingChord: Pending | null = null

function matches(sc: Shortcut, e: KeyboardEvent, key: string): 'full' | 'first' | null {
  const p = parseKeys(sc.keys)
  const isMac = /Mac|iPhone|iPad/.test(navigator.platform)
  const modDown = isMac ? e.metaKey : e.ctrlKey
  if (p.mod !== modDown) return null
  if (!p.mod && (e.metaKey || e.ctrlKey || e.altKey)) return null
  const first = p.chord[0]
  const isLetter = /^[a-z]$/.test(first)
  if (isLetter && p.shift !== e.shiftKey) return null
  if (p.chord.length === 1) return first === key ? 'full' : null
  if (pendingChord && Date.now() - pendingChord.at < 1000 && pendingChord.first === first && p.chord[1] === key) return 'full'
  if (!pendingChord && first === key) return 'first'
  return null
}

export function useShortcuts(scope: ShortcutScope | ShortcutScope[], handlers: Handlers, enabled = true): void {
  const ref = useRef(handlers)
  ref.current = handlers
  useEffect(() => {
    if (!enabled) return
    const scopes = Array.isArray(scope) ? scope : [scope]
    const table = scopes.flatMap(shortcutsFor)
    const onKey = (e: KeyboardEvent) => {
      const key = eventKey(e)
      if (isEditableTarget(e.target) && key !== 'escape' && !(e.metaKey || e.ctrlKey)) return
      let startedChord = false
      for (const sc of table) {
        const h = ref.current[sc.id]
        if (!h) continue
        const m = matches(sc, e, key)
        if (m === 'full') {
          e.preventDefault()
          pendingChord = null
          h(e)
          return
        }
        if (m === 'first') startedChord = true
      }
      if (startedChord) {
        pendingChord = { first: key, at: Date.now() }
        e.preventDefault()
      } else if (pendingChord) pendingChord = null
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [scope, enabled]) // eslint-disable-line react-hooks/exhaustive-deps
}

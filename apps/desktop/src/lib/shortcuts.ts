/**
 * The one shortcut table. It drives both the key handlers and the `?` help
 * sheet, so a shortcut cannot exist without being documented (README
 * "Keyboard, Help and Empty States"). Keys use the pattern "g f" for a
 * two-key chord and "shift+u" for modifiers.
 */

export type ShortcutScope = 'global' | 'list' | 'finds' | 'messages' | 'meshes' | 'director' | 'review'

export interface Shortcut {
  id: string
  keys: string
  label: string
  scope: ShortcutScope
}

export const SCOPE_LABELS: Record<ShortcutScope, string> = {
  global: 'Everywhere',
  list: 'Any list',
  finds: 'Finds',
  messages: 'Inbox and message',
  review: 'Spoil, Contamination and Unresolved',
  meshes: 'Meshes',
  director: 'Director',
}

export const SHORTCUTS: Shortcut[] = [
  { id: 'help', keys: '?', label: 'Show this help sheet', scope: 'global' },
  { id: 'search', keys: '/', label: 'Ask Riddle (search or a question)', scope: 'global' },
  { id: 'compose', keys: 'c', label: 'Compose a message', scope: 'global' },
  { id: 'task_centre', keys: 't', label: 'Open the Task Centre', scope: 'global' },
  { id: 'escape', keys: 'escape', label: 'Close a dialog, panel or tour', scope: 'global' },
  { id: 'go_summary', keys: 'g s', label: 'Go to Summary', scope: 'global' },
  { id: 'go_finds', keys: 'g f', label: 'Go to Finds', scope: 'global' },
  { id: 'go_unresolved', keys: 'g u', label: 'Go to Unresolved', scope: 'global' },
  { id: 'go_inbox', keys: 'g i', label: 'Go to Inbox', scope: 'global' },
  { id: 'go_spoil', keys: 'g p', label: 'Go to Spoil', scope: 'global' },
  { id: 'go_activity', keys: 'g a', label: 'Go to Activity', scope: 'global' },
  { id: 'go_meshes', keys: 'g m', label: 'Go to Meshes', scope: 'global' },
  { id: 'go_director', keys: 'g d', label: 'Go to the Director', scope: 'global' },
  { id: 'go_settings', keys: 'g ,', label: 'Go to Settings', scope: 'global' },
  { id: 'undo', keys: 'u', label: 'Undo the last undoable action', scope: 'global' },

  { id: 'next', keys: 'j', label: 'Next row', scope: 'list' },
  { id: 'prev', keys: 'k', label: 'Previous row', scope: 'list' },
  { id: 'open', keys: 'enter', label: 'Open the selected row', scope: 'list' },
  { id: 'select', keys: 'x', label: 'Select or deselect the row', scope: 'list' },
  { id: 'select_all', keys: 'shift+x', label: 'Select or clear every row', scope: 'list' },

  { id: 'resolve', keys: 'e', label: 'Resolve the Find', scope: 'finds' },
  { id: 'snooze', keys: 's', label: 'Snooze (opens the picker)', scope: 'finds' },
  { id: 'dismiss', keys: 'd', label: 'Dismiss: not important', scope: 'finds' },
  { id: 'retype', keys: 'r', label: 'Change the Find type', scope: 'finds' },
  { id: 'why', keys: 'w', label: 'Why? Show the explanation', scope: 'finds' },

  { id: 'archive', keys: 'e', label: 'Archive', scope: 'messages' },
  { id: 'spoil', keys: '#', label: 'Move to Spoil', scope: 'messages' },
  { id: 'contamination', keys: '!', label: 'Mark as Contamination', scope: 'messages' },
  { id: 'snooze_message', keys: 's', label: 'Snooze', scope: 'messages' },
  { id: 'label', keys: 'l', label: 'Add a Stratum', scope: 'messages' },
  { id: 'reply', keys: 'r', label: 'Reply', scope: 'messages' },
  { id: 'forward', keys: 'f', label: 'Forward', scope: 'messages' },
  { id: 'why_message', keys: 'w', label: 'Why? Show the explanation', scope: 'messages' },

  { id: 'not_spoil', keys: 'n', label: 'Not Spoil / Restore the selection', scope: 'review' },
  { id: 'never_hidden', keys: 'h', label: 'Never hide this sender', scope: 'review' },
  { id: 'decide_find', keys: 'f', label: 'Decide: it is a Find', scope: 'review' },
  { id: 'decide_filed', keys: 'i', label: 'Decide: Filed', scope: 'review' },
  { id: 'decide_spoil', keys: '#', label: 'Decide: Spoil', scope: 'review' },

  { id: 'mesh_preview', keys: 'mod+enter', label: 'Preview impact of the Mesh text', scope: 'meshes' },
  { id: 'mesh_save', keys: 'mod+s', label: 'Apply the Mesh text', scope: 'meshes' },

  { id: 'director_send', keys: 'mod+enter', label: 'Send to the Director', scope: 'director' },
]

export interface ParsedKeys {
  chord: string[]
  mod: boolean
  shift: boolean
}

export function parseKeys(keys: string): ParsedKeys {
  const parts = keys.split(' ')
  const chord: string[] = []
  let mod = false
  let shift = false
  for (const part of parts) {
    const segs = part.split('+')
    for (const s of segs) {
      if (s === 'mod') mod = true
      else if (s === 'shift') shift = true
      else chord.push(s)
    }
  }
  return { chord, mod, shift }
}

const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform)

/** The human form for the help sheet. */
export function keysLabel(keys: string): string[] {
  return keys.split(' ').map((part) =>
    part
      .split('+')
      .map((s) => {
        if (s === 'mod') return isMac ? '⌘' : 'Ctrl'
        if (s === 'shift') return '⇧'
        if (s === 'enter') return '↵'
        if (s === 'escape') return 'Esc'
        return s.length === 1 ? s : s[0].toUpperCase() + s.slice(1)
      })
      .join(''),
  )
}

/** The key name a KeyboardEvent produces, normalised for the table. */
export function eventKey(e: KeyboardEvent): string {
  const k = e.key
  if (k === 'Escape') return 'escape'
  if (k === 'Enter') return 'enter'
  if (k === ' ') return 'space'
  return k.length === 1 ? k.toLowerCase() : k.toLowerCase()
}

export function shortcutsFor(scope: ShortcutScope): Shortcut[] {
  return SHORTCUTS.filter((s) => s.scope === scope)
}

export function isEditableTarget(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false
  const tag = t.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || t.isContentEditable
}

/**
 * The one shortcut table. It drives both the key handlers and the `?` help
 * sheet, so a shortcut cannot exist without being documented (README
 * "Keyboard, Help and Empty States"). Keys use the pattern "g f" for a
 * two-key chord and "shift+u" for modifiers; `also` names a second key that
 * does the same (the arrow keys beside j/k), shown in the sheet and matched
 * by the handler.
 *
 * The same letter can mean different things on different pages (e resolves
 * a Find and archives a message); the sheet groups by scope so the reader
 * can see that, and a page binds only the scopes it stands in.
 */

export type ShortcutScope = 'global' | 'list' | 'finds' | 'messages' | 'meshes' | 'director' | 'review'

export interface Shortcut {
  id: string
  keys: string
  /** An alternative key that does the same. */
  also?: string
  label: string
  scope: ShortcutScope
}

export const SCOPE_LABELS: Record<ShortcutScope, string> = {
  global: 'Everywhere',
  list: 'Any list',
  finds: 'Finds',
  messages: 'Inbox, Spoil, Contamination and the message page',
  review: 'Spoil, Contamination, Unresolved and the Shadow review',
  meshes: 'Meshes',
  director: 'Director',
}

export const SHORTCUTS: Shortcut[] = [
  { id: 'help', keys: '?', label: 'Show this help sheet', scope: 'global' },
  { id: 'search', keys: '/', label: 'Ask Riddle: search, or a question for the Director', scope: 'global' },
  { id: 'compose', keys: 'c', label: 'Compose a message', scope: 'global' },
  { id: 'task_centre', keys: 't', label: 'Open the Task Centre', scope: 'global' },
  { id: 'escape', keys: 'escape', label: 'Close a dialog, panel or tip', scope: 'global' },
  { id: 'go_summary', keys: 'g s', label: 'Go to Summary', scope: 'global' },
  { id: 'go_finds', keys: 'g f', label: 'Go to Finds', scope: 'global' },
  { id: 'go_unresolved', keys: 'g u', label: 'Go to Unresolved', scope: 'global' },
  { id: 'go_inbox', keys: 'g i', label: 'Go to Inbox', scope: 'global' },
  { id: 'go_spoil', keys: 'g p', label: 'Go to Spoil', scope: 'global' },
  { id: 'go_activity', keys: 'g a', label: 'Go to Activity', scope: 'global' },
  { id: 'go_meshes', keys: 'g m', label: 'Go to Meshes', scope: 'global' },
  { id: 'go_director', keys: 'g d', label: 'Go to the Director', scope: 'global' },
  { id: 'go_settings', keys: 'g ,', label: 'Go to Settings', scope: 'global' },
  { id: 'settings', keys: 'mod+,', label: 'Open Settings', scope: 'global' },
  { id: 'palette', keys: 'mod+k', label: 'Command palette: every command here, with its key', scope: 'global' },
  { id: 'toggle_level', keys: 'g e', label: 'Show everything, or back to the essentials', scope: 'global' },
  { id: 'undo', keys: 'u', label: 'Undo the last undoable action', scope: 'global' },

  { id: 'next', keys: 'j', also: 'arrowdown', label: 'Next row', scope: 'list' },
  { id: 'prev', keys: 'k', also: 'arrowup', label: 'Previous row', scope: 'list' },
  { id: 'first', keys: 'home', label: 'First row', scope: 'list' },
  { id: 'last', keys: 'end', label: 'Last row', scope: 'list' },
  { id: 'open', keys: 'enter', label: 'Open the selected row', scope: 'list' },
  { id: 'select', keys: 'x', label: 'Select or deselect the row', scope: 'list' },
  { id: 'select_all', keys: 'shift+x', label: 'Select or clear every row', scope: 'list' },

  { id: 'resolve', keys: 'e', label: 'Resolve: mark the Find as dealt with', scope: 'finds' },
  { id: 'snooze', keys: 's', label: 'Snooze (opens the picker)', scope: 'finds' },
  { id: 'dismiss', keys: 'd', label: 'Not important: close it and tell Riddle it was wrong', scope: 'finds' },
  { id: 'retype', keys: 'r', label: 'Change the Find type (opens the picker)', scope: 'finds' },
  { id: 'why', keys: 'w', label: 'Why? Show or hide the explanation', scope: 'finds' },

  { id: 'archive', keys: 'e', label: 'Archive: record it as Filed', scope: 'messages' },
  { id: 'spoil', keys: '#', label: 'Move to Spoil', scope: 'messages' },
  { id: 'contamination', keys: '!', label: 'Mark as Contamination', scope: 'messages' },
  { id: 'snooze_message', keys: 's', label: 'Snooze (opens the picker)', scope: 'messages' },
  { id: 'label', keys: 'l', label: 'Add a Stratum (opens the picker)', scope: 'messages' },
  { id: 'go_place', keys: 'g l', label: 'Go to a label or Topic (opens the picker)', scope: 'messages' },
  { id: 'toggle_mail_column', keys: 'mod+\\', label: 'Collapse or expand the mail column', scope: 'messages' },
  { id: 'reply', keys: 'r', label: 'Reply', scope: 'messages' },
  { id: 'forward', keys: 'f', label: 'Forward', scope: 'messages' },
  { id: 'follow_up', keys: 'shift+f', label: 'Follow up by… (opens the date picker; a Find until a reply arrives)', scope: 'messages' },
  { id: 'why_message', keys: 'w', label: 'Why? Show the explanation', scope: 'messages' },
  { id: 'next_message', keys: 'n', label: 'Next message (on the message page)', scope: 'messages' },
  { id: 'prev_message', keys: 'p', label: 'Previous message (on the message page)', scope: 'messages' },

  { id: 'not_spoil', keys: 'n', label: 'Not Spoil / Restore (Spoil and Contamination)', scope: 'review' },
  { id: 'never_hidden', keys: 'h', label: 'Never hide this sender', scope: 'review' },
  { id: 'decide_find', keys: 'f', label: 'Decide: a Find (opens the type picker)', scope: 'review' },
  { id: 'decide_follow_up', keys: 'shift+f', label: 'Decide: follow up by… (opens the date picker)', scope: 'review' },
  { id: 'decide_filed', keys: 'i', label: 'Decide: Filed', scope: 'review' },
  { id: 'decide_spoil', keys: '#', label: 'Decide: Spoil', scope: 'review' },
  { id: 'decide_contamination', keys: '!', label: 'Decide: Contamination', scope: 'review' },
  { id: 'shadow_agree', keys: 'a', label: 'Agree with the Shadow proposal (Shadow review)', scope: 'review' },
  { id: 'shadow_disagree', keys: 'd', label: 'Disagree: decide it yourself (Shadow review)', scope: 'review' },

  { id: 'mesh_preview', keys: 'mod+enter', label: 'Preview impact of the Mesh text', scope: 'meshes' },
  { id: 'mesh_save', keys: 'mod+s', label: 'Apply the Mesh text', scope: 'meshes' },
  { id: 'mesh_insert', keys: 'mod+i', label: 'Insert a fact, header or Signal into the Mesh text', scope: 'meshes' },

  { id: 'director_send', keys: 'mod+enter', label: 'Send to the Director from anywhere on the page (in the composer, Enter sends and Shift+Enter adds a line)', scope: 'director' },
  { id: 'proposal_apply', keys: '1', label: 'Apply the selected proposal', scope: 'director' },
  { id: 'proposal_adjust', keys: '2', label: 'Adjust the selected proposal in the Mesh editor', scope: 'director' },
  { id: 'proposal_decline', keys: '3', label: 'Decline the selected proposal', scope: 'director' },
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

const KEY_NAMES: Record<string, string> = {
  arrowdown: '↓',
  arrowup: '↑',
  arrowleft: '←',
  arrowright: '→',
  home: 'Home',
  end: 'End',
  enter: '↵',
  escape: 'Esc',
  space: 'Space',
}

/** The human form for the help sheet: "g f", "⇧X", "⌘K". A letter under a modifier reads as the keycap. */
export function keysLabel(keys: string): string[] {
  return keys.split(' ').map((part) => {
    const withModifier = part.includes('+')
    return part
      .split('+')
      .map((s) => {
        if (s === 'mod') return isMac ? '⌘' : 'Ctrl'
        if (s === 'shift') return '⇧'
        if (KEY_NAMES[s]) return KEY_NAMES[s]
        if (s.length === 1) return withModifier ? s.toUpperCase() : s
        return s[0].toUpperCase() + s.slice(1)
      })
      .join('')
  })
}

/** The key name a KeyboardEvent produces, normalised for the table. */
export function eventKey(e: KeyboardEvent): string {
  const k = e.key
  if (k === 'Escape') return 'escape'
  if (k === 'Enter') return 'enter'
  if (k === ' ') return 'space'
  return k.toLowerCase()
}

export function shortcutsFor(scope: ShortcutScope): Shortcut[] {
  return SHORTCUTS.filter((s) => s.scope === scope)
}

/** The shortcut a page binds under an id, for a button title: "Resolve (e)". */
export function keyHint(id: string): string {
  const sc = SHORTCUTS.find((s) => s.id === id)
  return sc ? keysLabel(sc.keys).join(' ') : ''
}

export function isEditableTarget(t: EventTarget | null): boolean {
  if (!(t instanceof HTMLElement)) return false
  const tag = t.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || t.isContentEditable
}

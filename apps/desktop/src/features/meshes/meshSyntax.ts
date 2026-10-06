/**
 * The vocabulary the Mesh editor can insert (⌘I) and lists in its Syntax
 * box: the header lines, facts, the kept mail headers, and Signal references.
 *
 * FACTS and HEADERS are `Facts::NAMES` (crates/riddle-core/src/facts.rs) and
 * `KEPT_HEADERS` (crates/riddle-core/src/mail.rs) as `cargo test -p
 * riddle-core` writes them to bindings/meshVocabulary.ts. They were a hand
 * copy once, guarded by a second hand copy in the test; the Syntax box
 * before that ended its fact list in "…" because nothing in the page could
 * vouch for the rest.
 *
 * Snippets follow the real grammar (crates/riddle-mesh/src/parse.rs): a bare
 * fact means `= true`, text facts take `=` or `CONTAINS`, headers are
 * `header <Name>`, Signals are `name@vN > .80` (with `:CHOICE` for a Choice
 * Signal). Header lines (`Scope:`, `Priority:`, `Autonomy:`) are statements,
 * so they land on a line of their own with their value selected; the
 * `Autonomy:` line existed in the parser for a release before the editor
 * offered it anywhere, and nobody found it.
 */
import { MESH_FACTS, MESH_HEADERS } from '@/bindings/meshVocabulary'
import type { Signal } from '@/bindings/Signal'
import type { MenuEntry } from '@/components/ui/Menu'

export type FactKind = 'text' | 'bool' | 'number'

export interface FactDef {
  name: string
  kind: FactKind
}

export const FACTS: FactDef[] = MESH_FACTS.map((f) => ({ name: f.name, kind: f.kind }))

/** The extra headers Sift keeps; any other `header <Name>` reads as empty text. */
export const HEADERS: string[] = [...MESH_HEADERS]

/** A snippet and where the caret lands inside it, as offsets from its start. */
export interface Snippet {
  text: string
  /** [start, end] of the selection after insertion; equal for a plain caret. */
  cursor: [number, number]
  /** A whole statement: inserted on a line of its own, after the caret's line. */
  line?: boolean
}

/** A header line of the Mesh, for the Insert menu's first group. */
export interface HeaderLine {
  label: string
  hint: string
  snippet: Snippet
}

/** `Autonomy: Suggest` with "Suggest" selected, so typing replaces it. */
function headerLine(prefix: string, value: string): Snippet {
  return { text: `${prefix}${value}`, cursor: [prefix.length, prefix.length + value.length], line: true }
}

/**
 * The header statements in the order the grammar names them
 * (parse.rs: "expected Mesh:, Scope:, Priority:, Autonomy:, ROUTES or
 * GUARDS"). The hints are the DSL's own words; `Digs` stays.
 */
export const HEADER_LINES: HeaderLine[] = [
  { label: 'Scope:', hint: 'All Digs | <Group> Digs | Dig: <name>', snippet: headerLine('Scope:\n', 'All Digs') },
  { label: 'Priority: 0', hint: 'lower runs first', snippet: headerLine('Priority: ', '0') },
  { label: 'Autonomy: Suggest', hint: 'Observe | Suggest | Act', snippet: headerLine('Autonomy: ', 'Suggest') },
]

export function factSnippet(f: FactDef): Snippet {
  switch (f.kind) {
    case 'bool':
      return { text: f.name, cursor: [f.name.length, f.name.length] }
    case 'number': {
      const t = `${f.name} > `
      return { text: t, cursor: [t.length, t.length] }
    }
    case 'text': {
      const t = `${f.name} = ""`
      return { text: t, cursor: [t.length - 1, t.length - 1] }
    }
  }
}

export function headerSnippet(name: string): Snippet {
  const t = `header ${name} = ""`
  return { text: t, cursor: [t.length - 1, t.length - 1] }
}

export interface SignalRef {
  label: string
  snippet: Snippet
}

/** `name@vN > .80` with the threshold selected, so typing replaces it. */
export function signalSnippet(name: string, version: number, choice: string | null): Snippet {
  const t = `${name}@v${version}${choice ? `:${choice}` : ''} > .80`
  return { text: t, cursor: [t.length - 3, t.length] }
}

/**
 * One reference per usable Signal, at its latest non-rejected version; a
 * Choice Signal gets one per choice. Rejected versions cannot be referenced
 * (validate.rs), so they are left out rather than inserted and then refused.
 */
export function signalRefs(signals: Signal[]): SignalRef[] {
  const latest = new Map<string, Signal>()
  for (const s of signals) {
    if (s.status === 'rejected') continue
    const cur = latest.get(s.name)
    if (!cur || s.version > cur.version) latest.set(s.name, s)
  }
  const out: SignalRef[] = []
  for (const s of [...latest.values()].sort((a, b) => a.name.localeCompare(b.name))) {
    if (s.primitive === 'choice') {
      for (const c of s.choices) out.push({ label: `${s.name}@v${s.version}:${c.name}`, snippet: signalSnippet(s.name, s.version, c.name) })
    } else out.push({ label: `${s.name}@v${s.version}`, snippet: signalSnippet(s.name, s.version, null) })
  }
  return out
}

export interface Insertion {
  text: string
  selectionStart: number
  selectionEnd: number
}

/**
 * Replace [start, end) of `text` with the snippet. A space is added before
 * it when the caret follows a word character, so `IF|` becomes `IF name`. A
 * line snippet goes on a line of its own: a caret in the middle of a line
 * puts it after that line rather than splitting the line in two.
 */
export function insertAt(text: string, start: number, end: number, s: Snippet): Insertion {
  const lo = Math.max(0, Math.min(start, end, text.length))
  const hi = Math.max(lo, Math.min(Math.max(start, end), text.length))
  if (s.line) {
    let at = lo
    let cut = hi
    if (lo === hi && lo > 0 && text[lo - 1] !== '\n') {
      const nl = text.indexOf('\n', lo)
      at = cut = nl === -1 ? text.length : nl
    }
    const head = text.slice(0, at)
    const tail = text.slice(cut)
    const pre = head.length && !head.endsWith('\n') ? '\n' : ''
    const post = tail.length && !tail.startsWith('\n') ? '\n' : ''
    const pos = at + pre.length
    return { text: head + pre + s.text + post + tail, selectionStart: pos + s.cursor[0], selectionEnd: pos + s.cursor[1] }
  }
  const before = text.slice(0, lo)
  const pad = before.length && !/\s$/.test(before) ? ' ' : ''
  const at = lo + pad.length
  return {
    text: before + pad + s.text + text.slice(hi),
    selectionStart: at + s.cursor[0],
    selectionEnd: at + s.cursor[1],
  }
}

/** The Facts line of the Syntax box: every name, then the header form. */
export function factsLine(): string {
  return `Facts: ${FACTS.map((f) => f.name).join(', ')}, header <Name>`
}

const KIND_HINT: Record<FactKind, string> = { text: 'text', bool: 'yes/no', number: 'number' }

/**
 * The ⌘I Insert menu, as `Menu` entries: four groups of DSL tokens in mono,
 * each hinted with the kind of value it takes rather than a keycap. The two
 * kinds of "header" are named apart (Mesh header, Mail headers) because a
 * tester read "Headers" as the place to find `Autonomy:`. This was a
 * hand-copied dropdown of its own (InsertMenu.tsx) until Menu could group,
 * scroll and show a plain-text hint.
 */
export function insertMenuEntries(signals: Signal[], onInsert: (s: Snippet) => void): MenuEntry[] {
  const token = (label: string, hint: string, snippet: Snippet) => ({ label, hint, hintKind: 'text' as const, mono: true, onSelect: () => onInsert(snippet) })
  return [
    { group: 'Mesh header', items: HEADER_LINES.map((h) => token(h.label, h.hint, h.snippet)) },
    'separator',
    { group: 'Facts', items: FACTS.map((f) => token(f.name, KIND_HINT[f.kind], factSnippet(f))) },
    'separator',
    { group: 'Mail headers', items: HEADERS.map((h) => token(`header ${h}`, 'text', headerSnippet(h))) },
    'separator',
    { group: 'Signals', empty: 'No Signals to reference yet.', items: signalRefs(signals).map((r) => token(r.label, '> .80', r.snippet)) },
  ]
}

import { describe, expect, it } from 'vitest'
import type { Signal } from '@/bindings/Signal'
import { FACTS, HEADERS, HEADER_LINES, factSnippet, factsLine, headerSnippet, insertAt, insertMenuEntries, signalRefs, signalSnippet } from './meshSyntax'

// The fact and header lists are generated from the Rust core
// (bindings/meshVocabulary.ts), so only their shape is checked here.
describe('the editor vocabulary', () => {
  it('every fact name is unique and snake_case, and there are kept headers', () => {
    const names = FACTS.map((f) => f.name)
    expect(names.length).toBeGreaterThan(0)
    expect(new Set(names).size).toBe(names.length)
    for (const n of names) expect(n).toMatch(/^[a-z][a-z0-9_]*$/)
    expect(HEADERS.length).toBeGreaterThan(0)
  })

  it('the Syntax box lists every fact and the header form, with no ellipsis', () => {
    const line = factsLine()
    for (const f of FACTS) expect(line).toContain(f.name)
    expect(line).toContain('header <Name>')
    expect(line).not.toContain('…')
  })
})

describe('snippets', () => {
  it('a boolean fact inserts bare, with the caret after it', () => {
    expect(factSnippet({ name: 'has_attachments', kind: 'bool' })).toEqual({ text: 'has_attachments', cursor: [15, 15] })
  })

  it('a text fact opens quotes and leaves the caret inside them', () => {
    const s = factSnippet({ name: 'subject', kind: 'text' })
    expect(s.text).toBe('subject = ""')
    expect(s.text.slice(s.cursor[0])).toBe('"')
  })

  it('a number fact leaves the caret after a comparison', () => {
    expect(factSnippet({ name: 'thread_length', kind: 'number' }).text).toBe('thread_length > ')
  })

  it('a header reads as header <Name>', () => {
    expect(headerSnippet('X-GitHub-Reason').text).toBe('header X-GitHub-Reason = ""')
  })

  it('a Signal reference selects its threshold so typing replaces it', () => {
    const s = signalSnippet('routine', 1, null)
    expect(s.text).toBe('routine@v1 > .80')
    expect(s.text.slice(s.cursor[0], s.cursor[1])).toBe('.80')
    expect(signalSnippet('urgency', 2, 'HARD').text).toBe('urgency@v2:HARD > .80')
  })

  it('the header lines are the ones the grammar names, each a whole line with its value selected', () => {
    // parse.rs: "expected Mesh:, Scope:, Priority:, Autonomy:, ROUTES or GUARDS".
    expect(HEADER_LINES.map((h) => h.label)).toEqual(['Scope:', 'Priority: 0', 'Autonomy: Suggest'])
    for (const h of HEADER_LINES) expect(h.snippet.line).toBe(true)
    const autonomy = HEADER_LINES.find((h) => h.label.startsWith('Autonomy'))!.snippet
    expect(autonomy.text).toBe('Autonomy: Suggest')
    expect(autonomy.text.slice(autonomy.cursor[0], autonomy.cursor[1])).toBe('Suggest')
    const scope = HEADER_LINES.find((h) => h.label === 'Scope:')!.snippet
    expect(scope.text).toBe('Scope:\nAll Digs')
    expect(scope.text.slice(scope.cursor[0], scope.cursor[1])).toBe('All Digs')
  })
})

describe('signal references', () => {
  const sig = (name: string, version: number, status: Signal['status'], primitive: Signal['primitive'] = 'noul', choices: string[] = ['YES', 'NO']): Signal => ({
    id: `sig_${name}`,
    name,
    version,
    primitive,
    definition: '',
    choices: choices.map((c) => ({ name: c, definition: '' })),
    inputs: { message: true, thread: false, contextSlices: [] },
    languageScope: [],
    status,
    calibration: null,
    createdAt: '2026-01-01T00:00:00Z',
    createdBy: 'system',
    changeNote: null,
  })

  it('offers the latest version of each Signal and never a rejected one', () => {
    const refs = signalRefs([sig('routine', 1, 'deprecated'), sig('routine', 2, 'active'), sig('bad', 1, 'rejected')])
    expect(refs.map((r) => r.label)).toEqual(['routine@v2'])
  })

  it('a Choice Signal gets one reference per choice', () => {
    const refs = signalRefs([sig('urgency', 2, 'active', 'choice', ['NONE', 'SOFT', 'HARD'])])
    expect(refs.map((r) => r.label)).toEqual(['urgency@v2:NONE', 'urgency@v2:SOFT', 'urgency@v2:HARD'])
  })
})

describe('the Insert menu entries', () => {
  it('has the four groups, hints each token with its value kind, and says so when there are no Signals', () => {
    const entries = insertMenuEntries([], () => {})
    const groups = entries.filter((e) => typeof e === 'object' && 'group' in e)
    expect(groups.map((g) => (g as { group: string }).group)).toEqual(['Mesh header', 'Facts', 'Mail headers', 'Signals'])
    const facts = groups[1] as { items: { label: string; hint?: string; mono?: boolean }[] }
    expect(facts.items.find((i) => i.label === 'has_attachments')).toMatchObject({ hint: 'yes/no', mono: true })
    expect(facts.items.find((i) => i.label === 'thread_length')?.hint).toBe('number')
    expect(groups[3]).toMatchObject({ items: [], empty: 'No Signals to reference yet.' })
  })

  it('inserts the chosen snippet', () => {
    const got: string[] = []
    const entries = insertMenuEntries([], (s) => got.push(s.text))
    const facts = entries[2] as { items: { label: string; onSelect: () => void }[] }
    facts.items.find((i) => i.label === 'subject')!.onSelect()
    expect(got).toEqual(['subject = ""'])
  })
})

describe('inserting at the caret', () => {
  it('adds a space after a word so IF| becomes IF name', () => {
    const r = insertAt('IF', 2, 2, { text: 'has_attachments', cursor: [15, 15] })
    expect(r.text).toBe('IF has_attachments')
    expect(r.selectionStart).toBe(18)
  })

  it('adds nothing after whitespace or at the start', () => {
    expect(insertAt('IF ', 3, 3, { text: 'x', cursor: [1, 1] }).text).toBe('IF x')
    expect(insertAt('', 0, 0, { text: 'x', cursor: [1, 1] }).text).toBe('x')
  })

  it('replaces a selection and maps the snippet cursor into the new text', () => {
    const r = insertAt('IF old > .5', 3, 11, { text: 'routine@v1 > .80', cursor: [13, 16] })
    expect(r.text).toBe('IF routine@v1 > .80')
    expect(r.text.slice(r.selectionStart, r.selectionEnd)).toBe('.80')
  })

  const autonomy = { text: 'Autonomy: Suggest', cursor: [10, 17] as [number, number], line: true }

  it('a header line at the end of a line goes on the next line, with its value selected', () => {
    const r = insertAt('Mesh: X v1\n\nScope:', 10, 10, autonomy)
    expect(r.text).toBe('Mesh: X v1\nAutonomy: Suggest\n\nScope:')
    expect(r.text.slice(r.selectionStart, r.selectionEnd)).toBe('Suggest')
  })

  it('a header line with the caret mid-line goes after that line rather than splitting it', () => {
    expect(insertAt('Mesh: X v1\nScope:', 4, 4, autonomy).text).toBe('Mesh: X v1\nAutonomy: Suggest\nScope:')
    expect(insertAt('Mesh: X v1', 4, 4, autonomy).text).toBe('Mesh: X v1\nAutonomy: Suggest')
  })

  it('a header line at a line start or in empty text adds no blank line before it', () => {
    expect(insertAt('Mesh: X v1\n', 11, 11, autonomy).text).toBe('Mesh: X v1\nAutonomy: Suggest')
    expect(insertAt('', 0, 0, autonomy).text).toBe('Autonomy: Suggest')
    expect(insertAt('\nScope:', 0, 0, autonomy).text).toBe('Autonomy: Suggest\nScope:')
  })

  it('a header line replaces a selection and keeps what follows on its own line', () => {
    expect(insertAt('Mesh: X v1\nAutonomy: Act\nScope:', 11, 24, autonomy).text).toBe('Mesh: X v1\nAutonomy: Suggest\nScope:')
  })
})

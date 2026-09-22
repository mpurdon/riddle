import { describe, expect, it } from 'vitest'
import { SCOPE_LABELS, SHORTCUTS, keysLabel, parseKeys, shortcutsFor, type ShortcutScope } from './shortcuts'

describe('the shortcut table drives both the bindings and the help sheet', () => {
  it('gives every shortcut an id, keys, a label and a documented scope', () => {
    for (const s of SHORTCUTS) {
      expect(s.id).toMatch(/^[a-z_]+$/)
      expect(s.keys.length).toBeGreaterThan(0)
      expect(s.label.length).toBeGreaterThan(3)
      expect(SCOPE_LABELS[s.scope]).toBeDefined()
    }
  })
  it('never binds the same keys twice within a scope, and ids are unique', () => {
    const ids = new Set<string>()
    const seen = new Map<string, string>()
    for (const s of SHORTCUTS) {
      expect(ids.has(s.id)).toBe(false)
      ids.add(s.id)
      const k = `${s.scope}:${s.keys}`
      expect(seen.get(k), `duplicate ${k} (${seen.get(k)} and ${s.id})`).toBeUndefined()
      seen.set(k, s.id)
    }
  })
  it('never lets a list-scope key collide with a global key', () => {
    const global = new Set(shortcutsFor('global').map((s) => s.keys))
    for (const scope of Object.keys(SCOPE_LABELS) as ShortcutScope[]) {
      if (scope === 'global') continue
      for (const s of shortcutsFor(scope)) expect(global.has(s.keys), `${scope}:${s.keys} shadows a global shortcut`).toBe(false)
    }
  })
  it('parses chords and modifiers', () => {
    expect(parseKeys('g f')).toEqual({ chord: ['g', 'f'], mod: false, shift: false })
    expect(parseKeys('shift+x')).toEqual({ chord: ['x'], mod: false, shift: true })
    expect(parseKeys('mod+enter')).toEqual({ chord: ['enter'], mod: true, shift: false })
  })
  it('renders human key labels for the help sheet', () => {
    expect(keysLabel('g f')).toEqual(['g', 'f'])
    expect(keysLabel('escape')).toEqual(['Esc'])
    expect(keysLabel('mod+enter').join('')).toMatch(/(⌘|Ctrl)↵/)
  })
  it('documents the triage keys the README promises', () => {
    const finds = shortcutsFor('finds').map((s) => s.keys)
    expect(finds).toEqual(expect.arrayContaining(['e', 's', 'd', 'r']))
    const list = shortcutsFor('list').map((s) => s.keys)
    expect(list).toEqual(expect.arrayContaining(['j', 'k', 'enter', 'x']))
    expect(shortcutsFor('global').find((s) => s.keys === '?')?.label).toMatch(/help/i)
  })
})

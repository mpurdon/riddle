import { afterEach, describe, expect, it } from 'vitest'
import { isAdvancedFor, levelShows, levelTipDue, LEVEL_TIP_KEY, nextLevel, resetLevelTip, routeLevel, shortcutShown, siblingsFor, TIP_AFTER } from './level'

afterEach(() => localStorage.removeItem(LEVEL_TIP_KEY))

describe('the user level', () => {
  it('Developer implies Everything, which implies Essentials', () => {
    expect(levelShows('developer', 'everything')).toBe(true)
    expect(levelShows('developer', 'essentials')).toBe(true)
    expect(levelShows('everything', 'essentials')).toBe(true)
    expect(levelShows('everything', 'developer')).toBe(false)
    expect(levelShows('essentials', 'everything')).toBe(false)
  })

  it('gates rule authoring, the taxonomy and long jobs at Everything, and Signals and Developer at Developer', () => {
    for (const r of ['/meshes', '/meshes/mesh_1', '/packs', '/check', '/strata', '/labels', '/expeditions/x', '/activity']) expect(routeLevel(r), r).toBe('everything')
    for (const r of ['/signals', '/signals/marketing', '/developer', '/developer/traces']) expect(routeLevel(r), r).toBe('developer')
    for (const r of ['/', '/finds', '/unresolved', '/inbox', '/inbox/dig/m1', '/priority', '/spoil', '/contamination', '/director', '/settings']) expect(routeLevel(r), r).toBe('essentials')
  })

  it('an advanced page is one the level does not list; Activity is never advanced, being reachable from Settings', () => {
    expect(isAdvancedFor('/meshes', 'essentials')).toBe(true)
    expect(isAdvancedFor('/signals', 'everything')).toBe(true)
    expect(isAdvancedFor('/meshes', 'everything')).toBe(false)
    expect(isAdvancedFor('/signals', 'developer')).toBe(false)
    expect(isAdvancedFor('/activity', 'essentials')).toBe(false)
    expect(isAdvancedFor('/finds', 'essentials')).toBe(false)
  })

  it('go-to shortcuts follow their route and every other shortcut stays', () => {
    expect(shortcutShown('go_meshes', 'essentials')).toBe(false)
    expect(shortcutShown('go_meshes', 'everything')).toBe(true)
    expect(shortcutShown('go_activity', 'essentials')).toBe(false)
    expect(shortcutShown('go_unresolved', 'essentials')).toBe(true)
    expect(shortcutShown('go_spoil', 'essentials')).toBe(true)
    expect(shortcutShown('undo', 'essentials')).toBe(true)
    expect(shortcutShown('toggle_level', 'essentials')).toBe(true)
  })

  it('folds sibling pages under one entry', () => {
    expect(siblingsFor('/finds')?.label).toBe('Needs you')
    expect(siblingsFor('/unresolved')?.label).toBe('Needs you')
    expect(siblingsFor('/inbox/dig_work/m_1')?.label).toBe('Mail')
    expect(siblingsFor('/priority')?.label).toBe('Mail')
    expect(siblingsFor('/contamination')?.label).toBe('Set aside')
    expect(siblingsFor('/meshes')).toBeNull()
    expect(siblingsFor('/')).toBeNull()
  })

  it('the footer switch goes up from Essentials and back to it from anywhere else', () => {
    expect(nextLevel('essentials')).toBe('everything')
    expect(nextLevel('everything')).toBe('essentials')
    expect(nextLevel('developer')).toBe('essentials')
  })
})

describe('the one-time tip', () => {
  it('is due at the fifth correction, once, and never again for a Why? either', () => {
    for (let i = 1; i < TIP_AFTER; i++) expect(levelTipDue('correction'), `correction ${i}`).toBe(false)
    expect(levelTipDue('correction')).toBe(true)
    expect(levelTipDue('correction')).toBe(false)
    expect(levelTipDue('why')).toBe(false)
  })

  it('counts the two kinds apart and is due at the fifth Why? too', () => {
    for (let i = 0; i < 3; i++) expect(levelTipDue('correction')).toBe(false)
    for (let i = 1; i < TIP_AFTER; i++) expect(levelTipDue('why'), `why ${i}`).toBe(false)
    expect(levelTipDue('why')).toBe(true)
  })

  it('can be shown again from Settings', () => {
    for (let i = 0; i < TIP_AFTER; i++) levelTipDue('why')
    expect(levelTipDue('why')).toBe(false)
    resetLevelTip()
    for (let i = 1; i < TIP_AFTER; i++) levelTipDue('why')
    expect(levelTipDue('why')).toBe(true)
  })
})

import { describe, expect, it } from 'vitest'
import type { Dig } from '@/bindings/Dig'
import type { Mesh } from '@/bindings/Mesh'
import { belowAct, cappedRules, gmailEffect, groupAccounts } from './model'

describe('belowAct', () => {
  it('is true at Observe or Suggest, and false at Act, Trusted or with no level at all', () => {
    expect(belowAct('observe')).toBe(true)
    expect(belowAct('suggest')).toBe(true)
    expect(belowAct('act')).toBe(false)
    expect(belowAct('trusted')).toBe(false)
    expect(belowAct(undefined)).toBe(false)
    expect(belowAct(null)).toBe(false)
  })
})

describe('cappedRules', () => {
  const mesh = (name: string, autonomy: Mesh['autonomy'], scope: Mesh['scope'], extra: Partial<Mesh> = {}): Mesh =>
    ({ id: name, name, version: 1, scope, priority: 50, routes: [], guards: [], status: 'active', sourceText: '', createdAt: '', createdBy: 'user', changeNote: null, shadowStartedAt: null, shadowAgreed: 0, shadowProposed: 0, deletedAt: null, autonomy, ...extra }) as Mesh
  const personal = { id: 'dig_personal', group: 'Home' }

  it('keeps only rules capped below Act whose scope covers the account, and never a deleted one', () => {
    const rules = [
      mesh('Promotions', 'observe', { kind: 'all_digs' }),
      mesh('Newsletters', 'suggest', { kind: 'dig_group', group: 'Home' }),
      mesh('Receipts', 'act', { kind: 'all_digs' }),
      mesh('Uncapped', undefined, { kind: 'all_digs' }),
      mesh('Work only', 'observe', { kind: 'dig', dig_id: 'dig_work' }),
      mesh('Other group', 'observe', { kind: 'dig_group', group: 'Work' }),
      mesh('Gone', 'observe', { kind: 'all_digs' }, { deletedAt: '2026-09-01T00:00:00Z', status: 'deleted' }),
    ]
    expect(cappedRules(rules, personal).map((m) => m.name)).toEqual(['Promotions', 'Newsletters'])
    expect(cappedRules(rules, { id: 'dig_work', group: 'Work' }).map((m) => m.name)).toEqual(['Promotions', 'Work only', 'Other group'])
  })
})

describe('gmailEffect', () => {
  it('says Gmail is never changed when projection is off, whatever the autonomy', () => {
    expect(gmailEffect('trusted', 'none').writes).toBe('nothing')
    expect(gmailEffect('trusted', 'none').held).toBeNull()
  })

  it('holds a full projection below Act and says what Act would do', () => {
    const e = gmailEffect('observe', 'full')
    expect(e.writes).toBe('nothing')
    expect(e.held).toBe('autonomy')
    expect(e.heldBecause).toBe('autonomy is observe')
    expect(e.sentence).toContain('archive')
  })

  it('holds changes in an unarmed dev build and releases them once armed', () => {
    const held = gmailEffect('act', 'labels_only', { dev: true, armed: false })
    expect(held.held).toBe('dev_unarmed')
    expect(held.heldBecause).toBe('this dev build holds mailbox writes')
    expect(gmailEffect('act', 'labels_only', { dev: true, armed: true }).writes).toBe('labels')
  })

  it('writes labels and archiving at Act with full projection', () => {
    const e = gmailEffect('act', 'full')
    expect(e.writes).toBe('labels_and_archive')
    expect(e.held).toBeNull()
    expect(e.heldBecause).toBeNull()
  })

  it('takes the backend’s answer over the settings when the Dig carries one', () => {
    // The settings alone would say "labels and archiving"; the backend says the build holds it.
    const held = gmailEffect('act', 'full', { mailbox: { writes: 'nothing', held: 'dev_unarmed' }, dev: false, armed: true })
    expect(held.writes).toBe('nothing')
    expect(held.held).toBe('dev_unarmed')
    expect(held.sentence).toContain('once you send the waiting changes or allow mailbox writes')
    // And the other way: the settings would hold it, the backend says it writes.
    const writes = gmailEffect('observe', 'full', { mailbox: { writes: 'labels_and_archive', held: null } })
    expect(writes.writes).toBe('labels_and_archive')
    expect(writes.held).toBeNull()
    expect(writes.sentence).toBe('Riddle labels and archives mail in Gmail as it classifies.')
    // Held for autonomy: the backend says why, the level says which.
    const autonomy = gmailEffect('suggest', 'labels_only', { mailbox: { writes: 'nothing', held: 'autonomy' } })
    expect(autonomy.heldBecause).toBe('autonomy is suggest')
    expect(autonomy.sentence).toBe('Riddle classifies only. At Act it would add labels in Gmail.')
  })

  it('falls back to the settings when the Dig has no answer', () => {
    expect(gmailEffect('act', 'full', { mailbox: null }).writes).toBe('labels_and_archive')
    expect(gmailEffect('act', 'full', { mailbox: undefined, dev: true, armed: false }).held).toBe('dev_unarmed')
  })
})

describe('groupAccounts', () => {
  const dig = (name: string, group: string | null): Dig =>
    ({ id: name, name, email: `${name.toLowerCase()}@example.com`, provider: 'gmail', group, privacy: 'cloud_judgment', projection: 'none', status: { state: 'synced' }, shadowStartedAt: null, disabled: false, createdAt: '' }) as unknown as Dig

  it('puts named groups first, sorted, then ungrouped accounts', () => {
    const out = groupAccounts([dig('Zed', null), dig('Bee', 'Work'), dig('Ann', 'Home'), dig('Cat', 'Work')], '')
    expect(out.map((g) => g.group)).toEqual(['Home', 'Work', null])
    expect(out[1].digs.map((d) => d.name)).toEqual(['Bee', 'Cat'])
  })

  it('filters by name, address or group', () => {
    const all = [dig('Personal', null), dig('Acme', 'Work')]
    expect(groupAccounts(all, 'acme@').flatMap((g) => g.digs).map((d) => d.name)).toEqual(['Acme'])
    expect(groupAccounts(all, 'work').flatMap((g) => g.digs).map((d) => d.name)).toEqual(['Acme'])
  })
})

import { describe, expect, it } from 'vitest'
import type { Activity } from '@/bindings/Activity'
import type { Summary } from '@/bindings/Summary'
import type { TaskCentre } from '@/bindings/TaskCentre'
import { u64 } from './format'
import { digStatusChip, errorAffordance, skippedLines, skippedSummary, summaryCopy, taskCentreIndicator } from './selectors'

const summary = (over: Partial<Summary> = {}): Summary => ({
  asOf: '2026-09-22T07:42:00.000Z',
  caughtUp: true,
  since: '2026-09-21T07:42:00.000Z',
  arrived: u64(312),
  digCount: 3,
  findsOpen: u64(7),
  unresolved: u64(2),
  filed: u64(41),
  spoil: u64(236),
  contamination: u64(26),
  remainingToSift: u64(0),
  totalToSift: u64(0),
  directorNote: null,
  shadow: null,
  lastSyncedAt: '2026-09-22T07:40:00.000Z',
  projectionPending: u64(0),
  degraded: [],
  ...over,
})

describe('the morning summary is honest about whether Riddle has finished', () => {
  it('says caught up with counts when processing is complete', () => {
    const c = summaryCopy(summary(), new Date('2026-09-22T09:00:00'))
    expect(c.headline).toBe('Good morning.')
    expect(c.status).toMatch(/^Caught up as of /)
    expect(c.finds).toBe('7 Finds need your attention.')
    expect(c.unresolved).toBe('2 are Unresolved.')
    expect(c.attentionZero).toBe(false)
  })

  it('says catching up with the remaining count instead of implying completion', () => {
    const c = summaryCopy(summary({ caughtUp: false, findsOpen: u64(4), remainingToSift: u64(1240), totalToSift: u64(4981) }))
    expect(c.headline).toBe('Catching up.')
    expect(c.status).toBe('4 Finds so far. 1,240 of 4,981 messages still to sift.')
    expect(c.attentionZero).toBe(false)
  })

  it('reaches attention zero only when there are no Finds and nothing Unresolved, and shows its evidence', () => {
    const now = new Date('2026-09-22T07:42:00.000Z')
    const c = summaryCopy(summary({ findsOpen: u64(0), unresolved: u64(0) }), now)
    expect(c.attentionZero).toBe(true)
    expect(c.headline).toBe('Nothing needs your attention.')
    expect(c.evidence).toBe('Synced 2 minutes ago. 41 messages Filed, 236 in Spoil since yesterday.')
    const notZero = summaryCopy(summary({ findsOpen: u64(0), unresolved: u64(1) }), now)
    expect(notZero.attentionZero).toBe(false)
  })
})

describe('per-Dig status chips', () => {
  it('render every DigStatus variant', () => {
    expect(digStatusChip({ state: 'synced' }).label).toBe('Synced')
    expect(digStatusChip({ state: 'catching_up', remaining: u64(1240), total: u64(4981) }).label).toBe('Catching up 1,240')
    expect(digStatusChip({ state: 'sifting', remaining: u64(12) }).label).toBe('Sifting 12')
    expect(digStatusChip({ state: 'projection', pending: u64(41) }).label).toBe('Projection 41')
    const d = digStatusChip({ state: 'degraded', reason: 'judge unavailable' })
    expect(d.label).toBe('Degraded')
    expect(d.title).toMatch(/Nothing is hidden/)
    expect(digStatusChip({ state: 'shadow', day: 3, of: 7 }).label).toBe('Shadow day 3')
  })
})

describe('the Task Centre indicator', () => {
  const tc = (over: Partial<TaskCentre>): TaskCentre => ({ overall: 'caught_up', digs: [], jobs: [], storeFault: null, asOf: '2026-09-22T07:42:00.000Z', ...over })
  it('is Caught up when nothing is catching up and nothing is degraded', () => {
    expect(taskCentreIndicator(tc({})).label).toBe('Caught up')
  })
  it('is Catching up with the remaining total across Digs', () => {
    const i = taskCentreIndicator(tc({ overall: 'catching_up', digs: [{ digId: 'a', name: 'Work', status: { state: 'catching_up', remaining: u64(1000), total: u64(2000) } }, { digId: 'b', name: 'Home', status: { state: 'sifting', remaining: u64(240) } }] }))
    expect(i.label).toBe('Catching up')
    expect(i.detail).toBe('1,240 messages to go')
  })
  it('is Degraded when a Dig is degraded or the store has faulted, and says nothing is hidden', () => {
    const i = taskCentreIndicator(tc({ overall: 'degraded', digs: [{ digId: 'a', name: 'Work', status: { state: 'degraded', reason: 'judge unavailable' } }] }))
    expect(i.label).toBe('Degraded')
    expect(i.detail).toBe('judge unavailable. Nothing hidden.')
    expect(taskCentreIndicator(tc({ storeFault: 'disk full' })).label).toBe('Degraded')
  })
  it('counts running, queued and paused jobs', () => {
    const job = (state: 'running' | 'queued' | 'paused') => ({ id: state, kind: 'catch_up' as const, priority: 'catch_up' as const, status: state === 'paused' ? { state, reason: 'sleep' } : { state }, digId: null, label: state, progress: { done: u64(0), total: null, unit: 'x', step: 's', etaSeconds: null }, checkpoint: null, cancellable: true, attempts: 0, createdAt: '', startedAt: null, finishedAt: null, updatedAt: '' })
    const i = taskCentreIndicator(tc({ jobs: [job('running'), job('queued'), job('paused'), job('paused')] }))
    expect([i.running, i.queued, i.paused]).toEqual([1, 1, 2])
  })
})

describe('Undo All renders what it skipped', () => {
  const act = (result: Activity['result']): Activity => ({ id: 'a', actor: 'director', action: 'move_to_spoil', summary: 'Director moved 23 messages to Spoil', why: null, inputs: null, affected: [], result, provenance: null, undo: { state: 'not_undoable', reason: 'x' }, digId: null, traceId: null, undoes: null, createdAt: '' })
  it('lists each skipped entity with its reason', () => {
    const a = act({ status: 'partial', skipped: [{ entity: { kind: 'message', id: 'm1' }, reason: 'you archived it by hand since' }, { entity: { kind: 'message', id: 'm2' }, reason: 'you starred it since' }] })
    expect(skippedSummary(a)).toBe('2 skipped: they were handled by hand since.')
    expect(skippedLines(a)).toEqual(['Message m1: you archived it by hand since', 'Message m2: you starred it since'])
  })
  it('renders nothing for a complete result', () => {
    expect(skippedSummary(act({ status: 'ok' }))).toBeNull()
    expect(skippedLines(act({ status: 'ok' }))).toEqual([])
  })
})

describe('typed errors map to affordances', () => {
  it('maps each kind in the taxonomy to the right control', () => {
    expect(errorAffordance({ kind: 'provider_auth', message: '', detail: null }).action).toBe('sign_in')
    expect(errorAffordance({ kind: 'provider_admin_blocked', message: '', detail: null }).body).toMatch(/administrator/)
    expect(errorAffordance({ kind: 'judge_unavailable', message: 'timeout', detail: null })).toMatchObject({ degraded: true })
    expect(errorAffordance({ kind: 'llm_unavailable', message: 'timeout', detail: null }).body).toMatch(/nothing is hidden/)
    expect(errorAffordance({ kind: 'budget_exhausted', message: '', detail: null }).action).toBe('task_centre')
    expect(errorAffordance({ kind: 'policy_denied', message: 'not at this level', detail: null }).action).toBe('none')
    expect(errorAffordance({ kind: 'policy_denied', message: 'dev build is not armed', detail: null }).action).toBe('arm_dev')
    expect(errorAffordance({ kind: 'conflict', message: '', detail: null }).action).toBe('reconcile')
  })
})

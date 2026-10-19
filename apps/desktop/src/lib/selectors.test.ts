import { describe, expect, it } from 'vitest'
import type { Activity } from '@/bindings/Activity'
import type { Summary } from '@/bindings/Summary'
import type { TaskCentre } from '@/bindings/TaskCentre'
import type { Job } from '@/bindings/Job'
import { u64 } from './format'
import { NO_JUDGE_KEY, catchUpJob, catchUpLine, digStatusChip, errorAffordance, googleClientLine, jobPriorityLabel, judgeNotSetUp, judgeStateLine, skippedLines, skippedSummary, summaryCopy, taskCentreIndicator, digPipeline } from './selectors'

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
  packOffers: [],
  ...over,
})

describe('the judge key and the Google client, as setup states', () => {
  it('says what Riddle does without a judge key, in the same words as the account status', () => {
    expect(judgeStateLine({ provider: 'jev', keySource: 'keychain', apiKeyPresent: false })).toBe('Sorting by rules until you add a judge key; nothing hidden')
    expect(judgeStateLine({ provider: 'jev', keySource: 'keychain', apiKeyPresent: false })).toBe(NO_JUDGE_KEY)
    expect(judgeStateLine({ provider: 'null', keySource: 'keychain', apiKeyPresent: false })).toBe(NO_JUDGE_KEY)
    expect(judgeStateLine({ provider: 'jev', keySource: 'keychain', apiKeyPresent: true })).toBeNull()
    // An SSM or environment lookup is a lookup, not a missing setup.
    expect(judgeStateLine({ provider: 'jev', keySource: 'ssm', apiKeyPresent: false })).toBeNull()
    expect(judgeNotSetUp({ provider: 'scripted', keySource: 'keychain', apiKeyPresent: false })).toBe(false)
  })

  it('names which Google client sign-in uses: the own one first, then the built-in one, else none', () => {
    expect(googleClientLine({ clientId: '', builtinClientAvailable: false, useBuiltinClient: true })).toEqual({ source: 'none', label: 'Not set up yet', tone: 'warn' })
    expect(googleClientLine({ clientId: '', builtinClientAvailable: true, useBuiltinClient: true })).toEqual({ source: 'builtin', label: 'Using the built-in Google client', tone: 'ok' })
    expect(googleClientLine({ clientId: '', builtinClientAvailable: true, useBuiltinClient: false }).source).toBe('none')
    expect(googleClientLine({ clientId: 'own.apps.googleusercontent.com', builtinClientAvailable: true, useBuiltinClient: true })).toEqual({ source: 'own', label: 'Client set up', tone: 'ok' })
  })
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

  it('invites you to connect an account instead of claiming attention zero when there is none', () => {
    const c = summaryCopy(summary({ digCount: 0, findsOpen: u64(0), unresolved: u64(0), arrived: u64(0) }))
    expect(c.firstRun).toBe(true)
    expect(c.attentionZero).toBe(false)
    expect(c.headline).toBe('Connect an account to start.')
    expect(c.reviewRoute).toBeNull()
  })

  it('counts accounts, not Digs, and opens the brief with when it starts', () => {
    const c = summaryCopy(summary(), new Date('2026-09-22T09:00:00.000Z'))
    expect(c.arrived).toBe('312 messages arrived across your 3 accounts.')
    expect(c.since).toBe('Since yesterday')
  })

  it('sends Start review to Finds first, then Unresolved, then nowhere', () => {
    expect(summaryCopy(summary()).reviewRoute).toBe('/finds')
    expect(summaryCopy(summary({ findsOpen: u64(0), unresolved: u64(2) })).reviewRoute).toBe('/unresolved')
    expect(summaryCopy(summary({ findsOpen: u64(0), unresolved: u64(0) })).reviewRoute).toBeNull()
  })
})

describe('per-account status chips', () => {
  it('render every DigStatus variant', () => {
    expect(digStatusChip({ state: 'synced' }).label).toBe('Synced')
    expect(digStatusChip({ state: 'catching_up', remaining: u64(1240), total: u64(4981) }).label).toBe('Catching up 1,240')
    expect(digStatusChip({ state: 'sifting', remaining: u64(12) }).label).toBe('Sifting 12')
    expect(digStatusChip({ state: 'projection', pending: u64(41) }).label).toBe('41 waiting for Gmail')
    const d = digStatusChip({ state: 'degraded', reason: 'judge unavailable' })
    expect(d.label).toBe('Degraded')
    expect(d.title).toMatch(/Nothing is hidden/)
    expect(digStatusChip({ state: 'shadow', day: 3, of: 7 }).label).toBe('Shadow day 3')
  })

  it('tells a signed-out account where to sign in again rather than to remove itself', () => {
    const c = digStatusChip({ state: 'not_connected' })
    expect(c.label).toBe('Not signed in')
    expect(c.title).toMatch(/Sign in again/)
    expect(c.title).not.toMatch(/remove/)
  })
})

describe('the Task Centre indicator', () => {
  const tc = (over: Partial<TaskCentre>): TaskCentre => ({ overall: 'caught_up', digs: [{ digId: 'a', name: 'Work', status: { state: 'synced' } }], jobs: [], storeFault: null, judge: null, llm: null, asOf: '2026-09-22T07:42:00.000Z', ...over })
  it('is Caught up when nothing is catching up and nothing is degraded', () => {
    expect(taskCentreIndicator(tc({})).label).toBe('Caught up')
  })
  it('is Starting, with what the core is doing, while it boots, even with no account listed yet', () => {
    const i = taskCentreIndicator(tc({ digs: [], booting: 'Resolving the judge key' }))
    expect(i.label).toBe('Starting')
    expect(i.tone).toBe('busy')
    expect(i.detail).toBe('Resolving the judge key')
  })
  it('says Nothing connected, in a neutral tone, when there is no account to be caught up with', () => {
    const i = taskCentreIndicator(tc({ digs: [] }))
    expect(i.label).toBe('Nothing connected')
    expect(i.tone).toBe('neutral')
  })
  it('puts a job priority into words', () => {
    expect(jobPriorityLabel('catch_up')).toBe('Catching up')
    expect(jobPriorityLabel('interactive')).toBe('For you now')
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

describe('the Summary while catching up', () => {
  const tc = (over: Partial<TaskCentre>): TaskCentre => ({ overall: 'catching_up', digs: [{ digId: 'a', name: 'Work', status: { state: 'catching_up', remaining: u64(1240), total: u64(4981) } }], jobs: [], storeFault: null, judge: null, llm: null, asOf: '2026-09-22T07:42:00.000Z', ...over })
  const job = (over: Partial<Job>): Job => ({ id: 'j', kind: 'catch_up', priority: 'catch_up', status: { state: 'running' }, digId: 'a', label: 'Catch-up: Work', progress: { done: u64(3741), total: u64(4981), unit: 'messages', step: 'Fetching bodies', etaSeconds: u64(248) }, checkpoint: null, cancellable: true, attempts: 1, createdAt: '', startedAt: null, finishedAt: null, updatedAt: '', ...over })

  it('reads fetching, sorted and Finds from numbers the page already has', () => {
    const s = summary({ caughtUp: false, findsOpen: u64(4), remainingToSift: u64(1240), totalToSift: u64(2220) })
    expect(catchUpLine(s, tc({}), null)).toBe('Fetching 1,240 · sorted 980 · 4 need you so far')
  })

  it('leaves fetching out when no account is fetching, and counts one Find in the singular', () => {
    const s = summary({ caughtUp: false, findsOpen: u64(1), remainingToSift: u64(200), totalToSift: u64(1180) })
    expect(catchUpLine(s, tc({ digs: [{ digId: 'a', name: 'Work', status: { state: 'sifting', remaining: u64(200) } }] }), null)).toBe('Sorted 980 · 1 needs you so far')
  })

  it('scopes fetching to the selected account', () => {
    const s = summary({ caughtUp: false, findsOpen: u64(0), remainingToSift: u64(0), totalToSift: u64(0) })
    const both = tc({ digs: [{ digId: 'a', name: 'Work', status: { state: 'catching_up', remaining: u64(100), total: u64(500) } }, { digId: 'b', name: 'Home', status: { state: 'catching_up', remaining: u64(50), total: u64(80) } }] })
    expect(catchUpLine(s, both, 'b')).toBe('Fetching 50 · sorted 0 · 0 need you so far')
    expect(catchUpLine(s, both, null)).toBe('Fetching 150 · sorted 0 · 0 need you so far')
  })

  it('picks the running first sync over a catch-up, and only jobs for the selected account', () => {
    const first = job({ id: 'first', kind: 'initial_sync', digId: 'b', label: 'Initial sync: Home' })
    const t = tc({ jobs: [job({}), first, job({ id: 'done', status: { state: 'completed' } })] })
    expect(catchUpJob(t, null)?.id).toBe('first')
    expect(catchUpJob(t, 'a')?.id).toBe('j')
    expect(catchUpJob(t, 'c')).toBeNull()
    expect(catchUpJob(undefined, null)).toBeNull()
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

describe('digPipeline', () => {
  const job = (step: string, done = 3, total: number | null = 12): Job => ({ id: 'j', kind: 'incremental_sync', priority: 'interactive', status: { state: 'running' }, digId: 'd', label: 'Refresh', progress: { done: u64(done), total: total === null ? null : u64(total), unit: 'messages', step, etaSeconds: null }, checkpoint: null, cancellable: true, attempts: 1, createdAt: '', startedAt: '', finishedAt: null, updatedAt: '' })
  const states = (p: ReturnType<typeof digPipeline>) => p.stages.map((s) => s.state).join(' ')

  it('lights the stage the account is in and says what the running job is doing', () => {
    const fetching = digPipeline({ state: 'catching_up', remaining: u64(120), total: u64(400) }, job('Fetching headers', 280, 400))
    expect(states(fetching)).toBe('active todo todo')
    expect(fetching.detail).toBe('Fetching headers · 280 of 400 messages')
    expect(fetching.busy).toBe(true)
    const sifting = digPipeline({ state: 'sifting', remaining: u64(12) }, null)
    expect(states(sifting)).toBe('done active todo')
    expect(sifting.detail).toBe('12 messages waiting for the Sift')
    expect(states(digPipeline({ state: 'projection', pending: u64(3) }, null))).toBe('done done active')
  })

  it('is all done when up to date, and off when not connected or degraded', () => {
    const done = digPipeline({ state: 'synced' }, null)
    expect(states(done)).toBe('done done done')
    expect(done.detail).toBe('Up to date')
    expect(done.busy).toBe(false)
    expect(states(digPipeline({ state: 'synced' }, job('Checking for new mail')))).toBe('active todo todo')
    expect(states(digPipeline({ state: 'not_connected' }, null))).toBe('off off off')
    expect(digPipeline({ state: 'degraded', reason: 'token expired' }, null).detail).toBe('Degraded: token expired')
  })
})

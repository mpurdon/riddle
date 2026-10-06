import { describe, expect, it, vi } from 'vitest'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { byDig, runPerDig } from './runPerDig'

const row = (id: string, digId: string): MessageSummary => ({
  id,
  digId,
  threadId: `t_${id}`,
  subject: `Subject ${id}`,
  from: { email: 'x@example.com', name: null },
  date: '2026-09-25T09:00:00Z',
  snippet: '',
  labels: [],
  isSent: false,
  hasAttachments: false,
  disposition: null,
  strata: [],
  findId: null,
  shadow: false,
  shadowReview: null,
  importance: 'normal',
  importanceSetByYou: false,
})

/** An Action layer that answers every call with an activity id and remembers what it was asked. */
const actions = () => {
  const run = vi.fn(async (_action: string, args: Record<string, unknown> = {}) => ({ activityId: `act_${args.digId}`, affected: [], replayed: false, summary: '', result: null }) as never)
  const invalidateAfter = vi.fn(async () => {})
  return { run, invalidateAfter }
}

describe('a bulk verb under All accounts', () => {
  it('splits its targets by account, keeping the list order within each', () => {
    expect(byDig([row('a', 'dig-1'), row('b', 'dig-2'), row('c', 'dig-1')])).toEqual([
      ['dig-1', ['a', 'c']],
      ['dig-2', ['b']],
    ])
    expect(byDig([])).toEqual([])
  })

  it('runs the accounts side by side without a refetch each, refreshes once, and returns every activity id', async () => {
    const a = actions()
    const ids = await runPerDig(a, [row('a', 'dig-1'), row('b', 'dig-2'), row('c', 'dig-1')], 'archive_messages', {}, { invalidate: [['extra']] })
    expect(a.run).toHaveBeenCalledTimes(2)
    expect(a.run).toHaveBeenCalledWith('archive_messages', { digId: 'dig-1', messageIds: ['a', 'c'] }, expect.objectContaining({ digId: 'dig-1', skipInvalidate: true }))
    expect(a.run).toHaveBeenCalledWith('archive_messages', { digId: 'dig-2', messageIds: ['b'] }, expect.objectContaining({ digId: 'dig-2', skipInvalidate: true }))
    expect(a.invalidateAfter).toHaveBeenCalledTimes(1)
    expect(a.invalidateAfter).toHaveBeenCalledWith(['archive_messages'], [['extra']])
    expect(ids).toEqual(['act_dig-1', 'act_dig-2'])
  })

  it('leaves one account its own refetch, which also covers what the outcome names', async () => {
    const a = actions()
    await runPerDig(a, [row('a', 'dig-1'), row('c', 'dig-1')], 'move_to_spoil', { until: 'x' })
    expect(a.run).toHaveBeenCalledTimes(1)
    expect(a.run).toHaveBeenCalledWith('move_to_spoil', { digId: 'dig-1', messageIds: ['a', 'c'], until: 'x' }, expect.objectContaining({ digId: 'dig-1', skipInvalidate: undefined }))
    expect(a.invalidateAfter).not.toHaveBeenCalled()
  })
})

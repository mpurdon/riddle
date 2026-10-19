/**
 * The event bridge under a storm: a Sift pass emits one `message_classified`
 * per message, and the Inbox once refetched for each of them.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { QueryClient } from '@tanstack/react-query'
import type { RiddleEvent } from '@/bindings/RiddleEvent'
import { qk } from './api'
import { clearPendingInvalidations, COALESCE_MAX_MS, COALESCE_MS, handleEvent } from './events'

const classified = (n: number): RiddleEvent => ({ type: 'message_classified', message_id: `m_${n}`, dig_id: 'dig-1', disposition: { kind: 'unresolved' }, shadow: false })

/** A client that records which keys were invalidated, without fetching anything. */
function client() {
  const qc = new QueryClient()
  const spy = vi.spyOn(qc, 'invalidateQueries').mockResolvedValue()
  const hits = (key: readonly unknown[]) => spy.mock.calls.filter((c) => JSON.stringify(c[0]?.queryKey) === JSON.stringify(key)).length
  return { qc, hits }
}

describe('refetching on events during a pass', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => {
    clearPendingInvalidations()
    vi.useRealTimers()
  })

  it('refetches the lists once after a storm of classifications pauses, not once per event', () => {
    const { qc, hits } = client()
    for (let i = 0; i < 100; i++) {
      handleEvent(qc, classified(i))
      vi.advanceTimersByTime(5)
    }
    expect(hits(qk.messages)).toBe(0)
    expect(hits(qk.summary)).toBe(0)
    vi.advanceTimersByTime(COALESCE_MS)
    expect(hits(qk.messages)).toBe(1)
    expect(hits(qk.summary)).toBe(1)
    expect(hits(qk.recentTraces)).toBe(1)
    // The message named by each event refreshes at once, in case it is the one open.
    expect(hits(qk.message('dig-1', 'm_7'))).toBe(1)
  })

  it('still refetches while a stream never pauses', () => {
    const { qc, hits } = client()
    const step = COALESCE_MS / 2
    for (let t = 0; t <= COALESCE_MAX_MS * 2; t += step) {
      handleEvent(qc, classified(t))
      vi.advanceTimersByTime(step)
    }
    expect(hits(qk.messages)).toBeGreaterThanOrEqual(2)
    expect(hits(qk.messages)).toBeLessThanOrEqual(3)
  })

  it('refreshes Finds the moment one is created, since a notification points at it', () => {
    const { qc, hits } = client()
    handleEvent(qc, { type: 'find_created', find: { id: 'f_1' } as never })
    expect(hits(qk.finds)).toBe(1)
    expect(hits(qk.summary)).toBe(1)
  })

  it('coalesces job progress on top of the Task Centre poll', () => {
    const { qc, hits } = client()
    for (let i = 0; i < 20; i++) handleEvent(qc, { type: 'job_progress', job_id: 'j_1', done: BigInt(i), total: BigInt(20), step: 'Fetching' })
    expect(hits(qk.taskCentre)).toBe(0)
    vi.advanceTimersByTime(COALESCE_MS)
    expect(hits(qk.taskCentre)).toBe(1)
  })
})

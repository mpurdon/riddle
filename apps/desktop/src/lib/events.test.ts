/**
 * The event bridge under a storm: a Sift pass emits one `message_classified`
 * per message, and the Inbox once refetched for each of them.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { hashKey, QueryClient, QueryObserver } from '@tanstack/react-query'
import type { RiddleEvent } from '@/bindings/RiddleEvent'
import { taskCentre } from '@/test/fixtures'
import { qk } from './api'
import { clearPendingInvalidations, COALESCE_MAX_MS, COALESCE_MS, handleEvent, watchBoot } from './events'

const classified = (n: number): RiddleEvent => ({ type: 'message_classified', message_id: `m_${n}`, dig_id: 'dig-1', disposition: { kind: 'unresolved' }, shadow: false })

/** A client that records which keys were invalidated, without fetching anything. */
function client() {
  const qc = new QueryClient()
  const spy = vi.spyOn(qc, 'invalidateQueries').mockResolvedValue()
  const hits = (key: readonly unknown[]) => spy.mock.calls.filter((c) => c[0]?.queryKey && hashKey(c[0].queryKey) === hashKey(key)).length
  return { qc, hits, spy }
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
    // Nothing on screen reads the messages named, so none of them is asked for.
    expect(hits(qk.message('dig-1', 'm_7'))).toBe(0)
  })

  it('refreshes the one message that is open at once', () => {
    const { qc, hits } = client()
    const key = qk.message('dig-1', 'm_7')
    const unsubscribe = new QueryObserver(qc, { queryKey: key, queryFn: () => null, enabled: false }).subscribe(() => {})
    handleEvent(qc, classified(7))
    handleEvent(qc, classified(8))
    expect(hits(key)).toBe(1)
    expect(hits(qk.message('dig-1', 'm_8'))).toBe(0)
    unsubscribe()
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

describe('the core booting', () => {
  it('refetches everything once when booting ends, and not again', () => {
    const { qc, spy } = client()
    const off = watchBoot(qc)
    qc.setQueryData(qk.taskCentre, taskCentre({ booting: 'Resolving the judge key' }))
    qc.setQueryData(qk.taskCentre, taskCentre({ booting: 'Opening the store' }))
    expect(spy).not.toHaveBeenCalled()
    qc.setQueryData(qk.taskCentre, taskCentre())
    expect(spy).toHaveBeenCalledTimes(1)
    expect(spy).toHaveBeenCalledWith()
    qc.setQueryData(qk.taskCentre, taskCentre())
    expect(spy).toHaveBeenCalledTimes(1)
    off()
  })

  it('refetches nothing when the core was up before the first read', () => {
    const { qc, spy } = client()
    const off = watchBoot(qc)
    qc.setQueryData(qk.taskCentre, taskCentre())
    qc.setQueryData(qk.taskCentre, taskCentre())
    expect(spy).not.toHaveBeenCalled()
    off()
  })
})

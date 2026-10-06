import { describe, expect, it } from 'vitest'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { queuePosition, queueState, readQueue } from './queue'

const row = (i: number, digId = 'dig-a'): MessageSummary => ({
  id: `m${i}`,
  digId,
  threadId: `t${i}`,
  subject: `Subject ${i}`,
  from: { email: `s${i}@example.com`, name: null },
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

describe('the message queue in router state', () => {
  it('carries the ordered dig and message ids with the opened index', () => {
    const items = [row(1), row(2, 'dig-b'), row(3)]
    const q = queueState(items, 1)
    expect(q.queue).toEqual([
      { digId: 'dig-a', messageId: 'm1' },
      { digId: 'dig-b', messageId: 'm2' },
      { digId: 'dig-a', messageId: 'm3' },
    ])
    expect(q.index).toBe(1)
  })

  it('keeps the opened row at the right index when the list is cut to size', () => {
    const items = Array.from({ length: 2500 }, (_, i) => row(i))
    const q = queueState(items, 2000)
    expect(q.queue.length).toBe(1000)
    expect(q.queue[q.index].messageId).toBe('m2000')
  })

  it('reads nothing from state that is not a queue', () => {
    expect(readQueue(null)).toBeNull()
    expect(readQueue(undefined)).toBeNull()
    expect(readQueue({ from: '/inbox' })).toBeNull()
    expect(readQueue({ queue: [], index: 0 })).toBeNull()
    expect(readQueue({ queue: [{ digId: 1, messageId: 'x' }], index: 0 })).toBeNull()
  })

  it('reads a queue back and finds the position by id when the index has drifted', () => {
    const q = readQueue({ queue: [{ digId: 'd', messageId: 'a' }, { digId: 'd', messageId: 'b' }], index: 0 })!
    expect(queuePosition(q, 'd', 'a')).toBe(0)
    expect(queuePosition(q, 'd', 'b')).toBe(1)
    expect(queuePosition(q, 'other', 'b')).toBe(-1)
  })
})

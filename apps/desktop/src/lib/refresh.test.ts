/**
 * The paged lists after an Action: the row leaves or gains its Stratum
 * before any refetch, and the other lists are left alone.
 */
import { describe, expect, it } from 'vitest'
import { QueryClient, type InfiniteData } from '@tanstack/react-query'
import type { MessageSummary } from '@/bindings/MessageSummary'
import type { Page } from '@/bindings/Page'
import { qk } from './api'
import { addStrataToRows, dropMessageRows } from './refresh'

const row = (id: string, strata: string[] = []): MessageSummary => ({
  id,
  digId: 'dig-1',
  threadId: `t_${id}`,
  subject: `Subject ${id}`,
  from: { email: 'x@example.com', name: null },
  date: '2026-09-25T09:00:00Z',
  snippet: '',
  labels: [],
  isSent: false,
  hasAttachments: false,
  disposition: null,
  strata,
  findId: null,
  shadow: false,
  shadowReview: null,
  importance: 'normal',
  importanceSetByYou: false,
})

type Pages = InfiniteData<Page<MessageSummary>, string | null>
const INBOX = [...qk.messages, 'paged', null, { disposition: null }, null]
const SPOIL = [...qk.messages, 'paged', null, { disposition: 'spoil' }, null]

/** An Inbox three pages deep and a Spoil list of one page, as the wire sends them (the total a number). */
function lists() {
  const qc = new QueryClient()
  qc.setQueryData<Pages>(INBOX, {
    pages: [
      { items: [row('a'), row('b')], nextCursor: 'c1', total: 5 as unknown as bigint },
      { items: [row('c'), row('d')], nextCursor: 'c2', total: 5 as unknown as bigint },
      { items: [row('e')], nextCursor: null, total: 5 as unknown as bigint },
    ],
    pageParams: [null, 'c1', 'c2'],
  })
  qc.setQueryData<Pages>(SPOIL, { pages: [{ items: [row('s')], nextCursor: null, total: 1 as unknown as bigint }], pageParams: [null] })
  qc.setQueryData(qk.places, { pinned: [] })
  return qc
}

describe('a paged message list after an Action', () => {
  it('drops the rows the outcome named from every page and takes them off the total', () => {
    const qc = lists()
    dropMessageRows(qc, new Set(['b', 'e']))
    const inbox = qc.getQueryData<Pages>(INBOX)!
    expect(inbox.pages.flatMap((p) => p.items.map((m) => m.id))).toEqual(['a', 'c', 'd'])
    expect(inbox.pages[0].total).toBe(3)
    // The list the rows were never in is untouched, so it does not re-render.
    expect(qc.getQueryState(SPOIL)!.dataUpdateCount).toBe(1)
  })

  it('adds the Strata to the rows named and leaves the others as they were', () => {
    const qc = lists()
    const before = qc.getQueryData<Pages>(INBOX)!
    addStrataToRows(qc, new Set(['c']), ['Receipts', 'Travel'])
    const inbox = qc.getQueryData<Pages>(INBOX)!
    expect(inbox.pages[1].items[0].strata).toEqual(['Receipts', 'Travel'])
    expect(inbox.pages[0]).toBe(before.pages[0])
    addStrataToRows(qc, new Set(['c']), ['Receipts'])
    expect(qc.getQueryData<Pages>(INBOX)!.pages[1].items[0].strata).toEqual(['Receipts', 'Travel'])
  })

})

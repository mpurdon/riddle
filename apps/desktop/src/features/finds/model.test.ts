import { describe, expect, it } from 'vitest'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { bulkOutcome, decideOutcome, findStatusLabel, groupBySender, nextFindsCursor, outcomeOf, pendingStatuses, remainingFromSender, tabEmptyState, tabForStatus, tabStatuses, tabSubtitle, targetLabel, teachingLine } from './model'

const msg = (id: string, email: string, name: string | null = null): MessageSummary => ({
  id,
  digId: 'dig_work',
  threadId: `t_${id}`,
  subject: `Subject ${id}`,
  from: { email, name },
  date: '2026-09-22T07:42:00.000Z',
  snippet: '',
  labels: [],
  isSent: false,
  hasAttachments: false,
  disposition: { kind: 'unresolved' },
  strata: [],
  findId: null,
  shadow: false,
  shadowReview: null,
  importance: 'normal',
  importanceSetByYou: false,
})

describe('the Finds tabs', () => {
  it('map Needs you, Later and Done onto the statuses the API takes one at a time', () => {
    expect(tabStatuses('needs_you')).toEqual(['open'])
    expect(tabStatuses('later')).toEqual(['snoozed'])
    expect(tabStatuses('done')).toEqual(['resolved', 'dismissed', 'expired'])
  })

  it('know which tab a Find lands on after a status change', () => {
    expect(tabForStatus('resolved')).toBe('done')
    expect(tabForStatus('snoozed')).toBe('later')
    expect(tabForStatus('superseded')).toBeNull()
  })

  it('count in the subtitle in words that agree with the number', () => {
    expect(tabSubtitle('needs_you', 7)).toBe('7 need you')
    expect(tabSubtitle('needs_you', 1)).toBe('1 needs you')
    expect(tabSubtitle('later', 1)).toBe('1 for later')
    expect(tabSubtitle('done', 12)).toBe('12 done')
  })

  it('have an empty state per tab with no doubled spaces', () => {
    for (const tab of ['needs_you', 'later', 'done'] as const) {
      const e = tabEmptyState(tab, false)
      expect(e.title).not.toMatch(/ {2}/)
      expect(e.body).not.toMatch(/ {2}/)
    }
    expect(tabEmptyState('needs_you', false).title).toBe('Nothing needs you.')
    expect(tabEmptyState('done', true).title).toBe('No Finds of this type here.')
  })
})

describe('the multi-status cursor', () => {
  it('asks for every status on the first page and continues only the ones with more', () => {
    expect(pendingStatuses(['resolved', 'dismissed'], {})).toEqual([
      { status: 'resolved', cursor: null },
      { status: 'dismissed', cursor: null },
    ])
    expect(pendingStatuses(['resolved', 'dismissed'], { resolved: '200', dismissed: null })).toEqual([{ status: 'resolved', cursor: '200' }])
  })

  it('is exhausted only when every status is', () => {
    const after = nextFindsCursor({}, [
      { status: 'resolved', nextCursor: '200' },
      { status: 'dismissed', nextCursor: null },
    ])
    expect(after).toEqual({ resolved: '200', dismissed: null })
    expect(nextFindsCursor(after!, [{ status: 'resolved', nextCursor: null }])).toBeNull()
  })
})

describe('the labels', () => {
  it('say who closed a Find in plain words, never the enum', () => {
    expect(findStatusLabel({ status: 'resolved', resolvedBy: 'thread_state', snoozedUntil: null })).toBe('Resolved by your reply')
    expect(findStatusLabel({ status: 'resolved', resolvedBy: 'user', snoozedUntil: null })).toBe('Resolved by you')
    expect(findStatusLabel({ status: 'resolved', resolvedBy: 'director', snoozedUntil: null })).toBe('Resolved by the Director')
    expect(findStatusLabel({ status: 'resolved', resolvedBy: 'temporal', snoozedUntil: null })).toBe('Resolved when its date passed')
    expect(findStatusLabel({ status: 'dismissed', resolvedBy: 'user', snoozedUntil: null })).toBe('Dismissed by you')
    expect(findStatusLabel({ status: 'open', resolvedBy: null, snoozedUntil: null })).toBeNull()
    expect(findStatusLabel({ status: 'snoozed', resolvedBy: null, snoozedUntil: null })).toBe('Snoozed')
  })

  it('name the target of a toolbar button', () => {
    expect(targetLabel('Resolve', 0)).toBe('Resolve')
    expect(targetLabel('Resolve', 3)).toBe('Resolve 3 selected')
    expect(targetLabel('Filed', 12)).toBe('Filed 12 selected')
    // The message lists act on the cursor row without a selection, and say so.
    expect(targetLabel('Archive', 0, true)).toBe('Archive (cursor)')
    expect(targetLabel('Archive', 2, true)).toBe('Archive 2 selected')
  })

  it('summarise a bulk run in one toast and say where Undo lives', () => {
    expect(bulkOutcome('Resolved', 'Find', 3, 0)).toEqual({ title: 'Resolved 3 Finds', body: 'Undo each from Activity.', kind: 'success' })
    expect(bulkOutcome('Resolved', 'Find', 1, 0).title).toBe('Resolved 1 Find')
    const partial = bulkOutcome('Filed', 'message', 2, 1)
    expect(partial.title).toBe('Filed 2 of 3 messages')
    expect(partial.kind).toBe('warn')
    expect(partial.body).toBe('1 failed; the error is shown beside the list. Undo each from Activity.')
    // A page that writes the failure elsewhere says where.
    expect(outcomeOf('Marked 2 of 3 for follow-up', 1, 'under the header').body).toBe('1 failed; the error is shown under the header. Undo each from Activity.')
  })

  it('summarise a bulk decide by what it was corrected to', () => {
    expect(decideOutcome('Filed', 12, 0).title).toBe('Corrected 12 messages to Filed')
    expect(decideOutcome('Find: Respond', 1, 0).title).toBe('Corrected 1 message to Find: Respond')
    expect(decideOutcome('Spoil', 2, 1)).toMatchObject({ title: 'Corrected 2 of 3 to Spoil', kind: 'warn' })
    expect(decideOutcome('Filed', 0, 7)).toMatchObject({ title: 'Could not correct 7 messages to Filed', kind: 'warn' })
  })
})

describe('sender clusters in Unresolved', () => {
  const items = [msg('a', 'notifications@github.example', 'GitHub'), msg('b', 'tom@vantage.example', 'Tom'), msg('c', 'notifications@github.example'), msg('d', 'ines@sapo.example'), msg('e', 'Notifications@GitHub.example'), msg('f', 'tom@vantage.example')]

  it('groups three or more from one address at the position of the first, case-insensitively', () => {
    const groups = groupBySender(items)
    expect(groups.map((g) => [g.email, g.items.map((m) => m.id), g.cluster])).toEqual([
      ['notifications@github.example', ['a', 'c', 'e'], true],
      ['tom@vantage.example', ['b'], false],
      ['ines@sapo.example', ['d'], false],
      ['tom@vantage.example', ['f'], false],
    ])
    expect(groups[0].name).toBe('GitHub')
  })

  it('keeps every message exactly once so the keyboard order matches the screen', () => {
    const flat = groupBySender(items).flatMap((g) => g.items.map((m) => m.id))
    expect([...flat].sort()).toEqual(['a', 'b', 'c', 'd', 'e', 'f'])
    expect(flat).toEqual(['a', 'c', 'e', 'b', 'd', 'f'])
  })

  it('counts what is left from the same sender after a decision', () => {
    expect(remainingFromSender(items, [items[0]])).toBe(2)
    expect(remainingFromSender(items, [items[0], items[2], items[4]])).toBe(0)
    expect(remainingFromSender(items, [items[1], items[3]])).toBe(1)
  })

  it('teaches with the count', () => {
    expect(teachingLine('Filed', 2, 'notifications@github.example')).toBe('Recorded as a correction: Filed. 2 more from notifications@github.example are still Unresolved.')
    expect(teachingLine('Spoil', 1, 'x@y.example')).toBe('Recorded as a correction: Spoil. 1 more from x@y.example is still Unresolved.')
    expect(teachingLine('Filed', 0, 'x@y.example')).toBe('Recorded as a correction: Filed. Nothing else from x@y.example is Unresolved.')
  })
})

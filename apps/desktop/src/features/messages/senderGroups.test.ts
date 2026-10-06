import { describe, expect, it } from 'vitest'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { selectedMessages, senderGroups, senderRows } from './senderGroups'

const msg = (id: string, email: string, date: string, name: string | null = null): MessageSummary => ({
  id,
  digId: 'dig_a',
  threadId: `t_${id}`,
  subject: `Subject ${id}`,
  from: { email, name },
  date,
  snippet: '',
  labels: [],
  isSent: false,
  hasAttachments: false,
  disposition: { kind: 'spoil' },
  strata: [],
  findId: null,
  shadow: false,
  shadowReview: null,
  importance: 'normal',
  importanceSetByYou: false,
})

const items = [msg('m1', 'Alerts@Uptime.example', '2026-09-26T10:00:00Z', 'Uptime'), msg('m2', 'news@daily.example', '2026-09-26T09:00:00Z'), msg('m3', 'alerts@uptime.example', '2026-09-26T08:00:00Z')]

describe('Spoil grouped by sender', () => {
  it('groups by address regardless of case, keeps the newest first, and names the sender', () => {
    const groups = senderGroups(items)
    expect(groups.map((g) => [g.key, g.items.length, g.newest.id])).toEqual([
      ['alerts@uptime.example', 2, 'm1'],
      ['news@daily.example', 1, 'm2'],
    ])
    expect(groups[0].name).toBe('Uptime')
  })
  it('walks senders when collapsed and their messages when one is open', () => {
    const groups = senderGroups(items)
    expect(senderRows(groups, new Set()).map((r) => r.id)).toEqual(['sender:alerts@uptime.example', 'sender:news@daily.example'])
    expect(senderRows(groups, new Set(['alerts@uptime.example'])).map((r) => r.id)).toEqual(['sender:alerts@uptime.example', 'm1', 'm3', 'sender:news@daily.example'])
  })
  it('a selected sender stands for every message from them, once', () => {
    const groups = senderGroups(items)
    expect(selectedMessages(groups, new Set(['sender:alerts@uptime.example', 'm3', 'm2'])).map((m) => m.id)).toEqual(['m1', 'm3', 'm2'])
  })
})

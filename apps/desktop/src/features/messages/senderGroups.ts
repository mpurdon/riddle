/**
 * Spoil grouped by sender (Flow 5, option B). The audit watched a reader
 * scroll fourteen GitHub notifications to find one misfiled invoice: Spoil
 * is mostly a few senders many times over, so the unit of review is the
 * sender, and the count carries the weight. The rows are a flat list so
 * the same j/k cursor walks senders when collapsed and messages when one
 * is open. Pure functions, so the page and the tests agree on the order;
 * Unresolved's clusters derive from the same grouping.
 */
import type { MessageSummary } from '@/bindings/MessageSummary'
import { senderKey } from '@/features/rules/yourCorrections'
import { readLocal, writeLocal } from '@/lib/local'

export interface SenderGroup {
  /** The `senderKey` of the address: the row id and the rule value. */
  key: string
  email: string
  name: string | null
  /** Newest first, as the list came. */
  items: MessageSummary[]
  newest: MessageSummary
}

export type SenderRow = { kind: 'sender'; id: string; group: SenderGroup } | { kind: 'message'; id: string; m: MessageSummary; group: SenderGroup }

/** One group per address, in order of each sender's newest message (the list is newest first). */
export function senderGroups(items: MessageSummary[]): SenderGroup[] {
  const groups = new Map<string, SenderGroup>()
  for (const m of items) {
    const key = senderKey(m.from.email)
    const g = groups.get(key)
    if (g) {
      g.items.push(m)
      if (m.date > g.newest.date) g.newest = m
      if (!g.name && m.from.name) g.name = m.from.name
    } else groups.set(key, { key, email: m.from.email.trim(), name: m.from.name, items: [m], newest: m })
  }
  return [...groups.values()]
}

/** The rows the cursor walks: every sender, and the messages of the open ones under them. */
export function senderRows(groups: SenderGroup[], expanded: ReadonlySet<string>): SenderRow[] {
  const rows: SenderRow[] = []
  for (const group of groups) {
    rows.push({ kind: 'sender', id: `sender:${group.key}`, group })
    if (expanded.has(group.key)) for (const m of group.items) rows.push({ kind: 'message', id: m.id, m, group })
  }
  return rows
}

/** Every message covered by a selection of row ids: a selected sender stands for all of its messages. */
export function selectedMessages(groups: SenderGroup[], selected: ReadonlySet<string>): MessageSummary[] {
  const out = new Map<string, MessageSummary>()
  for (const g of groups) {
    const whole = selected.has(`sender:${g.key}`)
    for (const m of g.items) if (whole || selected.has(m.id)) out.set(m.id, m)
  }
  return [...out.values()]
}

// ---------------------------------------------------------------------------
// The remembered view, per machine; the default is the flat list.

export type SpoilView = 'messages' | 'sender'
export const SPOIL_VIEW_KEY = 'riddle.spoil.view'

export function readSpoilView(): SpoilView {
  return readLocal(SPOIL_VIEW_KEY) === 'sender' ? 'sender' : 'messages'
}

export function writeSpoilView(v: SpoilView): void {
  writeLocal(SPOIL_VIEW_KEY, v)
}

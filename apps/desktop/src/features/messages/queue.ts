/**
 * The list a message was opened from, carried in router state so the
 * message page can step through it with n/p and say "3 of 41". The audit
 * found the page a dead end: every message meant Back, find the row, Enter.
 * Router state rather than a store, so a link opened from the Director or a
 * Find, which has no list behind it, simply has no queue and hides n/p.
 */
import type { NavigateFunction } from 'react-router-dom'
import type { MessageSummary } from '@/bindings/MessageSummary'

export interface QueueEntry {
  digId: string
  messageId: string
}

export interface MessageQueue {
  queue: QueueEntry[]
  index: number
}

/** More than this and history state stops being cheap; nobody steps through more by hand. */
const MAX_QUEUE = 1000

export const messagePath = (e: { digId: string; messageId: string }) => `/inbox/${e.digId}/${e.messageId}`

/** Router state for opening `items[index]` from a list. */
export function queueState(items: MessageSummary[], index: number): MessageQueue {
  const start = Math.max(0, Math.min(index - Math.floor(MAX_QUEUE / 2), items.length - MAX_QUEUE))
  const slice = items.slice(start, start + MAX_QUEUE)
  return { queue: slice.map((m) => ({ digId: m.digId, messageId: m.id })), index: index - start }
}

/** Open `m` on its own page from `items`, carrying the list so n/p work there; `suffix` is a query or hash for the page (`?reply=1`, `#why`). */
export function openFromList(nav: NavigateFunction, items: MessageSummary[], m: MessageSummary, suffix = ''): void {
  const i = items.findIndex((x) => x.id === m.id)
  nav(`${messagePath({ digId: m.digId, messageId: m.id })}${suffix}`, { state: queueState(items, Math.max(0, i)) })
}

/** The queue from router state, or null when the page was not opened from a list. */
export function readQueue(state: unknown): MessageQueue | null {
  if (!state || typeof state !== 'object') return null
  const s = state as Partial<MessageQueue>
  if (!Array.isArray(s.queue) || typeof s.index !== 'number') return null
  const queue = s.queue.filter((e): e is QueueEntry => !!e && typeof e === 'object' && typeof e.digId === 'string' && typeof e.messageId === 'string')
  if (!queue.length) return null
  return { queue, index: s.index }
}

/** Where `messageId` sits in the queue: the recorded index when it still matches, else a search, else -1. */
export function queuePosition(q: MessageQueue, digId: string, messageId: string): number {
  const at = q.queue[q.index]
  if (at && at.digId === digId && at.messageId === messageId) return q.index
  return q.queue.findIndex((e) => e.digId === digId && e.messageId === messageId)
}

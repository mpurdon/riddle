/**
 * Pure derivations for the Finds and Unresolved pages, kept free of React so
 * the copy and the grouping can be tested as functions. The UX study found
 * the pages showed a raw enum ("resolved by thread_state"), never rendered
 * the backend's plain-English reason, and gave a bulk action no way to say
 * what it would touch; each of those is a function here.
 */
import type { Find } from '@/bindings/Find'
import type { FindStatus } from '@/bindings/FindStatus'
import type { MessageSummary } from '@/bindings/MessageSummary'
import type { ResolvedBy } from '@/bindings/ResolvedBy'
import { fmtTime, plural } from '@/lib/format'
import { senderGroups, type SenderGroup } from '@/features/messages/senderGroups'
import { senderKey } from '@/features/rules/yourCorrections'

// ---------------------------------------------------------------------------
// Tabs: Needs you / Later / Done. The API takes one status per call, so Done
// is several statuses fetched side by side (see `usePagedFinds`).

export type FindsTab = 'needs_you' | 'later' | 'done'

export const FINDS_TABS: { value: FindsTab; label: string; statuses: FindStatus[] }[] = [
  { value: 'needs_you', label: 'Needs you', statuses: ['open'] },
  { value: 'later', label: 'Later', statuses: ['snoozed'] },
  { value: 'done', label: 'Done', statuses: ['resolved', 'dismissed', 'expired'] },
]

export function tabStatuses(tab: FindsTab): FindStatus[] {
  return FINDS_TABS.find((t) => t.value === tab)?.statuses ?? ['open']
}

/** The tab a Find belongs on after a status change, so the URL can follow it. */
export function tabForStatus(status: FindStatus): FindsTab | null {
  const t = FINDS_TABS.find((x) => x.statuses.includes(status))
  return t?.value ?? null
}

/** The count under the title: "7 need you", "1 for later", "12 done". */
export function tabSubtitle(tab: FindsTab, total: number): string {
  switch (tab) {
    case 'needs_you':
      return `${total} need${total === 1 ? 's' : ''} you`
    case 'later':
      return `${total} for later`
    case 'done':
      return `${total} done`
  }
}

export function tabEmptyState(tab: FindsTab, typeFiltered: boolean): { title: string; body: string } {
  if (typeFiltered) return { title: 'No Finds of this type here.', body: 'Clear the type filter to see the rest of this tab.' }
  switch (tab) {
    case 'needs_you':
      return {
        title: 'Nothing needs you.',
        body: 'When the Sift surfaces something that deserves attention it appears here, with a Why? you can inspect. Corrections in Unresolved are the fastest way to teach Riddle what counts.',
      }
    case 'later':
      return { title: 'Nothing is snoozed.', body: 'Snooze a Find with s and it waits here until its time comes, then returns to Needs you.' }
    case 'done':
      return { title: 'Nothing is done yet.', body: 'Finds you resolve or mark not important stay here, so you can see what Riddle raised and how it was closed.' }
  }
}

// ---------------------------------------------------------------------------
// Paging across several statuses with one cursor. The cursor is a map from
// status to that status's next cursor: a status absent from the map has not
// been fetched yet, `null` means it is exhausted.

export type FindsCursor = Partial<Record<FindStatus, string | null>>

/** The statuses still worth asking for, with the cursor each one continues from. */
export function pendingStatuses(statuses: FindStatus[], cursor: FindsCursor): { status: FindStatus; cursor: string | null }[] {
  return statuses.filter((s) => !(s in cursor) || cursor[s] !== null).map((s) => ({ status: s, cursor: cursor[s] ?? null }))
}

/** The cursor after a round of fetches, or null when every status is exhausted. */
export function nextFindsCursor(previous: FindsCursor, fetched: { status: FindStatus; nextCursor: string | null }[]): FindsCursor | null {
  const next: FindsCursor = { ...previous }
  for (const f of fetched) next[f.status] = f.nextCursor
  return Object.values(next).some((c) => c !== null) ? next : null
}

// ---------------------------------------------------------------------------
// Labels

const RESOLVED_BY: Record<ResolvedBy, string> = {
  user: 'by you',
  thread_state: 'by your reply',
  temporal: 'when its date passed',
  director: 'by the Director',
}

/** "Resolved by your reply", "Dismissed by you", "Snoozed until Tue 8:00", "Expired". */
export function findStatusLabel(f: Pick<Find, 'status' | 'resolvedBy' | 'snoozedUntil'>): string | null {
  switch (f.status) {
    case 'open':
      return null
    case 'snoozed':
      return f.snoozedUntil ? `Snoozed until ${fmtTime(f.snoozedUntil)}` : 'Snoozed'
    case 'resolved':
      return f.resolvedBy ? `Resolved ${RESOLVED_BY[f.resolvedBy]}` : 'Resolved'
    case 'dismissed':
      return f.resolvedBy ? `Dismissed ${RESOLVED_BY[f.resolvedBy]}` : 'Dismissed'
    case 'expired':
      return 'Expired'
    case 'superseded':
      return 'Superseded by a newer Find'
  }
}

/**
 * A toolbar button names its target: "Resolve 3 selected" for a selection,
 * else "Resolve", or "Resolve (cursor)" where the page says so (the message
 * lists, whose verbs act on the cursor row without a selection).
 */
export function targetLabel(verb: string, selectedCount: number, cursor = false): string {
  if (selectedCount > 0) return `${verb} ${selectedCount} selected`
  return cursor ? `${verb} (cursor)` : verb
}

export interface BulkOutcome {
  title: string
  body: string
  kind: 'success' | 'warn'
}

/** One toast for a bulk run; Undo per item is not offered, so the body says where it lives. */
export function bulkOutcome(past: string, noun: string, done: number, failed: number): BulkOutcome {
  return outcomeOf(failed ? `${past} ${done} of ${done + failed} ${noun}s` : `${past} ${plural(done, noun)}`, failed)
}

/** The one toast after a bulk decide in Unresolved: what it was corrected to. */
export function decideOutcome(label: string, done: number, failed: number): BulkOutcome {
  if (!done) return outcomeOf(`Could not correct ${plural(failed, 'message')} to ${label}`, failed)
  return outcomeOf(failed ? `Corrected ${done} of ${done + failed} to ${label}` : `Corrected ${plural(done, 'message')} to ${label}`, failed)
}

/** The toast for a bulk run under `title`; `where` says where the page writes a failure. */
export function outcomeOf(title: string, failed: number, where = 'beside the list'): BulkOutcome {
  return { title, body: failed ? `${failed} failed; the error is shown ${where}. Undo each from Activity.` : 'Undo each from Activity.', kind: failed ? 'warn' : 'success' }
}

// ---------------------------------------------------------------------------
// Sender clusters in Unresolved. Three or more from one address become one
// run with a header, so a burst from a notifier can be decided in one go.

/** A sender's messages, and whether they are a cluster (3+) that earns a header. */
export type SenderCluster = SenderGroup & { cluster: boolean }

export const CLUSTER_MIN = 3

/** Spoil's grouping, cut for the tray: a cluster sits where its first message was; the rest stay one row each, in list order. */
export function groupBySender(items: MessageSummary[], min = CLUSTER_MIN): SenderCluster[] {
  const bySender = new Map(senderGroups(items).map((g) => [g.key, g]))
  const emitted = new Set<string>()
  const groups: SenderCluster[] = []
  for (const m of items) {
    const g = bySender.get(senderKey(m.from.email))!
    if (g.items.length < min) groups.push({ ...senderGroups([m])[0], cluster: false })
    else if (!emitted.has(g.key)) {
      emitted.add(g.key)
      groups.push({ ...g, cluster: true })
    }
  }
  return groups
}

/** How many loaded Unresolved messages from the same sender are left after `decided` are decided. */
export function remainingFromSender(items: MessageSummary[], decided: MessageSummary[]): number {
  const ids = new Set(decided.map((d) => d.id))
  const senders = new Set(decided.map((d) => senderKey(d.from.email)))
  return items.filter((m) => !ids.has(m.id) && senders.has(senderKey(m.from.email))).length
}

/** The teaching line after a decision: the correction is recorded, and here is what is left. */
export function teachingLine(decidedAs: string, remaining: number, sender: string): string {
  const lead = `Recorded as a correction: ${decidedAs}.`
  if (remaining === 0) return `${lead} Nothing else from ${sender} is Unresolved.`
  return `${lead} ${remaining} more from ${sender} ${remaining === 1 ? 'is' : 'are'} still Unresolved.`
}

/**
 * One Action call per account. A list under "All accounts" can mix
 * messages from several Digs, and every message Action takes one `digId`
 * with the message ids under it, so a bulk verb splits its targets by
 * account and runs the accounts side by side. The Inbox and the Spoil and
 * Contamination review had each grown a copy of the split and the loop.
 *
 * With several accounts the calls skip their own refetch and the queries
 * are refreshed once at the end, as `useBulkRun` does: side by side, each
 * call's invalidation cancelled the others' refetches, and the list came
 * back once per account. One account keeps the call's own invalidation,
 * which also covers the entities the outcome names.
 */
import type { MessageSummary } from '@/bindings/MessageSummary'
import type { RunOptions, useAction } from '@/lib/useAction'

type Actions = Pick<ReturnType<typeof useAction>, 'run' | 'invalidateAfter'>

/** The message ids under each account, accounts in the order they first appear. */
export function byDig(messages: MessageSummary[]): [digId: string, ids: string[]][] {
  const g = new Map<string, string[]>()
  for (const m of messages) {
    const ids = g.get(m.digId)
    if (ids) ids.push(m.id)
    else g.set(m.digId, [m.id])
  }
  return [...g.entries()]
}

/** Runs `action` once per account; returns the activity ids, for an Undo that covers them all. */
export async function runPerDig({ run, invalidateAfter }: Actions, messages: MessageSummary[], action: string, extra: Record<string, unknown> = {}, opts: Omit<RunOptions, 'digId'> = {}): Promise<string[]> {
  const groups = byDig(messages)
  const coalesce = groups.length > 1 && !opts.skipInvalidate
  const outcomes = await Promise.all(groups.map(([digId, messageIds]) => run(action, { digId, messageIds, ...extra }, { ...opts, digId, skipInvalidate: coalesce || opts.skipInvalidate })))
  if (coalesce) await invalidateAfter([action], opts.invalidate)
  return outcomes.flatMap((o) => (o?.activityId ? [o.activityId] : []))
}

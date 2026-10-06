/**
 * A run over the selected rows. One item goes through `run` as itself,
 * with its own toast and Undo; several run side by side with no toast
 * each and no refetch each, the queries are refreshed once at the end,
 * one toast says what happened, and a partial failure is written under
 * the list. Finds and Unresolved had each grown this loop, and the bar
 * that shows the count and the failure (`SelectionBar`).
 */
import { useCallback, useState } from 'react'
import type { QueryKey } from '@tanstack/react-query'
import { toast } from '@/lib/stores'
import { showError, useAction } from '@/lib/useAction'
import type { BulkOutcome } from './model'

export interface BulkSpec<T> {
  action: string
  args: (item: T) => Record<string, unknown>
  digId: (item: T) => string | null
  /** The one toast after a bulk run, from the counts. */
  outcome: (done: number, failed: number) => BulkOutcome
  /** Said before the outcome's body: what the decision taught. */
  lead?: string
  /** The line under the list when some items failed. */
  failure: (failed: number, total: number) => string
  /** After any failure (the first error has been shown): take back what the page assumed. */
  onFailure?: () => void
  /** Queries to refresh after a bulk run, beyond the Action's own. */
  invalidate?: QueryKey[]
}

export interface BulkResult {
  done: number
  failed: number
}

export function useBulkRun(list: { clear: () => void }) {
  const { run, invalidateAfter } = useAction()
  const [bulkError, setBulkError] = useState<string | null>(null)
  const clearError = useCallback(() => setBulkError(null), [])
  /** The counts, or null when nothing ran or the one item failed. */
  const runBulk = async <T,>(items: T[], spec: BulkSpec<T>): Promise<BulkResult | null> => {
    if (!items.length) return null
    setBulkError(null)
    if (items.length === 1) {
      try {
        await run(spec.action, spec.args(items[0]), { digId: spec.digId(items[0]) })
        return { done: 1, failed: 0 }
      } catch {
        spec.onFailure?.()
        return null
      }
    }
    // Errors toast once, not once per item: seven refusals for the same
    // reason (a dev build not armed for Gmail) said it seven times.
    let firstError: unknown = null
    const ok = await Promise.all(
      items.map((it) =>
        run(spec.action, spec.args(it), { digId: spec.digId(it), skipInvalidate: true, successToast: false, toastErrors: false }).then(
          () => true,
          (e: unknown) => {
            firstError ??= e
            return false
          },
        ),
      ),
    )
    const done = ok.filter(Boolean).length
    const failed = ok.length - done
    await invalidateAfter([spec.action], spec.invalidate)
    list.clear()
    const o = spec.outcome(done, failed)
    // The lead says what was taught, which is nothing when nothing ran.
    toast(o.title, { kind: o.kind, body: spec.lead && done ? `${spec.lead} ${o.body}` : o.body })
    if (failed) {
      showError(firstError)
      setBulkError(spec.failure(failed, items.length))
      spec.onFailure?.()
    }
    return { done, failed }
  }
  return { runBulk, bulkError, clearError }
}

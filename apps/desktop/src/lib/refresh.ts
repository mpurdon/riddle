/**
 * In-place patches of the paged lists. Every message verb used to wait for
 * the lists to refetch before the row moved, so an Archive on a deep Inbox
 * left the row sitting there until every loaded page had come back. For
 * the verbs whose effect on a row is known the moment the Action returns,
 * the row leaves (or gains its Stratum) at once, driven by what the outcome
 * says it affected; the refetch that follows in the background is the
 * correction if the guess was wrong. The loaded pages are kept as they
 * are: a reader three pages deep is not pulled back to the top by their
 * own action. `patchPaged` and `patchRows` own the one rule every patch
 * follows: an unchanged page stays the same object, and a list with no
 * page changed is not set at all, so nothing re-renders for nothing.
 */
import type { InfiniteData, QueryClient, QueryKey } from '@tanstack/react-query'
import type { MessageSummary } from '@/bindings/MessageSummary'
import type { Page } from '@/bindings/Page'
import { qk } from './api'
import { num, u64 } from './format'

/** Rewrite the pages of every infinite list under `keys`; `fn` returns a page as it was when nothing on it changed. */
export function patchPaged<P>(qc: QueryClient, keys: QueryKey[], fn: (pages: P[]) => P[]): void {
  for (const key of keys)
    qc.setQueriesData<InfiniteData<P, unknown>>({ queryKey: key }, (old) => {
      if (!old) return undefined
      const pages = fn(old.pages)
      return pages.some((p, i) => p !== old.pages[i]) ? { ...old, pages } : undefined
    })
}

/** Rewrite the rows of every page under `keys`; a row `fn` returns as it was keeps its page as it was. */
export function patchRows<R>(qc: QueryClient, keys: QueryKey[], fn: (row: R) => R): void {
  patchPaged<{ items: R[] }>(qc, keys, (pages) =>
    pages.map((p) => {
      let touched = false
      const items = p.items.map((r) => {
        const next = fn(r)
        if (next !== r) touched = true
        return next
      })
      return touched ? { ...p, items } : p
    }),
  )
}

/** The infinite lists a message row can sit in: the Inbox, Spoil, Contamination, Unresolved and search, and the Priority inbox. */
const PAGED_MESSAGE_KEYS: QueryKey[] = [[...qk.messages, 'paged'], qk.priorityInbox]

/** The total is a `bigint` on the type and a number on the wire; `num` reads either and `u64` writes the field. */
const lessTotal = (total: Page<unknown>['total'], n: number): Page<unknown>['total'] => (total === null ? null : u64(num(total) - n))

/** The rows an Action moved out of the list they were acted on from, with the count under the header. */
export function dropMessageRows(qc: QueryClient, ids: ReadonlySet<string>): void {
  if (!ids.size) return
  patchPaged<Page<MessageSummary>>(qc, PAGED_MESSAGE_KEYS, (pages) => {
    let gone = 0
    const next = pages.map((p) => {
      const items = p.items.filter((m) => !ids.has(m.id))
      gone += p.items.length - items.length
      return items.length === p.items.length ? p : { ...p, items }
    })
    if (!gone) return pages
    // The total rides on the first page; the header reads it from there.
    next[0] = { ...next[0], total: lessTotal(next[0].total, gone) }
    return next
  })
}

/** The Strata an Action added, on the rows it named. */
export function addStrataToRows(qc: QueryClient, ids: ReadonlySet<string>, strata: string[]): void {
  if (!ids.size || !strata.length) return
  patchRows<MessageSummary>(qc, PAGED_MESSAGE_KEYS, (m) => {
    if (!ids.has(m.id)) return m
    const merged = [...new Set([...m.strata, ...strata])]
    return merged.length === m.strata.length ? m : { ...m, strata: merged }
  })
}

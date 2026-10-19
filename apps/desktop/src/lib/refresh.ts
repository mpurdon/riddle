/**
 * In-place patches of the paged message rows. Every message verb used to
 * wait for the lists to refetch before the row moved, so an Archive on a
 * deep Inbox left the row sitting there until every loaded page had come
 * back. For the verbs whose effect on a row is known the moment the
 * Action returns, the row leaves (or gains its Stratum) at once, driven by
 * what the outcome says it affected; the refetch that follows in the
 * background is the correction if the guess was wrong. The loaded pages
 * are kept as they are: a reader three pages deep is not pulled back to
 * the top by their own action.
 */
import type { InfiniteData, QueryClient, QueryKey } from '@tanstack/react-query'
import type { MessageSummary } from '@/bindings/MessageSummary'
import type { Page } from '@/bindings/Page'
import { qk } from './api'

type MessagePages = InfiniteData<Page<MessageSummary>, unknown>

/** The infinite lists a message row can sit in: the Inbox, Spoil, Contamination, Unresolved and search, and the Priority inbox. */
const PAGED_MESSAGE_KEYS: QueryKey[] = [[...qk.messages, 'paged'], qk.priorityInbox]

/** Rewrite the cached message lists; a list `fn` leaves unchanged (returns undefined) stays as it was, so nothing re-renders for nothing. */
function patchMessageLists(qc: QueryClient, fn: (old: MessagePages) => MessagePages | undefined): void {
  for (const key of PAGED_MESSAGE_KEYS) qc.setQueriesData<MessagePages>({ queryKey: key }, (old) => (old ? fn(old) : undefined))
}

/** The total is a `bigint` on the type and a number on the wire; either goes down by `n`. */
function lessTotal(total: Page<unknown>['total'], n: number): Page<unknown>['total'] {
  if (total === null) return null
  return typeof total === 'bigint' ? total - BigInt(n) : (((total as unknown as number) - n) as unknown as bigint)
}

/** The rows an Action moved out of the list they were acted on from, with the count under the header. */
export function dropMessageRows(qc: QueryClient, ids: ReadonlySet<string>): void {
  if (!ids.size) return
  patchMessageLists(qc, (old) => {
    let gone = 0
    const pages = old.pages.map((p) => {
      const items = p.items.filter((m) => !ids.has(m.id))
      gone += p.items.length - items.length
      return items.length === p.items.length ? p : { ...p, items }
    })
    if (!gone) return undefined
    // The total rides on the first page; the header reads it from there.
    pages[0] = { ...pages[0], total: lessTotal(pages[0].total, gone) }
    return { ...old, pages }
  })
}

/** The Strata an Action added, on the rows it named. */
export function addStrataToRows(qc: QueryClient, ids: ReadonlySet<string>, strata: string[]): void {
  if (!ids.size || !strata.length) return
  patchMessageLists(qc, (old) => {
    let changed = false
    const pages = old.pages.map((p) => {
      const items = p.items.map((m) => {
        if (!ids.has(m.id)) return m
        const merged = [...new Set([...m.strata, ...strata])]
        if (merged.length === m.strata.length) return m
        changed = true
        return { ...m, strata: merged }
      })
      return items.some((m, i) => m !== p.items[i]) ? { ...p, items } : p
    })
    return changed ? { ...old, pages } : undefined
  })
}

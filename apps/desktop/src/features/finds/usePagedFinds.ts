/**
 * Paged Finds for one tab. `list_finds` takes a single status, and a tab
 * can stand for several (Done is resolved, dismissed and expired), so one
 * page here is one round of fetches, one per status still open, joined
 * under a composite cursor. Before this the page asked for 200 and
 * stopped; the 201st Find did not exist as far as the UI was concerned.
 */
import { useMemo } from 'react'
import { useInfiniteQuery } from '@tanstack/react-query'
import type { Find } from '@/bindings/Find'
import type { FindType } from '@/bindings/FindType'
import { listFinds, page, qk } from '@/lib/api'
import { num } from '@/lib/format'
import { type FindsCursor, type FindsTab, nextFindsCursor, pendingStatuses, tabStatuses } from './model'

export const FINDS_PAGE = 100

export function usePagedFinds(args: { tab: FindsTab; findType: FindType | null; digId: string | null; limit?: number }) {
  const statuses = tabStatuses(args.tab)
  const q = useInfiniteQuery({
    queryKey: [...qk.finds, 'paged', args.tab, args.findType, args.digId],
    queryFn: async ({ pageParam }) => {
      const rounds = pendingStatuses(statuses, pageParam)
      const pages = await Promise.all(rounds.map((r) => listFinds({ status: r.status, findType: args.findType ?? undefined, digId: args.digId, page: page(r.cursor, args.limit ?? FINDS_PAGE) })))
      const items = pages.flatMap((p) => p.items)
      const total = pages.reduce<number | null>((acc, p) => (p.total === null ? acc : (acc ?? 0) + num(p.total)), null)
      return { items, next: nextFindsCursor(pageParam, rounds.map((r, i) => ({ status: r.status, nextCursor: pages[i].nextCursor }))), total }
    },
    initialPageParam: {} as FindsCursor,
    getNextPageParam: (last) => last.next ?? undefined,
  })
  // Stable between renders: the page keys effects on the list's contents,
  // and a fresh array per render would run them on every render.
  const items: Find[] = useMemo(() => q.data?.pages.flatMap((p) => p.items) ?? [], [q.data])
  const total = q.data?.pages[0]?.total ?? null
  return { ...q, items, total }
}

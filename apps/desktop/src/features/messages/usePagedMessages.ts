import { useInfiniteQuery } from '@tanstack/react-query'
import { listMessages, page, qk, search } from '@/lib/api'
import type { MessageFilter } from '@/lib/types'

export function usePagedMessages(args: { digId: string | null; filter: MessageFilter; query?: string | null; limit?: number; enabled?: boolean }) {
  const q = useInfiniteQuery({
    queryKey: [...qk.messages, 'paged', args.digId, args.filter, args.query ?? null],
    queryFn: ({ pageParam }) =>
      args.query ? search({ query: args.query, digId: args.digId, page: page(pageParam, args.limit ?? 50), shadowOnly: args.filter?.shadowOnly }) : listMessages({ digId: args.digId, filter: args.filter, page: page(pageParam, args.limit ?? 50) }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.nextCursor,
    enabled: args.enabled ?? true,
  })
  const items = q.data?.pages.flatMap((p) => p.items) ?? []
  const total = q.data?.pages[0]?.total ?? null
  return { ...q, items, total }
}

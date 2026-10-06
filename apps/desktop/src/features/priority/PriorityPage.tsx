/**
 * The Priority inbox: only what you chose to see first. One real mailbox
 * had 6,000 messages and 400 open Finds, and no view showed just the
 * starred mail, the Strata that matter and the Finds worth a look. The
 * rows are the Inbox's rows fed by `priority_inbox`, with the Inbox's
 * keyboard triage, toolbar and preview pane (`useMessageTriage`), so
 * nothing is learnt twice and the two cannot drift. The criteria are
 * OR-combined (a message appears if any holds) and live in
 * `Settings.priorityInbox`; every change in the panel saves at once
 * through `save_settings`, so Settings and this page never disagree.
 */
import { useState } from 'react'
import { useInfiniteQuery } from '@tanstack/react-query'
import { ChevronDown, ChevronRight, Star } from 'lucide-react'
import type { PriorityInboxSettings } from '@/bindings/PriorityInboxSettings'
import { page as pageRequest, priorityInbox, qk } from '@/lib/api'
import { num } from '@/lib/format'
import { useDigContext, useSettings } from '@/lib/queries'
import { useAction } from '@/lib/useAction'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { EmptyState } from '@/components/ui/EmptyState'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'
import { MessageList } from '@/features/messages/MessageList'
import { PreviewAside } from '@/features/messages/MessagePreview'
import { MessageOverlays, MessageToolbar, TriageError, useMessageTriage } from '@/features/messages/useMessageTriage'
import { describeCriteria, PriorityCriteriaEditor } from './PriorityCriteria'

export { describeCriteria } from './PriorityCriteria'

const CRITERIA_ID = 'priority-criteria'

/** The rows, paged like the Inbox's; keyed under `messages` so every message Action refreshes them. */
function usePriorityRows(digId: string | null) {
  const q = useInfiniteQuery({
    queryKey: [...qk.priorityInbox, digId],
    queryFn: ({ pageParam }) => priorityInbox({ digId, page: pageRequest(pageParam, 50) }),
    initialPageParam: null as string | null,
    getNextPageParam: (last) => last.nextCursor,
  })
  const items = q.data?.pages.flatMap((p) => p.items) ?? []
  const total = q.data?.pages[0]?.total ?? null
  return { ...q, items, total }
}

export default function PriorityPage() {
  const { digId, digName } = useDigContext()
  const settings = useSettings()
  const { run } = useAction()
  const q = usePriorityRows(digId)
  const items = q.items
  const triage = useMessageTriage(items)
  const { list, open } = triage
  const [criteriaOpen, setCriteriaOpen] = useState(false)
  useShortcuts(['messages'], triage.shortcuts)

  const s = settings.data
  const chosen = s?.priorityInbox
  /** Save the whole settings with one field changed, as the Settings page does; the list refetches on success. */
  const save = (patch: Partial<PriorityInboxSettings>) => {
    if (!s) return
    void run('save_settings', { settings: { ...s, priorityInbox: { ...s.priorityInbox, ...patch } } }, { successToast: false, invalidate: [qk.priorityInbox] })
  }

  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <PageHeader title="Priority" subtitle={q.total !== null ? `${num(q.total)} messages · ${digName(digId)}` : 'Loading…'} actions={triage.targets().length ? <MessageToolbar triage={triage} digId={digId} /> : null}>
        <button type="button" aria-expanded={criteriaOpen} aria-controls={CRITERIA_ID} onClick={() => setCriteriaOpen((o) => !o)} className="inline-flex items-center gap-1 rounded px-1 text-xs font-medium text-ink hover:bg-sand-2">
          {criteriaOpen ? <ChevronDown className="size-3.5 text-clay" aria-hidden /> : <ChevronRight className="size-3.5 text-clay" aria-hidden />}
          What surfaces here
        </button>
        {chosen && !criteriaOpen ? <span className="truncate text-xs text-ink-2">{describeCriteria(chosen)}</span> : null}
      </PageHeader>
      <TriageError triage={triage} />
      {criteriaOpen && chosen ? (
        <section id={CRITERIA_ID} aria-label="What surfaces here" className="shrink-0 space-y-2 border-b border-line bg-sand/40 px-5 py-3 text-xs">
          <p className="text-ink-2">A message appears when any of these holds. Changes save at once; the same controls are under Settings.</p>
          <PriorityCriteriaEditor chosen={chosen} save={save} />
        </section>
      ) : null}
      <div className="flex min-h-0 flex-1">
        <section className="flex min-w-0 flex-1 flex-col">
          <MessageList
            items={items}
            cursor={list.cursor}
            selected={list.selected}
            onCursor={list.setCursor}
            onToggle={list.toggle}
            onOpen={(m) => open(m)}
            digName={digName}
            hasMore={!!q.hasNextPage}
            loadingMore={q.isFetchingNextPage}
            onLoadMore={() => void q.fetchNextPage()}
            loading={q.isLoading}
            emptyState={
              <EmptyState
                icon={<Star />}
                title="Nothing surfaces yet."
                body={chosen ? `Nothing in ${digName(digId)} is ${describeCriteria(chosen).toLowerCase()}. Widen what surfaces here, or star a message in Gmail.` : 'Nothing matches what you chose.'}
                actions={
                  <Button size="sm" onClick={() => setCriteriaOpen(true)}>
                    Change what surfaces here
                  </Button>
                }
              />
            }
          />
        </section>
        {list.current ? <PreviewAside row={list.current} digName={digName(list.current.digId)} onOpen={() => open(list.current!)} onFollowUp={(dueAt) => void triage.followUp([list.current!], dueAt)} /> : null}
      </div>
      <MessageOverlays triage={triage} />
    </div>
  )
}

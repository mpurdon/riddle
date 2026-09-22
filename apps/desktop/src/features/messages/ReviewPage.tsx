/**
 * Spoil and Contamination review (README "Spoil", "Contamination"): bulk
 * select, Not Spoil / Restore (a correction), Never Hidden quick-add, and
 * for Contamination a permanent delete behind the type-the-count tier.
 */
import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { AlertTriangle, ShieldOff, Trash2, Undo2 } from 'lucide-react'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { num } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { useAction } from '@/lib/useAction'
import { useListNav } from '@/lib/useListNav'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { EmptyState } from '@/components/ui/EmptyState'
import { Note } from '@/components/ui/Note'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'
import { MessageList } from './MessageList'
import { usePagedMessages } from './usePagedMessages'

export function ReviewPage({ kind }: { kind: 'spoil' | 'contamination' }) {
  const nav = useNavigate()
  const { digId, digName } = useDigContext()
  const { run } = useAction()
  const q = usePagedMessages({ digId, filter: { disposition: kind } })
  const items = q.items
  const list = useListNav(items, { onOpen: (m) => nav(`/inbox/${m.digId}/${m.id}`) })
  const [busy, setBusy] = useState(false)
  const targets = (): MessageSummary[] => (list.selected.size ? items.filter((m) => list.selected.has(m.id)) : list.current ? [list.current] : [])
  const byDig = (ms: MessageSummary[]) => {
    const g = new Map<string, string[]>()
    for (const m of ms) g.set(m.digId, [...(g.get(m.digId) ?? []), m.id])
    return [...g.entries()]
  }
  const restore = async () => {
    const ms = targets()
    if (!ms.length) return
    setBusy(true)
    try {
      for (const [d, ids] of byDig(ms)) await run('restore_messages', { digId: d, messageIds: ids }, { digId: d })
      list.clear()
    } finally {
      setBusy(false)
    }
  }
  const neverHide = async () => {
    const ms = targets()
    const senders = [...new Set(ms.map((m) => m.from.email.toLowerCase()))]
    for (const s of senders) await run('add_never_hidden', { entry: s })
    if (ms.length) await restore()
  }
  const del = async () => {
    const ms = targets()
    if (!ms.length) return
    for (const [d, ids] of byDig(ms)) await run('permanent_delete', { digId: d, messageIds: ids }, { digId: d })
    list.clear()
  }
  useShortcuts('review', { not_spoil: () => void restore(), never_hidden: () => void neverHide() })
  const isSpoil = kind === 'spoil'
  const count = list.selected.size
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={isSpoil ? 'Spoil' : 'Contamination'}
        subtitle={q.total !== null ? `${num(q.total)} messages · ${digName(digId)} · ${isSpoil ? 'legitimate, low value; not deleted, still searchable' : 'untrusted; decided by evidence, never read by the Director'}` : 'Loading…'}
        actions={
          <>
            <span className="text-xs text-clay">{count ? `${count} selected` : list.current ? 'current row' : ''}</span>
            <Button size="sm" icon={<Undo2 />} busy={busy} onClick={restore} disabledReason={items.length ? null : 'Nothing to restore'} title={isSpoil ? 'Not Spoil (n)' : 'Restore (n)'}>
              {isSpoil ? 'Not Spoil' : 'Restore'}
            </Button>
            <Button size="sm" icon={<ShieldOff />} onClick={neverHide} disabledReason={items.length ? null : 'Nothing selected'} title="Never hide this sender (h)">
              Never hide sender
            </Button>
            {!isSpoil ? (
              <Button size="sm" variant="danger" icon={<Trash2 />} onClick={del} disabledReason={items.length ? null : 'Nothing to delete'}>
                Delete permanently…
              </Button>
            ) : null}
          </>
        }
      >
        {isSpoil ? (
          <span className="text-xs text-ink-2">Riddle sends a Spoil digest daily so a misfiled message is caught within a day. "Not Spoil" records a correction; three of the same kind and Riddle proposes a Mesh change.</span>
        ) : (
          <span className="text-xs text-ink-2">Unimportant and untrustworthy are different. Restoring records a correction; Never hide makes the sender exempt from Spoil and Contamination for good.</span>
        )}
      </PageHeader>
      {!isSpoil && items.length ? (
        <Note tone="warn" className="mx-4 mt-3">
          <span className="flex items-center gap-2">
            <AlertTriangle className="size-4" /> Mail that argues for its own importance is treated as evidence against it. Check the sender and authentication before restoring.
          </span>
        </Note>
      ) : null}
      <MessageList
        items={items}
        cursor={list.cursor}
        selected={list.selected}
        onCursor={list.setCursor}
        onToggle={list.toggle}
        onOpen={(m) => nav(`/inbox/${m.digId}/${m.id}`)}
        digName={digName}
        hasMore={!!q.hasNextPage}
        loadingMore={q.isFetchingNextPage}
        onLoadMore={() => void q.fetchNextPage()}
        loading={q.isLoading}
        showDisposition={false}
        emptyState={
          <EmptyState
            icon={isSpoil ? <Trash2 /> : <AlertTriangle />}
            title={isSpoil ? 'Spoil is empty.' : 'No Contamination.'}
            body={isSpoil ? 'Nothing has been moved to Spoil for this Dig yet. During the Shadow Period, Riddle only proposes; see the Summary for what it would have done.' : 'No mail failed the evidence checks: Gmail spam verdict, authentication, lookalike domains, first-time senders asking for money or credentials.'}
          />
        }
      />
    </div>
  )
}

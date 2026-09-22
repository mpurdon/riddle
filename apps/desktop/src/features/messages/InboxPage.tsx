/**
 * The chronological inbox (README: it still exists; it is not the centre).
 * Filters by Dig, disposition, stratum, label, sender and date; keyboard
 * triage j/k/x/e/#/s/l; bulk actions through the confirmation tiers.
 */
import { useMemo, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { Archive, Inbox, Layers, Search, Trash2 } from 'lucide-react'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { num } from '@/lib/format'
import { useDigContext, useStrata } from '@/lib/queries'
import type { DispositionFilter, MessageFilter } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { useListNav } from '@/lib/useListNav'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { EmptyState } from '@/components/ui/EmptyState'
import { Input } from '@/components/ui/Field'
import { Select } from '@/components/ui/Select'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader, SnoozeMenu, StrataPicker } from '@/components/Bits'
import { MessageList } from './MessageList'
import { usePagedMessages } from './usePagedMessages'

const DISPOSITIONS: { value: DispositionFilter | 'all' | 'unsifted'; label: string }[] = [
  { value: 'all', label: 'Any disposition' },
  { value: 'find', label: 'Find' },
  { value: 'filed', label: 'Filed' },
  { value: 'spoil', label: 'Spoil' },
  { value: 'contamination', label: 'Contamination' },
  { value: 'unresolved', label: 'Unresolved' },
  { value: 'unsifted', label: 'Not yet sifted' },
]

export default function InboxPage() {
  const nav = useNavigate()
  const [params, setParams] = useSearchParams()
  const { digId, digName } = useDigContext()
  const strata = useStrata()
  const { run } = useAction()
  const query = params.get('q') ?? ''
  const disposition = (params.get('disposition') as DispositionFilter | 'all' | 'unsifted' | null) ?? 'all'
  const stratum = params.get('stratum') ?? ''
  const label = params.get('label') ?? ''
  const from = params.get('from') ?? ''
  const since = params.get('since') ?? ''
  const until = params.get('until') ?? ''
  const shadow = params.get('shadow') === '1'
  const set = (k: string, v: string) => {
    const next = new URLSearchParams(params)
    if (v) next.set(k, v)
    else next.delete(k)
    setParams(next, { replace: true })
  }
  const filter: MessageFilter = useMemo(
    () => ({
      disposition: disposition === 'all' || disposition === 'unsifted' ? undefined : disposition,
      unclassifiedOnly: disposition === 'unsifted' || undefined,
      stratum: stratum || undefined,
      label: label || undefined,
      from: from || undefined,
      since: since ? new Date(since).toISOString() : undefined,
      until: until ? new Date(`${until}T23:59:59`).toISOString() : undefined,
    }),
    [disposition, stratum, label, from, since, until],
  )
  const q = usePagedMessages({ digId, filter, query: query || null })
  const items = q.items
  const list = useListNav(items, { onOpen: (m) => nav(`/inbox/${m.digId}/${m.id}`) })
  const targets = (): MessageSummary[] => (list.selected.size ? items.filter((m) => list.selected.has(m.id)) : list.current ? [list.current] : [])
  const byDig = (ms: MessageSummary[]) => {
    const g = new Map<string, string[]>()
    for (const m of ms) g.set(m.digId, [...(g.get(m.digId) ?? []), m.id])
    return [...g.entries()]
  }
  const bulk = async (action: string, extra: Record<string, unknown> = {}) => {
    const ms = targets()
    if (!ms.length) return
    for (const [d, ids] of byDig(ms)) await run(action, { digId: d, messageIds: ids, ...extra }, { digId: d })
    list.clear()
  }
  const [strataOpen, setStrataOpen] = useState(false)
  useShortcuts('messages', {
    archive: () => bulk('archive_messages'),
    spoil: () => bulk('move_to_spoil'),
    contamination: () => bulk('mark_contamination'),
    label: () => setStrataOpen(true),
    snooze_message: () => bulk('snooze_messages', { until: new Date(Date.now() + 86_400_000).toISOString() }),
    reply: () => list.current && nav(`/inbox/${list.current.digId}/${list.current.id}?reply=1`),
    forward: () => list.current && nav(`/inbox/${list.current.digId}/${list.current.id}?forward=1`),
    why_message: () => list.current && nav(`/inbox/${list.current.digId}/${list.current.id}#why`),
  })
  const count = list.selected.size
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={query ? `Search: ${query}` : shadow ? 'Shadow proposals' : 'Inbox'}
        subtitle={q.total !== null ? `${num(q.total)} messages · ${digName(digId)}${shadow ? ' · what Riddle would have done; nothing applied' : ''}` : 'Loading…'}
        actions={
          count || list.current ? (
            <>
              <span className="text-xs text-clay">{count ? `${count} selected` : 'current row'}</span>
              <Button size="sm" icon={<Archive />} onClick={() => bulk('archive_messages')} title="Archive (e)">
                Archive
              </Button>
              <Button size="sm" icon={<Trash2 />} onClick={() => bulk('move_to_spoil')} title="Spoil (#)">
                Spoil
              </Button>
              <Button size="sm" icon={<Layers />} onClick={() => setStrataOpen(true)} title="Add Stratum (l)">
                Stratum
              </Button>
              <SnoozeMenu onPick={(until) => bulk('snooze_messages', { until })} />
              <Button size="sm" variant="danger" onClick={() => bulk('mark_contamination')} title="Contamination (!)">
                Contamination
              </Button>
            </>
          ) : null
        }
      >
        <div className="relative">
          <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-clay" />
          <Input defaultValue={query} key={query} placeholder="Search (from:, is:, stratum:, label:)" aria-label="Search messages" className="h-6 w-64 pl-7 text-xs" onKeyDown={(e) => e.key === 'Enter' && set('q', e.currentTarget.value)} />
        </div>
        <Select label="Disposition" size="sm" value={disposition} onChange={(v) => set('disposition', v === 'all' ? '' : v)} options={DISPOSITIONS} />
        <Select label="Stratum" size="sm" value={stratum || 'any'} onChange={(v) => set('stratum', v === 'any' ? '' : v)} options={[{ value: 'any', label: 'Any Stratum' }, ...(strata.data ?? []).map((s) => ({ value: s.name, label: s.name }))]} />
        <Select label="Label" size="sm" value={label || 'any'} onChange={(v) => set('label', v === 'any' ? '' : v)} options={[{ value: 'any', label: 'Any label' }, { value: 'INBOX', label: 'INBOX' }, { value: 'SENT', label: 'SENT' }, { value: 'IMPORTANT', label: 'IMPORTANT' }, { value: 'SNOOZED', label: 'SNOOZED' }, { value: 'CATEGORY_PROMOTIONS', label: 'Promotions' }, { value: 'CATEGORY_UPDATES', label: 'Updates' }, { value: 'CATEGORY_PERSONAL', label: 'Personal' }]} />
        <Input defaultValue={from} key={`from-${from}`} placeholder="Sender" aria-label="Filter by sender" className="h-6 w-36 text-xs" onKeyDown={(e) => e.key === 'Enter' && set('from', e.currentTarget.value)} />
        <Input type="date" value={since} aria-label="Since" className="h-6 w-36 text-xs" onChange={(e) => set('since', e.target.value)} />
        <Input type="date" value={until} aria-label="Until" className="h-6 w-36 text-xs" onChange={(e) => set('until', e.target.value)} />
        {query || stratum || label || from || since || until || disposition !== 'all' ? (
          <Button size="sm" variant="ghost" onClick={() => setParams(new URLSearchParams(), { replace: true })}>
            Clear filters
          </Button>
        ) : null}
      </PageHeader>
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
        shadow={shadow}
        emptyState={
          <EmptyState
            icon={<Inbox />}
            title={query ? 'No messages match.' : 'No messages here.'}
            body={query ? 'Try a sender, a subject word, or from:, is:, stratum: and label: operators. Or ask the Director a question.' : digId ? 'This Dig has no messages matching the filters. If it was just connected, the Task Centre shows the initial sync.' : 'Connect a Dig in Settings to start the excavation.'}
            actions={<Button size="sm" onClick={() => nav('/settings#digs')}>Open Settings</Button>}
          />
        }
      />
      <StrataPicker
        open={strataOpen}
        onClose={() => setStrataOpen(false)}
        onPick={(names) => bulk('label_messages', { strata: names })}
      />
    </div>
  )
}

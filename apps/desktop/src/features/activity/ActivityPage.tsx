/**
 * Activity (README "Activity & Provenance", "Undo"): filterable by Dig,
 * actor, action and time; each entry rendered with its provenance line,
 * note, and View Messages · View Reasoning · Edit Mesh · Undo / Undo All.
 * Undone entries say so; partial results list what was skipped.
 */
import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useInfiniteQuery } from '@tanstack/react-query'
import { Activity as ActivityIcon, Undo2 } from 'lucide-react'
import type { Activity } from '@/bindings/Activity'
import type { Actor } from '@/bindings/Actor'
import { listActivity, page, qk } from '@/lib/api'
import { cx, fmtClock, fmtDateTime, fmtTime } from '@/lib/format'
import { useDigContext, useActionsMeta } from '@/lib/queries'
import { actorLabel, canUndo, isBulk, provenanceLine, skippedLines, skippedSummary } from '@/lib/selectors'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { EmptyState } from '@/components/ui/EmptyState'
import { Select } from '@/components/ui/Select'
import { Input } from '@/components/ui/Field'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'

const ACTORS: { value: Actor | 'all'; label: string }[] = [
  { value: 'all', label: 'Any actor' },
  { value: 'user', label: 'You' },
  { value: 'director', label: 'Director' },
  { value: 'sift', label: 'Sift' },
  { value: 'mesh', label: 'Mesh' },
  { value: 'automation', label: 'Automation' },
  { value: 'system', label: 'System' },
]

export default function ActivityPage() {
  const { digId, digName, digs } = useDigContext()
  const [actor, setActor] = useState<Actor | 'all'>('all')
  const [action, setAction] = useState('all')
  const [since, setSince] = useState('')
  const [until, setUntil] = useState('')
  const [digFilter, setDigFilter] = useState<string>('ctx')
  const metas = useActionsMeta()
  const effectiveDig = digFilter === 'ctx' ? digId : digFilter === 'all' ? null : digFilter
  const q = useInfiniteQuery({
    queryKey: [...qk.activity, effectiveDig, actor, action, since, until],
    queryFn: ({ pageParam }) => listActivity({ digId: effectiveDig, actor: actor === 'all' ? null : actor, action: action === 'all' ? null : action, since: since ? new Date(since).toISOString() : null, until: until ? new Date(`${until}T23:59:59`).toISOString() : null, page: page(pageParam, 40) }),
    initialPageParam: null as string | null,
    getNextPageParam: (l) => l.nextCursor,
  })
  const items = useMemo(() => q.data?.pages.flatMap((p) => p.items) ?? [], [q.data])
  const actionNames = useMemo(() => [...new Set([...(metas.data?.map((m) => m.name) ?? []), ...items.map((a) => a.action)])].sort(), [metas.data, items])
  const byDay = useMemo(() => {
    const g = new Map<string, Activity[]>()
    for (const a of items) {
      const d = new Date(a.createdAt).toDateString()
      g.set(d, [...(g.get(d) ?? []), a])
    }
    return [...g.entries()]
  }, [items])
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Activity" subtitle="What happened, who did it, why, and whether it can be undone. Product Activity, not software logs.">
        <Select label="Dig" size="sm" value={digFilter} onChange={setDigFilter} options={[{ value: 'ctx', label: `Current: ${digName(digId)}` }, { value: 'all', label: 'All Digs' }, ...digs.map((d) => ({ value: d.id, label: d.name }))]} />
        <Select label="Actor" size="sm" value={actor} onChange={(v) => setActor(v as Actor | 'all')} options={ACTORS} />
        <Select label="Action" size="sm" value={action} onChange={setAction} options={[{ value: 'all', label: 'Any action' }, ...actionNames.map((n) => ({ value: n, label: n }))]} />
        <Input type="date" value={since} onChange={(e) => setSince(e.target.value)} aria-label="Since" className="h-6 w-36 text-xs" />
        <Input type="date" value={until} onChange={(e) => setUntil(e.target.value)} aria-label="Until" className="h-6 w-36 text-xs" />
      </PageHeader>
      <div className="min-h-0 flex-1 overflow-auto px-5 py-3">
        {q.data && !items.length ? <EmptyState icon={<ActivityIcon />} title="No Activity matches." body="Every meaningful mutation, by you, the Director, the Sift, a Mesh or automation, leaves an entry here. Loosen the filters or wait for the Sift." /> : null}
        {byDay.map(([day, entries]) => (
          <section key={day} className="mb-4">
            <h2 className="sticky top-0 z-10 bg-paper py-1 text-[11px] font-semibold uppercase tracking-wide text-clay">{day === new Date().toDateString() ? 'Today' : day}</h2>
            <ol className="flex flex-col gap-2">
              {entries.map((a) => (
                <ActivityEntry key={a.id} a={a} digName={digName} />
              ))}
            </ol>
          </section>
        ))}
        {q.hasNextPage ? (
          <div className="py-3 text-center">
            <Button size="sm" variant="ghost" busy={q.isFetchingNextPage} onClick={() => void q.fetchNextPage()}>
              Load more
            </Button>
          </div>
        ) : null}
      </div>
    </div>
  )
}

export function ActivityEntry({ a, digName, compact }: { a: Activity; digName: (id: string | null) => string; compact?: boolean }) {
  const nav = useNavigate()
  const { run } = useAction()
  const prov = provenanceLine(a)
  const undoable = canUndo(a)
  const bulk = isBulk(a)
  const messages = a.affected.filter((e) => e.kind === 'message')
  const meshRef = a.affected.find((e) => e.kind === 'mesh') ?? (a.provenance?.meshId ? { kind: 'mesh', id: a.provenance.meshId } : null)
  const undone = a.undo.state === 'undone'
  const failed = a.result.status === 'failed'
  return (
    <li className={cx('rounded-md border px-3 py-2 text-sm', undone ? 'border-line/60 bg-sand/40 text-ink-2' : failed ? 'border-rust/40' : 'border-line', compact && 'py-1.5')}>
      <div className="flex items-start gap-3">
        <span className="w-16 shrink-0 pt-0.5 text-xs text-clay" title={fmtDateTime(a.createdAt)}>
          {compact ? fmtTime(a.createdAt) : fmtClock(a.createdAt)}
        </span>
        <div className="min-w-0 flex-1">
          <div className={cx('font-semibold', undone && 'line-through decoration-clay')}>{a.summary}</div>
          {prov ? (
            <div className="mt-0.5 text-ink-2">
              {prov.startsWith('Triggered by Mesh') && a.provenance?.meshName ? (
                <>
                  Triggered by <button type="button" className="font-medium text-ink hover:underline" onClick={() => nav(`/meshes/${a.provenance!.meshId}`)}>Mesh: {a.provenance.meshName}</button>
                  {a.provenance.meshVersion !== null ? ` v${a.provenance.meshVersion}` : ''}
                </>
              ) : (
                prov
              )}
            </div>
          ) : null}
          {a.why ? <div className="mt-0.5 text-ink-2">{a.why}</div> : null}
          {a.provenance?.note ? <div className="mt-0.5 text-ink-2 whitespace-pre-line">{a.provenance.note}</div> : null}
          {failed && a.result.status === 'failed' ? <div className="mt-0.5 text-rust">Failed: {a.result.error}</div> : null}
          {a.result.status === 'partial' ? (
            <details className="mt-1 text-xs">
              <summary className="cursor-default text-ochre">{skippedSummary(a)}</summary>
              <ul className="mt-1 list-disc pl-5 text-ink-2">
                {skippedLines(a).map((l) => (
                  <li key={l}>{l}</li>
                ))}
              </ul>
            </details>
          ) : null}
          {undone && a.undo.state === 'undone' ? (
            <div className="mt-1 text-xs">
              <Chip tone="neutral">Undone</Chip> <span className="text-clay">by {actorLabel(a.undo.by).toLowerCase()} at {fmtTime(a.undo.at)}</span>
            </div>
          ) : null}
          {a.undo.state === 'not_undoable' && !compact ? <div className="mt-1 text-xs text-clay">Cannot undo: {a.undo.reason}.</div> : null}
          {!compact ? (
            <div className="mt-1.5 flex flex-wrap items-center gap-1">
              <Chip tone="neutral">{actorLabel(a.actor)}</Chip>
              {a.digId ? <span className="text-xs text-clay">{digName(a.digId)}</span> : null}
              <span className="mx-1 text-clay">·</span>
              {messages.length ? (
                <Button size="sm" variant="ghost" onClick={() => nav(messages.length === 1 && a.digId ? `/inbox/${a.digId}/${messages[0].id}` : `/inbox${a.action === 'move_to_spoil' ? '?disposition=spoil' : ''}`)}>
                  View Messages{messages.length > 1 ? ` (${messages.length})` : ''}
                </Button>
              ) : null}
              {a.traceId && a.digId ? (
                <Button size="sm" variant="ghost" onClick={() => nav(`/developer/messages/${a.digId}/${a.traceId}`)}>
                  View Reasoning
                </Button>
              ) : null}
              {meshRef ? (
                <Button size="sm" variant="ghost" onClick={() => nav(`/meshes/${meshRef.id}`)}>
                  Edit Mesh
                </Button>
              ) : null}
              {a.affected.some((e) => e.kind === 'find') ? (
                <Button size="sm" variant="ghost" onClick={() => nav(`/finds/${a.affected.find((e) => e.kind === 'find')!.id}`)}>
                  View Find
                </Button>
              ) : null}
              {undoable ? (
                <Button size="sm" variant="secondary" icon={<Undo2 />} onClick={() => run('undo_activity', { activityId: a.id }, { successToast: bulk ? 'Undone. Skipped what you handled by hand since.' : 'Undone' })} title={a.undo.state === 'undoable' ? a.undo.label : undefined}>
                  {bulk ? 'Undo All' : 'Undo'}
                </Button>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
    </li>
  )
}

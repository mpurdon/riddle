/**
 * Finds (README "Finds"): grouped by type with priority, sources and due
 * dates; j/k/enter/e/s/d/r/u from the keyboard; a detail pane with the
 * source messages and the Why? explanation inline. Every mutation goes
 * through useAction with an optimistic update and an Undo toast.
 */
import { useEffect, useMemo, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Clock, HelpCircle, Lightbulb, MessageSquare, Tag, XCircle } from 'lucide-react'
import type { Find } from '@/bindings/Find'
import type { FindType } from '@/bindings/FindType'
import type { FindStatus } from '@/bindings/FindStatus'
import type { Page } from '@/bindings/Page'
import { explain, getThread, listFinds, page, qk } from '@/lib/api'
import { cx, fmtDue, fmtRelative, fmtTime, num } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { findTypeLabel, FIND_TYPES } from '@/lib/selectors'
import { useDirector } from '@/lib/stores'
import { useAction } from '@/lib/useAction'
import { useListNav } from '@/lib/useListNav'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { EmptyState } from '@/components/ui/EmptyState'
import { Select } from '@/components/ui/Select'
import { ErrorCard } from '@/components/ErrorCard'
import { DevOverlay, ExplanationCard, PageHeader, RetypeMenu, SnoozeMenu, snoozeOptions as snoozeOptionsList } from '@/components/Bits'
import { MessageBody } from '@/components/MessageBody'
import { useComposerActions } from '@/features/messages/composer'

const STATUS_OPTIONS: { value: FindStatus | 'all'; label: string }[] = [
  { value: 'open', label: 'Open' },
  { value: 'snoozed', label: 'Snoozed' },
  { value: 'resolved', label: 'Resolved' },
  { value: 'dismissed', label: 'Dismissed' },
  { value: 'expired', label: 'Expired' },
  { value: 'all', label: 'All' },
]

export default function FindsPage() {
  const { findId } = useParams()
  const nav = useNavigate()
  const qc = useQueryClient()
  const { digId } = useDigContext()
  const [status, setStatus] = useState<FindStatus | 'all'>('open')
  const [type, setType] = useState<FindType | 'all'>('all')
  const { run } = useAction()

  const q = useQuery({
    queryKey: [...qk.finds, status, type, digId],
    queryFn: () => listFinds({ status: status === 'all' ? undefined : status, findType: type === 'all' ? undefined : type, digId, page: page(null, 200) }),
  })
  const finds = useMemo(() => q.data?.items ?? [], [q.data])
  const grouped = useMemo(() => {
    const g = new Map<FindType, Find[]>()
    for (const t of FIND_TYPES) g.set(t, [])
    for (const f of finds) g.get(f.findType)!.push(f)
    return [...g.entries()].filter(([, v]) => v.length)
  }, [finds])
  const ordered = useMemo(() => grouped.flatMap(([, v]) => v), [grouped])
  const nav_ = useListNav(ordered, { onOpen: (f) => nav(`/finds/${f.id}`), multi: false })
  const current = findId ? ordered.find((f) => f.id === findId) ?? null : nav_.current
  useEffect(() => {
    if (findId) {
      const i = ordered.findIndex((f) => f.id === findId)
      if (i >= 0 && i !== nav_.cursor) nav_.setCursor(i)
    }
  }, [findId, ordered]) // eslint-disable-line react-hooks/exhaustive-deps

  const optimistic = (id: string, patch: Partial<Find>) => {
    qc.setQueriesData<Page<Find>>({ queryKey: qk.finds }, (old) => (old ? { ...old, items: old.items.map((f) => (f.id === id ? { ...f, ...patch } : f)) } : old))
  }
  const act = async (f: Find, action: 'resolve_find' | 'dismiss_find' | 'snooze_find' | 'retype_find', extra: Record<string, unknown> = {}, patch: Partial<Find> = {}) => {
    optimistic(f.id, patch)
    try {
      await run(action, { findId: f.id, ...extra }, { digId: f.digIds[0] ?? null })
    } catch {
      void qc.invalidateQueries({ queryKey: qk.finds })
    }
  }
  const [snoozeOpen, setSnoozeOpen] = useState(false)
  const [retypeOpen, setRetypeOpen] = useState(false)
  const [whyOpen, setWhyOpen] = useState(true)
  useShortcuts('finds', {
    resolve: () => current && act(current, 'resolve_find', {}, { status: 'resolved' }),
    dismiss: () => current && act(current, 'dismiss_find', {}, { status: 'dismissed' }),
    snooze: () => current && setSnoozeOpen(true),
    retype: () => current && setRetypeOpen(true),
    why: () => setWhyOpen((v) => !v),
  })

  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="flex min-h-0 flex-1">
      <section className="flex w-[420px] shrink-0 flex-col border-r border-line">
        <PageHeader title="Finds" subtitle={q.data ? `${num(q.data.total)} ${status === 'all' ? '' : status}` : 'Loading…'}>
          <Select label="Status" size="sm" value={status} onChange={(v) => setStatus(v as FindStatus | 'all')} options={STATUS_OPTIONS} />
          <Select label="Type" size="sm" value={type} onChange={(v) => setType(v as FindType | 'all')} options={[{ value: 'all', label: 'All types' }, ...FIND_TYPES.map((t) => ({ value: t, label: findTypeLabel(t) }))]} />
        </PageHeader>
        <div className="min-h-0 flex-1 overflow-auto" role="listbox" aria-label="Finds">
          {q.data && !ordered.length ? (
            <EmptyState
              icon={<Lightbulb />}
              title={status === 'open' ? 'No open Finds.' : `No ${status === 'all' ? '' : status} Finds.`}
              body={status === 'open' ? 'When the Sift surfaces something that deserves attention it appears here, with a Why? you can inspect. Corrections in Unresolved are the fastest way to teach Riddle what counts.' : 'Change the status filter to see other Finds.'}
              actions={<Button size="sm" onClick={() => nav('/unresolved')}>Open Unresolved</Button>}
            />
          ) : null}
          {grouped.map(([t, items]) => (
            <div key={t}>
              <div className="sticky top-0 z-10 flex items-center justify-between bg-sand px-4 py-1 text-[11px] font-semibold uppercase tracking-wide text-clay">
                <span>{findTypeLabel(t)}</span>
                <span>{items.length}</span>
              </div>
              {items.map((f) => {
                const idx = ordered.indexOf(f)
                const active = current?.id === f.id
                const due = fmtDue(f.dueAt)
                return (
                  <div
                    key={f.id}
                    role="option"
                    aria-selected={active}
                    data-row-index={idx}
                    tabIndex={-1}
                    onClick={() => {
                      nav_.setCursor(idx)
                      nav(`/finds/${f.id}`)
                    }}
                    className={cx('cursor-default border-b border-line/60 px-4 py-2', active ? 'row-selected' : 'hover:bg-sand/60')}
                  >
                    <div className="flex items-start gap-2">
                      <div className="min-w-0 flex-1">
                        <div className={cx('truncate text-sm', f.status === 'open' ? 'font-medium' : 'text-ink-2')}>{f.title}</div>
                        <div className="mt-0.5 truncate text-xs text-ink-2">{f.summary}</div>
                        <div className="mt-1 flex flex-wrap items-center gap-1.5 text-[11px] text-clay">
                          <span title="Priority">P{f.priority}</span>
                          <span>· {f.sources.length} source{f.sources.length === 1 ? '' : 's'}</span>
                          {due ? <span className={cx(f.dueAt && new Date(f.dueAt) < new Date() ? 'text-rust' : 'text-ochre')}>· {due}</span> : null}
                          {f.status !== 'open' ? <Chip tone={f.status === 'resolved' ? 'ok' : 'neutral'}>{f.status}</Chip> : null}
                          {f.snoozedUntil && f.status === 'snoozed' ? <span>until {fmtTime(f.snoozedUntil)}</span> : null}
                        </div>
                      </div>
                      <div className="mt-1 h-1.5 w-8 shrink-0 rounded bg-sand-2" title={`Priority ${f.priority}`}>
                        <div className="h-full rounded bg-ochre" style={{ width: `${f.priority}%` }} />
                      </div>
                    </div>
                  </div>
                )
              })}
            </div>
          ))}
        </div>
      </section>
      <section className="flex min-w-0 flex-1 flex-col">
        {current ? (
          <FindDetail
            find={current}
            whyOpen={whyOpen}
            onResolve={() => act(current, 'resolve_find', {}, { status: 'resolved' })}
            onDismiss={() => act(current, 'dismiss_find', {}, { status: 'dismissed' })}
            onSnooze={(until) => act(current, 'snooze_find', { until }, { status: 'snoozed', snoozedUntil: until })}
            onRetype={(t) => act(current, 'retype_find', { findType: t }, { findType: t })}
            snoozeOpen={snoozeOpen}
            setSnoozeOpen={setSnoozeOpen}
            retypeOpen={retypeOpen}
            setRetypeOpen={setRetypeOpen}
          />
        ) : (
          <EmptyState title="Select a Find" body="j and k move through the list; Enter opens; e resolves, s snoozes, d dismisses, r retypes, w toggles Why?." />
        )}
      </section>
    </div>
  )
}

function FindDetail({ find, whyOpen, onResolve, onDismiss, onSnooze, onRetype, snoozeOpen, setSnoozeOpen, retypeOpen, setRetypeOpen }: { find: Find; whyOpen: boolean; onResolve: () => void; onDismiss: () => void; onSnooze: (until: string) => void; onRetype: (t: FindType) => void; snoozeOpen: boolean; setSnoozeOpen: (v: boolean) => void; retypeOpen: boolean; setRetypeOpen: (v: boolean) => void }) {
  const nav = useNavigate()
  const { digName } = useDigContext()
  const setPrefill = useDirector((s) => s.setPrefill)
  const { reply } = useComposerActions()
  const first = find.sources[0]
  const ex = useQuery({ queryKey: first ? qk.explain(first.digId, first.messageId) : ['explain', 'none'], queryFn: () => explain(first!.digId, first!.messageId), enabled: !!first && whyOpen })
  const thread = useQuery({ queryKey: first ? qk.thread(first.digId, first.threadId) : ['thread', 'none'], queryFn: () => getThread(first!.digId, first!.threadId), enabled: !!first })
  const due = fmtDue(find.dueAt)
  const open = find.status === 'open' || find.status === 'snoozed'
  const messages = thread.data?.messages ?? []
  const [expanded, setExpanded] = useState<string | null>(null)
  useEffect(() => setExpanded(messages.at(-1)?.id ?? null), [thread.data]) // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={find.title}
        subtitle={
          <span className="flex flex-wrap items-center gap-2">
            <Chip tone="ok">{findTypeLabel(find.findType)}</Chip>
            <span>P{find.priority}</span>
            <span>{find.digIds.map(digName).join(', ')}</span>
            <span>· {fmtRelative(find.createdAt)}</span>
            {due ? <span className="text-ochre">· {due}</span> : null}
            {find.status !== 'open' ? <Chip>{find.status}{find.resolvedBy ? ` by ${find.resolvedBy.replace('_', ' ')}` : ''}</Chip> : null}
            <DevOverlay digId={find.digIds[0] ?? ''} traceId={find.traceId} confidence={find.confidence} totalMs={null} />
          </span>
        }
        actions={
          <>
            <Button size="sm" icon={<Check />} onClick={onResolve} disabledReason={open ? null : `Already ${find.status}`} title="Resolve (e)">
              Resolve
            </Button>
            {open ? (
              <SnoozeMenu onPick={onSnooze} trigger={<Button size="sm" icon={<Clock />} title="Snooze (s)">Snooze</Button>} />
            ) : null}
            <RetypeMenu current={find.findType} onPick={onRetype} trigger={<Button size="sm" icon={<Tag />} title="Retype (r)">Retype</Button>} />
            <Button size="sm" icon={<XCircle />} onClick={onDismiss} disabledReason={open ? null : `Already ${find.status}`} title="Dismiss (d)">
              Not important
            </Button>
          </>
        }
      />
      {snoozeOpen ? <SnoozeOverlay onPick={(u) => { onSnooze(u); setSnoozeOpen(false) }} onClose={() => setSnoozeOpen(false)} /> : null}
      {retypeOpen ? <RetypeOverlay current={find.findType} onPick={(t) => { onRetype(t); setRetypeOpen(false) }} onClose={() => setRetypeOpen(false)} /> : null}
      <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
        <p className="text-sm">{find.summary}</p>
        <div className="mt-3 flex flex-wrap gap-2">
          <Button size="sm" variant="ghost" icon={<HelpCircle />} onClick={() => { setPrefill(`Why did you think "${find.title}" was a Find?`); nav('/director?send=1') }}>
            Why did you think this was a Find?
          </Button>
          <Button size="sm" variant="ghost" icon={<MessageSquare />} disabledReason={messages.length ? null : 'Loading the thread'} onClick={() => messages.length && reply(messages.at(-1)!)}>
            Reply
          </Button>
        </div>
        {whyOpen && ex.data ? (
          <div className="mt-4">
            <ExplanationCard ex={ex.data} digId={first!.digId} onNotImportant={open ? onDismiss : undefined} />
          </div>
        ) : null}
        {whyOpen && ex.error ? <div className="mt-4"><ErrorCard error={ex.error} compact /></div> : null}
        <h2 className="mt-6 text-xs font-semibold uppercase tracking-wide text-clay">
          Sources · {find.sources.length} message{find.sources.length === 1 ? '' : 's'}
        </h2>
        {thread.error ? <ErrorCard error={thread.error} compact /> : null}
        <div className="mt-2 flex flex-col gap-2">
          {messages.map((m) => {
            const isSource = find.sources.some((s) => s.messageId === m.id)
            const isOpen = expanded === m.id
            return (
              <div key={m.id} className={cx('rounded-md border', isSource ? 'border-ochre/40' : 'border-line')}>
                <button type="button" className="flex w-full items-center gap-3 px-3 py-2 text-left" onClick={() => setExpanded(isOpen ? null : m.id)} aria-expanded={isOpen}>
                  <span className="w-40 truncate text-sm font-medium">{m.from.name ?? m.from.email}</span>
                  <span className="min-w-0 flex-1 truncate text-sm text-ink-2">{isOpen ? m.subject : m.snippet}</span>
                  <span className="shrink-0 text-xs text-clay">{fmtTime(m.date)}</span>
                  {isSource ? <Chip tone="warn">source</Chip> : null}
                </button>
                {isOpen ? (
                  <div className="border-t border-line px-3 py-3">
                    <MessageBody html={m.bodyHtml} text={m.bodyText} />
                    <div className="mt-2 flex gap-2">
                      <Button size="sm" variant="ghost" onClick={() => nav(`/inbox/${m.digId}/${m.id}`)}>
                        Open message
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => reply(m)}>
                        Reply
                      </Button>
                    </div>
                  </div>
                ) : null}
              </div>
            )
          })}
        </div>
      </div>
    </div>
  )
}

/** Keyboard-triggered snooze picker: a small overlay with the same options as the menu. */
function SnoozeOverlay({ onPick, onClose }: { onPick: (until: string) => void; onClose: () => void }) {
  return (
    <OverlayList title="Snooze until" onClose={onClose} items={snoozeOptionsList().map((o) => ({ label: o.label, onSelect: () => onPick(o.until) }))} />
  )
}
function RetypeOverlay({ current, onPick, onClose }: { current: FindType; onPick: (t: FindType) => void; onClose: () => void }) {
  return <OverlayList title="Change type to" onClose={onClose} items={FIND_TYPES.filter((t) => t !== current).map((t) => ({ label: findTypeLabel(t), onSelect: () => onPick(t) }))} />
}
function OverlayList({ title, items, onClose }: { title: string; items: { label: string; onSelect: () => void }[]; onClose: () => void }) {
  const [i, setI] = useState(0)
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
      else if (e.key === 'j' || e.key === 'ArrowDown') setI((x) => Math.min(items.length - 1, x + 1))
      else if (e.key === 'k' || e.key === 'ArrowUp') setI((x) => Math.max(0, x - 1))
      else if (e.key === 'Enter') items[i]?.onSelect()
      else if (/^[1-9]$/.test(e.key)) items[Number(e.key) - 1]?.onSelect()
      else return
      e.preventDefault()
      e.stopPropagation()
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [i, items, onClose])
  return (
    <div className="absolute left-1/2 top-24 z-30 w-72 -translate-x-1/2 rounded-md border border-line bg-paper p-2 shadow-xl fade-in" role="dialog" aria-label={title}>
      <div className="px-2 py-1 text-xs font-semibold text-clay">{title}</div>
      {items.map((it, idx) => (
        <button key={it.label} type="button" className={cx('flex w-full items-center justify-between rounded px-2 py-1 text-left text-sm', idx === i ? 'bg-sand' : 'hover:bg-sand/60')} onClick={it.onSelect}>
          <span>{it.label}</span>
          <kbd>{idx + 1}</kbd>
        </button>
      ))}
    </div>
  )
}

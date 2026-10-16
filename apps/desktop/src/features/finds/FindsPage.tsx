/**
 * Finds (README "Finds"): grouped by type with priority, sources and due
 * dates, in three tabs (Needs you, Later, Done); j/k/x/enter/e/s/d/r/w from
 * the keyboard; a detail pane with the source messages and the Why?
 * explanation inline. Every mutation goes through useAction with an
 * optimistic update and an Undo toast; a bulk run gets one toast.
 *
 * Two decisions come from the UX study. Rows lead with `reason`, the one
 * sentence the backend wrote for a person, which the page had never
 * rendered. And the cursor, not the URL, is the source of truth for what
 * is shown: with `/finds/:id` in charge, resolving that Find left j/k
 * moving an invisible cursor while the pane kept showing a Find that had
 * gone. The URL now follows the cursor and is replaced when its Find leaves.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useLocation, useNavigate, useParams } from 'react-router-dom'
import { type InfiniteData, useQuery, useQueryClient } from '@tanstack/react-query'
import { Check, Clock, HelpCircle, Lightbulb, RefreshCw, Tag, X, XCircle } from 'lucide-react'
import type { Find } from '@/bindings/Find'
import type { FindType } from '@/bindings/FindType'
import { explain, getFind, getThread, qk } from '@/lib/api'
import { cx, fmtDue, fmtRelative, fmtTime, num, plural } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { findTypeLabel, FIND_TYPES, importanceOfPriority } from '@/lib/selectors'
import { keyHint } from '@/lib/shortcuts'
import { useListNav } from '@/lib/useListNav'
import { useAction } from '@/lib/useAction'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { EmptyState } from '@/components/ui/EmptyState'
import { Select } from '@/components/ui/Select'
import { TabPanel, Tabs } from '@/components/ui/Tabs'
import { ActionHelpButton } from '@/components/ActionHelp'
import { ErrorCard } from '@/components/ErrorCard'
import { ThreadRow, senderOf } from '@/components/ThreadRow'
import { buildThreadTree } from '@/lib/threadTree'
import { CopyCode, CopyMessageLink, DevOverlay, ExplanationCard, ImportanceChip, PageHeader, RetypeMenu, SnoozeMenu } from '@/components/Bits'
import { MessageBody } from '@/components/MessageBody'
import { useComposerActions } from '@/features/messages/composer'
import { messagePath, type MessageQueue } from '@/features/messages/queue'
import { bulkOutcome, FINDS_TABS, findStatusLabel, type FindsTab, tabEmptyState, tabForStatus, tabSubtitle, targetLabel } from './model'
import { FindTypeOverlay, SnoozeOverlay } from '@/components/OverlayList'
import { SelectionBar } from './SelectionBar'
import { useBulkRun } from './useBulkRun'
import { usePagedFinds } from './usePagedFinds'
import { Resizable } from '@/components/Resizable'

type FindAction = 'resolve_find' | 'dismiss_find' | 'snooze_find' | 'retype_find'
const PAST: Record<FindAction, string> = { resolve_find: 'Resolved', dismiss_find: 'Marked not important', snooze_find: 'Snoozed', retype_find: 'Retyped' }

const actionable = (fs: Find[]) => fs.filter((f) => f.status === 'open' || f.status === 'snoozed')

/** Opening a source message carries the Find's sources as the queue, so n/p step through them. */
const sourceQueue = (find: Find, index: number): MessageQueue => ({ queue: find.sources.map((s) => ({ digId: s.digId, messageId: s.messageId })), index })

export default function FindsPage() {
  const { findId } = useParams()
  const nav = useNavigate()
  const qc = useQueryClient()
  const { digId } = useDigContext()
  const [tab, setTab] = useState<FindsTab>('needs_you')
  const [type, setType] = useState<FindType | 'all'>('all')

  const q = usePagedFinds({ tab, findType: type === 'all' ? null : type, digId })
  const finds = q.items
  const grouped = useMemo(() => {
    const g = new Map<FindType, Find[]>()
    for (const t of FIND_TYPES) g.set(t, [])
    for (const f of finds) g.get(f.findType)!.push(f)
    return [...g.entries()].filter(([, v]) => v.length)
  }, [finds])
  const ordered = useMemo(() => grouped.flatMap(([, v]) => v), [grouped])
  const indexOf = useMemo(() => new Map(ordered.map((f, i) => [f.id, i])), [ordered])
  const location = useLocation()
  // A location this page wrote carries `own`; any other is a link to follow.
  const own = (location.state as { own?: boolean } | null)?.own === true
  const write = useCallback((path: string) => nav(path, { replace: true, state: { own: true } }), [nav])
  const pin = useCallback((f: Find) => write(`/finds/${f.id}`), [write])
  const list = useListNav(ordered, { onOpen: pin })
  const current = list.current
  const { runBulk, bulkError } = useBulkRun(list)

  // A link (the URL at mount; later, a location we did not write, from
  // Activity or the Summary) may point at a Find on another tab: look it up
  // once, follow it to its tab, put the cursor on it.
  const [deepLink, setDeepLink] = useState<string | null>(() => findId ?? null)
  const linked = useQuery({ queryKey: qk.find(deepLink ?? ''), queryFn: () => getFind(deepLink!), enabled: !!deepLink, retry: false })
  useEffect(() => {
    if (!deepLink || linked.isPending) return
    const t = linked.data ? tabForStatus(linked.data.status) : null
    if (t && t !== tab) {
      setTab(t)
      return
    }
    if (q.isPending) return
    const i = indexOf.get(deepLink)
    if (i !== undefined) list.setCursor(i)
    setDeepLink(null)
  }, [deepLink, linked.isPending, linked.data, tab, q.isPending, indexOf]) // eslint-disable-line react-hooks/exhaustive-deps

  // Once the URL names a Find it mirrors the cursor, never the other way
  // round: a click or Enter pins a row, then j/k and a row leaving the list
  // move the cursor and the URL follows. Writing only towards the cursor is
  // idempotent under a stale `findId`, so a transition mid-flight cannot
  // send the pane backwards. A location arriving without `own` is a link:
  // its Find is consumed once and followed above. The router hands out a
  // new location object per navigation (its key does not tell two
  // hash-typed locations apart), so that is what "arriving" compares.
  const seen = useRef(location)
  useEffect(() => {
    if (seen.current !== location) {
      seen.current = location
      if (!own && findId) {
        setDeepLink(findId)
        return
      }
    }
    if (deepLink || q.isPending || !findId) return
    if (!current) write('/finds')
    else if (findId !== current.id) pin(current)
  }, [location, own, findId, deepLink, q.isPending, current, write, pin])

  const selected = useMemo(() => ordered.filter((f) => list.selected.has(f.id)), [ordered, list.selected])
  /** The targets a verb can still change: open or snoozed among the checked rows, else the cursor row. */
  const openTargets = () => actionable(list.targets())

  const optimistic = (ids: Set<string>, patch: Partial<Find>) => {
    qc.setQueriesData<InfiniteData<{ items: Find[] }>>({ queryKey: [...qk.finds, 'paged'] }, (old) =>
      old ? { ...old, pages: old.pages.map((p) => ({ ...p, items: p.items.map((f) => (ids.has(f.id) ? { ...f, ...patch } : f)) })) } : old,
    )
  }
  const act = (fs: Find[], action: FindAction, extra: Record<string, unknown> = {}, patch: Partial<Find> = {}) => {
    if (!fs.length) return
    optimistic(new Set(fs.map((f) => f.id)), patch)
    return runBulk(fs, {
      action,
      args: (f) => ({ findId: f.id, ...extra }),
      digId: (f) => f.digIds[0] ?? null,
      outcome: (done, failed) => bulkOutcome(PAST[action], 'Find', done, failed),
      failure: (failed, total) => `${failed} of ${total} could not be changed; they are still as they were.`,
      // A failed item keeps its optimistic patch until the list is read again.
      onFailure: () => void qc.invalidateQueries({ queryKey: qk.finds }),
    })
  }
  const resolve = () => act(openTargets(), 'resolve_find', {}, { status: 'resolved', resolvedBy: 'user' })
  const dismiss = () => act(openTargets(), 'dismiss_find', {}, { status: 'dismissed', resolvedBy: 'user' })
  const snooze = (until: string) => act(openTargets(), 'snooze_find', { until }, { status: 'snoozed', snoozedUntil: until })
  const retype = (t: FindType) => current && act([current], 'retype_find', { findType: t }, { findType: t })

  const [snoozeOpen, setSnoozeOpen] = useState(false)
  const [retypeOpen, setRetypeOpen] = useState(false)
  const [whyOpen, setWhyOpen] = useState(false)
  useShortcuts('finds', {
    resolve,
    dismiss,
    snooze: () => openTargets().length && setSnoozeOpen(true),
    retype: () => current && setRetypeOpen(true),
    why: () => setWhyOpen((v) => !v),
  })

  const count = selected.length
  const selectedOpen = actionable(selected).length
  const empty = tabEmptyState(tab, type !== 'all')
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="flex min-h-0 flex-1">
      <Resizable as="section" id="finds" side="left" initial={420} min={300} max={760} label="Resize the Finds list" className="flex flex-col border-r border-line">
        <PageHeader title="Finds" subtitle={q.total !== null ? tabSubtitle(tab, num(q.total)) : 'Loading…'}>
          <Select label="Type" size="sm" value={type} onChange={(v) => setType(v as FindType | 'all')} options={[{ value: 'all', label: 'All types' }, ...FIND_TYPES.map((t) => ({ value: t, label: findTypeLabel(t) }))]} />
        </PageHeader>
        <Tabs value={tab} onChange={(v) => setTab(v as FindsTab)} tabs={FINDS_TABS.map((t) => ({ value: t.value, label: t.label }))} className="min-h-0 flex-1">
          <SelectionBar count={count} error={bulkError} onClear={list.clear} />
          <TabPanel value={tab}>
            <div role="listbox" aria-multiselectable aria-label="Finds">
              {!q.isPending && !ordered.length ? (
                <EmptyState icon={<Lightbulb />} title={empty.title} body={empty.body} actions={tab === 'needs_you' && type === 'all' ? <Button size="sm" onClick={() => nav('/unresolved')}>Open Unresolved</Button> : null} />
              ) : null}
              {grouped.map(([t, items]) => (
                <div key={t}>
                  <div className="sticky top-0 z-10 flex items-center justify-between bg-sand px-4 py-1 text-2xs font-semibold uppercase tracking-wide text-clay">
                    <span>{findTypeLabel(t)}</span>
                    <span>{items.length}</span>
                  </div>
                  {items.map((f) => (
                    <FindRow key={f.id} find={f} index={indexOf.get(f.id) ?? 0} active={current?.id === f.id} checked={list.selected.has(f.id)} onCursor={list.setCursor} onToggle={list.toggle} onPin={pin} />
                  ))}
                </div>
              ))}
              {q.hasNextPage ? (
                <div className="flex justify-center py-2">
                  <Button size="sm" variant="ghost" busy={q.isFetchingNextPage} onClick={() => void q.fetchNextPage()} title="Fetch the next page of Finds">
                    Load more
                  </Button>
                </div>
              ) : null}
            </div>
          </TabPanel>
        </Tabs>
      </Resizable>
      <section className="relative flex min-w-0 flex-1 flex-col">
        {current ? (
          <FindDetail find={current} selectedCount={count} selectedOpen={selectedOpen} whyOpen={whyOpen} onWhy={() => setWhyOpen((v) => !v)} onResolve={resolve} onDismiss={dismiss} onSnooze={snooze} onRetype={retype} />
        ) : (
          <EmptyState
            title="Select a Find"
            body={`${keyHint('next')} and ${keyHint('prev')} move; ${keyHint('select')} selects a row and ${keyHint('select_all')} every row. ${keyHint('resolve')} resolves, ${keyHint('snooze')} snoozes, ${keyHint('dismiss')} marks not important, ${keyHint('retype')} changes the type, ${keyHint('why')} shows or hides Why?.`}
          />
        )}
        {snoozeOpen ? (
          <SnoozeOverlay
            onPick={(u) => {
              snooze(u)
              setSnoozeOpen(false)
            }}
            onClose={() => setSnoozeOpen(false)}
          />
        ) : null}
        {retypeOpen && current ? (
          <FindTypeOverlay
            title="Change type to"
            current={current.findType}
            onPick={(t) => {
              retype(t)
              setRetypeOpen(false)
            }}
            onClose={() => setRetypeOpen(false)}
          />
        ) : null}
      </section>
    </div>
  )
}

/** One row: the title first, the reason under it, then type, priority, due and status. */
function FindRow({ find: f, index, active, checked, onCursor, onToggle, onPin }: { find: Find; index: number; active: boolean; checked: boolean; onCursor: (i: number) => void; onToggle: (id: string) => void; onPin: (f: Find) => void }) {
  const due = fmtDue(f.dueAt)
  const overdue = !!f.dueAt && new Date(f.dueAt) < new Date()
  const status = findStatusLabel(f)
  const reason = f.reason.trim()
  return (
    <div
      role="option"
      aria-selected={active}
      data-row-index={index}
      tabIndex={-1}
      onClick={() => {
        onCursor(index)
        onPin(f)
      }}
      className={cx('cursor-default border-b border-line/60 px-3 py-2', active ? 'row-selected' : checked ? 'bg-ochre/5' : 'hover:bg-sand/60')}
    >
      <div className="flex items-start gap-2">
        <input type="checkbox" checked={checked} onChange={() => onToggle(f.id)} onClick={(e) => e.stopPropagation()} aria-label={`Select ${f.title}`} title={`Select or deselect (${keyHint('select')})`} className="mt-1 shrink-0" />
        <div className="min-w-0 flex-1">
          <div className={cx('truncate text-sm selectable', f.status === 'open' ? 'font-medium' : 'text-ink-2')}>{f.title}</div>
          <div className="mt-0.5 line-clamp-2 text-xs text-ink-2 selectable">{reason || f.summary}</div>
          {f.code && f.status === 'open' ? (
            <div className="mt-1">
              <CopyCode code={f.code} />
            </div>
          ) : null}
          <div className="mt-1 flex flex-wrap items-center gap-1.5 text-2xs text-clay">
            <Chip tone="ok">{findTypeLabel(f.findType)}</Chip>
            <ImportanceChip level={importanceOfPriority(f.priority)} />
            <span title="Priority, 0 to 100">P{f.priority}</span>
            {due ? <span className={overdue ? 'text-rust' : 'text-ochre'}>· {due}</span> : null}
            {status ? <Chip tone={f.status === 'resolved' ? 'ok' : 'neutral'}>{status}</Chip> : null}
          </div>
        </div>
        <div className="mt-1 h-1.5 w-8 shrink-0 rounded bg-sand-2" title={`Priority ${f.priority} of 100`} role="img" aria-label={`Priority ${f.priority} of 100`}>
          <div className="h-full rounded bg-ochre" style={{ width: `${f.priority}%` }} />
        </div>
      </div>
    </div>
  )
}

function FindDetail({ find, selectedCount, selectedOpen, whyOpen, onWhy, onResolve, onDismiss, onSnooze, onRetype }: { find: Find; selectedCount: number; selectedOpen: number; whyOpen: boolean; onWhy: () => void; onResolve: () => void; onDismiss: () => void; onSnooze: (until: string) => void; onRetype: (t: FindType) => void }) {
  const nav = useNavigate()
  const { digName } = useDigContext()
  const { reply } = useComposerActions()
  const { run } = useAction()
  const first = find.sources[0]
  const ex = useQuery({ queryKey: first ? qk.explain(first.digId, first.messageId) : ['explain', 'none'], queryFn: () => explain(first!.digId, first!.messageId), enabled: !!first && whyOpen })
  const thread = useQuery({ queryKey: first ? qk.thread(first.digId, first.threadId) : ['thread', 'none'], queryFn: () => getThread(first!.digId, first!.threadId), enabled: !!first })
  const due = fmtDue(find.dueAt)
  const open = find.status === 'open' || find.status === 'snoozed'
  const messages = thread.data?.messages ?? []
  const source = first ? messages.find((m) => m.id === first.messageId) : undefined
  const from = senderOf(source) ?? undefined
  const status = findStatusLabel(find)
  const reason = find.reason.trim()
  const [expanded, setExpanded] = useState<string | null>(null)
  useEffect(() => setExpanded(null), [thread.data])
  // The mail to read: the message that raised the Find, else the latest in the thread.
  const lead = source ?? messages.at(-1)
  // With a selection, the buttons act on it and say so; otherwise on this Find.
  const bulk = selectedCount > 0
  const cannot = bulk ? (selectedOpen ? null : 'None of the selected Finds is open or snoozed') : open ? null : `Already ${status?.toLowerCase() ?? find.status}`
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={<span className="selectable">{find.title}</span>}
        subtitle={
          <span className="flex flex-wrap items-center gap-2">
            <Chip tone="ok">{findTypeLabel(find.findType)}</Chip>
            <span title="Priority, 0 to 100">P{find.priority}</span>
            <span>{find.digIds.map(digName).join(', ')}</span>
            <span>· {fmtRelative(find.createdAt)}</span>
            {first ? <CopyMessageLink digId={first.digId} messageId={first.messageId} about={source ? `${senderOf(source)} · ${source.subject}` : find.title} /> : null}
            {due ? <span className="text-ochre">· {due}</span> : null}
            {status ? <Chip tone={find.status === 'resolved' ? 'ok' : 'neutral'}>{status}</Chip> : null}
            <DevOverlay digId={find.digIds[0] ?? ''} traceId={find.traceId} confidence={find.confidence} totalMs={null} />
          </span>
        }
        actions={
          <>
            <Button size="sm" icon={<Check />} onClick={onResolve} disabledReason={cannot} title={`Mark as dealt with (${keyHint('resolve')}). Nothing changes in Gmail.`}>
              {targetLabel('Resolve', selectedCount)}
            </Button>
            {bulk || open ? <SnoozeMenu onPick={onSnooze} trigger={<Button size="sm" icon={<Clock />} title={`Snooze (${keyHint('snooze')})`}>{targetLabel('Snooze', selectedCount)}</Button>} /> : null}
            <RetypeMenu current={find.findType} onPick={onRetype} trigger={<Button size="sm" icon={<Tag />} title={`Change the type of this Find (${keyHint('retype')})`}>Retype</Button>} />
            <Button size="sm" icon={<XCircle />} onClick={onDismiss} disabledReason={cannot} title={`Close it and tell Riddle it should not have been raised (${keyHint('dismiss')}). Nothing changes in Gmail.`}>
              {targetLabel('Not important', selectedCount)}
            </Button>
            <Button size="sm" variant="ghost" icon={<HelpCircle />} onClick={onWhy} aria-pressed={whyOpen} title={`Open or close the explanation drawer (${keyHint('why')})`}>
              Why?
            </Button>
            <ActionHelpButton group="find" />
          </>
        }
      />
      {/* The mail first, so it can be read; the explanation is a drawer over the right edge, opened on demand. */}
      <div className="relative min-h-0 flex-1 overflow-hidden">
        <div className="h-full overflow-auto px-5 py-4">
        {reason ? <p className="text-sm font-medium selectable">{reason}</p> : null}
        <p className={cx('text-sm selectable', reason && 'mt-1 text-ink-2')}>{find.summary}</p>
        {thread.error ? <ErrorCard error={thread.error} compact /> : null}
        {lead ? (
          <article aria-label="Message" className="mt-4 rounded-md border border-line">
            <div className="flex flex-wrap items-center gap-x-3 border-b border-line px-3 py-2 text-xs text-ink-2">
              <span className="font-medium text-ink selectable">{senderOf(lead)}</span>
              {lead.from.name ? <span className="text-clay selectable">{lead.from.email}</span> : null}
              <span className="ml-auto shrink-0 text-clay">{fmtTime(lead.date)}</span>
            </div>
            <div className="px-3 py-3 selectable">
              <MessageBody html={lead.bodyHtml} text={lead.bodyText} />
            </div>
            <div className="flex gap-2 border-t border-line px-3 py-2">
              <Button size="sm" variant="ghost" onClick={() => reply(lead)} title="Reply to this message">
                Reply
              </Button>
              <Button size="sm" variant="ghost" onClick={() => nav(messagePath({ digId: lead.digId, messageId: lead.id }), { state: source ? sourceQueue(find, 0) : undefined })} title="Open this message on its own page">
                Open message
              </Button>
            </div>
          </article>
        ) : thread.isPending ? (
          <div className="mt-4 text-xs text-clay">Loading the message…</div>
        ) : null}
        {/* The raw thread stays folded: it is for checking what Riddle saw, not for reading the mail. */}
        <details className="mt-4">
          <summary className="cursor-pointer text-xs font-semibold uppercase tracking-wide text-clay hover:text-ink">
            Sources · {plural(find.sources.length, 'message')}{messages.length > find.sources.length ? ` in a thread of ${messages.length}` : ''}
          </summary>
        <div className="mt-2 flex flex-col">
          {buildThreadTree(messages).map((node) => {
            const m = node.message
            const sourceIndex = find.sources.findIndex((s) => s.messageId === m.id)
            const isSource = sourceIndex >= 0
            const isOpen = expanded === m.id
            return (
              <ThreadRow key={m.id} node={node} parentName={node.parentId ? senderOf(messages.find((x) => x.id === node.parentId)) : null}>
                <div className={cx('rounded-md border', isSource ? 'border-ochre/40' : 'border-line')}>
                  <button type="button" className="flex w-full items-center gap-3 px-3 py-2 text-left" onClick={() => setExpanded(isOpen ? null : m.id)} aria-expanded={isOpen} title={isOpen ? 'Collapse this message' : 'Expand this message'}>
                    <span className="w-40 truncate text-sm font-medium selectable">{senderOf(m)}</span>
                    <span className="min-w-0 flex-1 truncate text-sm text-ink-2 selectable">{isOpen ? m.subject : m.snippet}</span>
                    <span className="shrink-0 text-xs text-clay">{fmtTime(m.date)}</span>
                    {isSource ? <Chip tone="warn" title="This message is what raised the Find">source</Chip> : null}
                  </button>
                  {isOpen ? (
                    <div className="border-t border-line px-3 py-3 selectable">
                      <pre className="whitespace-pre-wrap text-xs mono text-ink-2">{m.bodyText || m.snippet}</pre>
                      <div className="mt-2 flex gap-2">
                        <Button size="sm" variant="ghost" onClick={() => nav(messagePath({ digId: m.digId, messageId: m.id }), { state: isSource ? sourceQueue(find, sourceIndex) : undefined })} title="Open this message on its own page">
                          Open message
                        </Button>
                        <Button size="sm" variant="ghost" onClick={() => reply(m)} title="Reply to this message">
                          Reply
                        </Button>
                      </div>
                    </div>
                  ) : null}
                </div>
              </ThreadRow>
            )
          })}
        </div>
        </details>
        </div>
        {whyOpen ? (
          <aside role="complementary" aria-label="Why this Find was raised" className="slide-in absolute inset-y-0 right-0 flex w-[min(32rem,100%)] flex-col border-l border-line bg-paper shadow-lg">
            <div className="flex shrink-0 items-center justify-between gap-2 border-b border-line px-4 py-2">
              <span className="text-sm font-semibold">Why?</span>
              <div className="flex items-center gap-1.5">
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<RefreshCw />}
                  disabledReason={!first ? 'This Find has no source message' : open ? null : 'Only an open or snoozed Find can be decided again'}
                  title="Send this message back to Sift under the current rules. This Find closes as Superseded and comes back if it still is one; undo puts both back."
                  onClick={() => first && run('decide_again', { digId: first.digId, messageIds: [first.messageId] })}
                >
                  Decide again
                </Button>
                <Button size="sm" variant="ghost" icon={<X />} onClick={onWhy} title={`Close the explanation (${keyHint('why')})`}>
                  Close
                </Button>
              </div>
            </div>
            <div className="min-h-0 flex-1 overflow-auto p-4">
              {ex.data && first ? (
                <ExplanationCard ex={ex.data} digId={first.digId} subject={source?.subject ?? find.title} from={from} digName={digName(first.digId)} />
              ) : ex.error ? (
                <ErrorCard error={ex.error} compact />
              ) : first ? (
                <div className="text-xs text-clay">Loading…</div>
              ) : (
                <div className="text-xs text-clay">This Find has no source message to explain.</div>
              )}
            </div>
          </aside>
        ) : null}
      </div>
    </div>
  )
}

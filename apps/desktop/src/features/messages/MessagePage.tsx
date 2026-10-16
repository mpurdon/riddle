/**
 * One message: header, sanitized HTML in a sandboxed iframe (remote images
 * blocked), plain-text fallback, thread, attachments, the Why? card, a
 * Correct menu (set_disposition) and Reply/Forward into the composer. When
 * opened from a list the router state carries that list, so n/p step
 * through it and the header says "3 of 41"; opened from a Find or the
 * Director there is no list and no n/p. The row's Strata and disposition
 * come from a thread-scoped list read, since `get_message` carries neither.
 */
import { useEffect, useMemo, useState } from 'react'
import { useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { ArrowLeft, ChevronDown, ChevronUp, Clock, Flag, Forward, Reply, ReplyAll, ShieldAlert } from 'lucide-react'
import type { Disposition } from '@/bindings/Disposition'
import type { Importance } from '@/bindings/Importance'
import type { Message } from '@/bindings/Message'
import { explain, getMessage, getThread, listMessages, page, qk } from '@/lib/api'
import { buildThreadTree } from '@/lib/threadTree'
import { AttachmentChips } from '@/components/AttachmentChips'
import { ThreadRow, senderOf } from '@/components/ThreadRow'
import { cx, fmtAddresses, fmtDateTime, fmtTime } from '@/lib/format'
import { useDigContext, useLabelName } from '@/lib/queries'
import { dispositionLabel, findTypeLabel, FIND_TYPES, followUpArgs, IMPORTANCES, importanceLabel, sameDisposition } from '@/lib/selectors'
import { keyHint } from '@/lib/shortcuts'
import { useAction, useLastUndoable } from '@/lib/useAction'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Menu, type MenuItem } from '@/components/ui/Menu'
import { Note } from '@/components/ui/Note'
import { ErrorCard } from '@/components/ErrorCard'
import { MessageBody } from '@/components/MessageBody'
import { AuthChip, CopyCode, CopyMessageLink, DevOverlay, DispositionChip, ExplanationCard, FollowUpMenu, ImportanceChip, PageHeader, SnoozeMenu, StrataChips, StrataPicker } from '@/components/Bits'
import { useComposerActions } from './composer'
import { ActionHelpButton } from '@/components/ActionHelp'
import { FollowUpOverlay, SnoozeOverlay } from '@/components/OverlayList'
import { messagePath, queuePosition, readQueue } from './queue'

/** Threads longer than this get Expand all / Collapse all / Jump to latest. */
const LONG_THREAD = 3

export default function MessagePage() {
  const { digId = '', messageId = '' } = useParams()
  const [params] = useSearchParams()
  const location = useLocation()
  const nav = useNavigate()
  const { digName } = useDigContext()
  const { run } = useAction()
  const lastUndoable = useLastUndoable((s) => s.lastActivityId)
  const { reply, forward } = useComposerActions()
  const msg = useQuery({ queryKey: qk.message(digId, messageId), queryFn: () => getMessage(digId, messageId), enabled: !!digId && !!messageId })
  const thread = useQuery({ queryKey: msg.data ? qk.thread(digId, msg.data.threadId) : ['thread', 'none'], queryFn: () => getThread(digId, msg.data!.threadId), enabled: !!msg.data })
  const ex = useQuery({ queryKey: qk.explain(digId, messageId), queryFn: () => explain(digId, messageId), enabled: !!msg.data, retry: false })
  const rows = useQuery({
    queryKey: [...qk.messages, 'thread-rows', digId, msg.data?.threadId ?? ''],
    queryFn: () => listMessages({ digId, filter: { threadId: msg.data!.threadId }, page: page(null, 100) }),
    enabled: !!msg.data,
  })
  const row = rows.data?.items.find((r) => r.id === messageId) ?? null
  const [strataOpen, setStrataOpen] = useState(false)
  const [snoozeOpen, setSnoozeOpen] = useState(false)
  const [followUpOpen, setFollowUpOpen] = useState(false)
  const [openIds, setOpenIds] = useState<Set<string>>(new Set([messageId]))
  useEffect(() => setOpenIds(new Set([messageId])), [messageId])
  useEffect(() => {
    if (msg.data && params.get('reply') === '1') reply(msg.data)
    if (msg.data && params.get('forward') === '1') forward(msg.data)
  }, [msg.data]) // eslint-disable-line react-hooks/exhaustive-deps
  // Opened with #why (w on a list): land on the card once there is one.
  useEffect(() => {
    if (location.hash === '#why' && (ex.data || ex.error)) document.getElementById('why')?.scrollIntoView({ block: 'start' })
  }, [location.hash, ex.data, ex.error])

  const queue = useMemo(() => readQueue(location.state), [location.state])
  const pos = queue ? queuePosition(queue, digId, messageId) : -1
  const hasQueue = !!queue && pos >= 0
  const go = (delta: number) => {
    const e = queue?.queue[pos + delta]
    // replace, so Back still returns to the list rather than through every message.
    if (e) nav(messagePath(e), { state: { queue: queue!.queue, index: pos + delta }, replace: true })
  }

  const m = msg.data
  const correct = (d: Disposition) => m && run('set_disposition', { digId, messageId, disposition: d, findType: d.kind === 'find' ? d.find_type : undefined }, { digId })
  const followUp = (dueAt: string | null) => m && run('set_disposition', followUpArgs(digId, messageId, dueAt), { digId })
  useShortcuts('messages', {
    archive: () => m && run('archive_messages', { digId, messageIds: [messageId] }, { digId }),
    spoil: () => m && run('move_to_spoil', { digId, messageIds: [messageId] }, { digId }),
    contamination: () => m && run('mark_contamination', { digId, messageIds: [messageId] }, { digId }),
    label: () => setStrataOpen(true),
    snooze_message: () => m && setSnoozeOpen(true),
    follow_up: () => m && setFollowUpOpen(true),
    reply: () => m && reply(m),
    forward: () => m && forward(m),
    why_message: () => document.getElementById('why')?.scrollIntoView({ block: 'start' }),
    next_message: hasQueue ? () => go(1) : undefined,
    prev_message: hasQueue ? () => go(-1) : undefined,
  })
  if (msg.error) return <ErrorCard error={msg.error} onRetry={() => msg.refetch()} />
  if (!m) return <div className="p-6 text-sm text-clay">Loading message…</div>
  const disposition = ex.data?.disposition ?? row?.disposition ?? null
  const isContamination = disposition?.kind === 'contamination'
  const correctTo = (d: Disposition, label: string, danger = false): MenuItem => ({ label, danger, onSelect: () => void correct(d), disabledReason: sameDisposition(disposition, d) ? 'Current' : null })
  const correctItems: (MenuItem | 'separator')[] = [
    ...FIND_TYPES.map((t) => correctTo({ kind: 'find', find_type: t }, `Find: ${findTypeLabel(t)}`)),
    'separator',
    correctTo({ kind: 'filed' }, 'Filed'),
    correctTo({ kind: 'spoil' }, 'Spoil'),
    correctTo({ kind: 'contamination' }, 'Contamination', true),
  ]
  // "Always do this": the decision on this message becomes a rule for its sender or domain in Your corrections.
  const domain = m.from.email.includes('@') ? m.from.email.slice(m.from.email.lastIndexOf('@') + 1).toLowerCase() : ''
  const undecided = !disposition || disposition.kind === 'unresolved' ? 'Decide the message first (Correct…); an Unresolved message has nothing to remember' : null
  const remember = (scope: 'sender' | 'domain') => run('remember_for_sender', { digId, messageId, scope }, { digId })
  const rememberItems: MenuItem[] = [
    { label: `For ${m.from.email.toLowerCase()}`, onSelect: () => void remember('sender'), disabledReason: undecided },
    ...(domain ? [{ label: `For everything from ${domain}`, onSelect: () => void remember('domain'), disabledReason: undecided }] : []),
  ]
  // The level: the user's own outlives a rule deciding the message again; "the rule's level" clears it.
  const importance = ex.data?.importance ?? row?.importance ?? 'normal'
  const importanceYours = ex.data?.importanceSetByYou ?? row?.importanceSetByYou ?? false
  const setImportance = (level: Importance | null) => run('set_importance', { digId, messageId, importance: level }, { digId })
  const importanceItems: (MenuItem | 'separator')[] = [
    ...IMPORTANCES.map((l): MenuItem => ({ label: importanceLabel(l), onSelect: () => void setImportance(l), disabledReason: importanceYours && importance === l ? 'Current' : null })),
    'separator',
    { label: "Back to the rule's level", onSelect: () => void setImportance(null), disabledReason: importanceYours ? null : 'You have not set one' },
  ]
  const messages = thread.data?.messages ?? [m]
  const tree = buildThreadTree(messages) // cheap; computed after the early returns, so no hook
  const latest = messages.reduce((a, b) => (a.date >= b.date ? a : b))
  const jumpToLatest = () => {
    setOpenIds((s) => new Set([...s, latest.id]))
    requestAnimationFrame(() => document.getElementById(`msg-${latest.id}`)?.scrollIntoView({ block: 'start' }))
  }
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={
          <span className="flex items-center gap-2">
            <button type="button" className="rounded p-0.5 text-clay hover:bg-sand hover:text-ink" onClick={() => nav(-1)} aria-label="Back" title="Back to the list">
              <ArrowLeft className="size-4" />
            </button>
            <span className="truncate selectable">{m.subject || '(no subject)'}</span>
          </span>
        }
        subtitle={
          <span className="flex flex-wrap items-center gap-2">
            <span>{digName(digId)}</span>
            <span>· thread of {messages.length}</span>
            <CopyMessageLink digId={digId} messageId={messageId} about={`${senderOf(m)} · ${m.subject || '(no subject)'}`} />
            {ex.isPending ? <Chip tone="neutral" title="Loading the decision">…</Chip> : <DispositionChip d={disposition} shadow={ex.data?.shadow} />}
            <StrataChips strata={row?.strata ?? []} />
            <ImportanceChip level={importance} yours={importanceYours} />
            {ex.data?.verificationCode ? <CopyCode code={ex.data.verificationCode} /> : null}
            {ex.data ? <DevOverlay digId={digId} traceId={ex.data.traceId} mechanism={ex.data.mechanism} confidence={ex.data.signals[0]?.score ?? null} meshVersion={ex.data.meshVersion} escalated={ex.data.escalated} /> : null}
          </span>
        }
        actions={
          <>
            {hasQueue && queue ? (
              <span className="mr-1 flex items-center gap-0.5 text-xs text-clay" aria-label={`Message ${pos + 1} of ${queue.queue.length}`}>
                <Button size="sm" variant="ghost" icon={<ChevronUp />} onClick={() => go(-1)} disabledReason={pos === 0 ? 'First in the list' : null} title={`Previous message (${keyHint('prev_message')})`} />
                <span>
                  {pos + 1} of {queue.queue.length}
                </span>
                <Button size="sm" variant="ghost" icon={<ChevronDown />} onClick={() => go(1)} disabledReason={pos >= queue.queue.length - 1 ? 'Last in the list' : null} title={`Next message (${keyHint('next_message')})`} />
              </span>
            ) : null}
            <Button size="sm" icon={<Reply />} onClick={() => reply(m)} title={`Reply (${keyHint('reply')})`}>
              Reply
            </Button>
            <Button size="sm" icon={<ReplyAll />} onClick={() => reply(m, true)} title="Reply to the sender and everyone on the message">
              Reply all
            </Button>
            <Button size="sm" icon={<Forward />} onClick={() => forward(m)} title={`Forward (${keyHint('forward')})`}>
              Forward
            </Button>
            <Menu label="Correct the disposition" items={correctItems} trigger={<Button size="sm" title="Change what Riddle decided; the current one is marked">Correct…</Button>} />
            <Menu label="Always decide this sender's mail this way" items={rememberItems} trigger={<Button size="sm" title="Make this message's decision, Strata and the importance you set a rule for the sender or domain (Your corrections), so their next message is decided the same way">Always…</Button>} />
            <Menu label="How much this message matters" items={importanceItems} trigger={<Button size="sm" title={importanceYours ? 'Importance you set; a rule deciding the message again keeps it' : "Importance from the rule that decided it; set your own"}>{importanceLabel(importance)}{importanceYours ? ' (yours)' : ''}</Button>} />
            <FollowUpMenu onPick={(dueAt) => void followUp(dueAt)} trigger={<Button size="sm" icon={<Flag />} title={`Follow up by a date: a Find until a reply arrives (${keyHint('follow_up')})`}>Follow up</Button>} />
            <SnoozeMenu onPick={(until) => void run('snooze_messages', { digId, messageIds: [messageId], until }, { digId })} trigger={<Button size="sm" icon={<Clock />} title={`Snooze (${keyHint('snooze_message')})`}>Snooze</Button>} />
            <Button size="sm" variant="ghost" onClick={() => setStrataOpen(true)} title={`Add a Stratum (${keyHint('label')})`}>
              Stratum
            </Button>
            <Button size="sm" variant="ghost" onClick={() => void run('archive_messages', { digId, messageIds: [messageId] }, { digId })} title={`Archive: record as Filed (${keyHint('archive')})`}>
              Archive
            </Button>
            <ActionHelpButton group="message" digId={digId} />
          </>
        }
      />
      <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
        {isContamination ? (
          <Note tone="bad" title="Contamination: untrusted mail" className="mb-4">
            <span className="flex items-center gap-2">
              <ShieldAlert className="size-4 shrink-0" aria-hidden /> Links are neutralised, remote images blocked, and the Director will not read this unless you ask it to. If this is legitimate, use Correct…
            </span>
          </Note>
        ) : null}
        {messages.length > LONG_THREAD ? (
          <div className="mb-2 flex items-center gap-2 text-xs text-clay">
            <span>{messages.length} messages in this thread</span>
            <Button size="sm" variant="ghost" onClick={() => setOpenIds(new Set(messages.map((x) => x.id)))} disabledReason={openIds.size === messages.length ? 'Everything is open' : null}>
              Expand all
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setOpenIds(new Set())} disabledReason={openIds.size === 0 ? 'Everything is collapsed' : null}>
              Collapse all
            </Button>
            <Button size="sm" variant="ghost" onClick={jumpToLatest} title={`Open the newest message, from ${fmtDateTime(latest.date)}`}>
              Jump to latest
            </Button>
          </div>
        ) : null}
        <div className="flex flex-col">
          {tree.map((node) => (
            <ThreadRow key={node.message.id} node={node} parentName={node.parentId ? senderOf(messages.find((x) => x.id === node.parentId)) : null}>
              <ThreadMessage
                m={node.message}
                open={openIds.has(node.message.id)}
                current={node.message.id === messageId}
                onToggle={() =>
                  setOpenIds((s) => {
                    const n = new Set(s)
                    if (n.has(node.message.id)) n.delete(node.message.id)
                    else n.add(node.message.id)
                    return n
                  })
                }
              />
            </ThreadRow>
          ))}
        </div>
        <div id="why" className="mt-6">
          {ex.data ? (
            <ExplanationCard
              ex={ex.data}
              digId={digId}
              subject={m.subject}
              from={senderOf(m)}
              digName={digName(digId)}
              onFiled={disposition?.kind === 'find' ? () => void correct({ kind: 'filed' }) : undefined}
              onUndo={lastUndoable ? () => void run('undo_activity', { activityId: lastUndoable }, { successToast: 'Undone' }).then(() => useLastUndoable.getState().set(null)) : undefined}
            />
          ) : ex.error ? (
            <Note tone="neutral" title="Not sifted yet">This message has no decision to explain yet. It is in the queue; the Task Centre shows progress.</Note>
          ) : null}
        </div>
      </div>
      {snoozeOpen ? (
        <SnoozeOverlay
          onPick={(until) => {
            setSnoozeOpen(false)
            void run('snooze_messages', { digId, messageIds: [messageId], until }, { digId })
          }}
          onClose={() => setSnoozeOpen(false)}
        />
      ) : null}
      {followUpOpen ? (
        <FollowUpOverlay
          onPick={(dueAt) => {
            setFollowUpOpen(false)
            void followUp(dueAt)
          }}
          onClose={() => setFollowUpOpen(false)}
        />
      ) : null}
      <StrataPicker open={strataOpen} onClose={() => setStrataOpen(false)} onPick={(names) => void run('label_messages', { digId, messageIds: [messageId], strata: names }, { digId })} />
    </div>
  )
}

function ThreadMessage({ m, open, current, onToggle }: { m: Message; open: boolean; current: boolean; onToggle: () => void }) {
  const nameOf = useLabelName()
  return (
    <article id={`msg-${m.id}`} className={cx('rounded-md border', current ? 'border-ochre/50' : 'border-line')}>
      <button type="button" className="flex w-full items-center gap-3 px-3 py-2 text-left" onClick={onToggle} aria-expanded={open} title={open ? 'Collapse this message' : 'Expand this message'}>
        <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-sand-2 text-xs font-semibold text-ink-2" aria-hidden>{senderOf(m).slice(0, 1).toUpperCase()}</span>
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2">
            <span className="truncate text-sm font-medium selectable">{senderOf(m)}</span>
            <span className="truncate text-xs text-clay selectable">{m.from.email}</span>
            <AuthChip auth={m.headers.authentication} />
          </span>
          {!open ? <span className="block truncate text-xs text-ink-2 selectable">{m.snippet}</span> : null}
        </span>
        <span className="shrink-0 text-xs text-clay">{open ? fmtDateTime(m.date) : fmtTime(m.date)}</span>
      </button>
      {open ? (
        <div className="border-t border-line px-3 py-3 selectable">
          <dl className="mb-3 grid grid-cols-[3rem_1fr] gap-x-2 gap-y-0.5 text-xs text-ink-2">
            <dt className="text-clay">To</dt>
            <dd>{fmtAddresses(m.to) || '—'}</dd>
            {m.cc.length ? (
              <>
                <dt className="text-clay">Cc</dt>
                <dd>{m.cc.map((a) => a.email).join(', ')}</dd>
              </>
            ) : null}
            {m.headers.replyTo && m.headers.replyTo.email !== m.from.email ? (
              <>
                <dt className="text-clay">Reply-To</dt>
                <dd className="text-rust">{m.headers.replyTo.email} (differs from the sender)</dd>
              </>
            ) : null}
            {m.headers.listUnsubscribe ? (
              <>
                <dt className="text-clay">List</dt>
                <dd>{m.headers.listId ?? 'mailing list'} · unsubscribe available</dd>
              </>
            ) : null}
            <dt className="text-clay">Labels</dt>
            <dd>{m.labels.map((l) => nameOf(l, m.digId)).join(', ') || '—'}</dd>
          </dl>
          <MessageBody html={m.bodyHtml} text={m.bodyText} />
          <AttachmentChips attachments={m.attachments} showType className="mt-3" />
        </div>
      ) : null}
    </article>
  )
}

export { dispositionLabel }

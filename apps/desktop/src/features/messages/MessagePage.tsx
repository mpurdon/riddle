/**
 * One message: header, sanitized HTML in a sandboxed iframe (remote images
 * blocked), plain-text fallback, thread, attachments, the Why? card, a
 * Correct menu (set_disposition) and Reply/Forward into the composer.
 */
import { useEffect, useState } from 'react'
import { useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { ArrowLeft, Forward, Paperclip, Reply, ReplyAll, ShieldAlert } from 'lucide-react'
import type { Disposition } from '@/bindings/Disposition'
import type { Message } from '@/bindings/Message'
import { explain, getMessage, getThread, qk } from '@/lib/api'
import { cx, fmtBytes, fmtDateTime, fmtTime } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { dispositionLabel, findTypeLabel, FIND_TYPES } from '@/lib/selectors'
import { useAction, useLastUndoable } from '@/lib/useAction'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Menu, type MenuItem } from '@/components/ui/Menu'
import { Note } from '@/components/ui/Note'
import { ErrorCard } from '@/components/ErrorCard'
import { MessageBody } from '@/components/MessageBody'
import { DevOverlay, DispositionChip, ExplanationCard, PageHeader, SnoozeMenu, StrataChips, StrataPicker } from '@/components/Bits'
import { useComposerActions } from './composer'

export default function MessagePage() {
  const { digId = '', messageId = '' } = useParams()
  const [params] = useSearchParams()
  const nav = useNavigate()
  const { digName } = useDigContext()
  const { run } = useAction()
  const lastUndoable = useLastUndoable((s) => s.lastActivityId)
  const { reply, forward } = useComposerActions()
  const msg = useQuery({ queryKey: qk.message(digId, messageId), queryFn: () => getMessage(digId, messageId), enabled: !!digId && !!messageId })
  const thread = useQuery({ queryKey: msg.data ? qk.thread(digId, msg.data.threadId) : ['thread', 'none'], queryFn: () => getThread(digId, msg.data!.threadId), enabled: !!msg.data })
  const ex = useQuery({ queryKey: qk.explain(digId, messageId), queryFn: () => explain(digId, messageId), enabled: !!msg.data, retry: false })
  const [strataOpen, setStrataOpen] = useState(false)
  const [openIds, setOpenIds] = useState<Set<string>>(new Set([messageId]))
  useEffect(() => setOpenIds(new Set([messageId])), [messageId])
  useEffect(() => {
    if (msg.data && params.get('reply') === '1') reply(msg.data)
    if (msg.data && params.get('forward') === '1') forward(msg.data)
  }, [msg.data]) // eslint-disable-line react-hooks/exhaustive-deps

  const m = msg.data
  const correct = (d: Disposition) => m && run('set_disposition', { digId, messageId, disposition: d, findType: d.kind === 'find' ? d.find_type : undefined }, { digId })
  useShortcuts('messages', {
    archive: () => m && run('archive_messages', { digId, messageIds: [messageId] }, { digId }),
    spoil: () => m && run('move_to_spoil', { digId, messageIds: [messageId] }, { digId }),
    contamination: () => m && run('mark_contamination', { digId, messageIds: [messageId] }, { digId }),
    label: () => setStrataOpen(true),
    snooze_message: () => m && run('snooze_messages', { digId, messageIds: [messageId], until: new Date(Date.now() + 86_400_000).toISOString() }, { digId }),
    reply: () => m && reply(m),
    forward: () => m && forward(m),
    why_message: () => document.getElementById('why')?.scrollIntoView({ block: 'start' }),
  })
  if (msg.error) return <ErrorCard error={msg.error} onRetry={() => msg.refetch()} />
  if (!m) return <div className="p-6 text-sm text-clay">Loading message…</div>
  const disposition = ex.data?.disposition ?? null
  const isContamination = disposition?.kind === 'contamination'
  const correctItems: (MenuItem | 'separator')[] = [
    ...FIND_TYPES.map((t) => ({ label: `Find: ${findTypeLabel(t)}`, onSelect: () => void correct({ kind: 'find', find_type: t }) })),
    'separator',
    { label: 'Filed', onSelect: () => void correct({ kind: 'filed' }) },
    { label: 'Spoil', onSelect: () => void correct({ kind: 'spoil' }) },
    { label: 'Contamination', onSelect: () => void correct({ kind: 'contamination' }), danger: true },
  ]
  const messages = thread.data?.messages ?? [m]
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={
          <span className="flex items-center gap-2">
            <button type="button" className="rounded p-0.5 text-clay hover:bg-sand hover:text-ink" onClick={() => nav(-1)} aria-label="Back">
              <ArrowLeft className="size-4" />
            </button>
            <span className="truncate">{m.subject || '(no subject)'}</span>
          </span>
        }
        subtitle={
          <span className="flex flex-wrap items-center gap-2">
            <span>{digName(digId)}</span>
            <span>· thread of {messages.length}</span>
            <DispositionChip d={disposition} shadow={ex.data?.shadow} />
            <StrataChips strata={ex.data ? [] : []} />
            {ex.data ? <DevOverlay digId={digId} traceId={ex.data.traceId} mechanism={ex.data.mechanism} confidence={ex.data.signals[0]?.score ?? null} meshVersion={ex.data.meshVersion} escalated={ex.data.escalated} /> : null}
          </span>
        }
        actions={
          <>
            <Button size="sm" icon={<Reply />} onClick={() => reply(m)} title="Reply (r)">
              Reply
            </Button>
            <Button size="sm" icon={<ReplyAll />} onClick={() => reply(m, true)}>
              Reply all
            </Button>
            <Button size="sm" icon={<Forward />} onClick={() => forward(m)} title="Forward (f)">
              Forward
            </Button>
            <Menu label="Correct the disposition" items={correctItems} trigger={<Button size="sm">Correct…</Button>} />
            <SnoozeMenu onPick={(until) => run('snooze_messages', { digId, messageIds: [messageId], until }, { digId })} />
            <Button size="sm" variant="ghost" onClick={() => setStrataOpen(true)} title="Add Stratum (l)">
              Stratum
            </Button>
            <Button size="sm" variant="ghost" onClick={() => run('archive_messages', { digId, messageIds: [messageId] }, { digId })} title="Archive (e)">
              Archive
            </Button>
          </>
        }
      />
      <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
        {isContamination ? (
          <Note tone="bad" title="Contamination: untrusted mail" className="mb-4">
            <span className="flex items-center gap-2">
              <ShieldAlert className="size-4" /> Links are neutralised, remote images blocked, and the Director will not read this unless you ask it to. If this is legitimate, use Correct or Restore.
            </span>
          </Note>
        ) : null}
        <div className="flex flex-col gap-3">
          {messages.map((tm) => (
            <ThreadMessage key={tm.id} m={tm} open={openIds.has(tm.id)} current={tm.id === messageId} onToggle={() => setOpenIds((s) => { const n = new Set(s); if (n.has(tm.id)) n.delete(tm.id); else n.add(tm.id); return n })} />
          ))}
        </div>
        <div id="why" className="mt-6">
          {ex.data ? (
            <ExplanationCard
              ex={ex.data}
              digId={digId}
              onNotImportant={disposition?.kind === 'find' ? () => void correct({ kind: 'filed' }) : undefined}
              onUndo={lastUndoable ? () => void run('undo_activity', { activityId: lastUndoable }, { successToast: 'Undone' }).then(() => useLastUndoable.getState().set(null)) : undefined}
            />
          ) : ex.error ? (
            <Note tone="neutral" title="Not sifted yet">This message has no decision to explain yet. It is in the queue; the Task Centre shows progress.</Note>
          ) : null}
        </div>
      </div>
      <StrataPicker open={strataOpen} onClose={() => setStrataOpen(false)} onPick={(names) => run('label_messages', { digId, messageIds: [messageId], strata: names }, { digId })} />
    </div>
  )
}

function ThreadMessage({ m, open, current, onToggle }: { m: Message; open: boolean; current: boolean; onToggle: () => void }) {
  const auth = m.headers.authentication
  const authBad = auth.dmarc === 'fail' || auth.spf === 'fail' || auth.dkim === 'fail'
  return (
    <article className={cx('rounded-md border', current ? 'border-ochre/50' : 'border-line')}>
      <button type="button" className="flex w-full items-center gap-3 px-3 py-2 text-left" onClick={onToggle} aria-expanded={open}>
        <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-sand-2 text-xs font-semibold text-ink-2">{(m.from.name ?? m.from.email).slice(0, 1).toUpperCase()}</span>
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2">
            <span className="truncate text-sm font-medium">{m.from.name ?? m.from.email}</span>
            <span className="truncate text-xs text-clay">{m.from.email}</span>
            {authBad ? <Chip tone="bad" title={`SPF ${auth.spf}, DKIM ${auth.dkim}, DMARC ${auth.dmarc}`}>auth failed</Chip> : null}
          </span>
          {!open ? <span className="block truncate text-xs text-ink-2">{m.snippet}</span> : null}
        </span>
        <span className="shrink-0 text-xs text-clay">{open ? fmtDateTime(m.date) : fmtTime(m.date)}</span>
      </button>
      {open ? (
        <div className="border-t border-line px-3 py-3">
          <dl className="mb-3 grid grid-cols-[3rem_1fr] gap-x-2 gap-y-0.5 text-xs text-ink-2">
            <dt className="text-clay">To</dt>
            <dd className="selectable">{m.to.map((a) => (a.name ? `${a.name} <${a.email}>` : a.email)).join(', ') || '—'}</dd>
            {m.cc.length ? (
              <>
                <dt className="text-clay">Cc</dt>
                <dd className="selectable">{m.cc.map((a) => a.email).join(', ')}</dd>
              </>
            ) : null}
            {m.headers.replyTo && m.headers.replyTo.email !== m.from.email ? (
              <>
                <dt className="text-clay">Reply-To</dt>
                <dd className="text-rust selectable">{m.headers.replyTo.email} (differs from the sender)</dd>
              </>
            ) : null}
            {m.headers.listUnsubscribe ? (
              <>
                <dt className="text-clay">List</dt>
                <dd className="selectable">{m.headers.listId ?? 'mailing list'} · unsubscribe available</dd>
              </>
            ) : null}
            <dt className="text-clay">Labels</dt>
            <dd>{m.labels.join(', ') || '—'}</dd>
          </dl>
          <MessageBody html={m.bodyHtml} text={m.bodyText} />
          {m.attachments.length ? (
            <ul className="mt-3 flex flex-wrap gap-2" aria-label="Attachments">
              {m.attachments.map((a) => (
                <li key={a.filename} className="flex items-center gap-1.5 rounded border border-line bg-sand px-2 py-1 text-xs">
                  <Paperclip className="size-3 text-clay" />
                  <span className="font-medium">{a.filename}</span>
                  <span className="text-clay">{a.mimeType} · {fmtBytes(a.sizeBytes)}</span>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      ) : null}
    </article>
  )
}

export { dispositionLabel }

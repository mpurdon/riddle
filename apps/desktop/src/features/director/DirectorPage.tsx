/**
 * The Director (README "The Director"): a chat panel with streaming deltas,
 * tool-call cards (with the "read untrusted content" badge that explains
 * egress gating), proposal cards with Preview / Apply / Dismiss, and the
 * conversation list. Prefilled questions arrive from Finds and the search
 * box.
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, Bot, Check, Eye, MessageSquare, Plus, Send, ShieldAlert, X } from 'lucide-react'
import type { ToolCall } from '@/bindings/ToolCall'
import type { PendingAction } from '@/bindings/PendingAction'
import { directorSend, getConversation, listConversations, listPending, qk } from '@/lib/api'
import { cx, fmtMs, fmtRelative, fmtUsd, num, safeJson } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { useDirector, useProposals, toast } from '@/lib/stores'
import { useAction, showError } from '@/lib/useAction'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Dialog } from '@/components/ui/Dialog'
import { EmptyState } from '@/components/ui/EmptyState'
import { Note } from '@/components/ui/Note'
import { Tip } from '@/components/ui/Tooltip'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'

const SUGGESTIONS = [
  'What actually needs my attention this morning?',
  'Why did you think this was a Find?',
  'What am I waiting for other people to respond to?',
  'These GitHub notifications are not useful unless something fails. Handle those automatically from now on.',
  'Find every subscription I have paid for during the last year and identify the ones I may no longer need.',
  'Draft replies to everything that is waiting on me.',
]

function Markdown({ text }: { text: string }) {
  // A small renderer: paragraphs, **bold**, and "- " / "1. " lists. No HTML, no links.
  const blocks = text.split(/\n{2,}/)
  const inline = (s: string) => s.split(/(\*\*[^*]+\*\*)/g).map((p, i) => (p.startsWith('**') ? <strong key={i}>{p.slice(2, -2)}</strong> : <span key={i}>{p}</span>))
  return (
    <div className="flex flex-col gap-2">
      {blocks.map((b, i) => {
        const lines = b.split('\n')
        if (lines.every((l) => /^(-|\d+\.)\s/.test(l))) {
          const ordered = /^\d+\./.test(lines[0])
          const items = lines.map((l, k) => <li key={k}>{inline(l.replace(/^(-|\d+\.)\s/, ''))}</li>)
          return ordered ? <ol key={i} className="list-decimal pl-5">{items}</ol> : <ul key={i} className="list-disc pl-5">{items}</ul>
        }
        return <p key={i} className="whitespace-pre-wrap">{inline(b)}</p>
      })}
    </div>
  )
}

export default function DirectorPage() {
  const { conversationId } = useParams()
  const [params, setParams] = useSearchParams()
  const nav = useNavigate()
  const qc = useQueryClient()
  const { digId, digs } = useDigContext()
  const convs = useQuery({ queryKey: qk.conversations, queryFn: listConversations })
  const conv = useQuery({ queryKey: conversationId ? qk.conversation(conversationId) : ['conversation', 'none'], queryFn: () => getConversation(conversationId!), enabled: !!conversationId })
  const live = useDirector((s) => (conversationId ? s.live[conversationId] : undefined))
  const prefill = useDirector((s) => s.prefill)
  const setPrefill = useDirector((s) => s.setPrefill)
  const begin = useDirector((s) => s.begin)
  const clear = useDirector((s) => s.clear)
  const [text, setText] = useState('')
  const [sending, setSending] = useState(false)
  const scroller = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    if (prefill) {
      setText(prefill)
      setPrefill(null)
      if (params.get('send') === '1') {
        setParams(new URLSearchParams(), { replace: true })
        void send(prefill)
      } else inputRef.current?.focus()
    }
  }, [prefill]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (live?.done && conversationId) {
      void qc.invalidateQueries({ queryKey: qk.conversation(conversationId) }).then(() => clear(conversationId))
    }
  }, [live?.done, conversationId, qc, clear])
  useEffect(() => {
    scroller.current?.scrollTo({ top: scroller.current.scrollHeight })
  }, [conv.data, live?.text, live?.toolCalls.length])

  const send = async (msg?: string) => {
    const m = (msg ?? text).trim()
    if (!m || sending) return
    setSending(true)
    try {
      const out = await directorSend({ conversationId: conversationId ?? null, message: m, digIds: digId ? [digId] : digs.map((d) => d.id) })
      begin(out.conversationId)
      setText('')
      if (out.conversationId !== conversationId) nav(`/director/${out.conversationId}`)
      else void qc.invalidateQueries({ queryKey: qk.conversation(out.conversationId) })
      void qc.invalidateQueries({ queryKey: qk.conversations })
    } catch (e) {
      showError(e)
    } finally {
      setSending(false)
    }
  }
  useShortcuts('director', { director_send: () => void send() })

  const turns = conv.data?.turns ?? []
  const readUntrusted = conv.data?.readUntrusted ?? false
  return (
    <div className="flex min-h-0 flex-1">
      <section className="flex w-64 shrink-0 flex-col border-r border-line">
        <PageHeader title="Director" actions={<Button size="sm" icon={<Plus />} onClick={() => nav('/director')}>New</Button>} />
        <div className="min-h-0 flex-1 overflow-auto" role="listbox" aria-label="Conversations">
          {(convs.data ?? []).map((c) => (
            <div key={c.id} role="option" aria-selected={c.id === conversationId} onClick={() => nav(`/director/${c.id}`)} className={cx('cursor-default border-b border-line/60 px-3 py-2', c.id === conversationId ? 'row-selected' : 'hover:bg-sand/60')}>
              <div className="truncate text-sm font-medium">{c.title}</div>
              <div className="flex items-center gap-1 text-[11px] text-clay">
                {fmtRelative(c.updatedAt)}
                {c.readUntrusted ? <Chip tone="warn">read mail</Chip> : null}
              </div>
            </div>
          ))}
          {convs.data && !convs.data.length ? <div className="p-4 text-xs text-clay">Conversations are not state: everything the Director does lands in Finds, drafts, Meshes and Activity.</div> : null}
        </div>
        <ProposalsPanel />
      </section>
      <section className="flex min-w-0 flex-1 flex-col">
        <div ref={scroller} className="min-h-0 flex-1 overflow-auto px-6 py-4">
          {!conversationId ? (
            <EmptyState
              icon={<Bot />}
              title="Ask the Director."
              body={
                <div>
                  <p>It works with the same Digs, Finds, Strata, Meshes and drafts you do, through the same Actions. Before anything leaves the machine it shows you the real recipients and asks.</p>
                  <ul className="mt-3 flex flex-col gap-1 text-left">
                    {SUGGESTIONS.map((s) => (
                      <li key={s}>
                        <button type="button" className="rounded px-2 py-1 text-left text-sm text-ochre hover:bg-sand" onClick={() => void send(s)}>
                          {s}
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              }
            />
          ) : null}
          {conv.error ? <ErrorCard error={conv.error} onRetry={() => conv.refetch()} /> : null}
          {readUntrusted ? (
            <Note tone="warn" className="mb-4">
              <span className="flex items-center gap-2">
                <ShieldAlert className="size-4 shrink-0" />
                <span>The Director read mail content in this conversation. Sending, forwarding, creating or updating a Mesh, and adding to Never Hidden now need your approval, and each control says so.</span>
              </span>
            </Note>
          ) : null}
          <div className="flex flex-col gap-4">
            {turns.map((t, i) => (
              <div key={i} className={cx('flex', t.role === 'user' ? 'justify-end' : 'justify-start')}>
                <div className={cx('max-w-[78%] rounded-lg px-4 py-2.5 text-sm', t.role === 'user' ? 'bg-ochre/10' : 'border border-line bg-sand')}>
                  {t.role !== 'user' ? (
                    <div className="mb-1.5 flex items-center gap-2 text-[11px] text-clay">
                      <Bot className="size-3.5" /> Director
                      {t.costUsd !== null ? <span>· {fmtUsd(t.costUsd)} · {num(t.inputTokens)} in / {num(t.outputTokens)} out</span> : null}
                    </div>
                  ) : null}
                  {t.toolCalls.length ? (
                    <div className="mb-2 flex flex-col gap-1">
                      {t.toolCalls.map((c) => (
                        <ToolCallCard key={c.id} c={c} />
                      ))}
                    </div>
                  ) : null}
                  <Markdown text={t.content} />
                </div>
              </div>
            ))}
            {live && !live.done ? (
              <div className="flex justify-start">
                <div className="max-w-[78%] rounded-lg border border-line bg-sand px-4 py-2.5 text-sm">
                  <div className="mb-1.5 flex items-center gap-2 text-[11px] text-clay">
                    <Bot className="size-3.5" /> Director · working
                  </div>
                  {live.toolCalls.length ? (
                    <div className="mb-2 flex flex-col gap-1">
                      {live.toolCalls.map((c) => (
                        <ToolCallCard key={c.id} c={c} />
                      ))}
                    </div>
                  ) : null}
                  {live.text ? <Markdown text={live.text} /> : <span className="text-clay">{live.toolCalls.length ? 'Reading results…' : 'Thinking…'}</span>}
                </div>
              </div>
            ) : null}
            {live?.error ? <Note tone="bad">{live.error}</Note> : null}
          </div>
        </div>
        <div className="border-t border-line p-3">
          <div className="flex items-end gap-2">
            <textarea
              ref={inputRef}
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
                  e.preventDefault()
                  void send()
                }
              }}
              rows={2}
              placeholder={conversationId ? 'Reply to the Director' : 'Ask the Director, or delegate work'}
              aria-label="Message to the Director"
              className="min-h-10 flex-1 resize-none rounded-md border border-line bg-paper px-3 py-2 text-sm placeholder:text-clay focus:border-ochre focus:outline-none"
            />
            <Button variant="primary" icon={<Send />} busy={sending} onClick={() => void send()} disabledReason={text.trim() ? null : 'Type a message'} title="Send (⌘↵)">
              Send
            </Button>
          </div>
          <div className="mt-1 text-[11px] text-clay">Scope: {digId ? digs.find((d) => d.id === digId)?.name : 'all Digs'}. Contamination never enters the Director's context unless you open it.</div>
        </div>
      </section>
    </div>
  )
}

function ToolCallCard({ c }: { c: ToolCall }) {
  const nav = useNavigate()
  return (
    <div className={cx('flex items-center gap-2 rounded border px-2 py-1 text-xs', c.ok ? 'border-line bg-paper' : 'border-rust/40 bg-rust/5')}>
      {c.ok ? <Check className="size-3 text-moss" /> : <X className="size-3 text-rust" />}
      <span className="mono font-medium">{c.action}</span>
      <span className="min-w-0 flex-1 truncate text-ink-2">{c.summary}</span>
      <span className="text-clay">{fmtMs(c.latencyMs)}</span>
      {c.untrusted ? (
        <Tip text="This result carried mail content. Mail is untrusted input: once the Director has read it in a task, egress Actions (send, forward, create or update a Mesh, add to Never Hidden) need your approval.">
          <span>
            <Chip tone="warn">
              <AlertTriangle className="size-3" /> read untrusted content
            </Chip>
          </span>
        </Tip>
      ) : null}
      {c.activityId ? (
        <button type="button" className="text-ochre hover:underline" onClick={() => nav('/activity')}>
          Activity
        </button>
      ) : null}
    </div>
  )
}

function ProposalsPanel() {
  const proposals = useProposals((s) => s.proposals)
  const remove = useProposals((s) => s.remove)
  const pending = useQuery({ queryKey: qk.pending, queryFn: listPending, enabled: proposals.length > 0 })
  const { apply } = useAction()
  const [previewing, setPreviewing] = useState<PendingAction | null>(null)
  const items = useMemo(() => proposals.map((p) => ({ p, pa: pending.data?.find((x) => x.id === p.pendingId) ?? null })), [proposals, pending.data])
  if (!proposals.length) return null
  return (
    <div className="border-t border-line p-3">
      <div className="mb-1 flex items-center gap-1 text-[11px] font-semibold uppercase tracking-wide text-clay">
        <MessageSquare className="size-3" /> Proposals ({proposals.length})
      </div>
      <ul className="flex flex-col gap-2">
        {items.map(({ p, pa }) => (
          <li key={p.pendingId} className="rounded-md border border-ochre/40 bg-ochre/5 p-2 text-xs">
            <div className="font-medium">{p.summary}</div>
            <div className="mt-0.5 text-clay">{fmtRelative(new Date(p.createdAt).toISOString())}{pa ? ` · ${pa.reason}` : ''}</div>
            <div className="mt-1.5 flex gap-1">
              <Button size="sm" variant="ghost" icon={<Eye />} disabledReason={pa ? null : 'Loading the proposal'} onClick={() => pa && setPreviewing(pa)}>
                Preview
              </Button>
              <Button
                size="sm"
                variant="primary"
                disabledReason={pa ? null : 'Loading the proposal'}
                onClick={async () => {
                  if (!pa) return
                  const out = await apply(pa, { successToast: 'Applied' })
                  if (out) remove(p.pendingId)
                }}
              >
                Apply
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  remove(p.pendingId)
                  toast('Proposal dismissed', { body: 'Nothing changed.', kind: 'info' })
                }}
              >
                Dismiss
              </Button>
            </div>
          </li>
        ))}
      </ul>
      {previewing ? (
        <Dialog open onOpenChange={(o) => !o && setPreviewing(null)} title={previewing.summary} description="The real arguments the Action layer will run with." wide>
          {typeof (previewing.realArguments as Record<string, unknown>)?.text === 'string' ? <pre className="max-h-80 overflow-auto rounded border border-line bg-sand p-2 mono text-xs whitespace-pre">{String((previewing.realArguments as Record<string, unknown>).text)}</pre> : null}
          <pre className="mt-2 max-h-40 overflow-auto rounded border border-line bg-sand p-2 text-xs">{safeJson({ ...(previewing.realArguments as object), text: undefined })}</pre>
          <div className="mt-4 flex justify-end gap-2">
            <Button variant="ghost" onClick={() => setPreviewing(null)}>
              Close
            </Button>
            <Button
              variant="primary"
              onClick={async () => {
                const out = await apply(previewing, { successToast: 'Applied' })
                if (out) {
                  remove(previewing.id)
                  setPreviewing(null)
                }
              }}
            >
              Apply…
            </Button>
          </div>
        </Dialog>
      ) : null}
    </div>
  )
}

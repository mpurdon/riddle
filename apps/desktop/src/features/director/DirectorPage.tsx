/**
 * The Director (README "The Director"): a chat panel with streaming deltas,
 * tool-call cards (with the "read untrusted content" badge that explains
 * egress gating), the proposals panel, and the conversation list.
 *
 * Prefilled questions arrive from Finds, messages and the search box, and
 * only ever land in the composer: the UX study watched a "Talk to the
 * Director" button spend a turn before the person had read what it asked,
 * so nothing sends until the person presses Enter in the composer (⌘↵ also
 * sends, from anywhere on the page). Shift+Enter adds a line, the way every
 * chat does; the earlier "Enter adds a line, ⌘↵ sends" was the one place in
 * the app that broke that habit. There is no Stop button: the backend
 * cannot cancel a running turn, and a control that pretends to would be a
 * lie the person discovers at the worst moment.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { AlertTriangle, Bot, Check, Link2, Plus, RefreshCw, Send, ShieldAlert, X } from 'lucide-react'
import type { ToolCall } from '@/bindings/ToolCall'
import type { Turn } from '@/bindings/Turn'
import { directorRetry, directorSend, getConversation, listConversations, qk } from '@/lib/api'
import { cx, fmtMs, fmtRelative, fmtUsd, num } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { keysLabel } from '@/lib/shortcuts'
import { useDirector } from '@/lib/stores'
import { showError } from '@/lib/useAction'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { EmptyState } from '@/components/ui/EmptyState'
import { Note } from '@/components/ui/Note'
import { Tip } from '@/components/ui/Tooltip'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader, useCopy } from '@/components/Bits'
import { ProposalsPanel } from './ProposalsPanel'
import { Resizable } from '@/components/Resizable'

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

/**
 * The last turn, when it is an error: the message, a countdown when the
 * provider named a wait (a tripped circuit breaker says "retry in 29s"),
 * and the retry itself once the wait is over, without a click. The person
 * used to read the error and re-type the question; the failed turn stays
 * until the retry starts, then the conversation drops it.
 */
function FailedTurn({ t, working, onRetry }: { t: Turn; working: boolean; onRetry: () => void }) {
  const deadline = t.failure?.retryAfterSecs != null ? new Date(t.createdAt).getTime() + t.failure.retryAfterSecs * 1000 : null
  const [now, setNow] = useState(() => Date.now())
  const fired = useRef(false)
  const left = deadline === null ? null : Math.max(0, Math.ceil((deadline - now) / 1000))
  // One tick a second while the countdown runs, and none once it reaches
  // zero: a retry that failed used to leave the turn re-rendering four times
  // a second until the conversation unmounted.
  useEffect(() => {
    if (deadline === null || left === 0) return
    const id = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(id)
  }, [deadline, left])
  useEffect(() => {
    if (left === 0 && !fired.current && !working) {
      fired.current = true
      onRetry()
    }
  }, [left, working, onRetry])
  return (
    <div className="max-w-[78%] rounded-lg border border-rust/40 bg-rust/5 px-4 py-2.5 text-sm selectable">
      <div className="mb-1.5 flex items-center gap-2 text-2xs text-clay">
        <Bot className="size-3.5" /> Director
      </div>
      <Markdown text={t.content} />
      <div className="mt-2 flex flex-wrap items-center gap-2 text-xs text-ink-2">
        {left !== null && left > 0 ? <span role="status">Retrying in {left}s…</span> : left === 0 ? <span role="status">Retrying…</span> : null}
        <Button
          size="sm"
          variant="ghost"
          icon={<RefreshCw />}
          disabledReason={working ? 'The Director is still working on the last message' : null}
          title="Run your last message again now"
          onClick={() => {
            fired.current = true
            onRetry()
          }}
        >
          Retry now
        </Button>
      </div>
    </div>
  )
}

export default function DirectorPage() {
  const { conversationId } = useParams()
  const nav = useNavigate()
  const qc = useQueryClient()
  const { digId, digs } = useDigContext()
  const convs = useQuery({ queryKey: qk.conversations, queryFn: listConversations })
  const conv = useQuery({ queryKey: conversationId ? qk.conversation(conversationId) : ['conversation', 'none'], queryFn: () => getConversation(conversationId!), enabled: !!conversationId })
  const live = useDirector((s) => (conversationId ? s.live[conversationId] : undefined))
  const prefill = useDirector((s) => s.prefill)
  const prefillTitle = useDirector((s) => s.prefillTitle)
  const setPrefill = useDirector((s) => s.setPrefill)
  const begin = useDirector((s) => s.begin)
  const clear = useDirector((s) => s.clear)
  const [text, setText] = useState('')
  // What a prefilled draft is about, kept until it starts a conversation.
  const [newTitle, setNewTitle] = useState<string | null>(null)
  const [sending, setSending] = useState(false)
  const scroller = useRef<HTMLDivElement>(null)
  const inputRef = useRef<HTMLTextAreaElement>(null)
  const working = !!live && !live.done

  useEffect(() => {
    // A prefill is a draft, never a send: show it, focus it, wait for ⌘↵.
    if (prefill) {
      setText(prefill)
      setNewTitle(prefillTitle)
      setPrefill(null)
      inputRef.current?.focus()
    }
  }, [prefill, prefillTitle, setPrefill])
  useEffect(() => {
    if (live?.done && conversationId) {
      void qc.invalidateQueries({ queryKey: qk.conversation(conversationId) }).then(() => clear(conversationId))
    }
  }, [live?.done, conversationId, qc, clear])
  useEffect(() => {
    scroller.current?.scrollTo({ top: scroller.current.scrollHeight })
  }, [conv.data, live?.text, live?.toolCalls.length])

  // `message` is a Continue control's words; otherwise the composer's text.
  const send = async (message?: string) => {
    const m = (message ?? text).trim()
    if (!m || sending || working) return
    setSending(true)
    try {
      const out = await directorSend({ conversationId: conversationId ?? null, message: m, digIds: digId ? [digId] : digs.map((d) => d.id), title: conversationId ? null : newTitle })
      setNewTitle(null)
      begin(out.conversationId)
      if (message === undefined) setText('')
      if (out.conversationId !== conversationId) nav(`/director/${out.conversationId}`)
      else void qc.invalidateQueries({ queryKey: qk.conversation(out.conversationId) })
      void qc.invalidateQueries({ queryKey: qk.conversations })
    } catch (e) {
      showError(e)
    } finally {
      setSending(false)
    }
  }
  // A failed turn: the failed Director turns come off, the last message runs again.
  const retry = useCallback(async () => {
    if (!conversationId || working) return
    begin(conversationId)
    try {
      await directorRetry(conversationId)
      void qc.invalidateQueries({ queryKey: qk.conversation(conversationId) })
    } catch (e) {
      clear(conversationId)
      showError(e)
    }
  }, [conversationId, working, begin, clear, qc])
  useShortcuts('director', { director_send: () => void send() })
  const suggest = (s: string) => {
    setText(s)
    inputRef.current?.focus()
  }

  const turns = conv.data?.turns ?? []
  const readUntrusted = conv.data?.readUntrusted ?? false
  const sendDisabled = working ? 'The Director is still working on the last message' : text.trim() ? null : 'Type a message'
  /** Said once, on the Send button and under the composer. */
  // One source for the two key labels: the button title says it in plain text, the footer as keycaps.
  const keys = { send: keysLabel('enter').join(''), newline: keysLabel('shift+enter').join('') }
  const composerHint = `${keys.send} sends · ${keys.newline} adds a line`
  // Enter sends; Shift+Enter (and Alt+Enter) keep the newline; ⌘↵ is the page shortcut. A key
  // pressed while an input method is still composing belongs to the composition, not to us.
  // The composer grows with the text, three lines to about ten, then scrolls; a prefilled question
  // from a Find used to arrive cut off at two lines with its end hidden.
  useEffect(() => {
    const el = inputRef.current
    if (!el) return
    el.style.height = 'auto'
    el.style.height = `${el.scrollHeight}px`
  }, [text])
  const onComposerKey = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key !== 'Enter' || e.shiftKey || e.altKey || e.metaKey || e.ctrlKey || e.nativeEvent.isComposing) return
    e.preventDefault()
    if (!sendDisabled) void send()
  }
  return (
    <div className="flex min-h-0 flex-1">
      <Resizable as="section" id="director" side="left" initial={256} min={180} max={480} label="Resize the conversation list" className="flex flex-col border-r border-line">
        <PageHeader title="Director" actions={<Button size="sm" icon={<Plus />} onClick={() => nav('/director')}>New</Button>} />
        <div className="min-h-0 flex-1 overflow-auto" role="listbox" aria-label="Conversations">
          {(convs.data ?? []).map((c) => (
            <div key={c.id} role="option" aria-selected={c.id === conversationId} onClick={() => nav(`/director/${c.id}`)} className={cx('cursor-default border-b border-line/60 px-3 py-2', c.id === conversationId ? 'row-selected' : 'hover:bg-sand/60')}>
              <div className="truncate text-sm font-medium">{c.title}</div>
              <div className="flex items-center gap-1 text-2xs text-clay">
                {fmtRelative(c.updatedAt)}
                {c.readUntrusted ? <Chip tone="warn">read mail</Chip> : null}
              </div>
            </div>
          ))}
          {convs.data && !convs.data.length ? <div className="p-4 text-xs text-clay">Conversations are just talk; what the Director does lands in Finds, drafts, Meshes and Activity.</div> : null}
        </div>
      </Resizable>
      <section className="flex min-w-0 flex-1 flex-col">
        <div ref={scroller} className="min-h-0 flex-1 overflow-auto px-6 py-4">
          {!conversationId ? (
            <EmptyState
              icon={<Bot />}
              title="Ask the Director."
              body={
                <div>
                  <p>It works with the same accounts, Finds, Strata, Meshes and drafts you do, through the same Actions. Before anything leaves the machine it shows you the real recipients and asks.</p>
                  <p className="mt-2 text-xs text-clay">Pick a question to start with; edit it, then press Enter to send.</p>
                  <ul className="mt-3 flex flex-col gap-1 text-left">
                    {SUGGESTIONS.map((s) => (
                      <li key={s}>
                        <button type="button" className="rounded px-2 py-1 text-left text-sm text-ochre hover:bg-sand" onClick={() => suggest(s)} title="Put this in the composer; nothing is sent until you press send">
                          {s}
                        </button>
                      </li>
                    ))}
                  </ul>
                </div>
              }
            />
          ) : null}
          {conv.data ? <ConversationHeading id={conv.data.id} title={conv.data.title} /> : null}
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
            {turns.map((t, i) =>
              t.failure && i === turns.length - 1 ? (
                <div key={i} className="flex justify-start">
                  <FailedTurn t={t} working={working} onRetry={() => void retry()} />
                </div>
              ) : (
              <div key={i} className={cx('flex', t.role === 'user' ? 'justify-end' : 'justify-start')}>
                <div className={cx('max-w-[78%] rounded-lg px-4 py-2.5 text-sm selectable', t.role === 'user' ? 'bg-ochre/10' : t.failure ? 'border border-rust/40 bg-rust/5' : 'border border-line bg-sand')}>
                  {t.role !== 'user' ? (
                    <div className="mb-1.5 flex items-center gap-2 text-2xs text-clay">
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
                  {t.stoppedEarly && i === turns.length - 1 ? (
                    <div className="mt-2 flex flex-wrap items-center gap-2 border-t border-line pt-2 text-2xs text-clay">
                      <span role="status">Stopped early: {t.stoppedEarly}.</span>
                      <Button size="sm" disabledReason={working ? 'The Director is still working' : null} onClick={() => void send('continue')} title="One more turn with the usual limits">
                        Continue
                      </Button>
                      <Button size="sm" disabledReason={working ? 'The Director is still working' : null} onClick={() => void send('keep going')} title="No round limit for the rest of this conversation; the cost limit still applies">
                        Keep going
                      </Button>
                    </div>
                  ) : null}
                </div>
              </div>
              ),
            )}
            {working ? (
              <div className="flex justify-start">
                <div className="max-w-[78%] rounded-lg border border-line bg-sand px-4 py-2.5 text-sm selectable">
                  <div className="mb-1.5 flex items-center gap-2 text-2xs text-clay">
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
        <ProposalsPanel />
        <div className="border-t border-line p-3">
          <div className="flex items-end gap-2">
            <textarea
              ref={inputRef}
              value={text}
              onChange={(e) => setText(e.target.value)}
              onKeyDown={onComposerKey}
              rows={3}
              placeholder={conversationId ? 'Reply to the Director' : 'Ask the Director, or delegate work'}
              aria-label="Message to the Director"
              className="max-h-64 min-h-20 flex-1 resize-none overflow-y-auto rounded-md border border-line bg-paper px-3 py-2 text-sm leading-6 placeholder:text-clay focus:border-ochre focus:outline-none"
            />
            <Button variant="primary" icon={<Send />} busy={sending} onClick={() => void send()} disabledReason={sendDisabled} title={composerHint}>
              Send
            </Button>
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-x-3 text-2xs text-clay">
            <span>Scope: {digId ? digs.find((d) => d.id === digId)?.name : 'all accounts'}. Contamination never enters the Director's context unless you open it.</span>
            {working ? (
              <span className="text-ink-2" role="status">
                Working… this cannot be stopped mid-turn.
              </span>
            ) : (
              <span>
                <kbd>{keys.send}</kbd> sends · <kbd>{keys.newline}</kbd> adds a line
              </span>
            )}
          </div>
        </div>
      </section>
    </div>
  )
}

function ToolCallCard({ c }: { c: ToolCall }) {
  const nav = useNavigate()
  return (
    <div className={cx('flex items-center gap-2 rounded border px-2 py-1 text-xs selectable', c.ok ? 'border-line bg-paper' : 'border-rust/40 bg-rust/5')}>
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

/**
 * The conversation's name and a link to it. There was no way to point
 * someone (or another Claude session) at one conversation: the list showed
 * ten rows titled the same and the id was nowhere on screen. The link is
 * `riddle://director/<id>`, with the title so it reads on its own; the id
 * is what the dev bridge's `get_conversation` takes.
 */
export function conversationLink(id: string, title: string): string {
  return `riddle://director/${id} (Director: ${title})`
}

function ConversationHeading({ id, title }: { id: string; title: string }) {
  const [copied, copy] = useCopy(conversationLink(id, title))
  return (
    <div className="mb-3 flex items-center gap-3">
      <h2 className="min-w-0 flex-1 truncate text-sm font-semibold" title={title}>
        {title}
      </h2>
      <Button size="sm" variant="ghost" icon={copied ? <Check /> : <Link2 />} onClick={() => void copy()} title="Copy a link to this conversation, to paste in a note or another chat">
        {copied ? 'Copied' : 'Copy link'}
      </Button>
    </div>
  )
}

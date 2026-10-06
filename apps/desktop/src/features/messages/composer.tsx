/**
 * The composer: a slide-over holding one draft. Drafts are real Riddle
 * drafts (create/update/delete through Actions), and Send is the TwoStep
 * tier showing the Action layer's real recipients.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { Send, Trash2, X } from 'lucide-react'
import type { Address } from '@/bindings/Address'
import type { Draft } from '@/bindings/Draft'
import type { Message } from '@/bindings/Message'
import { qk } from '@/lib/api'
import { fmtDateTime } from '@/lib/format'
import { useDigs } from '@/lib/queries'
import { useComposer, useUi, toast } from '@/lib/stores'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Select } from '@/components/ui/Select'
import { inputClass } from '@/components/ui/Field'
import { senderOf } from '@/components/ThreadRow'

function parseAddresses(s: string): Address[] {
  return s
    .split(/[,;]+/)
    .map((x) => x.trim())
    .filter(Boolean)
    .map((x) => {
      const m = /^(.*?)\s*<([^>]+)>$/.exec(x)
      return m ? { name: m[1].replace(/^"|"$/g, '') || null, email: m[2] } : { email: x, name: null }
    })
}
const fmtAddresses = (a: Address[]) => a.map((x) => (x.name ? `${x.name} <${x.email}>` : x.email)).join(', ')

function quote(m: Message): string {
  const body = (m.bodyText ?? m.snippet).split('\n').map((l) => `> ${l}`).join('\n')
  return `\n\nOn ${fmtDateTime(m.date)}, ${senderOf(m)} wrote:\n${body}\n`
}

export function useComposerActions() {
  const { run } = useAction()
  const show = useComposer((s) => s.show)
  const digs = useDigs()
  const uiDig = useUi((s) => s.digId)
  const open = useCallback(
    async (digId: string, draft: Partial<Draft>, mode: 'new' | 'reply' | 'forward') => {
      const out = await run<Draft>('create_draft', { digId, draft }, { successToast: false })
      if (out) show({ digId, draft: out.result, mode })
    },
    [run, show],
  )
  return {
    compose: (digId?: string) => {
      const d = digId ?? uiDig ?? digs.data?.[0]?.id
      if (!d) {
        toast('Add a Dig before composing', { kind: 'warn' })
        return
      }
      void open(d, {}, 'new')
    },
    reply: (m: Message, all = false) => {
      const to = m.isSent ? m.to : [m.from]
      const cc = all ? [...m.to, ...m.cc].filter((a) => a.email !== m.from.email) : []
      void open(m.digId, { threadId: m.threadId, inReplyTo: m.id, to, cc, subject: m.subject.startsWith('Re:') ? m.subject : `Re: ${m.subject}`, bodyText: quote(m) }, 'reply')
    },
    forward: (m: Message) => {
      void open(m.digId, { threadId: null, inReplyTo: m.id, to: [], subject: m.subject.startsWith('Fwd:') ? m.subject : `Fwd: ${m.subject}`, bodyText: `\n\n---------- Forwarded message ----------\nFrom: ${m.from.name ?? ''} <${m.from.email}>\nDate: ${fmtDateTime(m.date)}\nSubject: ${m.subject}\n\n${m.bodyText ?? m.snippet}` }, 'forward')
    },
  }
}

export function ComposerPanel() {
  const open = useComposer((s) => s.open)
  const close = useComposer((s) => s.close)
  const { run } = useAction()
  const qc = useQueryClient()
  const digs = useDigs()
  const [draft, setDraft] = useState<Draft | null>(open?.draft ?? null)
  const [to, setTo] = useState('')
  const [cc, setCc] = useState('')
  const [bcc, setBcc] = useState('')
  const [showBcc, setShowBcc] = useState(false)
  const [saved, setSaved] = useState<string | null>(null)
  const dirty = useRef(false)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)

  useEffect(() => {
    if (!open) return
    setDraft(open.draft)
    setTo(fmtAddresses(open.draft.to))
    setCc(fmtAddresses(open.draft.cc))
    setBcc(fmtAddresses(open.draft.bcc))
    setShowBcc(open.draft.bcc.length > 0)
    setSaved(null)
    dirty.current = false
  }, [open])

  const current = useCallback((): Draft | null => (draft ? { ...draft, to: parseAddresses(to), cc: parseAddresses(cc), bcc: parseAddresses(bcc) } : null), [draft, to, cc, bcc])

  const save = useCallback(async () => {
    const d = current()
    if (!d || !dirty.current) return
    dirty.current = false
    const out = await run<Draft>('update_draft', { digId: d.digId, draft: d }, { successToast: false })
    if (out) {
      setSaved(out.result.updatedAt)
      void qc.invalidateQueries({ queryKey: qk.drafts(d.digId) })
    }
  }, [current, run, qc])

  const touch = () => {
    dirty.current = true
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(() => void save(), 800)
  }

  useEffect(() => {
    if (!open) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation()
        void save().then(close)
      }
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [open, save, close])

  if (!open || !draft) return null
  const digOptions = (digs.data ?? []).map((d) => ({ value: d.id, label: `${d.name} <${d.email}>` }))
  const recipients = parseAddresses(to)

  const send = async () => {
    await save()
    const d = current()!
    const out = await run('send_message', { digId: d.digId, draftId: d.id }, { successToast: 'Sent' })
    if (out) close()
  }
  const discard = async () => {
    const out = await run('delete_draft', { digId: draft.digId, draftId: draft.id }, { successToast: 'Draft discarded' })
    if (out) close()
  }

  return (
    <aside className="fixed bottom-0 right-4 z-30 flex w-[640px] max-w-[90vw] flex-col rounded-t-lg border border-b-0 border-line bg-paper shadow-2xl fade-in" aria-label="Composer" role="dialog">
      <div className="flex items-center justify-between border-b border-line px-3 py-2">
        <div className="text-sm font-semibold">{open.mode === 'reply' ? 'Reply' : open.mode === 'forward' ? 'Forward' : 'New message'}</div>
        <div className="flex items-center gap-2 text-xs text-clay">
          {saved ? <span>Draft saved {fmtDateTime(saved)}</span> : <span>Draft</span>}
          <button type="button" aria-label="Close composer" className="rounded p-0.5 hover:bg-sand" onClick={() => void save().then(close)}>
            <X className="size-4" />
          </button>
        </div>
      </div>
      <div className="flex flex-col gap-1.5 px-3 py-2 text-sm">
        <div className="flex items-center gap-2">
          <span className="w-12 text-xs text-clay">From</span>
          <Select label="Send from Dig" size="sm" value={draft.digId} onChange={(v) => { setDraft({ ...draft, digId: v }); touch() }} options={digOptions} className="flex-1" />
        </div>
        <label className="flex items-center gap-2">
          <span className="w-12 text-xs text-clay">To</span>
          <input className={inputClass} value={to} onChange={(e) => { setTo(e.target.value); touch() }} placeholder="name <email>, …" aria-label="To" />
        </label>
        <label className="flex items-center gap-2">
          <span className="w-12 text-xs text-clay">Cc</span>
          <input className={inputClass} value={cc} onChange={(e) => { setCc(e.target.value); touch() }} aria-label="Cc" />
          {!showBcc ? (
            <button type="button" className="text-xs text-clay hover:text-ink" onClick={() => setShowBcc(true)}>
              Bcc
            </button>
          ) : null}
        </label>
        {showBcc ? (
          <label className="flex items-center gap-2">
            <span className="w-12 text-xs text-clay">Bcc</span>
            <input className={inputClass} value={bcc} onChange={(e) => { setBcc(e.target.value); touch() }} aria-label="Bcc" />
          </label>
        ) : null}
        <label className="flex items-center gap-2">
          <span className="w-12 text-xs text-clay">Subject</span>
          <input className={inputClass} value={draft.subject} onChange={(e) => { setDraft({ ...draft, subject: e.target.value }); touch() }} aria-label="Subject" />
        </label>
        <textarea className={`${inputClass} h-64 resize-y mono text-[13px]`} value={draft.bodyText} onChange={(e) => { setDraft({ ...draft, bodyText: e.target.value }); touch() }} aria-label="Message body" placeholder="Write your message" />
      </div>
      <div className="flex items-center justify-between border-t border-line px-3 py-2">
        <Button size="sm" variant="ghost" icon={<Trash2 />} onClick={discard}>
          Discard
        </Button>
        <div className="flex items-center gap-2">
          <span className="text-xs text-clay">Sent mail cannot be unsent.</span>
          <Button size="sm" variant="primary" icon={<Send />} onClick={send} disabledReason={recipients.length ? null : 'Add at least one recipient'}>
            Send…
          </Button>
        </div>
      </div>
    </aside>
  )
}

/**
 * Director proposals, as cases to decide. Each PendingAction comes from
 * `list_pending` (the one source of truth, so a proposal survives a restart
 * and the rail badge agrees with this list), and shows its summary, the
 * reason, the change in one verb-plus-noun line, the Mesh text behind a
 * disclosure, and Preview impact before Apply.
 *
 * The UX study found proposals vanishing on relaunch, "Dismiss" doing
 * nothing server-side, and Apply reachable by a reflex press. So Decline is
 * an Action with an optional one-line reason typed in place, keys 1 / 2 / 3
 * act only on the card the person selected, and nothing autofocuses Apply.
 * Adjust hands the text to the Mesh editor and leaves the proposal pending.
 */
import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { Eye, MessageSquare, AlertTriangle } from 'lucide-react'
import type { PendingAction } from '@/bindings/PendingAction'
import type { SimulationReport } from '@/bindings/SimulationReport'
import { simulateMesh } from '@/lib/api'
import { cx, fmtRelative } from '@/lib/format'
import { usePending } from '@/lib/queries'
import { keyHint } from '@/lib/shortcuts'
import { useAction } from '@/lib/useAction'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Input } from '@/components/ui/Field'
import { Tip } from '@/components/ui/Tooltip'
import { ErrorCard } from '@/components/ErrorCard'
import { SimulationTable } from '@/features/meshes/SimulationTable'
import { RealArgumentRows } from '@/components/confirm/RealArguments'
import { impactLine, meshName, meshText, pendingVerb, realArgs } from '@/lib/pendingVerb'

/** What the Mesh editor receives from Adjust, via `location.state`. */
export interface MeshDraftState {
  draftText: string
  fromPendingId: string
  meshId: string | null
  meshName: string | null
}

export function ProposalsPanel() {
  const pending = usePending()
  const nav = useNavigate()
  const { conversationId } = useParams()
  const { run, apply } = useAction()
  const items = pending.data ?? []
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [decliningId, setDecliningId] = useState<string | null>(null)
  const [busyId, setBusyId] = useState<string | null>(null)
  const [errors, setErrors] = useState<Record<string, unknown>>({})
  const selected = items.find((p) => p.id === selectedId) ?? null

  useEffect(() => {
    // A proposal that was applied or declined takes its selection with it.
    if (selectedId && !items.some((p) => p.id === selectedId)) setSelectedId(null)
    if (decliningId && !items.some((p) => p.id === decliningId)) setDecliningId(null)
  }, [items, selectedId, decliningId])

  const fail = (id: string, e: unknown) => setErrors((s) => ({ ...s, [id]: e }))
  const clearError = (id: string) => setErrors((s) => ({ ...s, [id]: undefined }))

  const applyOne = async (p: PendingAction) => {
    if (busyId) return
    setBusyId(p.id)
    clearError(p.id)
    try {
      await apply(p, { successToast: 'Applied' })
    } catch (e) {
      fail(p.id, e)
    } finally {
      setBusyId(null)
    }
  }
  const adjustOne = (p: PendingAction) => {
    const text = meshText(p)
    if (!text) return
    const real = realArgs(p)
    const state: MeshDraftState = { draftText: text, fromPendingId: p.id, meshId: typeof real.meshId === 'string' ? real.meshId : null, meshName: meshName(p) }
    nav('/meshes', { state })
  }
  const declineOne = async (p: PendingAction, reason: string) => {
    if (busyId) return
    setBusyId(p.id)
    clearError(p.id)
    const why = reason.trim()
    try {
      await run('dismiss_pending', why ? { pendingId: p.id, reason: why } : { pendingId: p.id }, { toastErrors: false })
      setDecliningId(null)
    } catch (e) {
      fail(p.id, e)
    } finally {
      setBusyId(null)
    }
  }

  useShortcuts('director', {
    proposal_apply: () => selected && void applyOne(selected),
    proposal_adjust: () => selected && adjustOne(selected),
    proposal_decline: () => selected && setDecliningId(selected.id),
  })

  if (pending.error) {
    return (
      <section aria-label="Proposals" className="border-t border-line px-6 py-3">
        <ErrorCard error={pending.error} onRetry={() => pending.refetch()} compact />
      </section>
    )
  }
  if (!items.length) return null
  return (
    <section aria-label="Proposals" className="max-h-[45vh] shrink-0 overflow-auto border-t border-line bg-sand/40 px-6 py-3">
      <div className="mb-2 flex items-center gap-2 text-2xs font-semibold uppercase tracking-wide text-clay">
        <MessageSquare className="size-3" /> Proposals ({items.length})
        <span className="ml-auto font-normal normal-case tracking-normal">
          Select a proposal, then <kbd>{keyHint('proposal_apply')}</kbd> Apply · <kbd>{keyHint('proposal_adjust')}</kbd> Adjust · <kbd>{keyHint('proposal_decline')}</kbd> Decline
        </span>
      </div>
      <ul className="flex flex-col gap-3">
        {items.map((p) => (
          <ProposalCard
            key={p.id}
            p={p}
            selected={p.id === selectedId}
            onSelect={() => setSelectedId(p.id)}
            busy={busyId === p.id}
            error={errors[p.id]}
            onApply={() => void applyOne(p)}
            onAdjust={() => adjustOne(p)}
            declining={decliningId === p.id}
            onDeclineStart={() => setDecliningId(p.id)}
            onDeclineCancel={() => setDecliningId(null)}
            onDeclineConfirm={(reason) => void declineOne(p, reason)}
            onOpenConversation={p.principal.kind === 'director' && p.principal.conversation_id !== conversationId ? () => nav(`/director/${(p.principal as { conversation_id: string }).conversation_id}`) : null}
          />
        ))}
      </ul>
    </section>
  )
}

function ProposalCard({
  p,
  selected,
  onSelect,
  busy,
  error,
  onApply,
  onAdjust,
  declining,
  onDeclineStart,
  onDeclineCancel,
  onDeclineConfirm,
  onOpenConversation,
}: {
  p: PendingAction
  selected: boolean
  onSelect: () => void
  busy: boolean
  error: unknown
  onApply: () => void
  onAdjust: () => void
  declining: boolean
  onDeclineStart: () => void
  onDeclineCancel: () => void
  onDeclineConfirm: (reason: string) => void
  onOpenConversation: (() => void) | null
}) {
  const verb = pendingVerb(p)
  const text = meshText(p)
  const untrusted = (p.principal.kind === 'director' || p.principal.kind === 'expedition') && p.principal.read_untrusted
  const [report, setReport] = useState<SimulationReport | null>(null)
  const [simulating, setSimulating] = useState(false)
  const [simError, setSimError] = useState<unknown>(null)
  const [reason, setReason] = useState('')
  useEffect(() => {
    if (!declining) setReason('')
  }, [declining])

  const preview = async () => {
    if (!text || simulating) return
    setSimulating(true)
    setSimError(null)
    try {
      const real = realArgs(p)
      setReport(await simulateMesh({ text, meshId: typeof real.meshId === 'string' ? real.meshId : undefined }))
    } catch (e) {
      setSimError(e)
    } finally {
      setSimulating(false)
    }
  }
  const key = (id: string) => (selected ? ` (${keyHint(id)})` : '')

  return (
    <li
      tabIndex={0}
      aria-current={selected ? 'true' : undefined}
      aria-label={verb.line}
      onFocusCapture={onSelect}
      onClick={onSelect}
      className={cx('rounded-md border bg-paper p-3 text-xs focus:outline-none', selected ? 'border-ochre ring-1 ring-ochre/40' : 'border-line')}
    >
      <div className="flex items-start gap-2">
        <div className="min-w-0 flex-1">
          <div className="text-sm font-medium selectable">{p.summary}</div>
          <dl className="mt-1 grid grid-cols-[3.5rem_1fr] gap-x-2 gap-y-0.5">
            <dt className="text-clay">Reason</dt>
            <dd className="selectable">{p.reason}</dd>
            <dt className="text-clay">Change</dt>
            <dd className="font-medium selectable">{verb.line}</dd>
          </dl>
          <div className="mt-1 flex flex-wrap items-center gap-2 text-clay">
            <span>{fmtRelative(p.createdAt)}</span>
            {onOpenConversation ? (
              <button type="button" className="text-ochre hover:underline" onClick={onOpenConversation}>
                Open the conversation
              </button>
            ) : null}
          </div>
        </div>
        {untrusted ? (
          <Tip text="The Director read mail content in this task, so applying this needs your approval: the dialog shows the real arguments.">
            <span>
              <Chip tone="warn">
                <AlertTriangle className="size-3" /> read untrusted content
              </Chip>
            </span>
          </Tip>
        ) : null}
      </div>
      {text ? (
        <details className="mt-2">
          <summary className="cursor-default text-ochre">Mesh text ({text.split('\n').length} lines)</summary>
          <pre className="mt-1 max-h-64 overflow-auto rounded border border-line bg-sand p-2 mono text-xs selectable whitespace-pre">{text}</pre>
        </details>
      ) : null}
      <details className="mt-1">
        <summary className="cursor-default text-ochre">Real arguments</summary>
        <RealArgumentRows real={p.realArguments} omit={['text']} className="mt-1" />
      </details>
      {report ? (
        <div className="mt-2 rounded border border-line bg-sand p-2">
          <div className="mb-1 font-medium">{impactLine(report)}</div>
          <SimulationTable r={report} compact />
        </div>
      ) : null}
      {simError ? (
        <div className="mt-2">
          <ErrorCard error={simError} onRetry={() => void preview()} compact />
        </div>
      ) : null}
      {error ? (
        <div className="mt-2">
          <ErrorCard error={error} compact />
        </div>
      ) : null}
      <div className="mt-2 flex flex-wrap items-center gap-1">
        {text ? (
          <Button size="sm" icon={<Eye />} busy={simulating} onClick={() => void preview()} title="Run the Mesh text against recent mail and show what would change, before anything is applied">
            {report ? 'Preview again' : 'Preview impact'}
          </Button>
        ) : null}
        <Button size="sm" variant="primary" busy={busy} disabledReason={declining ? 'Finish or cancel the decline first' : null} onClick={onApply} title={`${verb.button}${key('proposal_apply')}`}>
          Apply
        </Button>
        <Button size="sm" disabledReason={text ? null : 'Only a Mesh proposal can be adjusted in the editor'} onClick={onAdjust} title={`Open the Mesh editor with this text; the proposal stays here${key('proposal_adjust')}`}>
          Adjust
        </Button>
        {!declining ? (
          <Button size="sm" variant="ghost" busy={busy} onClick={onDeclineStart} title={`Decline the proposal; nothing changes${key('proposal_decline')}`}>
            Decline
          </Button>
        ) : null}
      </div>
      {declining ? (
        <div className="mt-2 flex items-center gap-1" role="group" aria-label="Decline this proposal">
          <Input
            autoFocus
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            placeholder="Why? (optional, one line)"
            aria-label="Reason for declining"
            className="h-6 flex-1 py-0 text-xs"
            onKeyDown={(e) => {
              if (e.key === 'Enter') {
                e.preventDefault()
                onDeclineConfirm(reason)
              } else if (e.key === 'Escape') {
                e.preventDefault()
                onDeclineCancel()
              }
            }}
          />
          <Button size="sm" variant="danger" busy={busy} onClick={() => onDeclineConfirm(reason)} title="Decline (Enter)">
            Decline
          </Button>
          <Button size="sm" variant="ghost" onClick={onDeclineCancel} title="Keep the proposal (Esc)">
            Keep
          </Button>
        </div>
      ) : null}
    </li>
  )
}

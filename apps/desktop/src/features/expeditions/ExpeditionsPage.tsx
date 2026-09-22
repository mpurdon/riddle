/**
 * Expeditions (README "Expeditions"): durable delegated work with a
 * declared scope and budget, progress, evidence, findings and cancel.
 */
import { useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Compass, Plus, X } from 'lucide-react'
import type { Expedition } from '@/bindings/Expedition'
import type { ExpeditionStatus } from '@/bindings/ExpeditionStatus'
import { getExpedition, listExpeditions, qk } from '@/lib/api'
import { cx, fmtDuration, fmtRelative, fmtUsd, num } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import type { Tone } from '@/lib/selectors'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Dialog } from '@/components/ui/Dialog'
import { EmptyState } from '@/components/ui/EmptyState'
import { Field, Input, TextArea } from '@/components/ui/Field'
import { ProgressBar } from '@/components/ui/Progress'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'

const TONE: Record<ExpeditionStatus, Tone> = { planned: 'neutral', running: 'busy', paused: 'warn', completed: 'ok', partial: 'warn', cancelled: 'neutral', failed: 'bad' }

export default function ExpeditionsPage() {
  const { expeditionId } = useParams()
  const nav = useNavigate()
  const list = useQuery({ queryKey: qk.expeditions, queryFn: listExpeditions, refetchInterval: 5000 })
  const [starting, setStarting] = useState(false)
  if (list.error) return <ErrorCard error={list.error} onRetry={() => list.refetch()} />
  const items = list.data ?? []
  return (
    <div className="flex min-h-0 flex-1">
      <section className="flex w-[380px] shrink-0 flex-col border-r border-line">
        <PageHeader title="Expeditions" subtitle="Delegated work with a budget. Results become Finds." actions={<Button size="sm" icon={<Plus />} onClick={() => setStarting(true)}>Start</Button>} />
        <div className="min-h-0 flex-1 overflow-auto" role="listbox" aria-label="Expeditions">
          {list.data && !items.length ? <EmptyState icon={<Compass />} title="No Expeditions yet." body='Ask the Director for something that needs more than a search: "Find every subscription I have paid for this year." It declares scope and budget before it starts.' actions={<Button size="sm" onClick={() => setStarting(true)}>Start one</Button>} /> : null}
          {items.map((e) => {
            const done = e.plan.filter((p) => p.done).length
            return (
              <div key={e.id} role="option" aria-selected={e.id === expeditionId} onClick={() => nav(`/expeditions/${e.id}`)} className={cx('cursor-default border-b border-line/60 px-4 py-2', e.id === expeditionId ? 'row-selected' : 'hover:bg-sand/60')}>
                <div className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-sm font-medium">{e.goal}</span>
                  <Chip tone={TONE[e.status]}>{e.status}</Chip>
                </div>
                <div className="mt-1">
                  <ProgressBar done={done} total={e.plan.length} label={e.goal} />
                </div>
                <div className="mt-0.5 flex justify-between text-[11px] text-clay">
                  <span>{done}/{e.plan.length} steps · {fmtUsd(e.spent.costUsd)} of {fmtUsd(e.budget.costUsd)}</span>
                  <span>{fmtRelative(e.updatedAt)}</span>
                </div>
              </div>
            )
          })}
        </div>
      </section>
      <section className="flex min-w-0 flex-1 flex-col">{expeditionId ? <Detail key={expeditionId} id={expeditionId} /> : <EmptyState title="Select an Expedition" body="Plan, evidence, findings and budget." />}</section>
      {starting ? <StartDialog onClose={() => setStarting(false)} /> : null}
    </div>
  )
}

function Detail({ id }: { id: string }) {
  const q = useQuery({ queryKey: qk.expedition(id), queryFn: () => getExpedition(id), refetchInterval: 3000 })
  const { run } = useAction()
  const nav = useNavigate()
  const { digName } = useDigContext()
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  const e: Expedition | undefined = q.data
  if (!e) return <div className="p-6 text-sm text-clay">Loading…</div>
  const pct = (a: number, b: number) => (b ? Math.min(100, Math.round((a / b) * 100)) : 0)
  const nearing = e.spent.costUsd / e.budget.costUsd > 0.8 || num(e.spent.seconds) / num(e.budget.seconds) > 0.8 || e.spent.toolCalls / e.budget.toolCalls > 0.8
  const live = e.status === 'running' || e.status === 'planned' || e.status === 'paused'
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={e.goal}
        subtitle={
          <span className="flex items-center gap-2">
            <Chip tone={TONE[e.status]}>{e.status}</Chip>
            <span>{e.scope.digIds.map(digName).join(', ')}</span>
            {e.scope.since ? <span>· since {new Date(e.scope.since).toLocaleDateString()}</span> : null}
            <span>· tools: {e.scope.toolsAllowed.join(', ')}</span>
          </span>
        }
        actions={
          <Button size="sm" variant="danger" icon={<X />} disabledReason={live ? null : `Already ${e.status}`} onClick={() => run('cancel_expedition', { expeditionId: id }, { invalidate: [qk.expedition(id)] })}>
            Cancel
          </Button>
        }
      />
      <div className="min-h-0 flex-1 overflow-auto px-5 py-4 text-sm">
        {nearing && live ? <div className="mb-3 rounded border border-ochre/40 bg-ochre/10 px-3 py-2 text-xs">Approaching its budget. It stops at the limit and reports what it has.</div> : null}
        <div className="grid grid-cols-3 gap-3">
          {[
            ['Cost', fmtUsd(e.spent.costUsd), fmtUsd(e.budget.costUsd), pct(e.spent.costUsd, e.budget.costUsd)],
            ['Time', fmtDuration(e.spent.seconds), fmtDuration(e.budget.seconds), pct(num(e.spent.seconds), num(e.budget.seconds))],
            ['Tool calls', String(e.spent.toolCalls), String(e.budget.toolCalls), pct(e.spent.toolCalls, e.budget.toolCalls)],
          ].map(([k, a, b, p]) => (
            <div key={k as string} className="rounded-md border border-line p-3">
              <div className="text-xs text-clay">{k}</div>
              <div className="mt-0.5 font-medium">{a} <span className="font-normal text-ink-2">of {b}</span></div>
              <div className="mt-1"><ProgressBar done={p as number} total={100} label={`${k} used`} /></div>
            </div>
          ))}
        </div>
        <h3 className="mt-5 text-xs font-semibold uppercase tracking-wide text-clay">Plan</h3>
        <ol className="mt-1 flex flex-col gap-1">
          {e.plan.map((p) => (
            <li key={p.n} className={cx('flex items-start gap-2', p.done ? 'text-ink-2' : '')}>
              <span className={cx('mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full text-[10px]', p.done ? 'bg-moss/20 text-moss' : 'bg-sand-2 text-clay')}>{p.done ? '✓' : p.n}</span>
              <span>{p.text}</span>
            </li>
          ))}
        </ol>
        <h3 className="mt-5 text-xs font-semibold uppercase tracking-wide text-clay">Evidence ({e.evidence.length})</h3>
        <ul className="mt-1 flex flex-wrap gap-1">
          {e.evidence.map((ev, i) => (
            <li key={`${ev.kind}-${ev.id}-${i}`}>
              <button type="button" className="rounded border border-line bg-sand px-2 py-0.5 text-xs hover:bg-sand-2" onClick={() => nav(ev.kind === 'message' ? `/inbox?q=${ev.id}` : '/inbox')}>
                {ev.kind} {ev.id}
              </button>
            </li>
          ))}
          {!e.evidence.length ? <li className="text-xs text-clay">Nothing read yet.</li> : null}
        </ul>
        <h3 className="mt-5 text-xs font-semibold uppercase tracking-wide text-clay">Findings</h3>
        {e.findings.length ? (
          <ul className="mt-1 list-disc pl-5">
            {e.findings.map((f) => (
              <li key={f}>{f}</li>
            ))}
          </ul>
        ) : (
          <p className="mt-1 text-xs text-clay">{live ? 'Findings appear as steps complete.' : 'No findings.'}</p>
        )}
        {e.error ? <div className="mt-3 text-rust">Error: {e.error}</div> : null}
        {e.checkpoint ? <p className="mt-4 text-xs text-clay">Checkpointed; resumes from step {(e.checkpoint as { step?: number }).step ?? '?'} after a restart.</p> : null}
      </div>
    </div>
  )
}

function StartDialog({ onClose }: { onClose: () => void }) {
  const { run } = useAction()
  const nav = useNavigate()
  const { digs } = useDigContext()
  const [goal, setGoal] = useState('')
  const [cost, setCost] = useState('1.00')
  const [minutes, setMinutes] = useState('15')
  const [calls, setCalls] = useState('60')
  const [picked, setPicked] = useState<string[]>(digs.map((d) => d.id))
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Start an Expedition" description="Declare the scope and the budget before it starts. It reports progress in the Task Centre and can be cancelled.">
      <Field label="Goal">
        <TextArea autoFocus value={goal} onChange={(e) => setGoal(e.target.value)} placeholder="Go through everything related to the house renovation and tell me what we are still waiting for." />
      </Field>
      <div className="mt-3 grid grid-cols-3 gap-2">
        <Field label="Cost budget (USD)"><Input value={cost} onChange={(e) => setCost(e.target.value)} inputMode="decimal" /></Field>
        <Field label="Time (minutes)"><Input value={minutes} onChange={(e) => setMinutes(e.target.value)} inputMode="numeric" /></Field>
        <Field label="Tool calls"><Input value={calls} onChange={(e) => setCalls(e.target.value)} inputMode="numeric" /></Field>
      </div>
      <div className="mt-3">
        <div className="mb-1 text-xs font-medium text-ink-2">Digs in scope</div>
        <div className="flex flex-wrap gap-2">
          {digs.map((d) => (
            <label key={d.id} className="flex items-center gap-1 text-sm">
              <input type="checkbox" checked={picked.includes(d.id)} onChange={() => setPicked(picked.includes(d.id) ? picked.filter((x) => x !== d.id) : [...picked, d.id])} /> {d.name}
              {d.privacy === 'local_only' ? <span className="text-xs text-clay">(local-only: no LLM)</span> : null}
            </label>
          ))}
        </div>
      </div>
      <div className="mt-4 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>Cancel</Button>
        <Button
          variant="primary"
          disabledReason={!goal.trim() ? 'Describe the goal' : !picked.length ? 'Pick at least one Dig' : null}
          onClick={async () => {
            const out = await run<Expedition>('start_expedition', { goal: goal.trim(), scope: { digIds: picked, since: null, until: null, toolsAllowed: ['search', 'get_thread', 'get_message', 'list_finds'] }, budget: { costUsd: Number(cost) || 1, seconds: (Number(minutes) || 15) * 60, toolCalls: Number(calls) || 60 } })
            if (out) {
              onClose()
              nav(`/expeditions/${out.result.id}`)
            }
          }}
        >
          Start…
        </Button>
      </div>
    </Dialog>
  )
}

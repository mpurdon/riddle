/**
 * Demo Mode (§90): inject a fixture message and watch the pipeline with
 * real telemetry. Each step reads real data (message, trace, Activity) as
 * it becomes available; nothing here fabricates a stage.
 */
import { useEffect, useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Play } from 'lucide-react'
import { getMessage, listActivity, page, qk, readTrace } from '@/lib/api'
import { cx, fmtClockMs, fmtInt, fmtMs, fmtScore, fmtUsd } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { dispositionLabel } from '@/lib/selectors'
import { usePipeline } from '@/lib/stores'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Select } from '@/components/ui/Select'
import { Input } from '@/components/ui/Field'
import { Distribution, KV } from './shared'

const STEPS = ['Input', 'Deterministic facts', 'Jev', 'Semantic Signals', 'Mesh', 'Escalation?', 'Result', 'Action', 'Activity'] as const

export function DemoMode() {
  const { digs } = useDigContext()
  const { run } = useAction()
  const nav = useNavigate()
  const [digId, setDigId] = useState<string>('')
  const [template, setTemplate] = useState('review_request')
  const [subject, setSubject] = useState('')
  const [injected, setInjected] = useState<{ messageId: string; digId: string; traceId: string } | null>(null)
  const allStages = usePipeline((s) => s.entries)
  const stages = useMemo(() => allStages.filter((e) => e.messageId === injected?.messageId), [allStages, injected])
  const dig = digId || digs[0]?.id || ''
  const msg = useQuery({ queryKey: injected ? qk.message(injected.digId, injected.messageId) : ['message', 'none'], queryFn: () => getMessage(injected!.digId, injected!.messageId), enabled: !!injected })
  const done = stages.some((s) => s.stage.startsWith('Disposition'))
  const trace = useQuery({ queryKey: injected ? qk.trace(injected.digId, injected.traceId) : ['trace', 'none'], queryFn: () => readTrace(injected!.digId, injected!.traceId), enabled: !!injected && done, retry: 3 })
  const activity = useQuery({ queryKey: [...qk.activity, 'demo', injected?.messageId], queryFn: () => listActivity({ page: page(null, 10) }), enabled: !!injected && done, refetchInterval: done ? 2000 : false })
  const related = useMemo(() => (activity.data?.items ?? []).filter((a) => a.affected.some((e) => e.id === injected?.messageId)), [activity.data, injected])
  const t = trace.data
  const reached = (i: number): boolean => {
    switch (i) {
      case 0:
        return !!msg.data
      case 1:
        return stages.some((s) => s.stage === 'Facts extracted')
      case 2:
        return stages.some((s) => s.stage.startsWith('Jev'))
      case 3:
        return stages.some((s) => s.stage === 'Signals persisted')
      case 4:
        return stages.some((s) => s.stage.includes('Mesh') || s.stage.includes('confidence policy'))
      case 5:
        return stages.some((s) => s.stage.includes('Mesh') || s.stage.includes('confidence policy'))
      case 6:
        return !!t
      case 7:
        return done
      case 8:
        return related.length > 0
    }
    return false
  }
  useEffect(() => {
    if (done && !trace.data) void trace.refetch()
  }, [done]) // eslint-disable-line react-hooks/exhaustive-deps
  return (
    <div className="min-h-0 flex-1 overflow-auto p-5">
      <div className="flex flex-wrap items-end gap-2 rounded-md border border-line bg-sand p-3">
        <label className="text-xs">
          <span className="mb-1 block text-clay">Dig</span>
          <Select label="Dig" size="sm" value={dig} onChange={setDigId} options={digs.map((d) => ({ value: d.id, label: d.name }))} />
        </label>
        <label className="text-xs">
          <span className="mb-1 block text-clay">Template</span>
          <Select label="Template" size="sm" value={template} onChange={setTemplate} options={[{ value: 'review_request', label: 'Review request (→ Find: Review)' }, { value: 'ci_success', label: 'CI success (→ Spoil by Mesh)' }, { value: 'ambiguous', label: 'Ambiguous first-time sender (→ escalate)' }]} />
        </label>
        <label className="text-xs">
          <span className="mb-1 block text-clay">Subject (optional)</span>
          <Input value={subject} onChange={(e) => setSubject(e.target.value)} className="h-6 w-72 text-xs" />
        </label>
        <Button
          size="sm"
          variant="primary"
          icon={<Play />}
          disabledReason={dig ? null : 'Add a Dig first'}
          onClick={async () => {
            setInjected(null)
            const out = await run<{ messageId: string; digId: string; traceId: string }>('inject_fixture_message', { digId: dig, template, subject: subject || undefined }, { successToast: false })
            if (out) setInjected(out.result)
          }}
        >
          Inject fixture message
        </Button>
        <span className="text-xs text-clay">Uses real telemetry; the pipeline runs as it would for synced mail.</span>
      </div>

      <ol className="mt-5 grid grid-cols-9 gap-1">
        {STEPS.map((s, i) => (
          <li key={s} className={cx('rounded border px-2 py-1.5 text-center text-[11px]', reached(i) ? 'border-moss/40 bg-moss/10 text-ink' : 'border-line text-clay')}>
            {s}
          </li>
        ))}
      </ol>

      {injected ? (
        <div className="mt-5 grid grid-cols-2 gap-8">
          <div>
            <Step n={1} title="Input" on={reached(0)}>
              {msg.data ? <KV rows={[['Message', msg.data.id], ['Thread', msg.data.threadId], ['Subject', msg.data.subject], ['Received', fmtClockMs(msg.data.date)]]} /> : 'Waiting for the message to land…'}
            </Step>
            <Step n={2} title="Deterministic facts" on={reached(1)}>
              {t ? <div className="mono text-xs">{Object.entries(t.facts).filter(([, v]) => v === true).map(([k]) => k).join(', ') || 'none true'}</div> : stages.find((s) => s.stage === 'Facts extracted') ? 'Facts extracted; persisted with the trace.' : '…'}
            </Step>
            <Step n={3} title="Jev" on={reached(2)}>
              {t ? <KV rows={[['Engine', t.judgeIdentity ?? 'none'], ['Questions', String(t.cost.judgeQuestions)], ['Input tokens', fmtInt(t.cost.judgeInputTokens)], ['Latency', fmtMs(t.timing.judgeMs)], ['Cost', fmtUsd(t.cost.judgeCostUsd)]]} /> : stages.filter((s) => s.stage.startsWith('Jev')).map((s) => s.stage).join(' → ') || '…'}
            </Step>
            <Step n={4} title="Semantic Signals" on={reached(3)}>
              {t ? t.signals.map((s) => <div key={s.signalName} className="mb-1 text-xs"><span className="mono">{s.signalName}:v{s.signalVersion}</span><Distribution d={s.calibrated} /></div>) : '…'}
            </Step>
          </div>
          <div>
            <Step n={5} title="Mesh" on={reached(4)}>
              {t ? (t.meshName ? <pre className="whitespace-pre-wrap rounded border border-moss/40 bg-moss/10 p-2 mono text-xs">{t.meshName} v{t.meshVersion}{'\n'}{t.matchedRule} ✓</pre> : <span className="text-xs">No Mesh matched; confidence policy.</span>) : stages.find((s) => s.stage.includes('Mesh'))?.stage ?? '…'}
            </Step>
            <Step n={6} title="Escalation?" on={reached(5)}>
              {t ? (t.escalation ? <div className="text-xs">LLM required: <span className="font-medium text-ochre">YES</span> · {t.escalation.modelId} · {fmtMs(t.escalation.latencyMs)}<blockquote className="mt-1 border-l-2 border-ochre/40 pl-2 text-ink-2">{t.escalation.rationale}</blockquote></div> : <div className="text-xs">LLM required: <span className="font-medium">NO</span></div>) : '…'}
            </Step>
            <Step n={7} title="Result" on={reached(6)}>
              {t ? <div className="text-sm"><span className="font-semibold">{dispositionLabel(t.conclusion)}</span> <span className="text-xs text-clay">by {t.mechanism}{t.confidence !== null ? ` · ${fmtScore(t.confidence)}` : ''} · {fmtMs(t.timing.totalMs)} total</span></div> : '…'}
            </Step>
            <Step n={8} title="Action" on={reached(7)}>
              {t ? <div className="text-xs">{t.conclusion.kind === 'find' ? 'A Find was created (or joined).' : t.conclusion.kind === 'spoil' ? 'Moved to Spoil by the Sift; projection follows.' : t.conclusion.kind === 'unresolved' ? 'Nothing hidden: it waits in Unresolved for your decision.' : 'Filed into Strata.'}</div> : '…'}
            </Step>
            <Step n={9} title="Activity" on={reached(8)}>
              {related.length ? related.map((a) => <div key={a.id} className="text-xs"><Chip tone="neutral">{a.actor}</Chip> {a.summary}{a.provenance?.meshName ? ` · Mesh: ${a.provenance.meshName} v${a.provenance.meshVersion}` : ''}</div>) : done ? <span className="text-xs text-clay">No Activity entry: Finds and Filed decisions are recorded on the trace; Spoil moves produce an entry.</span> : '…'}
              {t ? <div className="mt-2 flex gap-2"><Button size="sm" variant="ghost" onClick={() => nav(`/developer/messages/${t.digId}/${t.id}`)}>Open in the Message Inspector</Button><Button size="sm" variant="ghost" onClick={() => nav(`/inbox/${t.digId}/${t.messageId}`)}>Open the message</Button></div> : null}
            </Step>
          </div>
        </div>
      ) : (
        <p className="mt-5 text-sm text-clay">Inject a message to begin. Then correct the result, edit the Mesh, and replay: the Message Inspector and Mesh Simulator pick up where this stepper stops.</p>
      )}
    </div>
  )
}

function Step({ n, title, on, children }: { n: number; title: string; on: boolean; children: React.ReactNode }) {
  return (
    <section className={cx('mb-3 rounded border p-3', on ? 'border-line' : 'border-line/50 opacity-60')}>
      <div className="mb-1 flex items-center gap-2 text-xs font-semibold"><span className={cx('flex size-4 items-center justify-center rounded-full text-[10px]', on ? 'bg-moss/20 text-moss' : 'bg-sand-2 text-clay')}>{n}</span>{title}</div>
      <div className="text-xs">{children}</div>
    </section>
  )
}

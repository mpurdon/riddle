/**
 * Message Inspector (§68), Raw Jev Inspector (§69), Escalation Inspector
 * (§77) and Exportable Traces (§89), all reading `read_trace`.
 */
import { useMemo, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { Download } from 'lucide-react'
import type { DecisionTrace } from '@/bindings/DecisionTrace'
import type { DecisionPreview } from '@/bindings/DecisionPreview'
import { decidePreview, qk, readTrace } from '@/lib/api'
import { cx, fmtInt, fmtMs, fmtScore, fmtUsd, num, safeJson } from '@/lib/format'
import { dispositionLabel } from '@/lib/selectors'
import { jevRequest, jevResponse, outcomeLine, redactTrace } from '@/lib/traces'
import { toast } from '@/lib/stores'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Tabs, TabPanel } from '@/components/ui/Tabs'
import { ErrorCard } from '@/components/ErrorCard'
import { Distribution, JsonBlock, KV, Section, TracePicker, useRecentTraces } from './shared'

function useSelectedTrace(traces: DecisionTrace[] | undefined, routeDig?: string, routeTrace?: string) {
  const [picked, setPicked] = useState<DecisionTrace | null>(null)
  const routed = useQuery({ queryKey: routeDig && routeTrace ? qk.trace(routeDig, routeTrace) : ['trace', 'none'], queryFn: () => readTrace(routeDig!, routeTrace!), enabled: !!routeDig && !!routeTrace })
  const trace = picked ?? routed.data ?? traces?.[0] ?? null
  return { trace, setPicked, error: routed.error }
}

export function MessageInspector() {
  const { digId, traceId } = useParams()
  const nav = useNavigate()
  const traces = useRecentTraces()
  const { trace, error } = useSelectedTrace(traces.data, digId, traceId)
  const [tab, setTab] = useState('pretty')
  if (error) return <ErrorCard error={error} />
  return (
    <div className="flex min-h-0 flex-1">
      <TracePicker traces={traces.data ?? []} value={trace?.id ?? null} onPick={(t) => nav(`/developer/messages/${t.digId}/${t.id}`)} />
      {trace ? (
        <div className="flex min-w-0 flex-1 flex-col">
          <Tabs value={tab} onChange={setTab} className="min-h-0 flex-1" tabs={[{ value: 'pretty', label: 'Inspector' }, { value: 'jev', label: 'Raw Jev' }, { value: 'export', label: 'Export' }]}>
            <TabPanel value="pretty" className="p-5">
              <TraceView t={trace} />
            </TabPanel>
            <TabPanel value="jev" className="p-5">
              <RawJev t={trace} />
            </TabPanel>
            <TabPanel value="export" className="p-5">
              <ExportView t={trace} />
            </TabPanel>
          </Tabs>
        </div>
      ) : (
        <div className="p-6 text-sm text-clay">Pick a decision.</div>
      )}
    </div>
  )
}

/**
 * "Preview a re-decision": today's rules run over the message without
 * saving, and the diff in one sentence beside what stands. The button
 * replaces sending a message back to Sift just to see what would happen.
 */
function PreviewSection({ t }: { t: DecisionTrace }) {
  const [preview, setPreview] = useState<DecisionPreview | null>(null)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const run = async () => {
    setBusy(true)
    setError(null)
    try {
      setPreview(await decidePreview(t.digId, t.messageId))
    } catch (e) {
      setError((e as { message?: string })?.message ?? String(e))
    } finally {
      setBusy(false)
    }
  }
  return (
    <Section title="Preview">
      <div className="text-xs text-ink-2">Decide it again under today's Meshes, lists and Signals. Nothing is written; a fresh judge call is booked as usage like any other.</div>
      <Button size="sm" variant="ghost" className="mt-1" onClick={run} disabled={busy}>
        {busy ? 'Deciding…' : 'Preview a re-decision'}
      </Button>
      {error ? <div className="mt-1 text-xs text-rust">{error}</div> : null}
      {preview ? (
        <div className="mt-2 text-xs" data-testid="preview-result">
          <div className="flex items-center gap-2">
            <Chip tone={preview.changed ? 'warn' : 'ok'}>{preview.changed ? 'would change' : 'unchanged'}</Chip>
            <span className="font-medium selectable">{preview.diff}</span>
          </div>
          <div className="mt-1 text-ink-2">Signals: {preview.note}.</div>
          {preview.wouldTrace.meshOutcomes.length ? (
            <ul className="mt-1 flex flex-col gap-0.5 text-2xs text-ink-2 mono">
              {preview.wouldTrace.meshOutcomes.filter((o) => o.outcome.kind !== 'not_reached').map((o) => (
                <li key={`${o.meshId}@${o.version}`}>{outcomeLine(o)}</li>
              ))}
            </ul>
          ) : null}
          <div className="mt-1 text-ink-2">Preview pass {preview.wouldTrace.passId}. The stored decision stands.</div>
        </div>
      ) : null}
    </Section>
  )
}

export function TraceView({ t }: { t: DecisionTrace }) {
  const nav = useNavigate()
  const facts = Object.entries(t.facts).filter(([k]) => k !== 'deadlines')
  const ran = t.meshOutcomes.filter((o) => o.outcome.kind !== 'not_reached')
  const notReached = t.meshOutcomes.length - ran.length
  return (
    <div className="grid grid-cols-2 gap-x-8">
      <div>
        <Section title="Message">
          <KV rows={[['Message', t.messageId], ['Dig', t.digId], ['Trace', t.id], ['Pass', t.passId || '—'], ['Received', t.createdAt], ['Build', t.buildStamp], ['Strategy', t.strategy], ['Shadow', t.shadow ? 'yes: recorded, not applied' : 'no']]} />
          <Button size="sm" variant="ghost" className="mt-2" onClick={() => nav(`/inbox/${t.digId}/${t.messageId}`)}>Open the message</Button>
        </Section>
        <Section title="Deterministic facts">
          <div className="grid grid-cols-2 gap-x-4 text-xs">
            {facts.map(([k, v]) => (
              <div key={k} className="flex justify-between border-b border-line/40 py-px">
                <span className="mono text-ink-2">{k}</span>
                <span className={cx('mono truncate pl-2', v === true && 'text-moss', v === false && 'text-clay')} title={typeof v === 'object' && v ? JSON.stringify(v) : undefined}>{typeof v === 'object' && v ? `${Object.keys(v).length} fields` : String(v ?? '—')}</span>
              </div>
            ))}
            <div className="flex justify-between border-b border-line/40 py-px">
              <span className="mono text-ink-2">deadlines</span>
              <span className="mono">{t.facts.deadlines.map((d) => `${d.text} (${num(d.daysFromNow)}d)`).join('; ') || '—'}</span>
            </div>
          </div>
          {t.gate ? <div className="mt-2 text-xs"><Chip tone="warn">gate</Chip> {t.gate}</div> : null}
        </Section>
        <Section title="Jev Signals">
          {t.signals.length ? (
            <div className="flex flex-col gap-2">
              {t.signals.map((s) => (
                <div key={`${s.signalName}@${s.signalVersion}`} className="rounded border border-line p-2">
                  <div className="flex items-center gap-2 text-xs">
                    <span className="mono font-medium">{s.signalName}:v{s.signalVersion}</span>
                    <Chip tone={s.cached ? 'neutral' : 'busy'}>{s.cached ? 'cached' : `fresh · ${fmtMs(s.latencyMs)} · ${fmtInt(s.inputTokens)} tok`}</Chip>
                    <span className="text-clay">{s.engineIdentity}</span>
                  </div>
                  <div className="mt-1"><Distribution d={s.calibrated} /></div>
                </div>
              ))}
            </div>
          ) : (
            <span className="text-xs text-clay">No Signals evaluated: the gate decided before judgment.</span>
          )}
        </Section>
      </div>
      <div>
        <Section title="Mesh">
          {t.meshName ? (
            <div className="text-xs">
              <div className="mono font-medium">{t.meshName} v{t.meshVersion}</div>
              <pre className="mt-1 whitespace-pre-wrap rounded border border-moss/40 bg-moss/10 p-2 mono">{t.matchedRule} ✓</pre>
              {t.guardVetoes.length ? <div className="mt-1 text-rust">Guard vetoes: {t.guardVetoes.join('; ')}</div> : null}
              <Button size="sm" variant="ghost" className="mt-1" onClick={() => nav(`/meshes/${t.meshId}`)}>Open Mesh</Button>
            </div>
          ) : (
            <span className="text-xs text-clay">No Mesh matched; the confidence policy decided{t.escalation ? ' and escalated' : ''}.</span>
          )}
          <div className="mt-2 text-sm">→ <span className="font-semibold">{dispositionLabel(t.conclusion)}</span> <span className="text-xs text-clay">by {t.mechanism}{t.confidence !== null ? ` · confidence ${fmtScore(t.confidence)}` : ''}{t.margin != null ? ` · margin ${fmtScore(t.margin)}` : ''}</span></div>
          {ran.length ? (
            <div className="mt-2 text-xs" data-testid="mesh-outcomes">
              <div className="font-medium text-ink-2">Meshes, in the order they ran</div>
              <ul className="mt-1 flex flex-col gap-0.5 text-2xs mono">
                {ran.map((o) => (
                  <li key={`${o.meshId}@${o.version}`} className={cx(o.outcome.kind === 'matched' ? 'text-moss' : 'text-ink-2')}>{outcomeLine(o)}</li>
                ))}
              </ul>
              {notReached ? <div className="mt-0.5 text-2xs text-clay">{notReached} not reached</div> : null}
            </div>
          ) : null}
          {t.listResults.length ? (
            <div className="mt-2 text-xs">
              <div className="font-medium text-ink-2">Lists, as they read then</div>
              <ul className="mt-1 flex flex-col gap-0.5 text-2xs mono text-ink-2">
                {t.listResults.map((l, i) => (
                  <li key={i}>{l.list} · {l.value} · {l.matched ? 'on it' : 'not on it'}</li>
                ))}
              </ul>
            </div>
          ) : null}
          {t.policy ? (
            <div className="mt-2 text-2xs text-ink-2">
              Policy then: margins Find {fmtScore(t.policy.marginFind)}, Filed {fmtScore(t.policy.marginFiled)}, Spoil {fmtScore(t.policy.marginSpoil)}, Contamination {fmtScore(t.policy.marginContamination)}
              {t.policy.escalateNonEnglish ? '; non-English mail escalates' : ''}
            </div>
          ) : null}
        </Section>
        <PreviewSection t={t} />
        <Section title="Escalation">
          {t.escalation ? (
            <div className="text-xs">
              <div>LLM required: <span className="font-medium text-ochre">YES</span></div>
              <div className="text-ink-2">Reason: {t.escalation.reason}</div>
              <KV rows={[['Model', t.escalation.modelId], ['Latency', fmtMs(t.escalation.latencyMs)]]} />
              <div className="mt-1 text-ink-2">Refined Signals:</div>
              {t.escalation.refined.map((r) => (
                <div key={r.signalName} className="mt-1 flex items-center gap-2"><span className="w-40 mono">{r.signalName}</span><Distribution d={r.calibrated} /></div>
              ))}
              <blockquote className="mt-2 border-l-2 border-ochre/40 pl-2 text-ink-2">{t.escalation.rationale}</blockquote>
            </div>
          ) : (
            <div className="text-xs">LLM required: <span className="font-medium">NO</span><div className="text-ink-2">Reason: {t.gate ? 'deterministic gate' : t.meshName ? 'high-confidence Mesh result' : 'Signals above the auto threshold'}</div></div>
          )}
        </Section>
        <Section title="Timing">
          <table className="text-xs">
            <tbody>
              {(['syncMs', 'factsMs', 'judgeMs', 'meshMs', 'llmMs', 'persistMs'] as const).map((k) => (
                <tr key={k}><td className="w-20 py-px text-ink-2">{k.replace('Ms', '')}</td><td className="text-right tabular-nums">{fmtInt(t.timing[k])}ms</td></tr>
              ))}
              <tr className="border-t border-line font-medium"><td className="py-px">Total</td><td className="text-right tabular-nums">{fmtInt(t.timing.totalMs)}ms</td></tr>
            </tbody>
          </table>
        </Section>
        <Section title="Cost">
          <KV rows={[['Jev input tokens', fmtInt(t.cost.judgeInputTokens)], ['Jev questions', String(t.cost.judgeQuestions)], ['Jev cost', fmtUsd(t.cost.judgeCostUsd)], ['LLM tokens', `${fmtInt(t.cost.llmInputTokens)} in / ${fmtInt(t.cost.llmOutputTokens)} out`], ['LLM cost', fmtUsd(t.cost.llmCostUsd)], ['Total', t.cost.judgeCostUsd === null || t.cost.llmCostUsd === null ? 'unpriced' : fmtUsd((t.cost.judgeCostUsd ?? 0) + (t.cost.llmCostUsd ?? 0))]]} />
        </Section>
      </div>
    </div>
  )
}

export function RawJev({ t }: { t: DecisionTrace }) {
  const [mode, setMode] = useState<'pretty' | 'raw'>('pretty')
  const req = useMemo(() => jevRequest(t), [t])
  const res = useMemo(() => jevResponse(t), [t])
  return (
    <div>
      <div className="mb-3 flex gap-1">
        <Button size="sm" variant={mode === 'pretty' ? 'primary' : 'secondary'} onClick={() => setMode('pretty')}>Pretty</Button>
        <Button size="sm" variant={mode === 'raw' ? 'primary' : 'secondary'} onClick={() => setMode('raw')}>Raw</Button>
        <span className="ml-2 self-center text-xs text-clay">Secrets and credentials are never part of a trace.</span>
      </div>
      {mode === 'raw' ? (
        <div className="grid grid-cols-2 gap-4">
          <JsonBlock value={req} label="request" />
          <JsonBlock value={res} label="response" />
        </div>
      ) : (
        <div className="grid grid-cols-2 gap-8">
          <Section title="Request">
            <KV rows={[['model', req.model], ['Signal Set', req.signalSet.join(', ')], ['Signal versions', Object.entries(req.signalVersions).map(([k, v]) => `${k}@v${v}`).join(', ')], ['input state', `${req.inputState.messageId} · ctx ${req.inputState.contextHash ?? '—'} · facts: ${req.inputState.facts.join(', ') || 'none'}`], ['questions', String(req.questions.length)], ['choices', req.questions.map((q) => `${q.name}[${q.choices.join('/')}]`).join(' ')], ['configuration', `${req.configuration.freshQuestions} fresh, ${req.configuration.cachedQuestions} cached`], ['input tokens', fmtInt(req.inputTokens)], ['timestamp', req.timestamp]]} />
          </Section>
          <Section title="Response">
            <div className="flex flex-col gap-2">
              {res.raw.map((r) => (
                <div key={`${r.name}@${r.version}`} className="text-xs">
                  <span className="mono font-medium">{r.name}:v{r.version}</span> → {r.selected}
                  <div className="mt-0.5"><Distribution d={r.distribution} /></div>
                </div>
              ))}
            </div>
            <div className="mt-3"><KV rows={[['latency', fmtMs(res.latencyMs)], ['token usage', fmtInt(res.tokenUsage.input)], ['cost', fmtUsd(res.costUsd)], ['request ids', res.requestHashes.join(', ') || '—']]} /></div>
          </Section>
        </div>
      )}
    </div>
  )
}

export function RawJevInspector() {
  const traces = useRecentTraces()
  const { trace, setPicked } = useSelectedTrace(traces.data)
  return (
    <div className="flex min-h-0 flex-1">
      <TracePicker traces={traces.data ?? []} value={trace?.id ?? null} onPick={setPicked} filter={(t) => t.signals.length > 0} title="Jev evaluations" />
      <div className="min-h-0 flex-1 overflow-auto p-5">{trace ? <RawJev t={trace} /> : <span className="text-sm text-clay">Pick an evaluation.</span>}</div>
    </div>
  )
}

export function EscalationInspector() {
  const traces = useRecentTraces()
  const { trace, setPicked } = useSelectedTrace(traces.data?.filter((t) => t.escalation))
  const THRESHOLD = 0.8
  return (
    <div className="flex min-h-0 flex-1">
      <TracePicker traces={traces.data ?? []} value={trace?.id ?? null} onPick={setPicked} filter={(t) => !!t.escalation} title="Escalations" />
      <div className="min-h-0 flex-1 overflow-auto p-5">
        {trace?.escalation ? (
          <div className="grid grid-cols-2 gap-8">
            <Section title="Jev, before escalation">
              {trace.signals.map((s) => (
                <div key={s.signalName} className="mb-2 text-xs">
                  <div className="mono font-medium">{s.signalName}:v{s.signalVersion}</div>
                  <Distribution d={s.calibrated} threshold={THRESHOLD} />
                </div>
              ))}
              <div className="mt-2 text-xs">Auto threshold: <span className="mono">{THRESHOLD}</span></div>
              <div className="mt-1 text-sm font-semibold text-ochre">RESULT: ESCALATE</div>
              <div className="text-xs text-ink-2">{trace.escalation.reason}</div>
            </Section>
            <Section title="LLM verification">
              <KV rows={[['Model', trace.escalation.modelId], ['Context supplied', `message, thread state, ${trace.signals.length} Signal distributions, ${trace.meshName ? `Mesh ${trace.meshName} v${trace.meshVersion}` : 'no applicable Mesh'}`], ['Decision', dispositionLabel(trace.conclusion)], ['Cost', fmtUsd(trace.cost.llmCostUsd)], ['Latency', fmtMs(trace.escalation.latencyMs)]]} />
              <div className="mt-2 text-xs text-ink-2">Refined Signals:</div>
              {trace.escalation.refined.map((r) => (
                <div key={r.signalName} className="mt-1 text-xs"><span className="mono">{r.signalName}</span><Distribution d={r.calibrated} threshold={THRESHOLD} /></div>
              ))}
              <blockquote className="mt-2 border-l-2 border-ochre/40 pl-2 text-xs text-ink-2">{trace.escalation.rationale}</blockquote>
              <p className="mt-2 text-xs text-clay">The LLM refines Signals and gives a rationale; it never returns a disposition directly. If it cannot resolve the message either, the answer is Unresolved.</p>
            </Section>
          </div>
        ) : (
          <span className="text-sm text-clay">No escalations in the recent traces. Escalation happens when Signals fall between the auto thresholds.</span>
        )}
      </div>
    </div>
  )
}

export function ExportView({ t }: { t: DecisionTrace }) {
  const redacted = useMemo(() => redactTrace(t), [t])
  const download = () => {
    const blob = new Blob([safeJson(redacted)], { type: 'application/json' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `riddle-trace-${t.id}.json`
    a.click()
    URL.revokeObjectURL(url)
    toast('Trace exported', { body: 'Redacted: identifiers, facts, distributions, timings and costs only.', kind: 'success' })
  }
  return (
    <div>
      <div className="mb-3 flex items-center gap-2">
        <Button size="sm" icon={<Download />} onClick={download}>Download redacted JSON</Button>
        <span className="text-xs text-clay">Redaction is the default: no subjects, bodies, addresses, model text or tokens.</span>
      </div>
      <JsonBlock value={redacted} label="redacted trace" />
    </div>
  )
}

export function ExportTraces() {
  const traces = useRecentTraces()
  const { trace, setPicked } = useSelectedTrace(traces.data)
  return (
    <div className="flex min-h-0 flex-1">
      <TracePicker traces={traces.data ?? []} value={trace?.id ?? null} onPick={setPicked} />
      <div className="min-h-0 flex-1 overflow-auto p-5">{trace ? <ExportView t={trace} /> : null}</div>
    </div>
  )
}

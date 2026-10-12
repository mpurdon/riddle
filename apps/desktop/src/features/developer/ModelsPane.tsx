/**
 * Models (Developer): which model suits which job, measured on this
 * mailbox (`riddle_app::comparison`). The roster says which candidates the
 * account may call (Bedrock lists models weeks before an account can use
 * them, so it is re-checked every six hours); the scoreboard scores
 * escalation against the person's own decisions and the Director by the
 * person's picks; the side-by-side views show where models disagree. Every
 * output is a record: nothing a comparison produces is applied.
 */
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import type { BenchModel } from '@/bindings/BenchModel'
import type { BenchOutput } from '@/bindings/BenchOutput'
import type { BenchScore } from '@/bindings/BenchScore'
import type { BenchTask } from '@/bindings/BenchTask'
import type { Disposition } from '@/bindings/Disposition'
import { comparisonOverview, comparisonRows, directorComparisons, qk } from '@/lib/api'
import { cx, fmtInt, fmtUsd } from '@/lib/format'
import { useSettings } from '@/lib/queries'
import { dispositionLabel, sameDisposition } from '@/lib/selectors'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Select } from '@/components/ui/Select'
import { Switch } from '@/components/ui/Switch'
import { Section } from './shared'

const ACCESS: Record<BenchModel['access'], { label: string; tone: 'ok' | 'warn' | 'bad' | 'neutral' }> = {
  usable: { label: 'Usable', tone: 'ok' },
  not_enabled: { label: 'Not enabled', tone: 'warn' },
  failing: { label: 'Failing', tone: 'bad' },
  unchecked: { label: 'Not checked', tone: 'neutral' },
}

const pct = (n: number, of: number) => (of ? `${Math.round((n / of) * 100)}%` : '—')

/** Shorter names in the side-by-side columns. */
export function labelOf(models: BenchModel[], id: string): string {
  return models.find((m) => m.modelId === id)?.label ?? id
}

export function ModelsPane() {
  const overview = useQuery({ queryKey: [...qk.comparison, 'overview'], queryFn: comparisonOverview, refetchInterval: 4000 })
  const settings = useSettings()
  const { run } = useAction()
  const [count, setCount] = useState('20')
  const o = overview.data
  const models = o?.models ?? []
  const s = settings.data
  const useFor = (task: BenchTask, modelId: string) => {
    if (!s) return
    const llm = task === 'escalation' ? { ...s.llm, escalationModel: modelId } : { ...s.llm, directorModel: modelId }
    void run('save_settings', { settings: { ...s, llm } }, { invalidate: [qk.comparison], successToast: `${labelOf(models, modelId)} now does ${task === 'escalation' ? 'escalation' : 'the Director'}` })
  }
  const setChallengers = (on: boolean) => s && void run('save_settings', { settings: { ...s, comparison: { ...s.comparison, challengers: on } } }, { invalidate: [qk.comparison], successToast: on ? 'Challengers on' : 'Challengers off' })
  return (
    <div className="p-5">
      <Section
        title="Models"
        right={
          o ? (
            <span className="flex items-center gap-3 text-xs text-ink-2">
              {o.running ? <Chip tone="busy">Comparing…</Chip> : null}
              <span>
                Spent today {fmtUsd(o.spentTodayUsd)} of {fmtUsd(o.dailyCapUsd)}
              </span>
              <label className="flex items-center gap-1.5">
                <Switch checked={o.shadow} onChange={setChallengers} label="Run challengers beside every real escalation" />
                Challengers on every escalation
              </label>
            </span>
          ) : null
        }
      >
        <p className="mb-2 text-xs text-ink-2">Every usable model can answer the same mail. Escalation is scored against your own decisions; the Director by your picks. Outputs are recorded, never applied.</p>
        <table className="w-full text-sm" aria-label="Model roster">
          <thead>
            <tr className="border-b border-line text-left text-2xs uppercase tracking-wide text-clay">
              <th className="py-1 font-medium">Model</th>
              <th className="py-1 font-medium">Access</th>
              <th className="py-1 text-right font-medium">Price in / out</th>
              <th className="py-1 text-right font-medium">Answer time</th>
              <th className="py-1 pl-4 font-medium">Does now</th>
              <th className="py-1" />
            </tr>
          </thead>
          <tbody>
            {models.map((m) => (
              <tr key={m.modelId} className="border-b border-line/50">
                <td className="py-1">
                  <span className="font-medium">{m.label}</span> <span className="text-xs text-clay">{m.vendor}</span>
                </td>
                <td className="py-1">
                  <span title={m.accessDetail ?? undefined}>
                    <Chip tone={ACCESS[m.access].tone}>{ACCESS[m.access].label}</Chip>
                  </span>
                </td>
                <td className="py-1 text-right text-xs tabular-nums">{m.priceInPerMtok != null ? `$${m.priceInPerMtok} / $${m.priceOutPerMtok} per M` : 'unpriced'}</td>
                <td className="py-1 text-right text-xs tabular-nums">{m.probeLatencyMs != null ? `${fmtInt(m.probeLatencyMs)} ms` : '—'}</td>
                <td className="py-1 pl-4 text-xs">{m.usedFor.map((t) => (t === 'escalation' ? 'Escalation' : 'Director')).join(', ') || '—'}</td>
                <td className="py-1 text-right">
                  {m.access === 'usable' ? (
                    <span className="flex justify-end gap-1">
                      <Button size="sm" variant="ghost" disabledReason={m.usedFor.includes('escalation') ? 'It does escalation now' : null} onClick={() => useFor('escalation', m.modelId)}>
                        Use for escalation
                      </Button>
                      <Button size="sm" variant="ghost" disabledReason={m.usedFor.includes('director') ? 'It is the Director now' : null} onClick={() => useFor('director', m.modelId)}>
                        Use for Director
                      </Button>
                    </span>
                  ) : null}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        <div className="mt-2 flex items-center gap-2">
          <Button size="sm" onClick={() => void run('check_model_access', {}, { invalidate: [qk.comparison] })}>
            Check access now
          </Button>
          <span className="text-xs text-clay">Riddle re-checks every six hours, so a model joins the day the account may call it.</span>
        </div>
      </Section>

      <Section
        title="Escalation"
        right={
          <span className="flex items-center gap-2">
            <Select label="Messages" size="sm" value={count} onChange={setCount} options={['10', '20', '50'].map((v) => ({ value: v, label: `${v} messages` }))} />
            <Button size="sm" variant="primary" onClick={() => void run('run_model_benchmark', { count: Number(count) }, { invalidate: [qk.comparison] })}>
              Run on decided mail
            </Button>
          </span>
        }
      >
        <Scoreboard task="escalation" scores={o?.scores ?? []} models={models} />
        <EscalationRows models={models} />
      </Section>

      <Section
        title="Director"
        right={
          <Button size="sm" variant="primary" onClick={() => void run('run_director_comparison', { count: 5 }, { invalidate: [qk.comparison] })}>
            Replay my questions
          </Button>
        }
      >
        <Scoreboard task="director" scores={o?.scores ?? []} models={models} />
        <DirectorRows models={models} />
      </Section>
    </div>
  )
}

function Scoreboard({ task, scores, models }: { task: BenchTask; scores: BenchScore[]; models: BenchModel[] }) {
  const rows = scores.filter((s) => s.task === task)
  if (!rows.length) return <p className="mb-2 text-xs text-clay">No runs yet.</p>
  return (
    <table className="mb-3 w-full text-sm" aria-label={`${task} scores`}>
      <thead>
        <tr className="border-b border-line text-left text-2xs uppercase tracking-wide text-clay">
          <th className="py-1 font-medium">Model</th>
          <th className="py-1 text-right font-medium">Runs</th>
          {task === 'escalation' ? (
            <>
              <th className="py-1 text-right font-medium" title="Agrees with your own decision">Right</th>
              <th className="py-1 text-right font-medium" title="Agrees with what Riddle decided">Same as now</th>
            </>
          ) : (
            <th className="py-1 text-right font-medium">Your picks</th>
          )}
          <th className="py-1 text-right font-medium">Avg time</th>
          <th className="py-1 text-right font-medium">Per 1,000</th>
          <th className="py-1 text-right font-medium">Errors</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((s) => (
          <tr key={s.modelId} className="border-b border-line/50 tabular-nums">
            <td className="py-1">{labelOf(models, s.modelId)}</td>
            <td className="py-1 text-right">{fmtInt(s.runs)}</td>
            {task === 'escalation' ? (
              <>
                <td className="py-1 text-right" title={`${s.correct} of ${s.judged} you decided`}>
                  {pct(s.correct, s.judged)} <span className="text-xs text-clay">of {fmtInt(s.judged)}</span>
                </td>
                <td className="py-1 text-right">{pct(s.agreesWithCurrent, s.runs - s.errors)}</td>
              </>
            ) : (
              <td className="py-1 text-right">{fmtInt(s.preferred)}</td>
            )}
            <td className="py-1 text-right">{fmtInt(s.avgLatencyMs)} ms</td>
            <td className="py-1 text-right">{s.costPer1k != null ? fmtUsd(s.costPer1k) : 'unpriced'}</td>
            <td className={cx('py-1 text-right', s.errors ? 'text-rust' : 'text-clay')}>{fmtInt(s.errors)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

function OutputChip({ o, truth }: { o: BenchOutput; truth: Disposition | null }) {
  if (o.error) return <Chip tone="bad">{o.error.slice(0, 40)}</Chip>
  const tone = !truth ? 'neutral' : sameDisposition(truth, o.disposition) ? 'ok' : 'bad'
  return (
    <span title={o.text}>
      <Chip tone={tone}>{dispositionLabel(o.disposition)}</Chip>
    </span>
  )
}

function EscalationRows({ models }: { models: BenchModel[] }) {
  const q = useQuery({ queryKey: [...qk.comparison, 'rows'], queryFn: () => comparisonRows(50), refetchInterval: 4000 })
  const { run } = useAction()
  // Deciding here is a real correction: it answers the comparison and teaches Riddle.
  const decide = (digId: string, messageId: string, d: Disposition) => void run('set_disposition', { digId, messageId, disposition: d, findType: d.kind === 'find' ? d.find_type : undefined }, { digId, invalidate: [qk.comparison] })
  const [onlyDisagreements, setOnly] = useState(true)
  const rows = (q.data ?? []).filter((r) => !onlyDisagreements || new Set(r.outputs.map((o) => JSON.stringify(o.disposition))).size > 1 || r.outputs.some((o) => r.truth && !sameDisposition(r.truth, o.disposition)))
  const ids = [...new Set((q.data ?? []).flatMap((r) => r.outputs.map((o) => o.modelId)))]
  if (!q.data?.length) return null
  return (
    <div>
      <label className="mb-1 flex items-center gap-2 text-xs text-ink-2">
        <Switch checked={onlyDisagreements} onChange={setOnly} label="Only where models disagree or are wrong" /> Only where models disagree or are wrong
      </label>
      <table className="w-full text-xs" aria-label="Escalation side by side">
        <thead>
          <tr className="border-b border-line text-left text-2xs uppercase tracking-wide text-clay">
            <th className="py-1 font-medium">Message</th>
            <th className="py-1 font-medium">Yours</th>
            <th className="py-1 font-medium">Now</th>
            {ids.map((id) => (
              <th key={id} className="py-1 font-medium">
                {labelOf(models, id)}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.messageId} className="border-b border-line/50 align-top">
              <td className="max-w-xs py-1 pr-2">
                <div className="truncate font-medium" title={r.subject}>
                  {r.subject || '(no subject)'}
                </div>
                <div className="truncate text-clay">{r.from}</div>
              </td>
              <td className="py-1">{r.truth ? <Chip>{dispositionLabel(r.truth)}</Chip> : <span className="text-clay">—</span>}</td>
              <td className="py-1">
                <Chip>{dispositionLabel(r.current)}</Chip>
              </td>
              {ids.map((id) => {
                const o = r.outputs.find((x) => x.modelId === id)
                return (
                  <td key={id} className="py-1">
                    {o ? (
                      <span className="flex items-center gap-1">
                        <OutputChip o={o} truth={r.truth} />
                        {!r.truth && o.disposition && !o.error ? (
                          <Button size="sm" variant="ghost" title="Record this as your decision for the message (a correction Riddle learns from)" onClick={() => decide(r.digId, r.messageId, o.disposition!)}>
                            This one's right
                          </Button>
                        ) : null}
                      </span>
                    ) : (
                      <span className="text-clay">—</span>
                    )}
                  </td>
                )
              })}
            </tr>
          ))}
        </tbody>
      </table>
      {!rows.length ? <p className="mt-1 text-xs text-clay">Every model agrees with you here.</p> : null}
      <p className="mt-1 text-xs text-clay">Mail you have not decided shows each model's answer; "This one's right" records your decision, and the scores count it.</p>
    </div>
  )
}

function DirectorRows({ models }: { models: BenchModel[] }) {
  const q = useQuery({ queryKey: [...qk.comparison, 'director'], queryFn: () => directorComparisons(20), refetchInterval: 4000 })
  const { run } = useAction()
  if (!q.data?.length) return null
  return (
    <ul className="space-y-3" aria-label="Director side by side">
      {q.data.map((c) => (
        <li key={c.questionKey} className="rounded-md border border-line p-2">
          <div className="mb-1.5 text-sm font-medium">{c.question}</div>
          <div className="grid gap-2" style={{ gridTemplateColumns: `repeat(${Math.min(c.outputs.length, 3)}, minmax(0, 1fr))` }}>
            {c.outputs.map((o) => (
              <div key={o.modelId} className={cx('rounded border p-2 text-xs', c.preferred === o.modelId ? 'border-moss bg-moss/5' : 'border-line/70')}>
                <div className="mb-1 flex items-center justify-between gap-2">
                  <span className="font-medium">{labelOf(models, o.modelId)}</span>
                  <span className="text-clay">
                    {fmtInt(o.latencyMs)} ms{o.costUsd != null ? ` · ${fmtUsd(o.costUsd)}` : ''}
                  </span>
                </div>
                <p className={cx('max-h-48 overflow-auto whitespace-pre-wrap', o.error && 'text-rust')}>{o.error ?? o.text}</p>
                {!o.error ? (
                  <Button size="sm" variant={c.preferred === o.modelId ? 'secondary' : 'ghost'} className="mt-1" disabledReason={c.preferred === o.modelId ? 'Your pick' : null} onClick={() => void run('pick_director_answer', { questionKey: c.questionKey, modelId: o.modelId }, { invalidate: [qk.comparison], successToast: false })}>
                    {c.preferred === o.modelId ? 'Your pick' : 'Best answer'}
                  </Button>
                ) : null}
              </div>
            ))}
          </div>
        </li>
      ))}
    </ul>
  )
}

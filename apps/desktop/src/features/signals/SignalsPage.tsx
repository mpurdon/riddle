/**
 * Signals (ARCHITECTURE §22-31): the registry with status and versions,
 * a detail view with definition, choices and calibration, version
 * comparison, a propose form that shows the authoring rules, Test, and
 * Promote behind the Approval tier.
 */
import { useMemo, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { FlaskConical, Plus, Radio } from 'lucide-react'
import type { Signal } from '@/bindings/Signal'
import type { SignalStatus } from '@/bindings/SignalStatus'
import { qk, signalVersions } from '@/lib/api'
import { cx, fmtInt, fmtMs, fmtPct, fmtRelative, fmtUsd } from '@/lib/format'
import { diffLines } from '@/lib/diff'
import { useSignals } from '@/lib/queries'
import type { SignalTestReport } from '@/lib/types'
import type { Tone } from '@/lib/selectors'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Dialog } from '@/components/ui/Dialog'
import { EmptyState } from '@/components/ui/EmptyState'
import { Field, Input, TextArea } from '@/components/ui/Field'
import { Note } from '@/components/ui/Note'
import { Select } from '@/components/ui/Select'
import { Tabs, TabPanel } from '@/components/ui/Tabs'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'

const TONE: Record<SignalStatus, Tone> = { proposed: 'neutral', candidate: 'neutral', evaluating: 'busy', rejected: 'bad', experimental: 'warn', active: 'ok', deprecated: 'neutral' }

export const AUTHORING_RULES = [
  'One concept per Signal: atomic-ish, so a Mesh can combine them.',
  'Ask a question the message can answer without knowing the user\'s preferences; preferences live in Meshes.',
  'Noul (YES/NO) unless the choices are genuinely discrete; Score levels are weakly calibrated and not offered.',
  'Define each choice so two people would label the same way; the definition is the prompt.',
  'Name it snake_case; a new definition is a new version, never an edit.',
  'Say which Context slices it needs (sender relationship, thread state, user profile).',
  'A new version is tested against the corpus and calibrated before promotion; Meshes pin versions, so promotion proposes a re-simulation rather than changing behaviour silently.',
]

export default function SignalsPage() {
  const { name } = useParams()
  const nav = useNavigate()
  const list = useSignals()
  const [proposing, setProposing] = useState(false)
  const signals = list.data ?? []
  if (list.error) return <ErrorCard error={list.error} onRetry={() => list.refetch()} />
  return (
    <div className="flex min-h-0 flex-1">
      <section className="flex w-[380px] shrink-0 flex-col border-r border-line">
        <PageHeader title="Signals" subtitle="Riddle's semantic vocabulary: bounded questions the judge answers with probabilities." actions={<Button size="sm" icon={<Plus />} onClick={() => setProposing(true)}>Propose</Button>} />
        <div className="min-h-0 flex-1 overflow-auto" role="listbox" aria-label="Signals">
          {list.data && !signals.length ? <EmptyState icon={<Radio />} title="No Signals." body="System Signals ship with Riddle. Propose one when a Mesh needs a concept that does not exist yet." /> : null}
          {signals.map((s) => (
            <div key={s.name} role="option" aria-selected={s.name === name} onClick={() => nav(`/signals/${s.name}`)} className={cx('cursor-default border-b border-line/60 px-4 py-2', s.name === name ? 'row-selected' : 'hover:bg-sand/60')}>
              <div className="flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate mono text-sm font-medium">{s.name}</span>
                <span className="text-xs text-clay">v{s.version}</span>
                <Chip tone={TONE[s.status]}>{s.status}</Chip>
              </div>
              <div className="mt-0.5 truncate text-xs text-ink-2">{s.definition}</div>
              <div className="mt-0.5 text-[11px] text-clay">
                {s.primitive} · {s.calibration ? `calibrated on ${fmtInt(s.calibration.labelledCount)} · agreement ${fmtPct(s.calibration.heldOutAgreement, 1)}` : 'not calibrated'} · {s.createdBy}
              </div>
            </div>
          ))}
        </div>
      </section>
      <section className="flex min-w-0 flex-1 flex-col">{name ? <SignalDetail key={name} name={name} /> : <EmptyState title="Select a Signal" body="Definition, choices, calibration and the versions it has been through." />}</section>
      {proposing ? <ProposeDialog onClose={() => setProposing(false)} existing={signals} /> : null}
    </div>
  )
}

function SignalDetail({ name }: { name: string }) {
  const versions = useQuery({ queryKey: qk.signalVersions(name), queryFn: () => signalVersions(name) })
  const { run } = useAction()
  const [tab, setTab] = useState('definition')
  const [sel, setSel] = useState<number | null>(null)
  const [compareA, setCompareA] = useState<number | null>(null)
  const [compareB, setCompareB] = useState<number | null>(null)
  const [report, setReport] = useState<SignalTestReport | null>(null)
  const [testing, setTesting] = useState(false)
  const vs = versions.data ?? []
  const latest = vs[0]
  const shown = useMemo(() => vs.find((v) => v.version === sel) ?? latest, [vs, sel, latest])
  if (versions.error) return <ErrorCard error={versions.error} onRetry={() => versions.refetch()} />
  if (!shown) return <div className="p-6 text-sm text-clay">Loading…</div>
  const a = vs.find((v) => v.version === (compareA ?? Math.max(1, shown.version - 1))) ?? shown
  const b = vs.find((v) => v.version === (compareB ?? shown.version)) ?? shown
  const test = async () => {
    setTesting(true)
    try {
      const out = await run<SignalTestReport>('test_signal', { name, version: shown.version, sampleSize: 40 }, { successToast: false })
      if (out) setReport(out.result)
    } finally {
      setTesting(false)
    }
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={<span className="mono">{name} v{shown.version}</span>}
        subtitle={
          <span className="flex flex-wrap items-center gap-2">
            <Chip tone={TONE[shown.status]}>{shown.status}</Chip>
            <span>{shown.primitive}</span>
            <span>· by {shown.createdBy} {fmtRelative(shown.createdAt)}</span>
            {vs.length > 1 ? (
              <span className="flex items-center gap-1">
                · version
                <Select label="Version" size="sm" value={String(shown.version)} onChange={(v) => setSel(Number(v))} options={vs.map((v) => ({ value: String(v.version), label: `v${v.version} (${v.status})` }))} />
              </span>
            ) : null}
          </span>
        }
        actions={
          <>
            <Button size="sm" icon={<FlaskConical />} busy={testing} onClick={test}>
              Test
            </Button>
            <Button size="sm" variant="primary" onClick={() => run('promote_signal', { name, version: shown.version })} disabledReason={shown.status === 'active' ? 'Already active' : shown.status === 'rejected' ? 'Rejected versions are not promoted' : null}>
              Promote…
            </Button>
          </>
        }
      />
      <Tabs value={tab} onChange={setTab} className="min-h-0 flex-1" tabs={[{ value: 'definition', label: 'Definition' }, { value: 'compare', label: 'Compare versions' }, { value: 'test', label: report ? `Test (${fmtPct(report.agreement, 0)})` : 'Test' }]}>
        <TabPanel value="definition" className="p-5 text-sm">
          <p className="max-w-2xl">{shown.definition}</p>
          {shown.changeNote ? <p className="mt-1 text-xs text-ink-2">Change note: {shown.changeNote}</p> : null}
          <h3 className="mt-5 text-xs font-semibold uppercase tracking-wide text-clay">Choices</h3>
          <ul className="mt-1 flex flex-col gap-1">
            {shown.choices.map((c) => (
              <li key={c.name} className="flex gap-3">
                <span className="w-20 shrink-0 mono font-medium">{c.name}</span>
                <span className="text-ink-2">{c.definition}</span>
              </li>
            ))}
          </ul>
          <h3 className="mt-5 text-xs font-semibold uppercase tracking-wide text-clay">Inputs</h3>
          <p className="mt-1 text-ink-2">
            {[shown.inputs.message && 'message', shown.inputs.thread && 'thread', ...shown.inputs.contextSlices.map((c) => `context: ${c}`)].filter(Boolean).join(' · ') || 'message only'}
            {shown.languageScope.length ? ` · languages ${shown.languageScope.join(', ')}` : ' · English only'}
          </p>
          <h3 className="mt-5 text-xs font-semibold uppercase tracking-wide text-clay">Calibration</h3>
          {shown.calibration ? (
            <dl className="mt-1 grid max-w-md grid-cols-[10rem_1fr] gap-y-0.5 text-ink-2">
              <dt>Method</dt>
              <dd>{shown.calibration.method}{shown.calibration.plattA !== null ? ` (a=${shown.calibration.plattA.toFixed(2)}, b=${shown.calibration.plattB?.toFixed(2)})` : ''}</dd>
              <dt>Labelled examples</dt>
              <dd>{fmtInt(shown.calibration.labelledCount)}</dd>
              <dt>Engine</dt>
              <dd className="mono">{shown.calibration.engineIdentity}</dd>
              <dt>Held-out agreement</dt>
              <dd>{fmtPct(shown.calibration.heldOutAgreement)}</dd>
              <dt>Calibrated</dt>
              <dd>{fmtRelative(shown.calibration.calibratedAt)}</dd>
            </dl>
          ) : (
            <p className="mt-1 text-ink-2">Not calibrated. Thresholds in Meshes are written against calibrated scores; a Signal is calibrated before promotion.</p>
          )}
        </TabPanel>
        <TabPanel value="compare" className="p-5 text-sm">
          {vs.length < 2 ? (
            <p className="text-ink-2">Only one version exists.</p>
          ) : (
            <>
              <div className="flex items-center gap-2">
                <Select label="Left version" size="sm" value={String(a.version)} onChange={(v) => setCompareA(Number(v))} options={vs.map((v) => ({ value: String(v.version), label: `v${v.version}` }))} />
                <span className="text-clay">vs</span>
                <Select label="Right version" size="sm" value={String(b.version)} onChange={(v) => setCompareB(Number(v))} options={vs.map((v) => ({ value: String(v.version), label: `v${v.version}` }))} />
              </div>
              <pre className="mt-3 whitespace-pre-wrap rounded border border-line bg-sand p-3 mono text-xs">
                {diffLines(`${a.definition}\n${a.choices.map((c) => `${c.name}: ${c.definition}`).join('\n')}`, `${b.definition}\n${b.choices.map((c) => `${c.name}: ${c.definition}`).join('\n')}`).map((l, i) => (
                  <div key={i} className={cx(l.kind === 'add' && 'bg-moss/15 text-moss', l.kind === 'del' && 'bg-rust/15 text-rust')}>
                    <span className="inline-block w-4 select-none text-clay">{l.kind === 'add' ? '+' : l.kind === 'del' ? '−' : ' '}</span>
                    {l.text}
                  </div>
                ))}
              </pre>
              <table className="mt-4 w-full max-w-md text-sm">
                <thead className="text-left text-xs text-clay">
                  <tr>
                    <th className="font-semibold" />
                    <th className="font-semibold">v{a.version}</th>
                    <th className="font-semibold">v{b.version}</th>
                  </tr>
                </thead>
                <tbody>
                  <tr><td className="py-0.5 text-ink-2">Held-out agreement</td><td>{fmtPct(a.calibration?.heldOutAgreement)}</td><td>{fmtPct(b.calibration?.heldOutAgreement)}</td></tr>
                  <tr><td className="py-0.5 text-ink-2">Labelled examples</td><td>{a.calibration ? fmtInt(a.calibration.labelledCount) : '—'}</td><td>{b.calibration ? fmtInt(b.calibration.labelledCount) : '—'}</td></tr>
                  <tr><td className="py-0.5 text-ink-2">Status</td><td>{a.status}</td><td>{b.status}</td></tr>
                </tbody>
              </table>
              <p className="mt-2 text-xs text-clay">Dataset: the calibration set for each version; methodology: Platt scaling against user labels. Metrics always name their dataset.</p>
            </>
          )}
        </TabPanel>
        <TabPanel value="test" className="p-5 text-sm">
          {!report ? (
            <div>
              <p className="text-ink-2">Test runs this version against a sample of your labelled history (experimental evaluation, never production) and reports agreement, false positives and negatives, latency and cost.</p>
              <Button className="mt-3" size="sm" icon={<FlaskConical />} busy={testing} onClick={test}>
                Run test on 40 messages
              </Button>
            </div>
          ) : (
            <div>
              <table className="w-full max-w-lg">
                <tbody>
                  {[
                    ['Sample', fmtInt(report.sampleSize)],
                    ['Observed agreement', fmtPct(report.agreement)],
                    ['False positives', fmtInt(report.falsePositives)],
                    ['False negatives', fmtInt(report.falseNegatives)],
                    ['Uncertain (< .70)', fmtInt(report.uncertain)],
                    ['Average latency', fmtMs(report.latencyMs)],
                    ['Cost', fmtUsd(report.costUsd)],
                    ['Distribution', Object.entries(report.distribution).map(([k, v]) => `${k} ${v}`).join(' · ')],
                  ].map(([k, v]) => (
                    <tr key={k} className="border-b border-line/50">
                      <td className="py-0.5 text-ink-2">{k}</td>
                      <td className="py-0.5 text-right tabular-nums">{v}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <h3 className="mt-4 text-xs font-semibold uppercase tracking-wide text-clay">Samples</h3>
              <table className="mt-1 w-full text-xs">
                <thead className="text-left text-clay">
                  <tr><th className="font-semibold">Message</th><th className="font-semibold">Expected</th><th className="font-semibold">Got</th><th className="text-right font-semibold">Confidence</th></tr>
                </thead>
                <tbody>
                  {report.samples.map((s) => (
                    <tr key={s.messageId} className={cx('border-b border-line/40', s.expected && s.expected !== s.got && 'text-rust')}>
                      <td className="py-0.5 mono">{s.messageId}</td>
                      <td>{s.expected ?? '—'}</td>
                      <td>{s.got}</td>
                      <td className="text-right tabular-nums">{fmtPct(s.confidence, 0)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <Button className="mt-3" size="sm" busy={testing} onClick={test}>Run again</Button>
            </div>
          )}
        </TabPanel>
      </Tabs>
    </div>
  )
}

function ProposeDialog({ onClose, existing }: { onClose: () => void; existing: Signal[] }) {
  const { run } = useAction()
  const nav = useNavigate()
  const [name, setName] = useState('')
  const [definition, setDefinition] = useState('')
  const [primitive, setPrimitive] = useState<'noul' | 'choice'>('noul')
  const [choices, setChoices] = useState('YES: \nNO: ')
  const [slices, setSlices] = useState('')
  const valid = /^[a-z][a-z0-9_]*$/.test(name) && definition.trim().length > 20
  const clash = existing.find((s) => s.name === name)
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title="Propose a Signal" description="A proposal enters the registry as proposed; it is tested and calibrated before promotion." wide>
      <div className="grid grid-cols-[1fr_260px] gap-5">
        <div className="flex flex-col gap-3">
          <Field label="Name" hint={clash ? `${name} exists; this proposes v${clash.version + 1}` : 'snake_case'}>
            <Input value={name} onChange={(e) => setName(e.target.value.trim())} placeholder="waiting_on_me" autoFocus />
          </Field>
          <Field label="Definition" hint="The question the judge answers about one message.">
            <TextArea value={definition} onChange={(e) => setDefinition(e.target.value)} placeholder="Someone has asked the user a question and the user has not answered it." />
          </Field>
          <Field label="Primitive">
            <Select label="Primitive" value={primitive} onChange={(v) => setPrimitive(v as 'noul' | 'choice')} options={[{ value: 'noul', label: 'Noul (YES / NO)' }, { value: 'choice', label: 'Choice (discrete options)' }]} />
          </Field>
          {primitive === 'choice' ? (
            <Field label="Choices" hint="One per line: NAME: definition">
              <TextArea value={choices} onChange={(e) => setChoices(e.target.value)} />
            </Field>
          ) : null}
          <Field label="Context slices" hint="Comma separated, e.g. sender_relationship, thread_state">
            <Input value={slices} onChange={(e) => setSlices(e.target.value)} />
          </Field>
        </div>
        <Note title="Authoring rules">
          <ol className="list-decimal pl-4 text-xs">
            {AUTHORING_RULES.map((r) => (
              <li key={r} className="mt-1">{r}</li>
            ))}
          </ol>
        </Note>
      </div>
      <div className="mt-4 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button
          variant="primary"
          disabledReason={valid ? null : 'Needs a snake_case name and a definition of at least 20 characters'}
          onClick={async () => {
            const parsedChoices = primitive === 'choice' ? choices.split('\n').map((l) => l.split(':')).filter((p) => p[0]?.trim()).map((p) => ({ name: p[0].trim(), definition: p.slice(1).join(':').trim() })) : []
            const out = await run<Signal>('propose_signal', { signal: { name, definition, primitive, choices: parsedChoices, inputs: { message: true, thread: slices.includes('thread'), contextSlices: slices.split(',').map((s) => s.trim()).filter(Boolean) } } })
            if (out) {
              onClose()
              nav(`/signals/${out.result.name}`)
            }
          }}
        >
          Propose
        </Button>
      </div>
    </Dialog>
  )
}

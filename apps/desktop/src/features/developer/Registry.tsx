/**
 * Signal Registry Inspector (§71), Signal Playground (§72), Mesh Inspector
 * (§75) and Mesh Simulator (§76).
 */
import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Play } from 'lucide-react'
import type { DecisionTrace } from '@/bindings/DecisionTrace'
import type { Mesh } from '@/bindings/Mesh'
import type { SignalEvaluation } from '@/bindings/SignalEvaluation'
import type { SimulationReport } from '@/bindings/SimulationReport'
import { getMesh, meshVersions, qk, simulateMesh } from '@/lib/api'
import { cx, fmtInt, fmtMs, fmtPct, fmtScore, fmtUsd, num } from '@/lib/format'
import { useMeshes, useSignals } from '@/lib/queries'
import { dispositionLabel } from '@/lib/selectors'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Select } from '@/components/ui/Select'
import { SimulationTable } from '@/features/meshes/SimulationTable'
import { Distribution, JsonBlock, KV, Section, TracePicker, useRecentTraces } from './shared'

const normRule = (s: string | null) => (s ?? '').replace(/\s+/g, ' ').trim()

function meshesUsing(meshes: Mesh[], name: string): Mesh[] {
  return meshes.filter((m) => JSON.stringify([...m.routes.map((r) => r.condition), ...m.guards.map((g) => g.condition)]).includes(`"signal":"${name}"`))
}

export function SignalInspector() {
  const signals = useSignals()
  const meshes = useMeshes()
  const traces = useRecentTraces(200)
  const rows = useMemo(() => {
    const list = signals.data ?? []
    const ts = traces.data ?? []
    return list.map((s) => {
      const evals = ts.flatMap((t) => t.signals.filter((e) => e.signalName === s.name))
      const fresh = evals.filter((e) => !e.cached)
      const avg = (xs: number[]) => (xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : null)
      return {
        s,
        meshes: meshesUsing(meshes.data ?? [], s.name),
        usage: evals.length,
        avgConfidence: avg(evals.map((e) => e.confidence)),
        avgLatency: avg(fresh.map((e) => num(e.latencyMs))),
        avgCost: avg(fresh.map((e) => (num(e.inputTokens) / 1_000_000) * 0.042)),
      }
    })
  }, [signals.data, meshes.data, traces.data])
  return (
    <div className="overflow-auto p-5">
      <p className="mb-3 text-xs text-ink-2">Dataset: the {traces.data?.length ?? 0} most recent decision traces in the current Dig scope. Agreement comes from each version's calibration set. Metrics always name their dataset and methodology.</p>
      <table className="w-full text-xs">
        <thead className="text-left text-clay">
          <tr>
            {['Signal', 'Version', 'Status', 'Usage', 'Meshes using it', 'Observed agreement', 'Avg confidence', 'Avg latency', 'Avg cost'].map((h) => (
              <th key={h} className="border-b border-line py-1 pr-2 font-semibold">{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map(({ s, meshes: ms, usage, avgConfidence, avgLatency, avgCost }) => (
            <tr key={s.name} className="border-b border-line/50">
              <td className="py-1 pr-2 mono font-medium">{s.name}</td>
              <td className="py-1 pr-2">v{s.version}</td>
              <td className="py-1 pr-2"><Chip tone={s.status === 'active' ? 'ok' : s.status === 'proposed' ? 'neutral' : 'warn'}>{s.status}</Chip></td>
              <td className="py-1 pr-2 tabular-nums">{usage}</td>
              <td className="py-1 pr-2">{ms.map((m) => `${m.name} v${m.version}`).join(', ') || '—'}</td>
              <td className="py-1 pr-2 tabular-nums">{fmtPct(s.calibration?.heldOutAgreement)}</td>
              <td className="py-1 pr-2 tabular-nums">{avgConfidence === null ? '—' : fmtScore(avgConfidence)}</td>
              <td className="py-1 pr-2 tabular-nums">{avgLatency === null ? 'cached only' : fmtMs(avgLatency)}</td>
              <td className="py-1 pr-2 tabular-nums">{avgCost === null ? '—' : fmtUsd(avgCost)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

export function Playground() {
  const traces = useRecentTraces()
  const signals = useSignals()
  const { run } = useAction()
  const [trace, setTrace] = useState<DecisionTrace | null>(null)
  const [picked, setPicked] = useState<Set<string>>(new Set())
  const [results, setResults] = useState<SignalEvaluation[]>([])
  const [running, setRunning] = useState(false)
  const t = trace ?? traces.data?.[0] ?? null
  const toggle = (name: string) => setPicked((p) => { const n = new Set(p); if (n.has(name)) n.delete(name); else n.add(name); return n })
  const runJev = async () => {
    if (!t) return
    setRunning(true)
    setResults([])
    try {
      const out: SignalEvaluation[] = []
      for (const name of picked) {
        const sig = signals.data?.find((s) => s.name === name)
        const r = await run<SignalEvaluation>('evaluate_signal', { digId: t.digId, messageId: t.messageId, name, version: sig?.version ?? 1 }, { successToast: false })
        if (r) out.push(r.result)
        setResults([...out])
      }
    } finally {
      setRunning(false)
    }
  }
  return (
    <div className="flex min-h-0 flex-1">
      <TracePicker traces={traces.data ?? []} value={t?.id ?? null} onPick={(x) => { setTrace(x); setResults([]) }} title="Messages" />
      <div className="min-h-0 flex-1 overflow-auto p-5">
        {t ? (
          <div className="grid grid-cols-2 gap-8">
            <div>
              <Section title="Message">
                <KV rows={[['Message', t.messageId], ['Dig', t.digId], ['Current disposition', dispositionLabel(t.conclusion)]]} />
              </Section>
              <Section title="Signals">
                <ul className="flex flex-col gap-1 text-xs">
                  {(signals.data ?? []).map((s) => (
                    <li key={s.name}>
                      <label className="flex items-center gap-2">
                        <input type="checkbox" checked={picked.has(s.name)} onChange={() => toggle(s.name)} aria-label={s.name} />
                        <span className="mono">{s.name}:v{s.version}</span>
                        <span className="text-clay">{s.status}</span>
                      </label>
                    </li>
                  ))}
                </ul>
                <Button size="sm" variant="primary" icon={<Play />} className="mt-3" busy={running} disabledReason={picked.size ? null : 'Pick at least one Signal'} onClick={runJev}>
                  Run Jev
                </Button>
                <p className="mt-2 text-xs text-clay">Experimental evaluation: results are not written into production Signals and never change a disposition (§34).</p>
              </Section>
            </div>
            <div>
              <Section title="Results">
                {results.length ? (
                  <div className="flex flex-col gap-2">
                    {results.map((r) => (
                      <div key={`${r.signalName}@${r.signalVersion}`} className="rounded border border-line p-2 text-xs">
                        <div className="flex items-center gap-2"><span className="mono font-medium">{r.signalName}:v{r.signalVersion}</span><span>→ {r.selected}</span><span className="text-clay">{fmtMs(r.latencyMs)} · {fmtInt(r.inputTokens)} tokens · {fmtUsd((num(r.inputTokens) / 1_000_000) * 0.042)}</span></div>
                        <div className="mt-1"><Distribution d={r.distribution} /></div>
                      </div>
                    ))}
                    <JsonBlock value={results} label="playground results" />
                  </div>
                ) : (
                  <span className="text-xs text-clay">Run Jev to see probabilities, latency, tokens, cost, request and response.</span>
                )}
              </Section>
            </div>
          </div>
        ) : null}
      </div>
    </div>
  )
}

export function MeshInspector() {
  const meshes = useMeshes()
  const traces = useRecentTraces(200)
  const [meshId, setMeshId] = useState<string | null>(null)
  const busiest = useMemo(() => {
    const counts = new Map<string, number>()
    for (const t of traces.data ?? []) if (t.meshId) counts.set(t.meshId, (counts.get(t.meshId) ?? 0) + 1)
    return [...counts.entries()].sort((a, b) => b[1] - a[1])[0]?.[0] ?? null
  }, [traces.data])
  const id = meshId ?? busiest ?? meshes.data?.[0]?.id ?? null
  const versions = useQuery({ queryKey: id ? qk.meshVersions(id) : ['mesh_versions', 'none'], queryFn: () => meshVersions(id!), enabled: !!id })
  const [version, setVersion] = useState<number | null>(null)
  const mesh = useQuery({ queryKey: id ? qk.mesh(id, version) : ['mesh', 'none'], queryFn: () => getMesh(id!, version), enabled: !!id })
  const [trace, setTrace] = useState<DecisionTrace | null>(null)
  const scoped = (traces.data ?? []).filter((t) => t.meshId === id)
  const t = trace ?? scoped[0] ?? null
  const m = mesh.data
  const [report, setReport] = useState<SimulationReport | null>(null)
  const [simulating, setSimulating] = useState(false)
  return (
    <div className="flex min-h-0 flex-1">
      <TracePicker traces={traces.data ?? []} value={t?.id ?? null} onPick={setTrace} filter={(x) => x.meshId === id} title="Decisions by this Mesh" />
      <div className="min-h-0 flex-1 overflow-auto p-5">
        <div className="mb-4 flex items-center gap-2">
          <Select label="Mesh" size="sm" value={id ?? ''} onChange={(v) => { setMeshId(v); setVersion(null); setTrace(null); setReport(null) }} options={(meshes.data ?? []).map((x) => ({ value: x.id, label: `${x.name} (${x.status})` }))} />
          <Select label="Version" size="sm" value={String(m?.version ?? '')} onChange={(v) => setVersion(Number(v))} options={(versions.data ?? []).map((v) => ({ value: String(v.version), label: `v${v.version}` }))} />
        </div>
        {m && t ? (
          <div className="grid grid-cols-2 gap-8">
            <div>
              <Section title={`${m.name} v${m.version} · input facts`}>
                <div className="text-xs mono">{Object.entries(t.facts).filter(([, v]) => v === true).map(([k]) => k).join(', ') || 'none true'}</div>
              </Section>
              <Section title="Input Signals">
                <table className="text-xs">
                  <tbody>
                    {t.signals.map((s) => (
                      <tr key={s.signalName}><td className="pr-4 mono">{s.signalName}@v{s.signalVersion}</td><td className="tabular-nums">{fmtScore(s.calibrated.YES ?? s.confidence)}</td><td className="pl-2 text-clay">{s.selected}{s.cached ? ' · cached' : ''}</td></tr>
                    ))}
                  </tbody>
                </table>
              </Section>
            </div>
            <div>
              <Section title="Policy">
                <div className="flex flex-col gap-1">
                  {m.routes.map((r, i) => {
                    const match = t.meshVersion === m.version && normRule(t.matchedRule) === normRule(r.text)
                    return (
                      <pre key={i} className={cx('whitespace-pre-wrap rounded border p-2 mono text-xs', match ? 'border-moss/40 bg-moss/10' : 'border-line')}>
                        {r.text}
                        {match ? <span className="ml-2 font-semibold text-moss">✓ MATCH</span> : null}
                      </pre>
                    )
                  })}
                  {m.guards.map((g, i) => (
                    <pre key={`g${i}`} className={cx('rounded border p-2 mono text-xs', t.guardVetoes.includes(g.text) ? 'border-rust/40 bg-rust/10' : 'border-line')}>{g.text}{t.guardVetoes.includes(g.text) ? ' ✗ VETO' : ''}</pre>
                  ))}
                </div>
                {t.meshVersion !== m.version ? <p className="mt-2 text-xs text-clay">This decision was made by v{t.meshVersion}; you are viewing v{m.version}. Use the simulator below to see what v{m.version} would do.</p> : null}
              </Section>
              <Section title="Result">
                <div className="text-sm font-semibold">{dispositionLabel(t.conclusion)}</div>
                <div className="text-xs text-clay">Jev judgment on the left; deterministic policy on the right. The boundary is the point.</div>
              </Section>
            </div>
          </div>
        ) : (
          <div className="text-sm text-clay">{m ? 'No recent decision by this Mesh.' : 'Loading…'}</div>
        )}
        {m ? (
          <Section title="Simulator" right={<Button size="sm" busy={simulating} onClick={async () => { setSimulating(true); try { setReport(await simulateMesh({ meshId: m.id, version: m.version })) } finally { setSimulating(false) } }}>Execute v{m.version} against history</Button>}>
            {report ? <SimulationTable r={report} compact /> : <span className="text-xs text-clay">Compare this version's outcomes against the current classifications, and inspect the changed decisions.</span>}
          </Section>
        ) : null}
      </div>
    </div>
  )
}

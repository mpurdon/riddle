/**
 * The SimulationReport table (§38): unchanged, changed, the transitions,
 * new judge calls and cost. Free only when the Signals are cached.
 */
import type { SimulationReport } from '@/bindings/SimulationReport'
import { fmtInt, fmtUsd, num } from '@/lib/format'
import { titleCase } from '@/lib/format'

export function SimulationTable({ r, compact }: { r: SimulationReport; compact?: boolean }) {
  const label = (k: string) => (k === 'unresolved' ? 'Escalate' : titleCase(k))
  return (
    <div className={compact ? 'text-xs' : 'text-sm'}>
      <div className="mb-2 font-medium">
        {r.meshName} {r.fromVersion !== null ? `v${r.fromVersion} → ` : ''}v{r.toVersion} · last {r.windowDays} days
      </div>
      <table className="w-full">
        <tbody>
          <Row k="Historical messages" v={fmtInt(r.historicalMessages)} />
          <Row k="Unchanged" v={fmtInt(r.unchanged)} />
          <Row k="Changed" v={fmtInt(r.changed)} strong />
          {r.transitions.map((t) => (
            <Row key={`${t.from}-${t.to}`} k={`${label(t.from)} → ${label(t.to)}`} v={fmtInt(t.count)} indent />
          ))}
          <Row k="New judge calls required" v={fmtInt(r.newJudgeCallsRequired)} strong={num(r.newJudgeCallsRequired) > 0} />
          <Row k="Estimated cost" v={r.estimatedCostUsd === null ? 'unpriced' : fmtUsd(r.estimatedCostUsd)} />
        </tbody>
      </table>
      {num(r.newJudgeCallsRequired) > 0 ? <p className="mt-2 text-xs text-ink-2">Simulation is free only when the Signals it needs are cached. A new Signal or version has no cache; the backfill runs under the per-Dig budget.</p> : null}
      {r.sampleChanged.length ? <p className="mt-1 text-xs text-clay">Sample of changed messages: {r.sampleChanged.slice(0, 5).join(', ')}{r.sampleChanged.length > 5 ? '…' : ''}</p> : null}
    </div>
  )
}

function Row({ k, v, strong, indent }: { k: string; v: string; strong?: boolean; indent?: boolean }) {
  return (
    <tr className="border-b border-line/50">
      <td className={`py-0.5 ${indent ? 'pl-4 text-ink-2' : ''} ${strong ? 'font-medium' : ''}`}>{k}</td>
      <td className={`py-0.5 text-right tabular-nums ${strong ? 'font-semibold' : ''}`}>{v}</td>
    </tr>
  )
}

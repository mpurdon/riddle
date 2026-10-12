/**
 * The SimulationReport table (§38): what this edit changes and how, the
 * mail re-decided anyway (today's rules already disagree with how it was
 * stored), new judge calls and cost. Free only when the Signals are cached.
 */
import type { SimulationReport } from '@/bindings/SimulationReport'
import { fmtInt, fmtUsd, num } from '@/lib/format'
import { titleCase } from '@/lib/format'

export function SimulationTable({ r, compact, fallback }: { r: SimulationReport; compact?: boolean; fallback?: boolean }) {
  const label = (k: string) => (k === 'unresolved' ? 'Escalate' : titleCase(k))
  const drift = num(r.drift)
  const one = drift === 1
  return (
    <div className={`selectable ${compact ? 'text-xs' : 'text-sm'}`}>
      <div className="mb-2 font-medium">
        {r.meshName} {r.fromVersion !== null ? `v${r.fromVersion} → ` : ''}v{r.toVersion} · last {r.windowDays} days
      </div>
      <table className="w-full">
        <tbody>
          <Row k="Historical messages" v={fmtInt(r.historicalMessages)} />
          <Row k="Changed by this edit" v={fmtInt(r.editChanged)} strong />
          {r.editTransitions.map((t) => (
            <Row key={`${t.from}-${t.to}`} k={`${label(t.from)} → ${label(t.to)}`} v={fmtInt(t.count)} indent />
          ))}
          {drift > 0 ? <Row k="Re-decided anyway" v={fmtInt(drift)} /> : null}
          <Row k="New judge calls required" v={fmtInt(r.newJudgeCallsRequired)} strong={num(r.newJudgeCallsRequired) > 0} />
          <Row k="Estimated cost" v={r.estimatedCostUsd === null ? 'unpriced' : fmtUsd(r.estimatedCostUsd)} />
        </tbody>
      </table>
      {drift > 0 ? (
        <p className="mt-2 text-xs text-ink-2">
          {fmtInt(drift)} {one ? 'message is' : 'messages are'} decided differently by today's rules than when {one ? 'it was' : 'they were'} sorted, with or without this edit; applying to history re-decides {one ? 'it' : 'them'} too ({fmtInt(r.changed)} in all).
        </p>
      ) : null}
      {num(r.newJudgeCallsRequired) > 0 ? <p className="mt-2 text-xs text-ink-2">Simulation is free only when the Signals it needs are cached. A new Signal or version has no cache; the backfill runs under the per-Dig budget.</p> : null}
      {r.takenFirstBy.length ? (
        <p className={fallback ? 'mt-2 text-xs text-ink-2' : 'mt-2 rounded border border-ochre/40 bg-ochre/5 px-2 py-1 text-xs'}>
          <span className="font-medium">Taken first by earlier Meshes: </span>
          {r.takenFirstBy.map((o) => `${fmtInt(o.count)} by ${o.meshName} (Priority ${o.priority})`).join('; ')}.{' '}
          {fallback ? 'This Mesh is a fallback, so that is expected.' : 'This Mesh would decide them, but those run first. Give it a lower Priority to run before them, or narrow them.'}
        </p>
      ) : null}
      {r.takesFrom.length ? (
        <p className="mt-1 text-xs text-ink-2">
          <span className="font-medium">Takes from later Meshes: </span>
          {r.takesFrom.map((o) => `${fmtInt(o.count)} from ${o.meshName} (Priority ${o.priority})`).join('; ')}.
        </p>
      ) : null}
      {r.sampleEditChanged.length ? <p className="mt-1 text-xs text-clay">Sample of messages this edit changes: {r.sampleEditChanged.slice(0, 5).join(', ')}{r.sampleEditChanged.length > 5 ? '…' : ''}</p> : null}
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

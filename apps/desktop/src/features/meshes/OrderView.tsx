/**
 * The order Meshes run in, what each decided, and which earlier Meshes take
 * mail it also matches. Rules in one ordered list are only as safe as the
 * order is visible: a generic "account notification → Filed" rule at
 * Priority 0 filed GitHub push notifications before the GitHub Mesh at 10
 * saw them, and only a trace said so. The counts replay the running rules
 * over stored facts and cached Signals (`riddle_mesh::order`); nothing is
 * asked of the judge.
 */
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from 'react-router-dom'
import { ArrowDownWideNarrow } from 'lucide-react'
import type { MeshOrderRow } from '@/bindings/MeshOrderRow'
import { meshOrder, qk } from '@/lib/api'
import { cx, fmtInt, num } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Select } from '@/components/ui/Select'
import { ErrorCard } from '@/components/ErrorCard'
import { scopeLabel, StatusChip } from './MeshesPage'

const WINDOWS = [
  { value: '7', label: 'Last 7 days' },
  { value: '30', label: 'Last 30 days' },
  { value: '90', label: 'Last 90 days' },
]

export function OrderView() {
  const [windowDays, setWindowDays] = useState(30)
  const q = useQuery({ queryKey: qk.meshOrder(windowDays), queryFn: () => meshOrder(windowDays), staleTime: 60_000 })
  const { digName } = useDigContext()
  const nav = useNavigate()
  // A fallback losing mail to earlier Meshes is its job; only specific rules count.
  const conflicts = (q.data?.rows ?? []).filter((r) => !r.fallback && r.takenFirstBy.length > 0).length
  return (
    <div className="min-h-0 flex-1 overflow-auto px-6 py-4">
      <div className="flex items-start gap-3">
        <ArrowDownWideNarrow className="mt-0.5 size-5 text-clay" />
        <div className="min-w-0 flex-1">
          <h2 className="text-base font-semibold">Run order</h2>
          <p className="mt-0.5 max-w-2xl text-xs text-ink-2">
            Meshes run top to bottom: rules for one account first, then a group, then all accounts, each by Priority (lower runs first). The first rule that decides a message wins; guards from every Mesh still apply after it. “Taken first by” counts mail a Mesh would decide on its own that an earlier Mesh decides instead. Mark catch-alls as fallbacks: losing mail to earlier Meshes is what they are for, so they are not flagged.
          </p>
        </div>
        <Select label="Window" size="sm" value={String(windowDays)} onChange={(v) => setWindowDays(Number(v))} options={WINDOWS} />
      </div>
      {q.error ? <div className="mt-4"><ErrorCard error={q.error} onRetry={() => q.refetch()} /></div> : null}
      {q.data ? (
        <p className="mt-3 text-xs text-clay">
          {fmtInt(q.data.messages)} received messages replayed · {conflicts ? <span className="text-ochre">{conflicts} Mesh{conflicts === 1 ? ' has' : 'es have'} mail taken by an earlier one</span> : 'no Mesh has mail taken by an earlier one'}
        </p>
      ) : (
        <p className="mt-3 text-xs text-clay">{q.isLoading ? 'Replaying the rules over your mail…' : null}</p>
      )}
      <table className="mt-3 w-full text-sm" aria-label="Run order">
        <thead>
          <tr className="border-b border-line text-left text-2xs uppercase tracking-wide text-clay">
            <th className="w-8 py-1 font-medium">#</th>
            <th className="py-1 font-medium">Mesh</th>
            <th className="py-1 font-medium">Scope</th>
            <th className="w-16 py-1 text-right font-medium">Priority</th>
            <th className="w-20 py-1 text-right font-medium">Decided</th>
            <th className="py-1 pl-4 font-medium">Taken first by</th>
            <th className="w-24 py-1 font-medium" />
          </tr>
        </thead>
        <tbody>
          {(q.data?.rows ?? []).map((r) => (
            <Row key={r.meshId} r={r} scope={scopeLabel(r, digName)} onOpen={() => nav(`/meshes/${r.meshId}`)} />
          ))}
        </tbody>
      </table>
    </div>
  )
}

function Row({ r, scope, onOpen }: { r: MeshOrderRow; scope: string; onOpen: () => void }) {
  const { run } = useAction()
  const taken = r.takenFirstBy.reduce((n, o) => n + num(o.count), 0)
  const flagged = taken > 0 && !r.fallback
  return (
    <tr className={cx('border-b border-line/50 align-top', flagged && 'bg-ochre/5')}>
      <td className="py-1.5 tabular-nums text-clay">{r.position}</td>
      <td className="py-1.5">
        <button type="button" onClick={onOpen} className="text-left font-medium hover:underline">
          {r.meshName}
        </button>
        <span className="ml-1.5 text-xs text-clay">v{r.version}</span>
        {r.status !== 'active' ? <span className="ml-1.5"><StatusChip status={r.status} /></span> : null}
        {r.fallback ? <span className="ml-1.5"><Chip title="A catch-all for mail no specific Mesh decided; losing mail to earlier Meshes is expected.">fallback</Chip></span> : null}
      </td>
      <td className="py-1.5 text-xs text-ink-2">{scope}</td>
      <td className="py-1.5 text-right tabular-nums">{r.priority}</td>
      <td className="py-1.5 text-right tabular-nums">{fmtInt(r.decided)}</td>
      <td className="py-1.5 pl-4">
        <div className="flex flex-wrap gap-1">
          {r.takenFirstBy.map((o) => (
            <Chip key={o.meshId} tone={r.fallback ? 'neutral' : 'warn'} title={`${fmtInt(o.count)} messages ${r.meshName} would decide are decided first by ${o.meshName} (Priority ${o.priority}). Give ${r.meshName} a lower Priority to run before it, or narrow ${o.meshName}.`}>
              {fmtInt(o.count)} · {o.meshName}
            </Chip>
          ))}
        </div>
      </td>
      <td className="py-1 text-right">
        <Button size="sm" variant="ghost" title={r.fallback ? 'Treat it as a specific rule again: mail taken by earlier Meshes is flagged' : 'A catch-all for mail no specific Mesh decided: stop flagging mail earlier Meshes take'} onClick={() => void run('set_mesh_fallback', { meshId: r.meshId, fallback: !r.fallback }, { successToast: r.fallback ? `${r.meshName} is a specific rule` : `${r.meshName} is a fallback` })}>
          {r.fallback ? 'Specific rule' : 'Fallback'}
        </Button>
      </td>
    </tr>
  )
}

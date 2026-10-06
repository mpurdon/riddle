import { useQuery } from '@tanstack/react-query'
import { Copy } from 'lucide-react'
import { useState } from 'react'
import type { DecisionTrace } from '@/bindings/DecisionTrace'
import { qk, recentTraces } from '@/lib/api'
import { cx, fmtClockMs, fmtScore, safeJson } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { dispositionLabel } from '@/lib/selectors'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { toast } from '@/lib/stores'

export function useRecentTraces(limit = 80) {
  const { digId } = useDigContext()
  return useQuery({ queryKey: [...qk.recentTraces, digId, limit], queryFn: () => recentTraces({ digId, limit }), refetchInterval: 5000 })
}

export function TracePicker({ traces, value, onPick, filter, title = 'Recent decisions' }: { traces: DecisionTrace[]; value: string | null; onPick: (t: DecisionTrace) => void; filter?: (t: DecisionTrace) => boolean; title?: string }) {
  const list = filter ? traces.filter(filter) : traces
  return (
    <div className="flex w-72 shrink-0 flex-col border-r border-line">
      <div className="border-b border-line px-3 py-2 text-2xs font-semibold uppercase tracking-wide text-clay">{title} ({list.length})</div>
      <div className="min-h-0 flex-1 overflow-auto" role="listbox" aria-label={title}>
        {list.map((t) => (
          <div key={t.id} role="option" aria-selected={t.id === value} onClick={() => onPick(t)} className={cx('cursor-default border-b border-line/60 px-3 py-1.5 text-xs', t.id === value ? 'row-selected' : 'hover:bg-sand/60')}>
            <div className="flex items-center justify-between gap-2">
              <span className="truncate mono">{t.messageId}</span>
              <span className="text-clay">{fmtClockMs(t.createdAt).slice(0, 8)}</span>
            </div>
            <div className="mt-0.5 flex items-center gap-1">
              <Chip tone={t.conclusion.kind === 'find' ? 'ok' : t.conclusion.kind === 'unresolved' ? 'warn' : t.conclusion.kind === 'contamination' ? 'bad' : 'neutral'}>{dispositionLabel(t.conclusion)}</Chip>
              <span className="text-clay">{t.mechanism}</span>
              {t.escalation ? <Chip tone="warn">LLM</Chip> : null}
              {t.shadow ? <Chip>shadow</Chip> : null}
            </div>
          </div>
        ))}
        {!list.length ? <div className="p-3 text-xs text-clay">No traces yet. Every classified message writes one.</div> : null}
      </div>
    </div>
  )
}

export function Section({ title, children, right }: { title: string; children: React.ReactNode; right?: React.ReactNode }) {
  return (
    <section className="mb-5">
      <div className="mb-1.5 flex items-center justify-between border-b border-line pb-1">
        <h3 className="text-2xs font-semibold uppercase tracking-wide text-clay">{title}</h3>
        {right}
      </div>
      {children}
    </section>
  )
}

export function KV({ rows }: { rows: [string, React.ReactNode][] }) {
  return (
    <dl className="grid grid-cols-[11rem_1fr] gap-y-0.5 text-xs">
      {rows.map(([k, v]) => (
        <div key={k} className="contents">
          <dt className="text-clay">{k}</dt>
          <dd className="mono selectable">{v ?? '—'}</dd>
        </div>
      ))}
    </dl>
  )
}

/**
 * A YES/NO Signal always reads the same way: YES on top in green, NO below in
 * red. Sorting by score put whichever answer won first, so every row had to
 * be read label by label to tell a yes from a no. Other choice sets have no
 * natural order and stay sorted by score, coloured by the threshold.
 */
const NOUL: Record<string, string> = { yes: 'bg-moss', no: 'bg-rust' }

export function distributionRows(d: Record<string, number>, threshold = 0.8): { label: string; value: number; bar: string }[] {
  const entries = Object.entries(d)
  const noul = entries.length === 2 && entries.every(([k]) => k.toLowerCase() in NOUL)
  if (noul) return entries.sort(([a]) => (a.toLowerCase() === 'yes' ? -1 : 1)).map(([label, value]) => ({ label, value, bar: NOUL[label.toLowerCase()] }))
  return entries.sort((a, b) => b[1] - a[1]).map(([label, value]) => ({ label, value, bar: value >= threshold ? 'bg-moss' : 'bg-clay' }))
}

export function Distribution({ d, threshold }: { d: Record<string, number>; threshold?: number }) {
  return (
    <div className="flex flex-col gap-0.5">
      {distributionRows(d, threshold).map(({ label, value, bar }) => (
        <div key={label} className="flex items-center gap-2 text-xs">
          <span className="w-16 mono">{label}</span>
          <div className="relative h-2 w-40 rounded bg-sand-2">
            <div className={cx('h-full rounded', bar)} style={{ width: `${Math.round(value * 100)}%` }} />
            {threshold !== undefined ? <div className="absolute top-0 h-full w-px bg-ochre" style={{ left: `${threshold * 100}%` }} title={`threshold ${threshold}`} /> : null}
          </div>
          <span className="w-8 text-right tabular-nums">{fmtScore(value)}</span>
        </div>
      ))}
    </div>
  )
}

export function JsonBlock({ value, label }: { value: unknown; label: string }) {
  const [copied, setCopied] = useState(false)
  const text = safeJson(value)
  return (
    <div className="relative">
      <Button
        size="sm"
        variant="ghost"
        icon={<Copy />}
        className="absolute right-1 top-1"
        onClick={async () => {
          try {
            await navigator.clipboard.writeText(text)
            setCopied(true)
            setTimeout(() => setCopied(false), 1500)
          } catch {
            toast('Copy failed', { body: 'Clipboard access was refused.', kind: 'warn' })
          }
        }}
        aria-label={`Copy ${label}`}
      >
        {copied ? 'Copied' : 'Copy'}
      </Button>
      <pre className="max-h-[60vh] overflow-auto rounded border border-line bg-sand p-2 mono text-2xs leading-4 selectable">{text}</pre>
    </div>
  )
}

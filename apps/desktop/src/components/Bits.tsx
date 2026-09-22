/**
 * Small shared pieces: disposition chips, page headers, the snooze menu, a
 * Stratum picker, a "Why?" explanation card, and the dev overlay chips.
 */
import { useState, type ReactNode } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from 'react-router-dom'
import { Clock, HelpCircle, Layers } from 'lucide-react'
import type { Disposition } from '@/bindings/Disposition'
import type { Explanation } from '@/bindings/Explanation'
import type { FindType } from '@/bindings/FindType'
import { listStrata, qk } from '@/lib/api'
import { cx, fmtScore } from '@/lib/format'
import { dispositionLabel, dispositionTone, findTypeLabel, FIND_TYPES } from '@/lib/selectors'
import { useDirector, useUi } from '@/lib/stores'
import { Button } from './ui/Button'
import { Chip } from './ui/Chip'
import { Dialog } from './ui/Dialog'
import { Menu, type MenuItem } from './ui/Menu'
import { Input } from './ui/Field'
import { Tip } from './ui/Tooltip'

export function DispositionChip({ d, shadow }: { d: Disposition | null | undefined; shadow?: boolean }) {
  return (
    <Chip tone={dispositionTone(d)} title={shadow ? 'Shadow Period: proposed, not applied' : undefined}>
      {shadow ? 'Would be ' : ''}
      {dispositionLabel(d)}
    </Chip>
  )
}

export function StrataChips({ strata, max = 3 }: { strata: string[]; max?: number }) {
  if (!strata.length) return null
  const shown = strata.slice(0, max)
  const rest = strata.length - shown.length
  return (
    <span className="inline-flex items-center gap-1">
      {shown.map((s) => (
        <Chip key={s} tone="neutral" className="font-normal">
          {s}
        </Chip>
      ))}
      {rest > 0 ? <span className="text-[11px] text-clay">+{rest}</span> : null}
    </span>
  )
}

export function PageHeader({ title, subtitle, actions, children }: { title: ReactNode; subtitle?: ReactNode; actions?: ReactNode; children?: ReactNode }) {
  return (
    <div className="shrink-0 border-b border-line px-5 py-3">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <h1 className="truncate text-base font-semibold">{title}</h1>
          {subtitle ? <div className="text-xs text-ink-2">{subtitle}</div> : null}
        </div>
        {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
      </div>
      {children ? <div className="mt-2 flex flex-wrap items-center gap-2">{children}</div> : null}
    </div>
  )
}

export function snoozeOptions(now = new Date()): { label: string; until: string }[] {
  const at = (h: number, dayOffset: number) => {
    const d = new Date(now)
    d.setDate(d.getDate() + dayOffset)
    d.setHours(h, 0, 0, 0)
    return d.toISOString()
  }
  const opts = [] as { label: string; until: string }[]
  if (now.getHours() < 16) opts.push({ label: 'Later today (18:00)', until: at(18, 0) })
  opts.push({ label: 'Tomorrow morning (8:00)', until: at(8, 1) })
  const dow = now.getDay()
  const toMonday = ((8 - dow) % 7) || 7
  opts.push({ label: 'Next Monday (8:00)', until: at(8, toMonday) })
  opts.push({ label: 'In a week', until: at(8, 7) })
  return opts
}

export function SnoozeMenu({ onPick, trigger, label = 'Snooze' }: { onPick: (until: string) => void; trigger?: ReactNode; label?: string }) {
  const items: MenuItem[] = snoozeOptions().map((o) => ({ label: o.label, onSelect: () => onPick(o.until) }))
  return <Menu label={label} items={items} trigger={trigger ?? <Button size="sm" icon={<Clock />}>{label}</Button>} />
}

export function RetypeMenu({ current, onPick, trigger }: { current: FindType; onPick: (t: FindType) => void; trigger?: ReactNode }) {
  const items: MenuItem[] = FIND_TYPES.map((t) => ({ label: findTypeLabel(t), onSelect: () => onPick(t), disabledReason: t === current ? 'current type' : null }))
  return <Menu label="Change Find type" items={items} trigger={trigger ?? <Button size="sm">Retype</Button>} />
}

export function StrataPicker({ open, onClose, onPick, title = 'Add a Stratum' }: { open: boolean; onClose: () => void; onPick: (names: string[]) => void; title?: string }) {
  const strata = useQuery({ queryKey: qk.strata, queryFn: listStrata, enabled: open })
  const [q, setQ] = useState('')
  const [picked, setPicked] = useState<string[]>([])
  const list = (strata.data ?? []).filter((s) => s.name.toLowerCase().includes(q.toLowerCase()))
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()} title={title} description="A message can belong to several Strata.">
      <Input autoFocus placeholder="Filter Strata" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Filter Strata" />
      <ul className="mt-2 max-h-64 overflow-auto rounded border border-line">
        {list.map((s) => {
          const on = picked.includes(s.name)
          return (
            <li key={s.id}>
              <label className="flex cursor-default items-center gap-2 px-2 py-1 hover:bg-sand">
                <input type="checkbox" checked={on} onChange={() => setPicked(on ? picked.filter((x) => x !== s.name) : [...picked, s.name])} aria-label={s.name} />
                <span className="flex-1">{s.name}</span>
                <span className="text-xs text-clay">{s.scopeGroup ?? 'All Digs'}</span>
              </label>
            </li>
          )
        })}
        {!list.length ? <li className="px-2 py-3 text-center text-xs text-clay">No Stratum matches. Create one under Strata.</li> : null}
      </ul>
      <div className="mt-4 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button
          variant="primary"
          icon={<Layers />}
          disabledReason={picked.length ? null : 'Pick at least one Stratum'}
          onClick={() => {
            onPick(picked)
            setPicked([])
            onClose()
          }}
        >
          Add {picked.length || ''}
        </Button>
      </div>
    </Dialog>
  )
}

/** The "Why?" card (README "Mesh Feedback & Repair"). */
export function ExplanationCard({ ex, digId, onNotImportant, onUndo, compact }: { ex: Explanation; digId: string; onNotImportant?: () => void; onUndo?: () => void; compact?: boolean }) {
  const nav = useNavigate()
  const setPrefill = useDirector((s) => s.setPrefill)
  const mech = ex.mechanism
  const head =
    mech === 'mesh' && ex.meshName
      ? `Mesh ${ex.meshName} v${ex.meshVersion} classified it as ${dispositionLabel(ex.disposition)}.`
      : mech === 'gate'
        ? `A deterministic gate decided: ${ex.gate}.`
        : mech === 'llm'
          ? `Signals were uncertain; the LLM decided ${dispositionLabel(ex.disposition)}.`
          : mech === 'user'
            ? `You decided: ${dispositionLabel(ex.disposition)}.`
            : mech === 'temporal'
              ? 'Made by time, not by mail: Temporal Sift.'
              : `Decided on Signals: ${dispositionLabel(ex.disposition)}.`
  return (
    <div className={cx('rounded-md border border-line bg-sand', compact ? 'p-2 text-xs' : 'p-3 text-sm')}>
      <div className="flex items-start gap-2">
        <HelpCircle className="mt-0.5 size-4 shrink-0 text-ochre" />
        <div className="min-w-0 flex-1">
          <div className="font-medium">Why? {ex.shadow ? <Chip tone="neutral">Shadow: proposed, not applied</Chip> : null}</div>
          <div className="mt-0.5 text-ink-2">{head}</div>
          {ex.reason ? <div className="mt-0.5">{ex.reason}</div> : null}
          {ex.matched ? <pre className="mt-2 whitespace-pre-wrap rounded border border-line bg-paper p-2 text-xs mono">{ex.matched}</pre> : null}
          {ex.signals.length ? (
            <div className="mt-2 flex flex-wrap gap-1">
              {ex.signals.map((s) => (
                <Tip key={`${s.name}@${s.version}`} text={`${s.name} v${s.version}: ${s.selected} ${fmtScore(s.score)}${s.cached ? ' (cached)' : ' (fresh evaluation)'}`}>
                  <span>
                    <Chip tone={s.score >= 0.8 ? 'ok' : s.score <= 0.3 ? 'neutral' : 'warn'} mono>
                      {s.name}@v{s.version} {fmtScore(s.score)}
                    </Chip>
                  </span>
                </Tip>
              ))}
            </div>
          ) : null}
          {ex.llmRationale ? <blockquote className="mt-2 border-l-2 border-ochre/40 pl-2 text-ink-2">{ex.llmRationale}</blockquote> : null}
          <div className="mt-2 flex flex-wrap gap-1.5">
            <Button size="sm" variant="ghost" onClick={() => nav(`/developer/messages/${digId}/${ex.traceId}`)}>
              View Jev
            </Button>
            {ex.signals[0] ? (
              <Button size="sm" variant="ghost" onClick={() => nav(`/signals/${ex.signals[0].name}`)}>
                View Signal
              </Button>
            ) : null}
            {ex.meshName ? (
              <Button size="sm" variant="ghost" onClick={() => nav(`/meshes?name=${encodeURIComponent(ex.meshName!)}`)}>
                View Mesh
              </Button>
            ) : null}
            {onNotImportant ? (
              <Button size="sm" variant="ghost" onClick={onNotImportant}>
                Not Important
              </Button>
            ) : null}
            {onUndo ? (
              <Button size="sm" variant="ghost" onClick={onUndo}>
                Undo
              </Button>
            ) : null}
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                setPrefill(`Why did you think "${ex.messageId}" was ${dispositionLabel(ex.disposition)}?`)
                nav('/director')
              }}
            >
              Ask the Director
            </Button>
          </div>
        </div>
      </div>
    </div>
  )
}

/** Compact diagnostic chips in the normal UI (§83). Click opens the inspector. */
export function DevOverlay({ digId, traceId, mechanism, confidence, meshVersion, totalMs, escalated }: { digId: string; traceId: string | null; mechanism?: string | null; confidence?: number | null; meshVersion?: number | null; totalMs?: number | null; escalated?: boolean }) {
  const on = useUi((s) => s.devOverlay)
  const nav = useNavigate()
  if (!on || !traceId) return null
  return (
    <button type="button" className="inline-flex items-center gap-1" onClick={(e) => { e.stopPropagation(); nav(`/developer/messages/${digId}/${traceId}`) }} aria-label="Open the Message Inspector">
      {confidence !== null && confidence !== undefined ? <Chip tone="neutral" mono>{mechanism === 'llm' ? 'LLM' : 'JEV'} {fmtScore(confidence)}</Chip> : null}
      {meshVersion ? <Chip tone="neutral" mono>MESH v{meshVersion}</Chip> : null}
      <Chip tone="neutral" mono>{escalated ? 'LLM' : 'NO LLM'}</Chip>
      {totalMs ? <Chip tone="neutral" mono>{totalMs}ms</Chip> : null}
    </button>
  )
}

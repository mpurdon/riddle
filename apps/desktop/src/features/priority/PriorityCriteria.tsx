/**
 * The Priority inbox's criteria, as one editor used in two places: the
 * panel on the Priority page and the Priority inbox section in Settings.
 * The person expected to find it under Settings, the page needs it in
 * reach, and one component keeps the two from disagreeing. Every change
 * saves through `save_settings` at once.
 */
import { useState } from 'react'
import { Plus, X } from 'lucide-react'
import type { FindType } from '@/bindings/FindType'
import type { Importance } from '@/bindings/Importance'
import type { PriorityInboxSettings } from '@/bindings/PriorityInboxSettings'
import { cx } from '@/lib/format'
import { FIND_TYPES, findTypeLabel, importanceLabel } from '@/lib/selectors'
import { Button } from '@/components/ui/Button'
import { Select } from '@/components/ui/Select'
import { StrataPicker } from '@/components/Bits'
import { labelName } from '@/features/messages/labels'

/** The provider labels worth choosing here; anything else is a filter in the Inbox. */
const LABELS = ['STARRED', 'IMPORTANT'] as const

const IMPORTANCE_OPTIONS = [
  { value: 'off', label: 'Not by level' },
  { value: 'normal', label: 'Normal and above' },
  { value: 'high', label: 'High and above' },
  { value: 'critical', label: 'Critical only' },
]

const PRIORITY_OPTIONS = [
  { value: 'any', label: 'Any open Find' },
  { value: '50', label: 'Finds 50+' },
  { value: '70', label: 'Finds 70+' },
  { value: '85', label: 'Finds 85+' },
]

/** "Starred · Finance, Travel · Finds 70+ (Pay, Respond)": the criteria in one line. */
export function describeCriteria(p: PriorityInboxSettings): string {
  const parts: string[] = []
  if (p.labels.length) parts.push(p.labels.map(labelName).join(', '))
  if (p.strata.length) parts.push(p.strata.join(', '))
  if (p.minImportance) parts.push(p.minImportance === 'critical' ? 'Critical' : `${importanceLabel(p.minImportance)} and above`)
  // The backend writes "no threshold" as 0 (TOML has no null), so both read as any.
  const finds = !p.minFindPriority ? 'Any open Find' : `Finds ${p.minFindPriority}+`
  parts.push(p.findTypes.length ? `${finds} (${p.findTypes.map(findTypeLabel).join(', ')})` : finds)
  return parts.join(' · ')
}

function Toggle({ on, onClick, children, title }: { on: boolean; onClick: () => void; children: React.ReactNode; title?: string }) {
  return (
    <button type="button" aria-pressed={on} title={title} onClick={onClick} className={cx('rounded border px-2 py-0.5 text-xs leading-4', on ? 'border-ochre bg-ochre/15 font-medium text-ink' : 'border-line bg-sand text-ink-2 hover:bg-sand-2')}>
      {children}
    </button>
  )
}

const toggleIn = <T,>(xs: T[], x: T): T[] => (xs.includes(x) ? xs.filter((y) => y !== x) : [...xs, x])

/**
 * The four rows: labels, Strata, Importance, Finds. `save` receives only the fields
 * that changed. Labels and Find types are small fixed sets, so they are
 * toggles; Strata can number in the hundreds, so the row shows only the
 * chosen ones, each removable, and "Add Strata" opens the same filtering
 * picker that labels messages.
 */
export function PriorityCriteriaEditor({ chosen, save }: { chosen: PriorityInboxSettings; save: (patch: Partial<PriorityInboxSettings>) => void }) {
  const [picking, setPicking] = useState(false)
  const priorityValue = chosen.minFindPriority ? String(chosen.minFindPriority) : 'any'
  const priorityOptions = PRIORITY_OPTIONS.some((o) => o.value === priorityValue) ? PRIORITY_OPTIONS : [...PRIORITY_OPTIONS, { value: priorityValue, label: `Finds ${priorityValue}+` }]
  return (
    <div className="space-y-2 text-xs">
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="w-16 text-ink-2">Labels</span>
        {LABELS.map((l) => (
          <Toggle key={l} on={chosen.labels.includes(l)} onClick={() => save({ labels: toggleIn(chosen.labels, l) })}>
            {labelName(l)}
          </Toggle>
        ))}
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="w-16 text-ink-2">Strata</span>
        {chosen.strata.map((name) => (
          <span key={name} className="inline-flex items-center gap-1 rounded border border-ochre bg-ochre/15 px-2 py-0.5 text-xs font-medium leading-4 text-ink">
            {name}
            <button type="button" aria-label={`Remove ${name}`} title={`Stop surfacing ${name}`} onClick={() => save({ strata: chosen.strata.filter((x) => x !== name) })} className="rounded text-clay hover:text-ink">
              <X className="size-3" aria-hidden />
            </button>
          </span>
        ))}
        {chosen.strata.length ? null : <span className="text-clay">(none chosen)</span>}
        <Button size="sm" variant="ghost" icon={<Plus />} onClick={() => setPicking(true)} title="Choose Strata to surface; the picker filters as you type">
          Add Strata…
        </Button>
        <StrataPicker
          open={picking}
          onClose={() => setPicking(false)}
          onPick={(names) => save({ strata: [...new Set([...chosen.strata, ...names])] })}
          title="Surface these Strata"
        />
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="w-16 text-ink-2">Importance</span>
        <Select label="Importance" size="sm" value={chosen.minImportance ?? 'off'} onChange={(v) => save({ minImportance: v === 'off' ? null : (v as Importance) })} options={IMPORTANCE_OPTIONS} />
      </div>
      <div className="flex flex-wrap items-center gap-1.5">
        <span className="w-16 text-ink-2">Finds</span>
        <Select label="Find priority" size="sm" value={priorityValue} onChange={(v) => save({ minFindPriority: v === 'any' ? null : Number(v) })} options={priorityOptions} />
        <span className="text-clay">of type</span>
        {FIND_TYPES.map((t: FindType) => (
          <Toggle key={t} on={chosen.findTypes.includes(t)} onClick={() => save({ findTypes: toggleIn(chosen.findTypes, t) })}>
            {findTypeLabel(t)}
          </Toggle>
        ))}
        {chosen.findTypes.length ? null : <span className="text-clay">(none chosen: every type)</span>}
      </div>
    </div>
  )
}

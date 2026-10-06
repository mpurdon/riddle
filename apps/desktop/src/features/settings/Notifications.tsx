/**
 * Settings → General → Notifications (Flow 12). Two modes as option cards
 * rather than a bare threshold. Brief says everything in one summary at the
 * times you choose and interrupts at once only for an allow-list: a Find
 * from a Never Hidden sender, or a Find of a listed type due soon. Stream
 * is the old behaviour, every Find at or above the priority threshold as it
 * is raised. The study found the threshold alone produced either silence or
 * a stream of pings, and that nobody could say what "82" would let through;
 * it now lives under Stream only, with the open Finds either side of it as
 * examples. A brief time is refused here with the reason beside it, and by
 * the backend again, because "9:00" without the leading zero once saved
 * and never fired.
 */
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Bell, Plus } from 'lucide-react'
import { listFinds, page, qk, testNotification } from '@/lib/api'
import { isTauri } from '@/lib/ipc'
import { FIND_TYPES, findTypeLabel } from '@/lib/selectors'
import { toast } from '@/lib/stores'
import type { NotificationMode, NotificationSettings, Settings } from '@/lib/types'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Field'
import { Switch } from '@/components/ui/Switch'
import type { Option } from './accounts/model'
import { OptionCards } from './accounts/pieces'
import { NumberSetting, useDraft } from './shared'

export const MAX_BRIEF_TIMES = 4
const TIME_RE = /^([01]\d|2[0-3]):[0-5]\d$/

const MODE_OPTIONS: Option<NotificationMode>[] = [
  { value: 'brief', label: 'Brief', detail: 'One summary at the times you choose: "7 need you, 5 Unresolved, 41 filed." Only the allow-list below interrupts at once.' },
  { value: 'stream', label: 'Stream', detail: 'A notification for every Find at or above the priority threshold, as it is raised.' },
]

/** Why a brief time cannot be saved, or null. The backend applies the same rule. */
export function timeProblem(value: string, others: string[]): string | null {
  const v = value.trim()
  if (!TIME_RE.test(v)) return 'Use HH:MM on a 24-hour clock, for example 16:30'
  if (others.includes(v)) return 'Already in the list'
  return null
}

export function NotificationsSection({ settings, save }: { settings: Settings; save: (patch: Partial<Settings>) => void }) {
  const n = settings.notifications
  const saveN = (patch: Partial<NotificationSettings>) => save({ notifications: { ...n, ...patch } })
  return (
    <div className="max-w-2xl">
      <OptionCards label="Notification mode" options={MODE_OPTIONS} value={n.mode} onChange={(mode) => saveN({ mode })} />
      {n.mode === 'brief' ? (
        <div className="mt-4 space-y-4 text-sm">
          <div>
            <div className="mb-1 text-xs font-medium text-ink">Brief times</div>
            <BriefTimes times={n.briefTimes} onChange={(briefTimes) => saveN({ briefTimes })} />
          </div>
          <fieldset>
            <legend className="mb-1 text-xs font-medium text-ink">Interrupt at once for</legend>
            <div className="flex flex-wrap gap-x-4 gap-y-1">
              {FIND_TYPES.map((t) => (
                <label key={t} className="flex items-center gap-1.5">
                  <input
                    type="checkbox"
                    className="accent-ochre"
                    checked={n.immediateTypes.includes(t)}
                    onChange={(e) => saveN({ immediateTypes: FIND_TYPES.filter((x) => (x === t ? e.target.checked : n.immediateTypes.includes(x))) })}
                  />
                  {findTypeLabel(t)}
                </label>
              ))}
            </div>
            <label className="mt-2 flex items-center gap-2">
              …when due within
              <NumberSetting value={n.dueWithinHours} min={1} max={336} step={1} onCommit={(v) => saveN({ dueWithinHours: Math.round(v) })} className="w-20" label="Due within hours" />
              hours
            </label>
          </fieldset>
          <label className="flex items-center gap-2">
            <Switch checked={n.neverHiddenImmediate} onChange={(neverHiddenImmediate) => saveN({ neverHiddenImmediate })} label="Never Hidden senders interrupt at once" /> Never Hidden senders interrupt at once
          </label>
        </div>
      ) : (
        <div className="mt-4 text-sm">
          <label className="flex items-center gap-2">
            Priority threshold
            <NumberSetting value={settings.notifyPriorityThreshold} min={0} max={100} step={1} onCommit={(v) => save({ notifyPriorityThreshold: Math.round(v) })} className="w-20" label="Notify priority threshold" />
            <span className="text-xs text-clay">Stream mode only</span>
          </label>
          <ThresholdExplainer threshold={settings.notifyPriorityThreshold} />
        </div>
      )}
      <p className="mt-4 text-xs text-clay">The dock badge counts open Finds plus Unresolved. A notification says the type and the reason, never the subject.</p>
      <div className="mt-3">
        <TestNotification />
      </div>
    </div>
  )
}

/**
 * Up to four times, each saved on blur or Enter once it reads as HH:MM. The
 * list keeps at least one time in Brief mode: with none, nothing is ever
 * said, which reads as Riddle having stopped.
 */
function BriefTimes({ times, onChange }: { times: string[]; onChange: (t: string[]) => void }) {
  const [adding, setAdding] = useState(false)
  const sorted = (xs: string[]) => [...xs].sort()
  return (
    <div className="flex flex-wrap items-start gap-2">
      {times.map((t, i) => (
        <TimeField
          key={`${i}-${t}`}
          value={t}
          others={times.filter((_, j) => j !== i)}
          onCommit={(v) => onChange(sorted(times.map((x, j) => (j === i ? v : x))))}
          onRemove={() => onChange(times.filter((_, j) => j !== i))}
          removeDisabledReason={times.length <= 1 ? 'Keep at least one time' : null}
        />
      ))}
      {adding ? (
        <TimeField
          value=""
          others={times}
          autoFocus
          onCommit={(v) => {
            onChange(sorted([...times, v]))
            setAdding(false)
          }}
          onCancel={() => setAdding(false)}
        />
      ) : (
        <Button size="sm" icon={<Plus />} disabledReason={times.length >= MAX_BRIEF_TIMES ? `${MAX_BRIEF_TIMES} times is the most` : null} onClick={() => setAdding(true)}>
          Add a time
        </Button>
      )}
    </div>
  )
}

function TimeField({ value, others, onCommit, onRemove, onCancel, removeDisabledReason, autoFocus }: { value: string; others: string[]; onCommit: (v: string) => void; onRemove?: () => void; onCancel?: () => void; removeDisabledReason?: string | null; autoFocus?: boolean }) {
  const [draft, setDraft] = useDraft(value)
  const [touched, setTouched] = useState(false)
  const problem = draft.trim() === value ? null : timeProblem(draft, others)
  const commit = () => {
    setTouched(true)
    const v = draft.trim()
    if (v === value) {
      if (!v) onCancel?.()
      return
    }
    if (!v && onCancel) {
      onCancel()
      return
    }
    if (!problem) onCommit(v)
  }
  const shown = touched && problem
  const id = `time-${value || 'new'}`
  return (
    <div className="flex flex-col gap-0.5">
      <div className="flex items-center gap-1">
        <Input
          value={draft}
          autoFocus={autoFocus}
          inputMode="numeric"
          placeholder="HH:MM"
          aria-label={value ? `Brief time ${value}` : 'New brief time'}
          aria-invalid={shown ? true : undefined}
          aria-describedby={shown ? `${id}-problem` : undefined}
          className={`w-20 mono ${shown ? 'border-rust' : ''}`}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === 'Enter') commit()
            if (e.key === 'Escape') {
              setDraft(value)
              setTouched(false)
              onCancel?.()
            }
          }}
        />
        {onRemove ? (
          <Button size="sm" variant="ghost" title={`Remove ${value}`} disabledReason={removeDisabledReason} onClick={onRemove}>
            <span aria-hidden>×</span>
          </Button>
        ) : null}
      </div>
      {shown ? (
        <span id={`${id}-problem`} role="alert" className="text-xs text-rust">
          {problem}
        </span>
      ) : null}
    </div>
  )
}

function TestNotification() {
  return (
    <Button
      icon={<Bell />}
      onClick={async () => {
        if (!isTauri) {
          toast('Test notification', { body: 'System notifications are only sent by the desktop app.', kind: 'info' })
          return
        }
        await testNotification()
        toast('Test notification sent', { body: 'If nothing appeared, allow notifications for Script Editor (macOS) or Riddle in System Settings → Notifications.', kind: 'info', ttlMs: 8000 })
      }}
    >
      Test Notification
    </Button>
  )
}

/**
 * What the threshold means, with the open Finds either side of it as the
 * examples: "P82 was Jamie's review request; P40 was a newsletter" says
 * more than any definition of priority could.
 */
export function ThresholdExplainer({ threshold }: { threshold: number }) {
  const q = useQuery({ queryKey: [...qk.finds, 'threshold-examples'], queryFn: () => listFinds({ status: 'open', page: page(null, 50) }), staleTime: 60_000 })
  const finds = q.data?.items ?? []
  const atOrAbove = finds.filter((f) => f.priority >= threshold)
  const nearestAbove = [...atOrAbove].sort((a, b) => a.priority - b.priority)[0]
  const nearestBelow = finds.filter((f) => f.priority < threshold).sort((a, b) => b.priority - a.priority)[0]
  return (
    <div className="mt-3 max-w-2xl text-xs text-ink-2">
      <p>
        Priority runs from 0 to 100: how sure Riddle is that a Find needs you soon, from its type, its deadline and who is waiting on it. In Stream mode, P{threshold} means a Find scoring {threshold} or more interrupts you with a notification; the rest wait quietly in Finds.
      </p>
      {q.error ? <p className="mt-1 text-rust">Could not read the open Finds to show examples.</p> : null}
      {q.data && finds.length === 0 ? <p className="mt-1">No open Finds yet to compare against.</p> : null}
      {finds.length ? (
        <>
          <p className="mt-1 selectable">
            {nearestAbove ? (
              <>
                P{nearestAbove.priority} was “{nearestAbove.title}”
              </>
            ) : (
              <>Nothing open scores {threshold} or more</>
            )}
            {'; '}
            {nearestBelow ? (
              <>
                P{nearestBelow.priority} was “{nearestBelow.title}”
              </>
            ) : (
              <>nothing open scores below {threshold}</>
            )}
            .
          </p>
          <p className="mt-1">
            Of the {finds.length} open Finds Riddle has loaded, {atOrAbove.length} would have notified you.
          </p>
        </>
      ) : null}
    </div>
  )
}

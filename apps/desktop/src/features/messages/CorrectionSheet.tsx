/**
 * The sheet under the Spoil and Contamination toolbar: Not Spoil or
 * Restore, scoped to this message, to every loaded message from the same
 * sender, or to the sender's whole domain, with Never hide beside it.
 * Until this the button acted at once on the cursor row, and a sender with
 * fourteen newsletters in Spoil meant fourteen presses. A modal was the
 * obvious fix and the wrong one: the row and the count the sheet names
 * must stay in view. Enter takes the default, Esc closes, and both are
 * stopped here so the list beneath does not act.
 *
 * The sender and domain scopes also add a rule ("never Spoil from this
 * sender") to the Your corrections Mesh, and the sheet says so before
 * Enter: the second wave of the study found people restoring the same
 * sender week after week because a correction taught nothing durable.
 */
import { NeverHideButton } from '@/components/NeverHideButton'
import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { Undo2, X } from 'lucide-react'
import { plural } from '@/lib/format'
import { Button } from '@/components/ui/Button'
import { neverLabel, type CorrectionKind } from '@/features/rules/yourCorrections'

export type CorrectionScope = 'message' | 'sender' | 'domain' | 'selection'

export interface CorrectionTarget {
  /** The verb on the button: "Not Spoil" or "Restore". */
  verb: string
  /** Checked rows; when set the sheet acts on them, not on the cursor row. */
  selectedCount: number
  senderName: string
  senderEmail: string
  /** Loaded messages from that sender: the count the sender scope names. */
  senderCount: number
  domain: string
  /** Loaded messages from that domain. */
  domainCount: number
  /** What the rule vetoes: Spoil on the Spoil page, Contamination on Contamination. */
  never: CorrectionKind
  /** False when the cursor is on a sender row, where "this message" names nothing. */
  withMessage?: boolean
}

/** The line the sheet shows for a scope that adds a rule; null when it adds none. */
export function ruleLine(scope: CorrectionScope, never: CorrectionKind): string | null {
  if (scope === 'sender') return `Also adds a rule: never ${neverLabel(never)} from this sender.`
  if (scope === 'domain') return `Also adds a rule: never ${neverLabel(never)} from this domain.`
  return null
}

export function CorrectionSheet({ target, busy, onConfirm, onNeverHide, onClose }: { target: CorrectionTarget; busy: boolean; onConfirm: (scope: CorrectionScope) => void; onNeverHide: () => void; onClose: () => void }) {
  const withMessage = target.withMessage !== false
  const [scope, setScope] = useState<CorrectionScope>(target.selectedCount ? 'selection' : withMessage ? 'message' : 'sender')
  const first = useRef<HTMLInputElement>(null)
  const confirmBtn = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    ;(first.current ?? confirmBtn.current)?.focus()
  }, [])
  const onKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onClose()
      return
    }
    if (e.key !== 'Enter') return
    // A focused button keeps its own Enter; anywhere else Enter takes the chosen scope.
    e.stopPropagation()
    if ((e.target as HTMLElement).tagName === 'BUTTON') return
    e.preventDefault()
    if (!busy) onConfirm(scope)
  }
  const count = target.selectedCount ? target.selectedCount : scope === 'sender' ? target.senderCount : scope === 'domain' ? target.domainCount : 0
  const label = count ? `${target.verb} ${count}` : target.verb
  const rule = target.selectedCount ? null : ruleLine(scope, target.never)
  return (
    <div role="group" aria-label={`${target.verb}: choose what it applies to`} className="shrink-0 border-b border-line bg-sand px-4 py-2 fade-in" onKeyDown={onKey}>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2 text-sm">
        <span className="font-medium">{target.verb}:</span>
        {target.selectedCount ? (
          <span>the {plural(target.selectedCount, 'selected message')}</span>
        ) : (
          <>
            {withMessage ? (
              <label className="flex items-center gap-1.5">
                <input ref={first} type="radio" name="correction-scope" checked={scope === 'message'} onChange={() => setScope('message')} />
                this message
              </label>
            ) : null}
            <label className="flex items-center gap-1.5" title={`Every loaded message from ${target.senderEmail}; more may be on later pages`}>
              <input ref={withMessage ? undefined : first} type="radio" name="correction-scope" checked={scope === 'sender'} onChange={() => setScope('sender')} />
              this sender ({target.senderCount})
            </label>
            <label className="flex items-center gap-1.5" title={`Every loaded message from @${target.domain}; more may be on later pages`}>
              <input type="radio" name="correction-scope" checked={scope === 'domain'} onChange={() => setScope('domain')} />
              this domain ({target.domainCount})
            </label>
          </>
        )}
        <span className="ml-auto flex items-center gap-2">
          <Button ref={confirmBtn} size="sm" variant="primary" icon={<Undo2 />} busy={busy} onClick={() => onConfirm(scope)} title="Enter">
            {label}
          </Button>
          <NeverHideButton email={target.senderEmail} disabledReason={busy ? 'Working' : null} onAdd={onNeverHide} title={`Add ${target.senderEmail} to Never Hidden and restore every loaded message from them`}>
            Never hide this sender
          </NeverHideButton>
          <Button size="sm" variant="ghost" icon={<X />} onClick={onClose} title="Cancel (Esc)">
            Cancel
          </Button>
        </span>
      </div>
      <div className="mt-1 text-xs text-ink-2">
        {rule ? (
          <>
            <span className="font-medium text-ink">{rule}</span> Edit or remove it in Settings → Mail rules.
          </>
        ) : (
          'Recorded as a correction. When the same kind keeps coming up, the Director can propose a Mesh change.'
        )}
      </div>
    </div>
  )
}

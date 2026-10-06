/**
 * The Shadow review list's pieces (Flow 7, option B): the Agree / Disagree…
 * controls on a row, and the chooser that opens under the row when the
 * reader disagrees. The audit found the Shadow view was the Inbox with a
 * different chip, and its Archive and Spoil buttons acted for real on a
 * list that promised nothing was applied; here the only verbs are the two
 * the list is for, and a disagreement is recorded as a correction against
 * the proposing Mesh through `review_shadow`, never as a mailbox action.
 * The row carries its own review (`shadowReview`); the marks used to be a
 * separate query per account, merged on the page.
 */
import { useEffect } from 'react'
import { Check, X } from 'lucide-react'
import type { Disposition } from '@/bindings/Disposition'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { dispositionLabel, findTypeLabel, FIND_TYPES, sameDisposition } from '@/lib/selectors'
import { keyHint } from '@/lib/shortcuts'
import { Button } from '@/components/ui/Button'

/** What sits on the right of a review row: the two verbs, or the mark once one was used. Every row here is a proposal. */
export function ShadowControls({ m, busy, onAgree, onDisagree }: { m: MessageSummary; busy: boolean; onAgree: () => void; onDisagree: () => void }) {
  if (m.shadowReview === 'agreed') {
    return (
      <span className="text-2xs text-clay" title="You agreed with this proposal; the Mesh that made it earns the agreement">
        Agreed
      </span>
    )
  }
  if (m.shadowReview === 'disagreed') {
    return (
      <span className="text-2xs text-clay" title="You disagreed; your decision was recorded as a correction against the Mesh that proposed this">
        Disagreed
      </span>
    )
  }
  return (
    <span className="inline-flex items-center gap-1">
      <Button size="sm" variant="ghost" icon={<Check />} disabledReason={busy ? 'Working' : null} onClick={onAgree} title={`Agree: Riddle would have got this right (${keyHint('shadow_agree')})`}>
        Agree
      </Button>
      <Button size="sm" variant="ghost" icon={<X />} disabledReason={busy ? 'Working' : null} onClick={onDisagree} title={`Disagree: say what it should have been (${keyHint('shadow_disagree')})`}>
        Disagree…
      </Button>
    </span>
  )
}

/** The chooser's height under a row: one line of verbs, or two once the Find types are open. */
export const CHOOSER_HEIGHT = 40
export const CHOOSER_TYPES_HEIGHT = 72
/** An inline error under a row. */
export const ROW_ERROR_HEIGHT = 28

/**
 * Under the row: what Riddle would have done, and the four things it could
 * have been. Esc closes; the page binds the decide keys while it is open.
 */
export function DisagreeChooser({ m, busy, typesOpen, onTypes, onPick, onClose }: { m: MessageSummary; busy: boolean; typesOpen: boolean; onTypes: () => void; onPick: (d: Disposition) => void; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== 'Escape') return
      e.preventDefault()
      e.stopPropagation()
      onClose()
    }
    window.addEventListener('keydown', onKey, true)
    return () => window.removeEventListener('keydown', onKey, true)
  }, [onClose])
  const proposed = m.disposition
  const option = (d: Disposition, label: string, title: string, danger = false) => (
    <Button key={label} size="sm" variant={danger ? 'danger' : 'secondary'} disabledReason={busy ? 'Working' : sameDisposition(proposed, d) ? 'That is the proposal; Agree instead' : null} onClick={() => onPick(d)} title={title}>
      {label}
    </Button>
  )
  return (
    <div role="group" aria-label="Disagree: what should it have been?" className="flex flex-wrap items-center gap-1.5 px-1 pb-2 text-xs">
      <span className="text-ink-2">
        Riddle would have: <span className="font-medium text-ink">{dispositionLabel(proposed)}</span>. It should be:
      </span>
      {option({ kind: 'filed' }, 'Filed', `Filed (${keyHint('decide_filed')})`)}
      {option({ kind: 'spoil' }, 'Spoil', `Spoil (${keyHint('decide_spoil')})`)}
      {option({ kind: 'contamination' }, 'Contamination', `Contamination (${keyHint('decide_contamination')})`, true)}
      {typesOpen ? (
        FIND_TYPES.map((t) => option({ kind: 'find', find_type: t }, `Find: ${findTypeLabel(t)}`, `A Find of type ${findTypeLabel(t)}`))
      ) : (
        <Button size="sm" disabledReason={busy ? 'Working' : null} onClick={onTypes} title={`A Find; then pick its type (${keyHint('decide_find')})`}>
          Find…
        </Button>
      )}
      <Button size="sm" variant="ghost" icon={<X />} onClick={onClose} title="Cancel (Esc)">
        Cancel
      </Button>
    </div>
  )
}

/** An error under the row, where the click was; the toast would say it somewhere else. */
export function RowError({ text, onDismiss }: { text: string; onDismiss: () => void }) {
  return (
    <div role="alert" className="flex items-center gap-2 px-1 pb-1.5 text-xs text-rust">
      <span className="min-w-0 flex-1 truncate selectable">{text}</span>
      <Button size="sm" variant="ghost" onClick={onDismiss} title="Dismiss this error">
        Dismiss
      </Button>
    </div>
  )
}

/**
 * Type-the-count (§51): permanent deletes and Dig removal. The exact count
 * comes from the Action layer's preview; the button stays disabled, with
 * its reason, until the typed number matches. The button is the verb every
 * other confirmation uses (`pendingVerb`) with the count beside it, and the
 * line above the field says the same thing: a "Delete" button under "moves
 * to Trash" copy was the audit's example of the app contradicting itself,
 * and this file's own verb table once said "Remove account" where the
 * approval dialog said "Remove the account".
 */
import { useState } from 'react'
import { Dialog } from '@/components/ui/Dialog'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Field'
import { expectedCount } from '@/lib/confirm'
import { pendingVerb } from '@/lib/pendingVerb'
import type { PendingAction } from '@/bindings/PendingAction'

function descriptionFor(action: string): string {
  if (action === 'permanent_delete') return "They go to Gmail's Trash, which Gmail empties after 30 days; Riddle forgets them and cannot undo this. Type the exact count to continue."
  return 'This cannot be undone. Type the exact count to continue.'
}

export function TypeCountDialog({ pending, onConfirm, onCancel }: { pending: PendingAction; onConfirm: () => void; onCancel: () => void }) {
  const [typed, setTyped] = useState('')
  const count = expectedCount(pending)
  const matches = count !== null && typed.trim() === String(count)
  const verb = pendingVerb(pending, 'Confirm').button
  return (
    <Dialog open onOpenChange={(o) => !o && onCancel()} title={pending.summary} description={descriptionFor(pending.action)}>
      <p className="mb-3 text-sm text-ink-2">{pending.reason}</p>
      <label className="block">
        <span className="mb-1 block text-xs font-medium text-ink-2">Type {count ?? '?'} to confirm</span>
        <Input autoFocus inputMode="numeric" value={typed} onChange={(e) => setTyped(e.target.value)} placeholder={count !== null ? String(count) : ''} aria-label={`Type ${count} to confirm`} onKeyDown={(e) => e.key === 'Enter' && matches && onConfirm()} />
      </label>
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="danger" onClick={onConfirm} disabledReason={matches ? null : count === null ? 'Riddle could not count these messages; try again' : `Type ${count} exactly`}>
          {count === null ? verb : `${verb} (${count})`}
        </Button>
      </div>
    </Dialog>
  )
}

/**
 * Type-the-count (§51): permanent deletes and Dig removal. The exact count
 * comes from the Action layer's preview; the button stays disabled, with
 * its reason, until the typed number matches.
 */
import { useState } from 'react'
import { Dialog } from '@/components/ui/Dialog'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Field'
import { expectedCount } from '@/lib/confirm'
import type { PendingAction } from '@/bindings/PendingAction'

export function TypeCountDialog({ pending, onConfirm, onCancel }: { pending: PendingAction; onConfirm: () => void; onCancel: () => void }) {
  const [typed, setTyped] = useState('')
  const count = expectedCount(pending)
  const matches = count !== null && typed.trim() === String(count)
  return (
    <Dialog open onOpenChange={(o) => !o && onCancel()} title={pending.summary} description="This cannot be undone. Type the exact count to continue.">
      <p className="mb-3 text-sm text-ink-2">{pending.reason}</p>
      <label className="block">
        <span className="mb-1 block text-xs font-medium text-ink-2">Type {count ?? '?'} to confirm</span>
        <Input autoFocus inputMode="numeric" value={typed} onChange={(e) => setTyped(e.target.value)} placeholder={count !== null ? String(count) : ''} aria-label={`Type ${count} to confirm`} onKeyDown={(e) => e.key === 'Enter' && matches && onConfirm()} />
      </label>
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="danger" onClick={onConfirm} disabledReason={matches ? null : count === null ? 'The Action layer gave no count' : `Type ${count} exactly`}>
          {pending.action === 'remove_dig' ? 'Remove Dig' : `Delete ${count ?? ''}`}
        </Button>
      </div>
    </Dialog>
  )
}

/**
 * Approval (§51, §58): the reason is on the control, and the real
 * arguments are shown, so a Director proposal is approved on what it will
 * actually do rather than on its description of it.
 *
 * Focus lands on Reject, not the primary: the UX study watched an Enter
 * reflex approve a Mesh before the arguments were read. The title and the
 * primary button say the verb and the noun ("Apply the Mesh"), and the
 * arguments are rows, not JSON, so there is something to read.
 */
import { Dialog } from '@/components/ui/Dialog'
import { Button } from '@/components/ui/Button'
import { Note } from '@/components/ui/Note'
import { RealArgumentRows } from './RealArguments'
import { pendingVerb } from '@/lib/pendingVerb'
import type { PendingAction } from '@/bindings/PendingAction'

export function ApprovalDialog({ pending, onConfirm, onCancel }: { pending: PendingAction; onConfirm: () => void; onCancel: () => void }) {
  const verb = pendingVerb(pending)
  const untrusted = pending.principal.kind === 'director' || pending.principal.kind === 'expedition' ? pending.principal.read_untrusted : false
  return (
    <Dialog open onOpenChange={(o) => !o && onCancel()} title={verb.line} description={pending.reason} wide>
      {untrusted ? (
        <Note tone="warn" title="This task read untrusted content">
          The Director read mail in this task, so this egress Action needs your approval. Check the real arguments below, not the Director's description of them.
        </Note>
      ) : null}
      <div className="mt-3">
        {verb.line !== pending.summary ? <p className="mb-2 text-sm text-ink-2 selectable">{pending.summary}</p> : null}
        <div className="mb-1 text-xs font-medium text-ink-2">Real arguments</div>
        <RealArgumentRows real={pending.realArguments} className="text-sm" />
      </div>
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel} autoFocus title="Nothing changes; the proposal stays pending">
          Reject
        </Button>
        <Button variant="primary" onClick={onConfirm}>
          {verb.button}
        </Button>
      </div>
    </Dialog>
  )
}

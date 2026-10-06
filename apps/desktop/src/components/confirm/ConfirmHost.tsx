/**
 * Renders whichever confirmation tier the head of the queue needs. The
 * in-place tier never reaches here (useAction confirms it directly).
 */
import { useConfirmQueue } from '@/lib/stores'
import { tierFor } from '@/lib/confirm'
import { TwoStepDialog } from './TwoStepDialog'
import { TypeCountDialog } from './TypeCountDialog'
import { RealArgumentRows } from './RealArguments'
import { ApprovalDialog } from './ApprovalDialog'
import { HoldButton } from './HoldButton'
import { Dialog } from '@/components/ui/Dialog'
import { Button } from '@/components/ui/Button'

export function ConfirmHost() {
  const head = useConfirmQueue((s) => s.queue[0])
  const settle = useConfirmQueue((s) => s.settle)
  if (!head) return null
  const onConfirm = () => settle(head.id, true)
  const onCancel = () => settle(head.id, false)
  switch (tierFor(head.pending.confirmation)) {
    case 'TwoStepDialog':
      return <TwoStepDialog pending={head.pending} onConfirm={onConfirm} onCancel={onCancel} />
    case 'TypeCountDialog':
      return <TypeCountDialog pending={head.pending} onConfirm={onConfirm} onCancel={onCancel} />
    case 'ApprovalDialog':
      return <ApprovalDialog pending={head.pending} onConfirm={onConfirm} onCancel={onCancel} />
    case 'HoldButton':
      return (
        <Dialog open onOpenChange={(o) => !o && onCancel()} title={head.pending.summary} description={head.pending.reason}>
          <div className="max-h-56 overflow-auto rounded border border-line bg-sand p-2 text-xs selectable">
            <RealArgumentRows real={head.pending.realArguments} />
          </div>
          <div className="mt-4 flex items-end justify-between gap-3">
            <HoldButton label="Hold to confirm" coaching="Press and hold for 1.5 seconds. Letting go cancels." onComplete={onConfirm} />
            <Button variant="ghost" onClick={onCancel}>
              Cancel
            </Button>
          </div>
        </Dialog>
      )
    default:
      onConfirm()
      return null
  }
}

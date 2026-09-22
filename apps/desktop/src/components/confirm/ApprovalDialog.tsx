/**
 * Approval (§51, §58): the reason is on the control, and the real
 * arguments are shown, so a Director proposal is approved on what it will
 * actually do rather than on its description of it.
 */
import { Dialog } from '@/components/ui/Dialog'
import { Button } from '@/components/ui/Button'
import { Note } from '@/components/ui/Note'
import { safeJson } from '@/lib/format'
import type { PendingAction } from '@/bindings/PendingAction'

export function ApprovalDialog({ pending, onConfirm, onCancel }: { pending: PendingAction; onConfirm: () => void; onCancel: () => void }) {
  const real = pending.realArguments as Record<string, unknown> | null
  const text = real && typeof real.text === 'string' ? real.text : null
  const untrusted = pending.principal.kind === 'director' || pending.principal.kind === 'expedition' ? pending.principal.read_untrusted : false
  return (
    <Dialog open onOpenChange={(o) => !o && onCancel()} title={pending.summary} description={pending.reason} wide>
      {untrusted ? (
        <Note tone="warn" title="This task read untrusted content">
          The Director read mail in this task, so this egress Action needs your approval. Check the real arguments below, not the Director's description of them.
        </Note>
      ) : null}
      <div className="mt-3">
        <div className="mb-1 text-xs font-medium text-ink-2">Real arguments</div>
        {text ? <pre className="max-h-72 overflow-auto rounded border border-line bg-sand p-2 text-xs mono selectable whitespace-pre">{text}</pre> : null}
        <pre className="mt-2 max-h-40 overflow-auto rounded border border-line bg-sand p-2 text-xs selectable">{safeJson(text ? { ...real, text: undefined } : pending.realArguments)}</pre>
      </div>
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          Reject
        </Button>
        <Button variant="primary" onClick={onConfirm} autoFocus>
          Approve
        </Button>
      </div>
    </Dialog>
  )
}

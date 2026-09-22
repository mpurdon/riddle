/**
 * Two-step confirmation (§51): the dialog shows the Action layer's real
 * arguments, never the caller's description. For send_message that means
 * the actual recipients, and the line that sent mail cannot be unsent.
 */
import { Dialog } from '@/components/ui/Dialog'
import { Button } from '@/components/ui/Button'
import { realRecipients } from '@/lib/confirm'
import { safeJson } from '@/lib/format'
import type { PendingAction } from '@/bindings/PendingAction'

export function TwoStepDialog({ pending, onConfirm, onCancel }: { pending: PendingAction; onConfirm: () => void; onCancel: () => void }) {
  const isSend = pending.action === 'send_message'
  const rec = realRecipients(pending)
  const real = pending.realArguments as Record<string, unknown> | null
  return (
    <Dialog open onOpenChange={(o) => !o && onCancel()} title={isSend ? 'Send this message?' : pending.summary} description={isSend ? 'These are the real recipients the Action layer will send to.' : pending.reason}>
      {isSend && rec ? (
        <dl className="grid grid-cols-[4rem_1fr] gap-y-1">
          {real?.from ? (
            <>
              <dt className="text-clay">From</dt>
              <dd className="selectable">{String(real.from)}</dd>
            </>
          ) : null}
          <dt className="text-clay">To</dt>
          <dd className="selectable">{rec.to.length ? rec.to.map((r) => (r.name ? `${r.name} <${r.email}>` : r.email)).join(', ') : <span className="text-rust">nobody</span>}</dd>
          {rec.cc.length ? (
            <>
              <dt className="text-clay">Cc</dt>
              <dd className="selectable">{rec.cc.map((r) => r.email).join(', ')}</dd>
            </>
          ) : null}
          {rec.bcc.length ? (
            <>
              <dt className="text-clay">Bcc</dt>
              <dd className="selectable">{rec.bcc.map((r) => r.email).join(', ')}</dd>
            </>
          ) : null}
          <dt className="text-clay">Subject</dt>
          <dd className="selectable">{rec.subject || '(no subject)'}</dd>
        </dl>
      ) : (
        <pre className="max-h-64 overflow-auto rounded border border-line bg-sand p-2 text-xs selectable">{safeJson(pending.realArguments)}</pre>
      )}
      {isSend ? <p className="mt-4 text-sm font-medium text-rust">Sent mail cannot be unsent.</p> : null}
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel}>
          Cancel
        </Button>
        <Button variant="primary" onClick={onConfirm} autoFocus disabledReason={isSend && rec && !rec.to.length ? 'Add at least one recipient' : null}>
          {isSend ? 'Send' : 'Confirm'}
        </Button>
      </div>
    </Dialog>
  )
}

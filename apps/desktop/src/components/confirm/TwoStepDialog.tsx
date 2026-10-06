/**
 * Two-step confirmation (§51): the dialog shows the Action layer's real
 * arguments, never the caller's description. For send_message that means
 * the actual recipients, and the line that sent mail cannot be unsent.
 *
 * Focus lands on Cancel: the UX study watched Enter send a message whose
 * recipients had not been read. For everything else the title is the verb
 * and the noun, the primary says the verb, and the arguments are rows.
 */
import { Dialog } from '@/components/ui/Dialog'
import { Button } from '@/components/ui/Button'
import { realRecipients } from '@/lib/confirm'
import { RealArgumentRows } from './RealArguments'
import { pendingVerb } from '@/lib/pendingVerb'
import type { PendingAction } from '@/bindings/PendingAction'

export function TwoStepDialog({ pending, onConfirm, onCancel }: { pending: PendingAction; onConfirm: () => void; onCancel: () => void }) {
  const isSend = pending.action === 'send_message'
  const rec = realRecipients(pending)
  const real = pending.realArguments as Record<string, unknown> | null
  const verb = pendingVerb(pending, 'Confirm')
  return (
    <Dialog open onOpenChange={(o) => !o && onCancel()} title={isSend ? 'Send this message?' : verb.line} description={isSend ? 'These are the real recipients the Action layer will send to.' : pending.reason}>
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
        <>
          {verb.line !== pending.summary ? <p className="mb-2 text-ink-2 selectable">{pending.summary}</p> : null}
          <RealArgumentRows real={pending.realArguments} />
        </>
      )}
      {isSend ? <p className="mt-4 text-sm font-medium text-rust">Sent mail cannot be unsent.</p> : null}
      <div className="mt-5 flex justify-end gap-2">
        <Button variant="ghost" onClick={onCancel} autoFocus>
          Cancel
        </Button>
        <Button variant="primary" onClick={onConfirm} disabledReason={isSend && rec && !rec.to.length ? 'Add at least one recipient' : null}>
          {isSend ? 'Send' : verb.button}
        </Button>
      </div>
    </Dialog>
  )
}

/**
 * The in-place tier: nothing to click before the Action runs; the control
 * is the Undo in the toast that follows. Exported for the shell's Toaster,
 * and as a standalone toast rendering for tests.
 */
import { Undo2, X } from 'lucide-react'
import { Button } from '@/components/ui/Button'
import { cx } from '@/lib/format'
import type { Toast } from '@/lib/stores'

export function ConfirmInPlace({ toast, onDismiss }: { toast: Toast; onDismiss: () => void }) {
  const tone = toast.kind === 'error' ? 'border-rust/40' : toast.kind === 'warn' ? 'border-ochre/40' : 'border-line'
  return (
    <div role="status" aria-live="polite" className={cx('toast-in pointer-events-auto flex w-80 items-start gap-2 rounded-md border bg-paper px-3 py-2 text-sm shadow-lg', tone)}>
      <div className="min-w-0 flex-1">
        <div className="font-medium text-ink">{toast.title}</div>
        {toast.body ? <div className="mt-0.5 text-xs text-ink-2 whitespace-pre-wrap">{toast.body}</div> : null}
      </div>
      {toast.action ? (
        <Button
          size="sm"
          variant="secondary"
          icon={toast.action.label === 'Undo' ? <Undo2 /> : undefined}
          onClick={async () => {
            await toast.action!.onClick()
            onDismiss()
          }}
        >
          {toast.action.label}
        </Button>
      ) : null}
      <button type="button" aria-label="Dismiss" className="rounded p-0.5 text-clay hover:text-ink" onClick={onDismiss}>
        <X className="size-3.5" />
      </button>
    </div>
  )
}

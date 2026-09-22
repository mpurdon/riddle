import { useEffect } from 'react'
import { useToasts } from '@/lib/stores'
import { ConfirmInPlace } from './confirm/ConfirmInPlace'

export function Toaster() {
  const toasts = useToasts((s) => s.toasts)
  const dismiss = useToasts((s) => s.dismiss)
  useEffect(() => {
    if (!toasts.length) return
    const timers = toasts.map((t) => setTimeout(() => dismiss(t.id), Math.max(0, t.createdAt + t.ttlMs - Date.now())))
    return () => timers.forEach(clearTimeout)
  }, [toasts, dismiss])
  return (
    <div className="pointer-events-none fixed bottom-4 right-4 z-[60] flex flex-col gap-2" aria-label="Notifications">
      {toasts.map((t) => (
        <ConfirmInPlace key={t.id} toast={t} onDismiss={() => dismiss(t.id)} />
      ))}
    </div>
  )
}

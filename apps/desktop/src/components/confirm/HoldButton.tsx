/**
 * Press-and-hold (§51): the control fills while held with the pointer or
 * with Space/Enter, releases early to cancel, and shows coaching text so
 * the user knows what holding will do. Screen readers get a progressbar.
 */
import { useEffect, useRef, useState, type KeyboardEvent } from 'react'
import { cx } from '@/lib/format'
import { Tip } from '@/components/ui/Tooltip'

export interface HoldButtonProps {
  label: string
  coaching: string
  onComplete: () => unknown | Promise<unknown>
  holdMs?: number
  disabledReason?: string | null
  danger?: boolean
  className?: string
}

export function HoldButton({ label, coaching, onComplete, holdMs = 1500, disabledReason, danger = true, className }: HoldButtonProps) {
  const [progress, setProgress] = useState(0)
  const [holding, setHolding] = useState(false)
  const [done, setDone] = useState(false)
  // A timer, not requestAnimationFrame: the hold must complete even when
  // the window is not being painted (a hidden tab, a screenshot in flight).
  const timer = useRef<ReturnType<typeof setInterval> | null>(null)
  const startedAt = useRef<number | null>(null)
  const completedRef = useRef(false)

  const stop = () => {
    if (timer.current) clearInterval(timer.current)
    timer.current = null
    startedAt.current = null
    setHolding(false)
    if (!completedRef.current) setProgress(0)
  }

  const tick = () => {
    if (startedAt.current === null) return
    const p = Math.min(1, (Date.now() - startedAt.current) / holdMs)
    setProgress(p)
    if (p >= 1) {
      completedRef.current = true
      setDone(true)
      stop()
      void Promise.resolve(onComplete()).finally(() => {
        completedRef.current = false
        setDone(false)
        setProgress(0)
      })
    }
  }

  const start = () => {
    if (disabledReason || holding || completedRef.current) return
    startedAt.current = Date.now()
    setHolding(true)
    timer.current = setInterval(tick, 40)
  }

  useEffect(() => () => stop(), []) // eslint-disable-line react-hooks/exhaustive-deps

  const onKeyDown = (e: KeyboardEvent<HTMLButtonElement>) => {
    if ((e.key === ' ' || e.key === 'Enter') && !e.repeat) {
      e.preventDefault()
      start()
    }
  }
  const onKeyUp = (e: KeyboardEvent<HTMLButtonElement>) => {
    if (e.key === ' ' || e.key === 'Enter') {
      e.preventDefault()
      stop()
    }
  }

  const btn = (
    <div className={cx('inline-flex flex-col gap-1', className)}>
      <button
        type="button"
        aria-disabled={!!disabledReason || undefined}
        aria-label={`${label}. Press and hold to confirm. ${coaching}`}
        aria-describedby={undefined}
        className={cx(
          'relative h-9 min-w-48 select-none overflow-hidden rounded-md border px-4 text-sm font-medium',
          danger ? 'border-rust/40 text-rust' : 'border-ochre/40 text-ochre',
          disabledReason && 'cursor-not-allowed opacity-50',
          done && 'opacity-80',
        )}
        onPointerDown={(e) => {
          if (e.button !== 0) return
          try {
            e.currentTarget.setPointerCapture(e.pointerId)
          } catch {
            /* synthetic or already-released pointer */
          }
          start()
        }}
        onPointerUp={stop}
        onPointerCancel={stop}
        onPointerLeave={() => {
          if (holding) stop()
        }}
        onKeyDown={onKeyDown}
        onKeyUp={onKeyUp}
        onBlur={stop}
        onContextMenu={(e) => e.preventDefault()}
      >
        <span aria-hidden className={cx('absolute inset-y-0 left-0 hold-progress', danger ? 'bg-rust/20' : 'bg-ochre/20')} style={{ width: `${progress * 100}%` }} />
        <span className="relative">{done ? 'Confirmed' : holding ? 'Keep holding…' : label}</span>
      </button>
      <div role="progressbar" aria-label="Hold progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={Math.round(progress * 100)} className="sr-only" />
      <span className="text-xs text-ink-2">{holding ? `Hold for ${(holdMs / 1000).toFixed(1)}s. Release to cancel.` : coaching}</span>
    </div>
  )
  return disabledReason ? <Tip text={disabledReason}>{btn}</Tip> : btn
}

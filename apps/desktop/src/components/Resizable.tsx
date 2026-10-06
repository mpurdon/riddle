/**
 * A pane whose width the person can drag, with the width remembered per
 * pane in this browser. The list panes were fixed at 420 px and the message
 * preview at 46% of the window, which cut sender names in one and wasted
 * the other on a wide screen. The handle is a real separator: focusable,
 * arrow keys move it 16 px, Home or a double-click puts it back. Stored in
 * localStorage only (a per-viewer convenience), never in settings.
 */
import { useState, type ComponentProps, type ElementType, type PointerEvent, type ReactNode } from 'react'
import { cx } from '@/lib/format'
import { readLocalJson, writeLocalJson } from '@/lib/local'

const STEP = 16

export interface ResizableProps {
  /** Storage key; one per pane so each remembers its own width. */
  id: string
  /** Which side of the window the pane sits on, so the handle is on its inner edge. */
  side: 'left' | 'right'
  initial: number
  min: number
  max: number
  as?: 'section' | 'aside' | 'div' | 'ol'
  label?: string
  className?: string
  children: ReactNode
}

const clamp = (n: number, min: number, max: number) => Math.min(max, Math.max(min, Math.round(n)))

export function Resizable({ id, side, initial, min, max, as = 'div', label, className, children, ...rest }: ResizableProps & Omit<ComponentProps<'div'>, keyof ResizableProps>) {
  const key = `pane:${id}`
  const [width, setWidth] = useState(() => clamp(readLocalJson<number>(key, initial), min, max))
  const set = (w: number) => {
    const next = clamp(w, min, max)
    setWidth(next)
    writeLocalJson(key, next)
  }
  const drag = (e: PointerEvent<HTMLElement>) => {
    if (e.button !== 0) return
    e.preventDefault()
    const startX = e.clientX
    const startWidth = width
    const target = e.currentTarget
    target.setPointerCapture?.(e.pointerId)
    const move = (ev: globalThis.PointerEvent) => set(startWidth + (side === 'left' ? ev.clientX - startX : startX - ev.clientX))
    const up = () => {
      target.removeEventListener('pointermove', move)
      target.removeEventListener('pointerup', up)
      target.removeEventListener('pointercancel', up)
    }
    target.addEventListener('pointermove', move)
    target.addEventListener('pointerup', up)
    target.addEventListener('pointercancel', up)
  }
  const keys = (e: React.KeyboardEvent<HTMLElement>) => {
    const grow = side === 'left' ? 'ArrowRight' : 'ArrowLeft'
    const shrink = side === 'left' ? 'ArrowLeft' : 'ArrowRight'
    if (e.key === grow) set(width + STEP)
    else if (e.key === shrink) set(width - STEP)
    else if (e.key === 'Home') set(initial)
    else return
    e.preventDefault()
  }
  const Tag = as as ElementType
  const Handle = (as === 'ol' ? 'li' : 'div') as ElementType
  return (
    <Tag className={cx('relative shrink-0', className)} style={{ width }} {...rest}>
      {children}
      <Handle
        role="separator"
        aria-orientation="vertical"
        aria-label={label ?? 'Resize pane'}
        aria-valuenow={width}
        aria-valuemin={min}
        aria-valuemax={max}
        tabIndex={0}
        title="Drag to resize. Arrow keys move it; Home or a double-click puts it back."
        onPointerDown={drag}
        onKeyDown={keys}
        onDoubleClick={() => set(initial)}
        className={cx('absolute inset-y-0 z-10 w-1.5 cursor-col-resize select-none hover:bg-ochre/40 focus-visible:bg-ochre/60 focus-visible:outline-none', side === 'left' ? '-right-[3px]' : '-left-[3px]')}
      />
    </Tag>
  )
}

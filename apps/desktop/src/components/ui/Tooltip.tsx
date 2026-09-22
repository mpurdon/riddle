import * as RT from '@radix-ui/react-tooltip'
import type { ReactNode } from 'react'

export function TooltipProvider({ children }: { children: ReactNode }) {
  return <RT.Provider delayDuration={400}>{children}</RT.Provider>
}

/** A tooltip whose text is also the accessible label of what it wraps. */
export function Tip({ text, children, side = 'bottom' }: { text: string | null | undefined; children: ReactNode; side?: 'top' | 'bottom' | 'left' | 'right' }) {
  if (!text) return <>{children}</>
  return (
    <RT.Root>
      <RT.Trigger asChild>{children}</RT.Trigger>
      <RT.Portal>
        <RT.Content side={side} sideOffset={6} className="z-50 max-w-xs rounded-md border border-line bg-sand px-2.5 py-1.5 text-xs text-ink shadow-md fade-in">
          {text}
          <RT.Arrow className="fill-sand" />
        </RT.Content>
      </RT.Portal>
    </RT.Root>
  )
}

import * as RD from '@radix-ui/react-dialog'
import { X } from 'lucide-react'
import type { ReactNode } from 'react'
import { cx } from '@/lib/format'

export function Dialog({ open, onOpenChange, title, description, children, footer, wide }: { open: boolean; onOpenChange: (o: boolean) => void; title: string; description?: ReactNode; children?: ReactNode; footer?: ReactNode; wide?: boolean }) {
  return (
    <RD.Root open={open} onOpenChange={onOpenChange}>
      <RD.Portal>
        <RD.Overlay className="fixed inset-0 z-40 bg-ink/30 fade-in" />
        <RD.Content className={cx('fixed left-1/2 top-1/2 z-50 -translate-x-1/2 -translate-y-1/2 rounded-lg border border-line bg-paper p-5 shadow-xl fade-in focus:outline-none', wide ? 'w-[720px] max-w-[92vw]' : 'w-[480px] max-w-[92vw]')}>
          <div className="flex items-start justify-between gap-4">
            <RD.Title className="text-base font-semibold">{title}</RD.Title>
            <RD.Close className="rounded p-1 text-clay hover:bg-sand hover:text-ink active:bg-sand-2" aria-label="Close" title="Close (Esc)">
              <X className="size-4" />
            </RD.Close>
          </div>
          {description ? <RD.Description className="mt-1 text-sm text-ink-2">{description}</RD.Description> : <RD.Description className="sr-only">{title}</RD.Description>}
          {children ? <div className="mt-4 text-sm">{children}</div> : null}
          {footer ? <div className="mt-5 flex items-center justify-end gap-2">{footer}</div> : null}
        </RD.Content>
      </RD.Portal>
    </RD.Root>
  )
}

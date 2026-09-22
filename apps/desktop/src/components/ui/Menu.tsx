import * as RM from '@radix-ui/react-dropdown-menu'
import type { ReactNode } from 'react'
import { cx } from '@/lib/format'

export interface MenuItem {
  label: string
  onSelect: () => void
  disabledReason?: string | null
  danger?: boolean
  hint?: string
}

export function Menu({ trigger, items, label, align = 'end' }: { trigger: ReactNode; items: (MenuItem | 'separator')[]; label: string; align?: 'start' | 'end' }) {
  return (
    <RM.Root>
      <RM.Trigger asChild aria-label={label}>
        {trigger}
      </RM.Trigger>
      <RM.Portal>
        <RM.Content align={align} sideOffset={4} className="z-50 min-w-44 rounded-md border border-line bg-paper p-1 text-sm shadow-lg fade-in">
          {items.map((it, i) =>
            it === 'separator' ? (
              <RM.Separator key={i} className="my-1 h-px bg-line" />
            ) : (
              <RM.Item
                key={it.label}
                disabled={!!it.disabledReason}
                title={it.disabledReason ?? undefined}
                onSelect={it.onSelect}
                className={cx('flex cursor-default select-none items-center justify-between gap-4 rounded px-2 py-1 outline-none data-[highlighted]:bg-sand data-[disabled]:opacity-50', it.danger && 'text-rust')}
              >
                <span>{it.label}</span>
                {it.hint ? <kbd>{it.hint}</kbd> : it.disabledReason ? <span className="text-[11px] text-clay">{it.disabledReason}</span> : null}
              </RM.Item>
            ),
          )}
        </RM.Content>
      </RM.Portal>
    </RM.Root>
  )
}

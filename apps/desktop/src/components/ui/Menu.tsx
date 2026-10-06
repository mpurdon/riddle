/**
 * The dropdown menu. Uncontrolled by default; `open` and `onOpenChange`
 * make it controlled so a keyboard shortcut can open it, `onCloseAutoFocus`
 * lets the caller send focus somewhere other than the trigger (a textarea
 * whose caret the menu inserts at), and entries can be grouped
 * (`{ group, items }`) or headed (`{ heading }` after a 'separator'). The
 * body scrolls within the height Radix reports rather than running off the
 * window. The Mesh Insert menu grew its own copy of this file for want of
 * those three things (features/meshes/InsertMenu.tsx); now it need not.
 */
import * as RM from '@radix-ui/react-dropdown-menu'
import type { ReactNode } from 'react'
import { cx } from '@/lib/format'

export interface MenuItem {
  label: string
  onSelect: () => void
  disabledReason?: string | null
  danger?: boolean
  hint?: string
  /** A hint is a keycap by default; `'text'` renders it as plain muted text ("yes/no"). */
  hintKind?: 'keys' | 'text'
  /** Monospace label, for DSL tokens. */
  mono?: boolean
}

/** A titled run of items. */
export interface MenuGroup {
  group: string
  items: (MenuItem | 'separator')[]
  /** Shown in place of the items when there are none. */
  empty?: string
}

/** A title on its own, for callers that lay out groups with 'separator'. */
export interface MenuHeading {
  heading: string
}

export type MenuEntry = MenuItem | MenuGroup | MenuHeading | 'separator'

export interface MenuProps {
  trigger: ReactNode
  items: MenuEntry[]
  label: string
  align?: 'start' | 'end'
  /** Controlled mode; leave both out for an uncontrolled menu. */
  open?: boolean
  onOpenChange?: (open: boolean) => void
  /** Called as focus returns on close; `preventDefault()` to keep it where the caller put it. */
  onCloseAutoFocus?: (e: Event) => void
}

const labelClass = 'px-2 pb-0.5 pt-1 text-2xs font-semibold uppercase tracking-wide text-clay'

function entries(list: MenuEntry[], prefix: string): ReactNode[] {
  return list.map((it, i) => {
    const key = `${prefix}${i}`
    if (it === 'separator') return <RM.Separator key={key} className="my-1 h-px bg-line" />
    if ('group' in it) {
      return (
        <RM.Group key={key}>
          <RM.Label className={labelClass}>{it.group}</RM.Label>
          {it.items.length ? entries(it.items, `${key}.`) : it.empty ? <div className="px-2 py-1 text-2xs text-clay">{it.empty}</div> : null}
        </RM.Group>
      )
    }
    if ('heading' in it) {
      return (
        <RM.Label key={key} className={labelClass}>
          {it.heading}
        </RM.Label>
      )
    }
    return (
      <RM.Item
        key={key}
        disabled={!!it.disabledReason}
        title={it.disabledReason ?? undefined}
        textValue={it.label}
        onSelect={it.onSelect}
        className={cx(
          'flex cursor-default select-none items-center justify-between gap-4 rounded px-2 py-1 outline-none data-[highlighted]:bg-sand data-[disabled]:opacity-50',
          it.danger && 'text-rust',
        )}
      >
        <span className={it.mono ? 'mono' : undefined}>{it.label}</span>
        {it.hint ? it.hintKind === 'text' ? <span className="text-2xs text-clay">{it.hint}</span> : <kbd>{it.hint}</kbd> : it.disabledReason ? <span className="text-2xs text-clay">{it.disabledReason}</span> : null}
      </RM.Item>
    )
  })
}

export function Menu({ trigger, items, label, align = 'end', open, onOpenChange, onCloseAutoFocus }: MenuProps) {
  return (
    <RM.Root open={open} onOpenChange={onOpenChange}>
      <RM.Trigger asChild aria-label={label}>
        {trigger}
      </RM.Trigger>
      <RM.Portal>
        <RM.Content
          align={align}
          sideOffset={4}
          onCloseAutoFocus={onCloseAutoFocus}
          className="z-50 max-h-[var(--radix-dropdown-menu-content-available-height)] min-w-44 overflow-y-auto rounded-md border border-line bg-paper p-1 text-sm shadow-lg fade-in"
        >
          {entries(items, '')}
        </RM.Content>
      </RM.Portal>
    </RM.Root>
  )
}

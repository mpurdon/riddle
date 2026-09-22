import * as RS from '@radix-ui/react-select'
import { Check, ChevronDown } from 'lucide-react'
import { cx } from '@/lib/format'

export interface SelectOption {
  value: string
  label: string
}

export function Select({ value, onChange, options, label, className, size = 'md', placeholder }: { value: string; onChange: (v: string) => void; options: SelectOption[]; label: string; className?: string; size?: 'sm' | 'md'; placeholder?: string }) {
  return (
    <RS.Root value={value} onValueChange={onChange}>
      <RS.Trigger aria-label={label} className={cx('inline-flex items-center justify-between gap-2 rounded-md border border-line bg-sand text-ink hover:bg-sand-2', size === 'sm' ? 'h-6 px-2 text-xs' : 'h-8 px-2.5 text-sm', className)}>
        <RS.Value placeholder={placeholder} />
        <RS.Icon>
          <ChevronDown className="size-3.5 text-clay" />
        </RS.Icon>
      </RS.Trigger>
      <RS.Portal>
        <RS.Content position="popper" sideOffset={4} className="z-50 min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-md border border-line bg-paper shadow-lg fade-in">
          <RS.Viewport className="p-1">
            {options.map((o) => (
              <RS.Item key={o.value} value={o.value} className="relative flex cursor-default select-none items-center rounded px-6 py-1 text-sm text-ink outline-none data-[highlighted]:bg-sand data-[state=checked]:font-medium">
                <RS.ItemIndicator className="absolute left-1.5">
                  <Check className="size-3.5" />
                </RS.ItemIndicator>
                <RS.ItemText>{o.label}</RS.ItemText>
              </RS.Item>
            ))}
          </RS.Viewport>
        </RS.Content>
      </RS.Portal>
    </RS.Root>
  )
}

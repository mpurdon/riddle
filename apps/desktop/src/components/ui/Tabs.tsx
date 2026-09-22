import * as RTabs from '@radix-ui/react-tabs'
import type { ReactNode } from 'react'
import { cx } from '@/lib/format'

export function Tabs({ value, onChange, tabs, children, className }: { value: string; onChange: (v: string) => void; tabs: { value: string; label: ReactNode }[]; children: ReactNode; className?: string }) {
  return (
    <RTabs.Root value={value} onValueChange={onChange} className={cx('flex min-h-0 flex-col', className)}>
      <RTabs.List className="flex shrink-0 gap-1 border-b border-line px-1" aria-label="Sections">
        {tabs.map((t) => (
          <RTabs.Trigger key={t.value} value={t.value} className="-mb-px border-b-2 border-transparent px-2.5 py-1.5 text-sm text-ink-2 hover:text-ink data-[state=active]:border-ochre data-[state=active]:text-ink">
            {t.label}
          </RTabs.Trigger>
        ))}
      </RTabs.List>
      {children}
    </RTabs.Root>
  )
}

export const TabPanel = ({ value, children, className }: { value: string; children: ReactNode; className?: string }) => (
  <RTabs.Content value={value} className={cx('min-h-0 flex-1 overflow-auto focus:outline-none', className)}>
    {children}
  </RTabs.Content>
)

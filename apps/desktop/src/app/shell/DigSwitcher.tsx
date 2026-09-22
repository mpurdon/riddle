import { ChevronDown } from 'lucide-react'
import { Chip } from '@/components/ui/Chip'
import { Menu, type MenuItem } from '@/components/ui/Menu'
import { digStatusChip } from '@/lib/selectors'
import { useUi } from '@/lib/stores'
import { useDigs } from '@/lib/queries'

export function DigSwitcher() {
  const digs = useDigs()
  const digId = useUi((s) => s.digId)
  const setDigId = useUi((s) => s.setDigId)
  const current = digs.data?.find((d) => d.id === digId) ?? null
  const items: (MenuItem | 'separator')[] = [
    { label: 'All Digs', onSelect: () => setDigId(null), hint: digs.data ? `${digs.data.length}` : undefined },
    'separator',
    ...(digs.data ?? []).map((d) => ({ label: `${d.name} · ${digStatusChip(d.status).label}`, onSelect: () => setDigId(d.id) })),
  ]
  const chip = current ? digStatusChip(current.status) : null
  return (
    <Menu
      label="Choose a Dig"
      align="start"
      items={items}
      trigger={
        <button type="button" className="inline-flex h-7 items-center gap-1.5 rounded-md border border-line bg-sand px-2 text-sm hover:bg-sand-2" aria-label={`Dig: ${current?.name ?? 'All Digs'}`}>
          <span className="font-medium">{current?.name ?? 'All Digs'}</span>
          {chip ? (
            <Chip tone={chip.tone} title={chip.title}>
              {chip.label}
            </Chip>
          ) : digs.data ? (
            <span className="text-xs text-clay">{digs.data.length}</span>
          ) : null}
          <ChevronDown className="size-3.5 text-clay" />
        </button>
      }
    />
  )
}

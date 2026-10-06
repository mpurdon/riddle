/**
 * The account switcher. It shows the two things that decide what Riddle is
 * doing to the mailbox you are looking at: the autonomy rung and the sync
 * state. Before this the chrome showed sync only, so Work at Act and
 * Personal at Observe looked the same, and the rung lived three clicks away
 * in Settings. The rung's words and tooltip are the account settings' own
 * (`AUTONOMY_OPTIONS`), not a second telling of them. Disabled accounts
 * are not offered; one line says how many there are and leads to Settings,
 * the only place to switch them back on.
 */
import { useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { ChevronDown } from 'lucide-react'
import { Chip } from '@/components/ui/Chip'
import { Menu, type MenuItem } from '@/components/ui/Menu'
import { digStatusChip } from '@/lib/selectors'
import { useUi } from '@/lib/stores'
import { useAllDigs, useDigs, useSettings } from '@/lib/queries'
import { AUTONOMY_OPTIONS, autonomyLabel, autonomyOf, detailOf } from '@/features/settings/accounts/model'

export function DigSwitcher() {
  const digs = useDigs()
  const settings = useSettings()
  const digId = useUi((s) => s.digId)
  const setDigId = useUi((s) => s.setDigId)
  const nav = useNavigate()
  const disabled = (useAllDigs().data ?? []).filter((d) => d.disabled).length
  const current = digs.data?.find((d) => d.id === digId) ?? null
  // The chosen account was disabled (here or in Settings): show all.
  useEffect(() => {
    if (digId && digs.data && !current) setDigId(null)
  }, [digId, digs.data, current, setDigId])
  const rung = (id: string) => (settings.data ? autonomyOf(settings.data, id).level : null)
  const items: (MenuItem | 'separator')[] = [
    { label: 'All accounts', onSelect: () => setDigId(null), hint: digs.data ? `${digs.data.length}` : undefined },
    'separator',
    ...(digs.data ?? []).map((d) => {
      const r = rung(d.id)
      return { label: `${d.name} · ${r ? autonomyLabel(r) : '…'} · ${digStatusChip(d.status).label}`, onSelect: () => setDigId(d.id) }
    }),
    ...(disabled ? ['separator' as const, { label: `${disabled} disabled · manage in Settings`, onSelect: () => nav('/settings#digs') }] : []),
  ]
  const chip = current ? digStatusChip(current.status) : null
  const currentRung = current ? rung(current.id) : null
  return (
    <Menu
      label="Choose an account"
      align="start"
      items={items}
      trigger={
        <button
          type="button"
          className="inline-flex h-7 items-center gap-1.5 rounded-md border border-line bg-sand px-2 text-sm hover:bg-sand-2"
          aria-label={`Account: ${current?.name ?? 'All accounts'}`}
          title={current ? 'The account the pages, badges and Summary cover. Each account is its own Dig.' : 'All accounts at once. Pick one to see its rung and sync state.'}
        >
          <span className="font-medium">{current?.name ?? 'All accounts'}</span>
          {currentRung ? (
            <Chip tone="neutral" title={detailOf(AUTONOMY_OPTIONS, currentRung)}>
              {autonomyLabel(currentRung)}
            </Chip>
          ) : null}
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

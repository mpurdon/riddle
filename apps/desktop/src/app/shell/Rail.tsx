import { NavLink } from 'react-router-dom'
import { Activity, AlertTriangle, Compass, FlaskConical, Inbox, Layers, Lightbulb, ListChecks, MessageSquare, Network, Radio, Settings, Sunrise, Trash2, Wrench } from 'lucide-react'
import { cx, num } from '@/lib/format'
import { useSettings, useSummary, useDevInfo } from '@/lib/queries'
import { useProposals } from '@/lib/stores'

interface Item {
  to: string
  label: string
  icon: React.ReactNode
  count?: number | null
  tone?: 'ok' | 'warn'
}

export function Rail() {
  const settings = useSettings()
  const summary = useSummary()
  const dev = useDevInfo()
  const proposals = useProposals((s) => s.proposals.length)
  const s = summary.data
  const groups: { title: string | null; items: Item[] }[] = [
    {
      title: null,
      items: [
        { to: '/', label: 'Summary', icon: <Sunrise /> },
        { to: '/finds', label: 'Finds', icon: <Lightbulb />, count: s ? num(s.findsOpen) : null, tone: 'ok' },
        { to: '/unresolved', label: 'Unresolved', icon: <ListChecks />, count: s ? num(s.unresolved) : null, tone: 'warn' },
        { to: '/inbox', label: 'Inbox', icon: <Inbox /> },
      ],
    },
    {
      title: 'Excavation',
      items: [
        { to: '/spoil', label: 'Spoil', icon: <Trash2 /> },
        { to: '/contamination', label: 'Contamination', icon: <AlertTriangle /> },
        { to: '/strata', label: 'Strata', icon: <Layers /> },
        { to: '/meshes', label: 'Meshes', icon: <Network /> },
        { to: '/signals', label: 'Signals', icon: <Radio /> },
        { to: '/activity', label: 'Activity', icon: <Activity /> },
      ],
    },
    {
      title: 'Director',
      items: [
        { to: '/director', label: 'Director', icon: <MessageSquare />, count: proposals || null, tone: 'warn' },
        { to: '/expeditions', label: 'Expeditions', icon: <Compass /> },
      ],
    },
    {
      title: null,
      items: [{ to: '/settings', label: 'Settings', icon: <Settings /> }, ...(settings.data?.developerMode ? [{ to: '/developer', label: 'Developer', icon: <Wrench /> }] : [])],
    },
  ]
  return (
    <nav className="flex w-48 shrink-0 flex-col border-r border-line bg-sand/60 pt-1" aria-label="Sections">
      <div className="flex-1 overflow-auto px-2 pb-2">
        {groups.map((g, i) => (
          <div key={i} className={cx(i > 0 && 'mt-3')}>
            {g.title ? <div className="px-2 pb-1 text-[10px] font-semibold uppercase tracking-wider text-clay">{g.title}</div> : null}
            {g.items.map((it) => (
              <NavLink
                key={it.to}
                to={it.to}
                end={it.to === '/'}
                className={({ isActive }) => cx('flex h-7 items-center gap-2 rounded-md px-2 text-sm text-ink-2 hover:bg-sand-2 hover:text-ink', isActive && 'bg-sand-2 text-ink font-medium')}
                aria-label={it.count ? `${it.label}, ${it.count}` : it.label}
              >
                <span className="[&>svg]:size-4 text-clay">{it.icon}</span>
                <span className="flex-1 truncate">{it.label}</span>
                {it.count ? <span className={cx('rounded px-1 text-[11px] font-medium', it.tone === 'warn' ? 'bg-ochre/15 text-ochre' : 'bg-moss/15 text-moss')}>{it.count}</span> : null}
              </NavLink>
            ))}
          </div>
        ))}
      </div>
      <div className="border-t border-line px-3 py-2 text-[11px] text-clay">
        {dev.data ? (
          <span className="mono">
            {dev.data.build.version} {dev.data.build.gitSha}
            {dev.data.build.dirty ? '-dirty' : ''}
          </span>
        ) : (
          '…'
        )}
        {dev.data?.dev ? <FlaskConical className="ml-1 inline size-3 text-ochre" aria-label="Development build" /> : null}
      </div>
    </nav>
  )
}

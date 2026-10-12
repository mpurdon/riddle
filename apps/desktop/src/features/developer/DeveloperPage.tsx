/**
 * Developer Mode (§67-90): an interactive observability environment. It
 * shows more and grants nothing (§91).
 */
import { NavLink, Navigate, Route, Routes } from 'react-router-dom'
import { cx } from '@/lib/format'
import { useSettings } from '@/lib/queries'
import { Note } from '@/components/ui/Note'
import { OpsPane } from './OpsPane'
import { EscalationInspector, ExportTraces, MessageInspector, RawJevInspector } from './Inspectors'
import { MeshInspector, Playground, SignalInspector } from './Registry'
import { ActionInspector, ActivityTimeline, CostObservatory, DecisionQuality, DirectorInspector, FailureInspector, LivePipeline, PerformanceObservatory } from './Observatories'
import { DemoMode } from './DemoMode'
import { ModelsPane } from './ModelsPane'

const NAV: { to: string; label: string; group: string }[] = [
  { to: '', label: 'Operations', group: 'Ops' },
  { to: 'messages', label: 'Message Inspector', group: 'Inspectors' },
  { to: 'jev', label: 'Raw Jev', group: 'Inspectors' },
  { to: 'signals', label: 'Signal Registry', group: 'Inspectors' },
  { to: 'playground', label: 'Signal Playground', group: 'Inspectors' },
  { to: 'meshes', label: 'Mesh Inspector / Simulator', group: 'Inspectors' },
  { to: 'escalation', label: 'Escalation', group: 'Inspectors' },
  { to: 'director', label: 'Director', group: 'Inspectors' },
  { to: 'actions', label: 'Actions', group: 'Inspectors' },
  { to: 'timeline', label: 'Activity Timeline', group: 'Live' },
  { to: 'pipeline', label: 'Live Pipeline', group: 'Live' },
  { to: 'demo', label: 'Demo Mode', group: 'Live' },
  { to: 'models', label: 'Models', group: 'Observatories' },
  { to: 'cost', label: 'Cost', group: 'Observatories' },
  { to: 'quality', label: 'Decision Quality', group: 'Observatories' },
  { to: 'performance', label: 'Performance', group: 'Observatories' },
  { to: 'failures', label: 'Failures', group: 'Observatories' },
  { to: 'traces', label: 'Exportable Traces', group: 'Observatories' },
]

export default function DeveloperPage() {
  const settings = useSettings()
  if (settings.data && !settings.data.developerMode) {
    return (
      <Note className="m-6" title="Developer Mode is off">
        Turn it on in Settings. It shows more and grants no additional authority.
      </Note>
    )
  }
  const groups = [...new Set(NAV.map((n) => n.group))]
  return (
    <div className="flex min-h-0 flex-1">
      <nav className="w-52 shrink-0 overflow-auto border-r border-line bg-sand/40 py-2" aria-label="Developer Mode">
        {groups.map((g) => (
          <div key={g} className="mb-2">
            <div className="px-3 pb-0.5 text-2xs font-semibold uppercase tracking-wider text-clay">{g}</div>
            {NAV.filter((n) => n.group === g).map((n) => (
              <NavLink key={n.to} to={n.to ? `/developer/${n.to}` : '/developer'} end={n.to === ''} className={({ isActive }) => cx('block px-3 py-1 text-sm text-ink-2 hover:text-ink', isActive && 'bg-sand-2 font-medium text-ink')}>
                {n.label}
              </NavLink>
            ))}
          </div>
        ))}
      </nav>
      <div className="flex min-h-0 min-w-0 flex-1 flex-col overflow-auto">
        <Routes>
          <Route index element={<OpsPane />} />
          <Route path="messages" element={<MessageInspector />} />
          <Route path="messages/:digId/:traceId" element={<MessageInspector />} />
          <Route path="jev" element={<RawJevInspector />} />
          <Route path="signals" element={<SignalInspector />} />
          <Route path="playground" element={<Playground />} />
          <Route path="meshes" element={<MeshInspector />} />
          <Route path="escalation" element={<EscalationInspector />} />
          <Route path="director" element={<DirectorInspector />} />
          <Route path="actions" element={<ActionInspector />} />
          <Route path="timeline" element={<ActivityTimeline />} />
          <Route path="pipeline" element={<LivePipeline />} />
          <Route path="demo" element={<DemoMode />} />
          <Route path="models" element={<ModelsPane />} />
          <Route path="cost" element={<CostObservatory />} />
          <Route path="quality" element={<DecisionQuality />} />
          <Route path="performance" element={<PerformanceObservatory />} />
          <Route path="failures" element={<FailureInspector />} />
          <Route path="traces" element={<ExportTraces />} />
          <Route path="*" element={<Navigate to="" replace />} />
        </Routes>
      </div>
    </div>
  )
}

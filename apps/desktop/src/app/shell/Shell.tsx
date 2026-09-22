import { Outlet, useLocation, useNavigate } from 'react-router-dom'
import { useEffect } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { ErrorBoundary } from '@/components/ErrorBoundary'
import { HelpSheet } from '@/components/HelpSheet'
import { Toaster } from '@/components/Toaster'
import { Tour } from '@/components/Tour'
import { ConfirmHost } from '@/components/confirm/ConfirmHost'
import { useEvents } from '@/lib/events'
import { useShortcuts } from '@/lib/useShortcuts'
import { useUi, toast } from '@/lib/stores'
import { useAction, useLastUndoable } from '@/lib/useAction'
import { qk } from '@/lib/api'
import { Banners } from './Banners'
import { Rail } from './Rail'
import { TopBar } from './TopBar'
import { ComposerPanel, useComposerActions } from '@/features/messages/composer'

export function Shell() {
  useEvents()
  const nav = useNavigate()
  const loc = useLocation()
  const qc = useQueryClient()
  const setHelpOpen = useUi((s) => s.setHelpOpen)
  const setTaskCentreOpen = useUi((s) => s.setTaskCentreOpen)
  const { compose } = useComposerActions()
  const { run } = useAction()
  const lastUndoable = useLastUndoable((s) => s.lastActivityId)

  useShortcuts('global', {
    help: () => setHelpOpen(true),
    search: () => window.dispatchEvent(new Event('riddle:focus-search')),
    compose: () => compose(),
    task_centre: () => setTaskCentreOpen(true),
    escape: () => {
      setHelpOpen(false)
      setTaskCentreOpen(false)
    },
    go_summary: () => nav('/'),
    go_finds: () => nav('/finds'),
    go_unresolved: () => nav('/unresolved'),
    go_inbox: () => nav('/inbox'),
    go_spoil: () => nav('/spoil'),
    go_activity: () => nav('/activity'),
    go_meshes: () => nav('/meshes'),
    go_director: () => nav('/director'),
    go_settings: () => nav('/settings'),
    undo: () => {
      if (!lastUndoable) {
        toast('Nothing to undo', { body: 'The last action left nothing undoable, or was already undone.', kind: 'info' })
        return
      }
      void run('undo_activity', { activityId: lastUndoable }, { successToast: 'Undone' }).then(() => useLastUndoable.getState().set(null))
    },
  })

  // A wake, focus or visibility change triggers a refresh pass (§5).
  useEffect(() => {
    const refresh = () => void qc.invalidateQueries({ queryKey: qk.taskCentre })
    window.addEventListener('focus', refresh)
    document.addEventListener('visibilitychange', refresh)
    return () => {
      window.removeEventListener('focus', refresh)
      document.removeEventListener('visibilitychange', refresh)
    }
  }, [qc])

  return (
    <div className="flex h-full flex-col">
      <TopBar />
      <Banners />
      <div className="flex min-h-0 flex-1">
        <Rail />
        <main className="flex min-h-0 min-w-0 flex-1 flex-col">
          <ErrorBoundary key={loc.pathname.split('/')[1] || 'summary'} name={loc.pathname}>
            <Outlet />
          </ErrorBoundary>
        </main>
      </div>
      <ComposerPanel />
      <ConfirmHost />
      <HelpSheet />
      <Toaster />
      <Tour />
    </div>
  )
}

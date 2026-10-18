import { Outlet, useLocation, useNavigate } from 'react-router-dom'
import { useEffect, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { CommandPalette } from '@/components/CommandPalette'
import { ErrorBoundary } from '@/components/ErrorBoundary'
import { HelpSheet } from '@/components/HelpSheet'
import { Toaster } from '@/components/Toaster'
import { Tour } from '@/components/Tour'
import { ConfirmHost } from '@/components/confirm/ConfirmHost'
import { useEvents } from '@/lib/events'
import { GO_ROUTES, nextLevel, shortcutShown, useLevel, useSetLevel } from '@/lib/level'
import { useShortcuts } from '@/lib/useShortcuts'
import { useUi, toast } from '@/lib/stores'
import { useAction, useLastUndoable } from '@/lib/useAction'
import { qk } from '@/lib/api'
import { Banners } from './Banners'
import { LevelStrip } from './LevelStrip'
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
  const level = useLevel()
  const setLevel = useSetLevel()
  // The palette's open state lives here, not in a store: nothing else needs
  // it, and ⌘K must toggle it from inside the palette's own input.
  const [paletteOpen, setPaletteOpen] = useState(false)

  // A go-to shortcut is bound only when the level lists its page, so the
  // palette (which lists what is bound) and the help sheet agree with the
  // rail; the page itself stays reachable by link.
  const go = (id: keyof typeof GO_ROUTES) => (shortcutShown(id, level) ? () => nav(GO_ROUTES[id]) : undefined)
  useShortcuts('global', {
    help: () => setHelpOpen(true),
    search: () => window.dispatchEvent(new Event('riddle:focus-search')),
    palette: () => setPaletteOpen((o) => !o),
    compose: () => compose(),
    task_centre: () => setTaskCentreOpen(true),
    escape: () => {
      setHelpOpen(false)
      setTaskCentreOpen(false)
    },
    go_summary: go('go_summary'),
    go_finds: go('go_finds'),
    go_unresolved: go('go_unresolved'),
    go_inbox: go('go_inbox'),
    go_spoil: go('go_spoil'),
    go_activity: go('go_activity'),
    go_meshes: go('go_meshes'),
    go_director: go('go_director'),
    go_settings: go('go_settings'),
    settings: () => nav('/settings'),
    toggle_level: () => void setLevel(nextLevel(level)),
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
          <LevelStrip />
          <ErrorBoundary key={loc.pathname.split('/')[1] || 'summary'} name={loc.pathname}>
            <Outlet />
          </ErrorBoundary>
        </main>
      </div>
      <ComposerPanel />
      <ConfirmHost />
      <HelpSheet />
      <CommandPalette open={paletteOpen} onOpenChange={setPaletteOpen} />
      <Toaster />
      <Tour />
    </div>
  )
}

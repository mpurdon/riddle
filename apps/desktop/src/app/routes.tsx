import { lazy, Suspense, type ReactNode } from 'react'
import { createHashRouter, Navigate } from 'react-router-dom'
import { Shell } from './shell/Shell'

const SummaryPage = lazy(() => import('@/features/summary/SummaryPage'))
const FindsPage = lazy(() => import('@/features/finds/FindsPage'))
const UnresolvedPage = lazy(() => import('@/features/finds/UnresolvedPage'))
const PriorityPage = lazy(() => import('@/features/priority/PriorityPage'))
const InboxPage = lazy(() => import('@/features/messages/InboxPage'))
const MessagePage = lazy(() => import('@/features/messages/MessagePage'))
const SpoilPage = lazy(() => import('@/features/messages/SpoilPage'))
const ContaminationPage = lazy(() => import('@/features/messages/ContaminationPage'))
const StrataPage = lazy(() => import('@/features/strata/StrataPage'))
const MeshesPage = lazy(() => import('@/features/meshes/MeshesPage'))
const PacksPage = lazy(() => import('@/features/packs/PacksPage'))
const RuleCheckPage = lazy(() => import('@/features/packs/RuleCheckPage'))
const SignalsPage = lazy(() => import('@/features/signals/SignalsPage'))
const ActivityPage = lazy(() => import('@/features/activity/ActivityPage'))
const DirectorPage = lazy(() => import('@/features/director/DirectorPage'))
const ExpeditionsPage = lazy(() => import('@/features/expeditions/ExpeditionsPage'))
const SettingsPage = lazy(() => import('@/features/settings/SettingsPage'))
const DeveloperPage = lazy(() => import('@/features/developer/DeveloperPage'))

const S = ({ children }: { children: ReactNode }) => <Suspense fallback={<div className="p-6 text-sm text-clay">Loading…</div>}>{children}</Suspense>

export const router = createHashRouter([
  {
    path: '/',
    element: <Shell />,
    children: [
      { index: true, element: <S><SummaryPage /></S> },
      { path: 'finds', element: <S><FindsPage /></S> },
      { path: 'finds/:findId', element: <S><FindsPage /></S> },
      { path: 'unresolved', element: <S><UnresolvedPage /></S> },
      { path: 'priority', element: <S><PriorityPage /></S> },
      { path: 'inbox', element: <S><InboxPage /></S> },
      { path: 'inbox/:digId/:messageId', element: <S><MessagePage /></S> },
      { path: 'spoil', element: <S><SpoilPage /></S> },
      { path: 'contamination', element: <S><ContaminationPage /></S> },
      { path: 'strata', element: <S><StrataPage /></S> },
      { path: 'meshes', element: <S><MeshesPage /></S> },
      { path: 'meshes/:meshId', element: <S><MeshesPage /></S> },
      { path: 'packs', element: <S><PacksPage /></S> },
      { path: 'check', element: <S><RuleCheckPage /></S> },
      { path: 'signals', element: <S><SignalsPage /></S> },
      { path: 'signals/:name', element: <S><SignalsPage /></S> },
      { path: 'activity', element: <S><ActivityPage /></S> },
      { path: 'director', element: <S><DirectorPage /></S> },
      { path: 'director/:conversationId', element: <S><DirectorPage /></S> },
      { path: 'expeditions', element: <S><ExpeditionsPage /></S> },
      { path: 'expeditions/:expeditionId', element: <S><ExpeditionsPage /></S> },
      { path: 'settings', element: <S><SettingsPage /></S> },
      { path: 'developer/*', element: <S><DeveloperPage /></S> },
      { path: '*', element: <Navigate to="/" replace /> },
    ],
  },
])

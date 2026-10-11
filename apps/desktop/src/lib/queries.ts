/**
 * Shared TanStack Query hooks for the reads most pages need.
 */
import { labelName } from '@/features/messages/labels'
import { useQuery } from '@tanstack/react-query'
import type { Dig } from '@/bindings/Dig'
import * as api from './api'
import { qk } from './api'
import { useUi } from './stores'

/** The Summary for the selected Dig (or all), so it and the rail badges agree with the pages. */
export function useSummary() {
  const digId = useUi((s) => s.digId)
  return useQuery({ queryKey: [...qk.summary, digId], queryFn: () => api.summary(digId), refetchInterval: 30_000 })
}
/** Pending Director proposals, fetched on launch so they survive a restart. */
export const usePending = () => useQuery({ queryKey: qk.pending, queryFn: api.listPending, staleTime: 15_000 })
export const useTaskCentre = () => useQuery({ queryKey: qk.taskCentre, queryFn: api.taskCentre, refetchInterval: 5_000 })
/** The accounts Riddle works on: disabled ones are left out of every view, as the backend leaves them out of every read. */
export const useDigs = () => useQuery({ queryKey: qk.digs, queryFn: api.listDigs, staleTime: 30_000, select: enabledOnly })
/** Every account, disabled ones included, for managing accounts. */
export const useAllDigs = () => useQuery({ queryKey: qk.digs, queryFn: api.listDigs, staleTime: 30_000 })
const enabledOnly = (digs: Dig[]) => digs.filter((d) => !d.disabled)
export const useDigGroups = () => useQuery({ queryKey: qk.digGroups, queryFn: api.listDigGroups, staleTime: 30_000 })
export const useSettings = () => useQuery({ queryKey: qk.settings, queryFn: api.getSettings, staleTime: 30_000 })
export const useProfile = () => useQuery({ queryKey: qk.profile, queryFn: api.getUserProfile, staleTime: 30_000 })
export const useDevInfo = () => useQuery({ queryKey: qk.devInfo, queryFn: api.devInfo, staleTime: 60_000 })
export const useActionsMeta = () => useQuery({ queryKey: qk.actions, queryFn: api.listActions, staleTime: 5 * 60_000 })
export const useStrata = () => useQuery({ queryKey: qk.strata, queryFn: api.listStrata, staleTime: 30_000 })
export const useMeshes = (includeDeleted = false) => useQuery({ queryKey: [...qk.meshes, includeDeleted], queryFn: () => api.listMeshes(includeDeleted) })
export const useSignals = () => useQuery({ queryKey: qk.signals, queryFn: api.listSignals })

/** The selected Dig (null for All), and a helper to name Dig ids. */
export function useDigContext() {
  const digId = useUi((s) => s.digId)
  const digs = useDigs()
  const name = (id: string | null | undefined) => (id ? digs.data?.find((d) => d.id === id)?.name ?? id : 'All accounts')
  return { digId, digs: digs.data ?? [], digName: name }
}

/** The labels of the selected account (or all), refreshed by every sync. */
export function useLabels() {
  const digId = useUi((s) => s.digId)
  return useQuery({ queryKey: [...qk.labels, digId], queryFn: () => api.listLabels(digId), staleTime: 60_000 })
}

/** A label id as its name: the account's own label list first, then Gmail's built-in names. */
export function useLabelName(): (id: string, digId?: string) => string {
  const labels = useLabels().data
  return (id, digId) => {
    const own = labels?.find((l) => l.id === id && (!digId || l.digId === digId))
    return own && own.name !== id ? labelName(own.name) : labelName(id)
  }
}

/** The trust card's evidence for the selected account (or all). Every Action that moves it invalidates the key; the interval is a backstop. */
export function useTrustStats(sinceDays = 7) {
  const digId = useUi((s) => s.digId)
  return useQuery({ queryKey: [...qk.trustStats, digId, sinceDays], queryFn: () => api.trustStats({ digId, sinceDays }), refetchInterval: 5 * 60_000 })
}

/** Whether this is a dev build and whether it is armed for provider mutations. One hook for the five places that asked. */
export function useDevArmed() {
  const dev = useDevInfo().data?.dev ?? false
  const armed = useQuery({ queryKey: qk.devArmed, queryFn: api.devArmed, enabled: dev }).data ?? false
  // Hold, Ask or Allow; `armed` is Allow, the one setting under which writes go unattended.
  const writes = useQuery({ queryKey: qk.mailboxWrites, queryFn: api.mailboxWrites, enabled: dev }).data?.writes ?? 'hold'
  return { dev, armed, writes }
}

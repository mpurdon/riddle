/**
 * Shared TanStack Query hooks for the reads most pages need.
 */
import { useQuery } from '@tanstack/react-query'
import * as api from './api'
import { qk } from './api'
import { useUi } from './stores'

export const useSummary = () => useQuery({ queryKey: qk.summary, queryFn: api.summary, refetchInterval: 30_000 })
export const useTaskCentre = () => useQuery({ queryKey: qk.taskCentre, queryFn: api.taskCentre, refetchInterval: 5_000 })
export const useDigs = () => useQuery({ queryKey: qk.digs, queryFn: api.listDigs, staleTime: 30_000 })
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
  const name = (id: string | null | undefined) => (id ? digs.data?.find((d) => d.id === id)?.name ?? id : 'All Digs')
  return { digId, digs: digs.data ?? [], digName: name }
}

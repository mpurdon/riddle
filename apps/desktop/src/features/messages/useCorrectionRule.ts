/**
 * Adding one sender or domain rule to the "Your corrections" Mesh. The Mesh
 * is the only place a rule lives, so a click on "Not Spoil for this sender"
 * becomes one more guard line the user can read and remove in Settings.
 * The Mesh list is fetched fresh rather than read from the render, because
 * two quick corrections against a cached "no such Mesh" once created two
 * Meshes of the same name.
 */
import { useCallback } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { listMeshes, qk } from '@/lib/api'
import { useAction } from '@/lib/useAction'
import { withCorrection, YOUR_CORRECTIONS_MESH } from '@/features/rules/yourCorrections'

export function useCorrectionRule() {
  const qc = useQueryClient()
  const { run } = useAction()
  /**
   * Adds the guard line, creating the Mesh (Active) when there is none.
   * Returns the activity id for Undo, or null when the rule was already there.
   * No toast of its own: the caller says what happened in one message.
   */
  const addRule = useCallback(
    async (line: string): Promise<string | null> => {
      const meshes = await qc.fetchQuery({ queryKey: [...qk.meshes, false], queryFn: () => listMeshes(false), staleTime: 0 })
      const mesh = meshes.find((m) => m.name === YOUR_CORRECTIONS_MESH)
      if (mesh) {
        const text = withCorrection(mesh.sourceText, line)
        if (text === mesh.sourceText) return null
        const out = await run('update_mesh', { meshId: mesh.id, text, changeNote: 'Added a correction' }, { successToast: false })
        return out?.activityId ?? null
      }
      const out = await run('create_mesh', { text: withCorrection(null, line), status: 'active' }, { successToast: false })
      return out?.activityId ?? null
    },
    [qc, run],
  )
  return { addRule }
}

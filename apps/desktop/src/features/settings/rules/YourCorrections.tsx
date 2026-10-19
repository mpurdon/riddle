/**
 * Settings → Mail rules → Your corrections: the sender and domain rules made
 * with one click on the Spoil page ("Not Spoil for this sender", Flow 6),
 * listed beside the other mail rules, each with Remove. The rules are guard
 * lines in one Mesh (`features/rules/yourCorrections.ts`); this list exists
 * because nobody in the study looked for a correction they had made on the
 * Meshes page, and nobody there could tell a guard line from a route.
 * Remove rewrites the Mesh without the line, or moves the Mesh to Recently
 * Deleted when that was the last one; either comes back from the toast's
 * Undo, so nothing here asks first.
 */
import { useState } from 'react'
import { Link } from 'react-router-dom'
import { toRiddleError } from '@/lib/ipc'
import { useMeshes } from '@/lib/queries'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { ErrorCard } from '@/components/ErrorCard'
import { parseCorrections, withoutCorrection, YOUR_CORRECTIONS_MESH, type CorrectionRule } from '@/features/rules/yourCorrections'

export const MESH_LINK = `/meshes?name=${encodeURIComponent(YOUR_CORRECTIONS_MESH)}`

export function correctionLabel(r: CorrectionRule): { never: string; who: string } {
  return { never: r.never === 'spoil' ? 'Never Spoil' : 'Never Contamination', who: r.scope === 'domain' ? `@${r.value}` : r.value }
}

export function YourCorrections() {
  const q = useMeshes()
  const { run } = useAction()
  const [busy, setBusy] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} compact />
  if (!q.data) return <p className="text-xs text-clay">Reading your corrections…</p>
  const mesh = q.data.find((m) => m.name === YOUR_CORRECTIONS_MESH && !m.deletedAt)
  const rules = parseCorrections(mesh?.sourceText)
  const remove = async (r: CorrectionRule) => {
    if (!mesh) return
    setBusy(r.line)
    setError(null)
    try {
      const text = withoutCorrection(mesh.sourceText, r.line)
      if (text === null) await run('delete_mesh', { meshId: mesh.id }, { successToast: 'Correction removed', toastErrors: false })
      else await run('update_mesh', { meshId: mesh.id, text, changeNote: 'Removed a correction', simulate: false }, { successToast: 'Correction removed', toastErrors: false })
    } catch (e) {
      setError(toRiddleError(e).message)
    } finally {
      setBusy(null)
    }
  }
  return (
    <div className="max-w-2xl">
      {rules.length ? (
        <ul className="space-y-1.5" aria-label="Your corrections">
          {rules.map((r) => {
            const { never, who } = correctionLabel(r)
            return (
              <li key={r.line} className="flex items-center gap-2 rounded-md border border-line px-2.5 py-1.5 text-sm">
                <span className="font-medium">{never}</span>
                <span aria-hidden className="text-clay">
                  ·
                </span>
                <span className="mono selectable">{who}</span>
                <span className="flex-1" />
                <Button size="sm" variant="ghost" busy={busy === r.line} title={`Remove ${never} for ${who}`} onClick={() => remove(r)}>
                  Remove
                </Button>
              </li>
            )
          })}
        </ul>
      ) : (
        <p className="text-sm text-ink-2" role="status">
          No corrections yet. <span className="font-medium text-ink">Not Spoil for this sender</span> on the Spoil page adds one.
        </p>
      )}
      {error ? (
        <p role="alert" className="mt-2 text-xs text-rust">
          {error}
        </p>
      ) : null}
      <p className="mt-2 text-xs text-clay">
        {mesh ? (
          <>
            These are lines in the Mesh{' '}
            <Link to={MESH_LINK} className="underline hover:text-ink">
              {YOUR_CORRECTIONS_MESH}
            </Link>
            ; you can edit them there like any rule.
          </>
        ) : (
          <>Corrections are kept as lines in a Mesh named “{YOUR_CORRECTIONS_MESH}”, which appears on the Meshes page once you add one.</>
        )}
      </p>
    </div>
  )
}

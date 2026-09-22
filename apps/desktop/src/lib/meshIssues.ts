/**
 * Maps a `parse_mesh` failure (`invalid` with line info in the detail) to
 * editor markers. Tolerant of the shapes the backend may use: a single
 * {line, message}, an `issues`/`errors` array, or a bare array.
 */
import type { RiddleError } from '@/bindings/RiddleError'
import { isRiddleError } from './ipc'
import type { MeshParseIssue } from './types'

export function issuesFrom(e: unknown): MeshParseIssue[] {
  if (!isRiddleError(e)) return [{ line: 1, message: e instanceof Error ? e.message : String(e) }]
  const err = e as RiddleError
  const d = err.detail as Record<string, unknown> | unknown[] | null
  const pick = (v: unknown): MeshParseIssue | null => {
    if (!v || typeof v !== 'object') return null
    const o = v as Record<string, unknown>
    const line = typeof o.line === 'number' ? o.line : typeof o.line === 'string' ? Number(o.line) : NaN
    if (!Number.isFinite(line)) return null
    return { line: Math.max(1, line), column: typeof o.column === 'number' ? o.column : undefined, message: typeof o.message === 'string' ? o.message : err.message }
  }
  const out: MeshParseIssue[] = []
  if (Array.isArray(d)) for (const v of d) { const p = pick(v); if (p) out.push(p) }
  else if (d && typeof d === 'object') {
    const arr = (d.issues ?? d.errors) as unknown
    if (Array.isArray(arr)) for (const v of arr) { const p = pick(v); if (p) out.push(p) }
    if (!out.length) {
      const p = pick(d)
      if (p) out.push(p)
    }
  }
  if (!out.length) {
    const m = /line (\d+)/i.exec(err.message)
    out.push({ line: m ? Number(m[1]) : 1, message: err.message })
  }
  return out.sort((a, b) => a.line - b.line)
}

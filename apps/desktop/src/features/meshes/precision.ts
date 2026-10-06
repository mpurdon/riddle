/**
 * One Mesh's precision, from the trust card's per-rule rows
 * (`TrustStats.rules`: one row per Mesh version that decided in the window).
 *
 * The Meshes page showed a Mesh's Shadow agreement and nothing about what it
 * did once Active: a Mesh the user kept reversing looked the same as one that
 * never missed, and the backend's step back to Shadow arrived as a bare
 * status change. The page then guessed the step back from the status and the
 * Shadow start date, and guessed wrong for a Mesh the user had put in Shadow
 * themselves; the backend now records the demotion on the Mesh
 * (`Mesh.demoted`) and the page reads it. The rows are summed across
 * versions because the page is about the Mesh; a version applied yesterday
 * would otherwise read "Decided 0". "Reversed often" on a row is the
 * engine's own verdict (`RulePrecision.reversedOften`); on the sum it is
 * the same rule applied here. The Shadow counters come from the row when
 * there is one (so this page and the trust card agree) and from the Mesh
 * itself otherwise, which is where the rows copy them from.
 */
import type { Demotion } from '@/bindings/Demotion'
import type { Mesh } from '@/bindings/Mesh'
import type { RulePrecision } from '@/bindings/RulePrecision'
import { num } from '@/lib/format'

/** The engine steps a Mesh back once it has this many decisions and a quarter or more of them are reversed. */
export const REVERSED_OFTEN_FLOOR = 20

export interface MeshPrecision {
  decisions: number
  reversed: number
  /** reversed / decisions; null with no decisions. */
  reversedShare: number | null
  /** The engine's demotion rule applied to the sum across versions. */
  reversedOften: boolean
  shadowProposed: number
  shadowAgreed: number
  /** shadowAgreed / shadowProposed; null with no proposals. */
  agreement: number | null
}

export function precisionFor(rules: RulePrecision[], mesh: Pick<Mesh, 'id' | 'shadowProposed' | 'shadowAgreed'>): MeshPrecision {
  const rows = rules.filter((r) => r.meshId === mesh.id)
  const decisions = rows.reduce((n, r) => n + num(r.decisions), 0)
  const reversed = rows.reduce((n, r) => n + num(r.reversed), 0)
  const latest = rows.reduce<RulePrecision | null>((best, r) => (!best || r.version > best.version ? r : best), null)
  const shadowProposed = latest?.shadowProposed ?? mesh.shadowProposed
  const shadowAgreed = latest?.shadowAgreed ?? mesh.shadowAgreed
  return {
    decisions,
    reversed,
    reversedShare: decisions ? reversed / decisions : null,
    reversedOften: reversedOften(decisions, reversed),
    shadowProposed,
    shadowAgreed,
    agreement: shadowProposed ? shadowAgreed / shadowProposed : null,
  }
}

/**
 * The engine's demotion rule, `decisions >= 20 && reversed * 4 >= decisions`,
 * for the page's sum across versions. A single row carries the engine's own
 * verdict in `RulePrecision.reversedOften`; read that instead of calling this.
 */
export function reversedOften(decisions: number, reversed: number): boolean {
  return decisions >= REVERSED_OFTEN_FLOOR && reversed * 4 >= decisions
}

/**
 * The step back to Shadow the backend recorded on this Mesh, while it is
 * still in Shadow. The backend clears `demoted` when the user sets a status
 * themselves; the status check covers a Mesh it has since promoted itself.
 */
export function steppedBack(mesh: Pick<Mesh, 'status' | 'demoted'>): Demotion | null {
  return mesh.status === 'shadow' ? (mesh.demoted ?? null) : null
}

/** "Sep 23": the day only, for "stepped back to Shadow on". */
export function dayLabel(iso: string): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' })
}

/**
 * The words for a Mesh's own autonomy cap (Flow 7, per-rule autonomy). An
 * `Autonomy: Observe | Suggest | Act` header line is the most that Mesh's
 * decisions may do in the mailbox whatever the account allows; the
 * projection gate holds anything capped below Act inside Riddle. The cap
 * reached the backend before the editor knew about it, so a user who typed
 * the line saw nothing change on the page and doubted it had worked; every
 * surface (list chip, Valid line, Structure tab) now says so in the same
 * words. The level names and the "below Act" test are the account
 * settings' own (settings/accounts/model.ts), so the two pages cannot
 * drift. The cap itself comes from the parser (`Mesh.autonomy`); this file
 * no longer reads the header for itself.
 */
import type { Autonomy } from '@/bindings/Autonomy'
import type { Mesh } from '@/bindings/Mesh'
import { autonomyLabel, belowAct } from '@/features/settings/accounts/model'

/** The list-row chip's tooltip. */
export function capChipTip(cap: 'observe' | 'suggest'): string {
  return `Capped at ${autonomyLabel(cap)}: this Mesh's decisions never reach the mailbox, whatever the account allows`
}

/** What the Valid line ends with when the text carries a cap; empty otherwise. */
export function capValidTail(cap: Autonomy | null | undefined): string {
  if (!cap) return ''
  return belowAct(cap) ? ` · capped at ${autonomyLabel(cap)} (decisions stay in Riddle)` : ` · capped at ${autonomyLabel(cap)}`
}

/** The Structure tab's sentence. */
export function autonomySentence(m: Pick<Mesh, 'autonomy'>): string {
  const cap = m.autonomy
  if (!cap) return 'Autonomy: follows the account.'
  if (belowAct(cap)) return `Autonomy: capped at ${autonomyLabel(cap)}. Its decisions stay in Riddle whatever the account allows. Remove the Autonomy line to follow the account.`
  return `Autonomy: capped at ${autonomyLabel(cap)}. Its decisions reach the mailbox when the account allows, never as Trusted. Remove the Autonomy line to follow the account.`
}

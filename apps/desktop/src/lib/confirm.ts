/**
 * Confirmation-tier routing (ARCHITECTURE.md §51). A PendingAction names
 * its tier; this maps the tier to the control that must be rendered. The
 * in-place tier needs no dialog: the action proceeds and an Undo toast is
 * the control. Everything else waits on a real interaction.
 */
import type { PendingAction } from '@/bindings/PendingAction'
import type { ConfirmationPolicy } from '@/bindings/ConfirmationPolicy'
import type { RiddleError } from '@/bindings/RiddleError'
import { isRiddleError } from './ipc'

export type TierComponent = 'auto' | 'ConfirmInPlace' | 'TwoStepDialog' | 'TypeCountDialog' | 'HoldButton' | 'ApprovalDialog'

export function tierFor(policy: ConfirmationPolicy): TierComponent {
  switch (policy) {
    case 'none':
      return 'auto'
    case 'in_place_undo':
      return 'ConfirmInPlace'
    case 'two_step':
      return 'TwoStepDialog'
    case 'type_count':
      return 'TypeCountDialog'
    case 'press_hold':
      return 'HoldButton'
    case 'approval':
      return 'ApprovalDialog'
  }
}

/** Tiers that complete without waiting for a dialog. */
export function confirmsImmediately(policy: ConfirmationPolicy): boolean {
  return policy === 'none' || policy === 'in_place_undo'
}

export function pendingFrom(e: unknown): PendingAction | null {
  if (!isRiddleError(e)) return null
  if (e.kind !== 'needs_confirmation') return null
  const d = (e as RiddleError).detail as Partial<PendingAction> | null
  if (!d || typeof d !== 'object' || typeof d.id !== 'string' || typeof d.action !== 'string') return null
  return d as PendingAction
}

/** The count a TypeCount dialog demands: from realArguments, else from args. */
export function expectedCount(p: PendingAction): number | null {
  const real = p.realArguments as Record<string, unknown> | null
  const args = p.args as Record<string, unknown> | null
  for (const src of [real, args]) {
    if (!src || typeof src !== 'object') continue
    if (typeof src.count === 'number') return src.count
    const ids = src.messageIds
    if (Array.isArray(ids)) return ids.length
  }
  return null
}

export interface Recipient {
  email: string
  name: string | null
}

/** Real recipients for a TwoStep send, read from the Action layer's preview. */
export function realRecipients(p: PendingAction): { to: Recipient[]; cc: Recipient[]; bcc: Recipient[]; subject: string | null } | null {
  const real = p.realArguments as Record<string, unknown> | null
  if (!real || typeof real !== 'object') return null
  const list = (v: unknown): Recipient[] =>
    Array.isArray(v)
      ? v.map((x) => (typeof x === 'string' ? { email: x, name: null } : { email: String((x as Recipient).email ?? ''), name: ((x as Recipient).name as string | null) ?? null }))
      : []
  if (!('to' in real)) return null
  return { to: list(real.to), cc: list(real.cc), bcc: list(real.bcc), subject: typeof real.subject === 'string' ? real.subject : null }
}

/**
 * `useAction`: the one way the UI runs a mutation. It invokes through the
 * Action layer, catches `needs_confirmation`, routes the PendingAction to
 * the right confirmation tier (§51), confirms, invalidates queries, and
 * offers Undo in a toast when the Action was reversible.
 */
import { useCallback } from 'react'
import { useQueryClient, type QueryKey } from '@tanstack/react-query'
import { confirm, invoke, listActions, qk } from './api'
import { confirmsImmediately, pendingFrom } from './confirm'
import { fmtBy } from './format'
import { gmailEffectLine, isMailboxAction } from './gmailEffect'
import { isRiddleError, log, toRiddleError } from './ipc'
import { dispositionLabel, errorAffordance, importanceLabel } from './selectors'
import { toast, useConfirmQueue } from './stores'
import type { DevInfo } from './types'
import type { ActionOutcome } from '@/bindings/ActionOutcome'
import type { EntityKind } from '@/bindings/EntityKind'
import type { EntityRef } from '@/bindings/EntityRef'
import type { Dig } from '@/bindings/Dig'
import type { Disposition } from '@/bindings/Disposition'
import type { Importance } from '@/bindings/Importance'
import type { PendingAction } from '@/bindings/PendingAction'
import { create } from 'zustand'

interface UndoState {
  lastActivityId: string | null
  set: (id: string | null) => void
}
export const useLastUndoable = create<UndoState>((set) => ({ lastActivityId: null, set: (lastActivityId) => set({ lastActivityId }) }))

export interface RunOptions {
  digId?: string | null
  idempotencyKey?: string | null
  /** The user already held or otherwise confirmed inline; the Action layer re-validates. */
  confirmed?: boolean
  /** Toast title on success; false for none. Defaults to the Activity summary when available. */
  successToast?: string | false
  /** Extra query keys to invalidate. */
  invalidate?: QueryKey[]
  /** Offer Undo in the toast (default: when the Action is reversible). */
  offerUndo?: boolean
  /**
   * Confirm a pending Action where it was asked for, instead of the modal
   * queue: resolve true to proceed. The Action layer still validates the
   * pending id, so this changes where the question is asked, not whether.
   */
  confirmWith?: (pending: PendingAction) => Promise<boolean>
  /** The caller renders the failure inline; skip the error toast so it is not said twice. */
  toastErrors?: boolean
  /**
   * Skip the query invalidation after success. A bulk loop runs many Actions
   * and invalidates once at the end with `invalidateAfter`, instead of
   * refetching every list after every item.
   */
  skipInvalidate?: boolean
}

const DEFAULT_INVALIDATIONS: Record<string, QueryKey[]> = {
  move_to_spoil: [qk.messages, qk.summary, qk.finds],
  archive_messages: [qk.messages],
  mark_contamination: [qk.messages, qk.summary],
  restore_messages: [qk.messages, qk.summary],
  set_disposition: [qk.messages, qk.summary, qk.finds, qk.trustStats],
  set_importance: [qk.messages, qk.finds, qk.priorityInbox],
  label_messages: [qk.messages, qk.strata],
  snooze_messages: [qk.messages],
  permanent_delete: [qk.messages, qk.summary],
  resolve_find: [qk.finds, qk.summary],
  dismiss_find: [qk.finds, qk.summary],
  snooze_find: [qk.finds, qk.summary],
  retype_find: [qk.finds],
  undo_activity: [qk.activity, qk.messages, qk.finds, qk.summary, qk.meshes, qk.strata, qk.profile, qk.settings, qk.signals, qk.trustStats],
  create_mesh: [qk.meshes],
  update_mesh: [qk.meshes],
  set_mesh_status: [qk.meshes],
  set_mesh_fallback: [qk.meshes],
  remember_for_sender: [qk.meshes],
  install_pack: [qk.meshes],
  remove_pack: [qk.meshes],
  import_pack: [qk.meshes],
  create_pack: [qk.meshes],
  save_list: [qk.meshes],
  delete_list: [qk.meshes],
  grade_decision: [qk.meshes],
  delete_mesh: [qk.meshes],
  restore_mesh: [qk.meshes],
  propose_signal: [qk.signals],
  promote_signal: [qk.signals],
  rename_stratum: [qk.strata, qk.messages],
  merge_strata: [qk.strata, qk.messages],
  create_stratum: [qk.strata],
  send_message: [qk.messages, qk.finds, qk.summary],
  start_expedition: [qk.expeditions, qk.taskCentre],
  cancel_expedition: [qk.expeditions, qk.taskCentre],
  add_never_hidden: [qk.profile],
  remove_never_hidden: [qk.profile],
  save_user_profile: [qk.profile],
  // Dig.mailbox depends on autonomy and the armed state, so both refresh the Digs.
  save_settings: [qk.settings, qk.trustStats, qk.digs],
  set_secret: [qk.settings],
  clear_secret: [qk.settings],
  import_gmail_client: [qk.settings],
  add_fixture_dig: [qk.digs, qk.taskCentre, qk.summary],
  remove_dig: [qk.digs, qk.summary, qk.messages],
  update_dig: [qk.digs],
  set_dig_enabled: [qk.digs, qk.summary, qk.finds, qk.messages, qk.trustStats],
  save_dig_group: [qk.digGroups, qk.digs],
  rename_dig_group: [qk.digGroups, qk.digs, qk.meshes, qk.strata],
  delete_dig_group: [qk.digGroups, qk.digs],
  check_model_access: [qk.comparison],
  run_model_benchmark: [qk.comparison],
  run_director_comparison: [qk.comparison],
  pick_director_answer: [qk.comparison],
  pin_place: [qk.places],
  unpin_place: [qk.places],
  set_mailbox_writes: [qk.devArmed, qk.digs, qk.mailboxWrites],
  send_held_changes: [qk.mailboxWrites, qk.digs],
  discard_held_changes: [qk.mailboxWrites],
  resift: [qk.digs, qk.finds, qk.summary],
  decide_again: [qk.finds, qk.summary],
  apply_mesh_to_history: [qk.finds, qk.summary, qk.meshes],
  refresh_now: [qk.taskCentre],
  cancel_job: [qk.taskCentre],
  pause_job: [qk.taskCentre],
  resume_job: [qk.taskCentre],
  inject_fixture_message: [qk.messages],
  dismiss_pending: [qk.pending],
  review_shadow: [qk.messages, qk.summary, qk.meshes, qk.trustStats],
  connect_gmail: [qk.digs, qk.summary, qk.taskCentre],
}

export function useAction() {
  const qc = useQueryClient()

  /**
   * Refetch what the Actions touched, each query once: the defaults for the
   * Action, the Activity and pending lists, and `extra` (the entity kinds
   * the backend said it affected, a caller's own keys). Every success comes
   * through here; a bulk loop calls it once at the end instead.
   */
  const invalidateAfter = useCallback(
    async (actions: string[], extra: QueryKey[] = []) => {
      const keys = new Map<string, QueryKey>()
      for (const k of [...actions.flatMap((a) => DEFAULT_INVALIDATIONS[a] ?? []), qk.activity, qk.pending, ...extra]) keys.set(JSON.stringify(k), k)
      await Promise.all([...keys.values()].map((k) => qc.invalidateQueries({ queryKey: k })))
    },
    [qc],
  )

  const run = useCallback(
    async <T = unknown>(action: string, args: Record<string, unknown> = {}, opts: RunOptions = {}): Promise<(ActionOutcome & { result: T }) | null> => {
      let outcome: (ActionOutcome & { result: T }) | null = null
      try {
        outcome = await invoke<T>(action, args, { digId: opts.digId, idempotencyKey: opts.idempotencyKey, confirmed: opts.confirmed })
      } catch (e) {
        const pending = pendingFrom(e)
        if (!pending) {
          if (opts.toastErrors !== false) showError(e)
          throw toRiddleError(e)
        }
        outcome = await settle<T>(pending, opts)
        if (!outcome) return null
      }
      await afterSuccess(action, outcome, opts, args)
      return outcome
    },
    [qc],
  )

  const settle = useCallback(
    async <T,>(pending: PendingAction, opts: RunOptions): Promise<(ActionOutcome & { result: T }) | null> => {
      if (!confirmsImmediately(pending.confirmation)) {
        const ok = await (opts.confirmWith ?? useConfirmQueue.getState().ask)(pending)
        if (!ok) return null
      }
      try {
        return await confirm<T>(pending, { digId: opts.digId, idempotencyKey: opts.idempotencyKey })
      } catch (e) {
        if (opts.toastErrors !== false) showError(e)
        throw toRiddleError(e)
      }
    },
    [],
  )

  /** Confirm a PendingAction that arrived by other means (a Director proposal). */
  const apply = useCallback(
    async <T = unknown>(pending: PendingAction, opts: RunOptions = {}): Promise<(ActionOutcome & { result: T }) | null> => {
      const outcome = await settle<T>(pending, opts)
      if (!outcome) return null
      await afterSuccess(pending.action, outcome, opts, (pending.args ?? {}) as Record<string, unknown>)
      return outcome
    },
    [settle],
  )

  async function afterSuccess(action: string, outcome: ActionOutcome, opts: RunOptions, args: Record<string, unknown> = {}) {
    if (!opts.skipInvalidate) await invalidateAfter([action], [...keysForEntities(outcome.affected ?? []), ...(opts.invalidate ?? [])])
    if (opts.successToast === false) return
    let reversible = false
    if (outcome.activityId && action !== 'undo_activity' && opts.offerUndo !== false) {
      try {
        const metas = await qc.fetchQuery({ queryKey: qk.actions, queryFn: listActions, staleTime: 5 * 60_000 })
        reversible = metas.find((m) => m.name === action)?.reversible ?? false
      } catch {
        reversible = false
      }
    }
    const activityId = outcome.activityId
    if (reversible && activityId) useLastUndoable.getState().set(activityId)
    // The client's short titles where it has them; otherwise the Activity sentence the backend wrote.
    const base = opts.successToast ?? (outcome.replayed ? null : (summaryFor(action, args) ?? (SILENT.has(action) ? null : outcome.summary || null)))
    if (!base) return
    const effect = effectSuffix(qc, action, args, opts)
    const title = effect ? `${base} · ${effect}` : base
    toast(title, {
      kind: 'success',
      action:
        reversible && activityId
          ? {
              label: 'Undo',
              onClick: async () => {
                await run('undo_activity', { activityId }, { successToast: 'Undone' })
                if (useLastUndoable.getState().lastActivityId === activityId) useLastUndoable.getState().set(null)
              },
            }
          : undefined,
    })
  }

  return { run, apply, invalidateAfter }
}

/**
 * The mailbox effect for the toast, from what the query cache already holds
 * about the account; nothing is fetched. Null when the action never touches
 * a mailbox or the account is unknown.
 */
function effectSuffix(qc: ReturnType<typeof useQueryClient>, action: string, args: Record<string, unknown>, opts: RunOptions): string | null {
  if (!isMailboxAction(action)) return null
  const digId = opts.digId ?? (typeof args.digId === 'string' ? args.digId : null)
  if (!digId) return null
  const dig = qc.getQueryData<Dig[]>(qk.digs)?.find((d) => d.id === digId)
  if (!dig) return null
  const dev = qc.getQueryData<DevInfo>(qk.devInfo)?.dev ?? false
  const armed = qc.getQueryData<boolean>(qk.devArmed) ?? false
  return gmailEffectLine(action, args, { dig, dev, armed })
}

/** Actions whose success is its own feedback; no toast even though the outcome carries a sentence. */
const SILENT = new Set(['create_draft', 'update_draft', 'refresh_now', 'test_signal', 'evaluate_signal', 'connect_gmail'])

/** The queries an affected entity kind makes stale; the backend says what it touched, this says what to refetch. */
const ENTITY_KEYS: Partial<Record<EntityKind, QueryKey[]>> = {
  message: [qk.messages, qk.summary, qk.trustStats],
  find: [qk.finds, qk.summary],
  mesh: [qk.meshes, qk.trustStats],
  stratum: [qk.strata],
  dig: [qk.digs, qk.digGroups, qk.summary, qk.taskCentre],
  signal: [qk.signals],
  expedition: [qk.expeditions, qk.taskCentre],
  job: [qk.taskCentre],
}

function keysForEntities(affected: EntityRef[]): QueryKey[] {
  const seen = new Set<string>()
  const out: QueryKey[] = []
  for (const e of affected) {
    if (seen.has(e.kind)) continue
    seen.add(e.kind)
    out.push(...(ENTITY_KEYS[e.kind] ?? []))
  }
  return out
}

function summaryFor(action: string, args: Record<string, unknown>): string | null {
  const ids = Array.isArray(args.messageIds) ? args.messageIds.length : 0
  const count = ids > 1 ? ids : null
  switch (action) {
    case 'move_to_spoil':
      return count ? `Moved ${count} to Spoil` : 'Moved to Spoil'
    case 'archive_messages':
      return count ? `Archived ${count}` : 'Archived'
    case 'mark_contamination':
      return count ? `Marked ${count} as Contamination` : 'Marked as Contamination'
    case 'restore_messages':
      return count ? `Restored ${count}` : 'Restored'
    case 'set_disposition': {
      const d = args.disposition as Disposition | undefined
      if (d?.kind === 'find' && d.find_type === 'follow_up') return typeof args.dueAt === 'string' ? `Marked for follow-up by ${fmtBy(args.dueAt)}` : 'Marked for follow-up'
      return d ? `Corrected to ${dispositionLabel(d)}` : 'Corrected'
    }
    case 'set_importance':
      return typeof args.importance === 'string' ? `Importance: ${importanceLabel(args.importance as Importance)}` : 'Importance back to the rule\'s'
    case 'dismiss_pending':
      return 'Proposal declined'
    case 'review_shadow':
      return args.agree === false ? 'Disagreed; your decision is recorded' : 'Agreed'
    case 'label_messages':
      return 'Stratum added'
    case 'resolve_find':
      return 'Resolved'
    case 'dismiss_find':
      return 'Dismissed'
    case 'snooze_find':
      return 'Snoozed'
    case 'retype_find':
      return 'Type changed'
    case 'undo_activity':
      return 'Undone'
    case 'create_mesh':
      return 'Mesh created'
    case 'update_mesh':
      return 'Mesh updated'
    case 'set_mesh_status':
      return 'Mesh status changed'
    case 'delete_mesh':
      return 'Mesh moved to Recently Deleted'
    case 'restore_mesh':
      return 'Mesh restored'
    case 'send_message':
      return 'Sent'
    case 'save_settings':
      return 'Settings saved'
    case 'set_secret':
      return 'Key stored in the keychain'
    case 'clear_secret':
      return 'Key removed from the keychain'
    case 'save_user_profile':
      return 'Profile saved'
    default:
      // No short title: the Activity sentence, unless the Action is in SILENT.
      return null
  }
}

export function showError(e: unknown): void {
  const err = toRiddleError(e)
  if (isRiddleError(e) && e.kind === 'needs_confirmation') return
  const a = errorAffordance(err)
  log('warn', 'ui', `action failed: ${err.kind}`)
  toast(a.title, { body: a.body, kind: 'error', ttlMs: 8000 })
}

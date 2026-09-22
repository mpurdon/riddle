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
import { isRiddleError, log, toRiddleError } from './ipc'
import { errorAffordance } from './selectors'
import { toast, useConfirmQueue } from './stores'
import type { ActionOutcome } from '@/bindings/ActionOutcome'
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
}

const DEFAULT_INVALIDATIONS: Record<string, QueryKey[]> = {
  move_to_spoil: [qk.messages, qk.summary, qk.finds],
  archive_messages: [qk.messages],
  mark_contamination: [qk.messages, qk.summary],
  restore_messages: [qk.messages, qk.summary],
  set_disposition: [qk.messages, qk.summary, qk.finds],
  label_messages: [qk.messages, qk.strata],
  snooze_messages: [qk.messages],
  permanent_delete: [qk.messages, qk.summary],
  resolve_find: [qk.finds, qk.summary],
  dismiss_find: [qk.finds, qk.summary],
  snooze_find: [qk.finds, qk.summary],
  retype_find: [qk.finds],
  undo_activity: [qk.activity, qk.messages, qk.finds, qk.summary, qk.meshes, qk.strata, qk.profile, qk.settings, qk.signals],
  create_mesh: [qk.meshes],
  update_mesh: [qk.meshes],
  set_mesh_status: [qk.meshes],
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
  save_settings: [qk.settings],
  add_fixture_dig: [qk.digs, qk.taskCentre, qk.summary],
  remove_dig: [qk.digs, qk.summary, qk.messages],
  update_dig: [qk.digs],
  save_dig_group: [qk.digGroups, qk.digs],
  arm_dev: [qk.devArmed],
  refresh_now: [qk.taskCentre],
  cancel_job: [qk.taskCentre],
  pause_job: [qk.taskCentre],
  resume_job: [qk.taskCentre],
  inject_fixture_message: [qk.messages],
}

export function useAction() {
  const qc = useQueryClient()

  const run = useCallback(
    async <T = unknown>(action: string, args: Record<string, unknown> = {}, opts: RunOptions = {}): Promise<(ActionOutcome & { result: T }) | null> => {
      let outcome: (ActionOutcome & { result: T }) | null = null
      try {
        outcome = await invoke<T>(action, args, { digId: opts.digId, idempotencyKey: opts.idempotencyKey, confirmed: opts.confirmed })
      } catch (e) {
        const pending = pendingFrom(e)
        if (!pending) {
          showError(e)
          throw toRiddleError(e)
        }
        outcome = await settle<T>(pending, opts)
        if (!outcome) return null
      }
      await afterSuccess(action, outcome, opts)
      return outcome
    },
    [qc],
  )

  const settle = useCallback(
    async <T,>(pending: PendingAction, opts: RunOptions): Promise<(ActionOutcome & { result: T }) | null> => {
      if (!confirmsImmediately(pending.confirmation)) {
        const ok = await useConfirmQueue.getState().ask(pending)
        if (!ok) return null
      }
      try {
        return await confirm<T>(pending, { digId: opts.digId, idempotencyKey: opts.idempotencyKey })
      } catch (e) {
        showError(e)
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
      await afterSuccess(pending.action, outcome, opts)
      return outcome
    },
    [settle],
  )

  async function afterSuccess(action: string, outcome: ActionOutcome, opts: RunOptions) {
    const keys = [...(DEFAULT_INVALIDATIONS[action] ?? []), qk.activity, qk.pending, ...(opts.invalidate ?? [])]
    await Promise.all(keys.map((k) => qc.invalidateQueries({ queryKey: k })))
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
    const title = opts.successToast ?? summaryFor(action, outcome)
    if (!title) return
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

  return { run, apply }
}

function summaryFor(action: string, outcome: ActionOutcome): string | null {
  if (outcome.replayed) return null
  const r = outcome.result as Record<string, unknown> | null
  const count = r && typeof r === 'object' && typeof r.count === 'number' ? r.count : null
  switch (action) {
    case 'move_to_spoil':
      return count ? `Moved ${count} to Spoil` : 'Moved to Spoil'
    case 'archive_messages':
      return count ? `Archived ${count}` : 'Archived'
    case 'mark_contamination':
      return count ? `Marked ${count} as Contamination` : 'Marked as Contamination'
    case 'restore_messages':
      return count ? `Restored ${count}` : 'Restored'
    case 'set_disposition':
      return 'Corrected'
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
    case 'save_user_profile':
      return 'Profile saved'
    case 'create_draft':
    case 'update_draft':
    case 'refresh_now':
    case 'test_signal':
    case 'evaluate_signal':
      return null
    default:
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

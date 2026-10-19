import { beforeEach, describe, expect, it } from 'vitest'
import type { ActionInvocation } from '@/bindings/ActionInvocation'
import type { PendingAction } from '@/bindings/PendingAction'
import type { RiddleError } from '@/bindings/RiddleError'
import { invokeAction } from './actions'
import { buildState, type MockState } from './state'
import { listMessages, simulate, summary } from './queries'
import { MESH_GITHUB_V7 } from './fixtures'

let state: MockState
beforeEach(() => {
  state = buildState()
})

const inv = (action: string, args: Record<string, unknown>, extra: Partial<ActionInvocation> = {}): Record<string, unknown> => ({
  invocation: { action, args, principal: { kind: 'user' }, digId: (args.digId as string) ?? null, idempotencyKey: null, confirmed: false, pendingId: null, ...extra } satisfies ActionInvocation,
})

async function expectPending(action: string, args: Record<string, unknown>): Promise<PendingAction> {
  try {
    await invokeAction(state, inv(action, args))
  } catch (e) {
    const err = e as RiddleError
    expect(err.kind).toBe('needs_confirmation')
    return err.detail as PendingAction
  }
  throw new Error('expected needs_confirmation')
}

describe('the mock Action layer follows the confirmation tiers', () => {
  it('a bulk move to Spoil confirms in place and can be undone', async () => {
    const pending = await expectPending('move_to_spoil', { digId: 'dig_work', messageIds: ['m_work_pagerduty'] })
    expect(pending.confirmation).toBe('in_place_undo')
    const out = await invokeAction(state, inv('move_to_spoil', { digId: 'dig_work', messageIds: ['m_work_pagerduty'] }, { pendingId: pending.id }))
    expect(state.classifications.get('m_work_pagerduty')?.disposition).toEqual({ kind: 'spoil' })
    expect(out.activityId).toBeTruthy()
    const entry = state.activity.find((a) => a.id === out.activityId)!
    expect(entry.undo.state).toBe('undoable')
    await invokeAction(state, inv('undo_activity', { activityId: entry.id }))
    expect(state.classifications.get('m_work_pagerduty')?.disposition).toEqual({ kind: 'filed' })
    expect(state.activity.find((a) => a.id === entry.id)!.undo.state).toBe('undone')
  })

  it('a stale confirmation is refused', async () => {
    await expect(invokeAction(state, inv('resolve_find', { findId: 'find_review_412' }, { pendingId: 'nope' }))).rejects.toMatchObject({ kind: 'invalid' })
  })

  it('send_message is two-step and previews the real recipients; a reply resolves its Find', async () => {
    state.writes = 'allow'
    state.armed = true
    const pending = await expectPending('send_message', { digId: 'dig_personal', draftId: 'draft_1' })
    expect(pending.confirmation).toBe('two_step')
    const real = pending.realArguments as { to: { email: string }[]; subject: string }
    expect(real.to[0].email).toBe('priya@northwind.example')
    expect(real.subject).toMatch(/Datadog/)
    await invokeAction(state, inv('send_message', { digId: 'dig_work', draftId: 'draft_1' }, { pendingId: pending.id }))
    expect(state.drafts.find((d) => d.id === 'draft_1')).toBeUndefined()
    expect(state.finds.find((f) => f.id === 'find_decide_renewal')?.status).toBe('resolved')
    expect(state.finds.find((f) => f.id === 'find_decide_renewal')?.resolvedBy).toBe('thread_state')
  })

  it('a dev build on Hold refuses what calls the provider and runs corrections, whose change waits', async () => {
    await expect(invokeAction(state, inv('send_message', { digId: 'dig_work', draftId: 'draft_1' }))).rejects.toMatchObject({ kind: 'policy_denied' })
    const id = state.messages[0].id
    await invokeAction(state, inv('set_disposition', { digId: state.messages[0].digId, messageId: id, disposition: { kind: 'filed' } }, { confirmed: true }))
    expect(state.held).toBe(1)
  })

  it('a dev build on Ask sends the waiting changes with one confirmation', async () => {
    state.writes = 'ask'
    state.held = 7
    const pending = await expectPending('send_held_changes', {})
    expect(pending.summary).toBe('Write 7 waiting changes to the mailbox')
    await invokeAction(state, inv('send_held_changes', {}, { pendingId: pending.id }))
    expect(state.held).toBe(0)
  })

  it('removing a Dig needs the exact count typed', async () => {
    const pending = await expectPending('remove_dig', { digId: 'dig_consulting' })
    expect(pending.confirmation).toBe('type_count')
    expect((pending.realArguments as { count: number }).count).toBe(state.messages.filter((m) => m.digId === 'dig_consulting').length)
  })

  it('activating a Mesh that proposes Spoil needs a held confirmation, and an inline hold passes confirmed', async () => {
    const pending = await expectPending('set_mesh_status', { meshId: 'mesh_aws', status: 'active' })
    expect(pending.confirmation).toBe('press_hold')
    await invokeAction(state, inv('set_mesh_status', { meshId: 'mesh_aws', status: 'active' }, { confirmed: true }))
    expect(state.meshes.find((m) => m.id === 'mesh_aws')?.status).toBe('active')
  })

  it('promoting a Signal is an approval', async () => {
    const pending = await expectPending('promote_signal', { name: 'waiting_on_me', version: 1 })
    expect(pending.confirmation).toBe('approval')
  })

  it('updating a Mesh creates a new version and starts it in Shadow, without a simulation', async () => {
    const text = MESH_GITHUB_V7.replace(' v7', ' v8').replace('routine@v1 > .90', 'routine@v1 > .50')
    const out = await invokeAction(state, inv('update_mesh', { meshId: 'mesh_github', text, changeNote: 'lower the bar' }))
    const r = out.result as { mesh: { version: number; status: string }; report?: unknown }
    expect(r.mesh.version).toBe(8)
    expect(r.mesh.status).toBe('shadow')
    expect(r.report).toBeUndefined()
    await expect(invokeAction(state, inv('update_mesh', { meshId: 'mesh_github', text: 'Mesh: Broken v9\nROUTES\nIF routine > .9\n    → Spoil\n' }))).rejects.toMatchObject({ kind: 'invalid', detail: expect.objectContaining({ line: 3 }) })
  })

  it('Undo All skips messages the user handled by hand since', async () => {
    const director = state.activity.find((a) => a.id === 'act_director_nl')!
    await invokeAction(state, inv('set_disposition', { digId: 'dig_personal', messageId: 'm_personal_nl_1', disposition: { kind: 'filed' } }, { confirmed: true }))
    const out = await invokeAction(state, inv('undo_activity', { activityId: director.id }))
    const r = out.result as { restored: number; skipped: number }
    expect(r.skipped).toBe(1)
    expect(r.restored).toBe(22)
    const undoEntry = state.activity.find((a) => a.undoes === director.id)!
    expect(undoEntry.result.status).toBe('partial')
  })

  it('idempotency keys replay the same outcome', async () => {
    const a = await invokeAction(state, inv('resolve_find', { findId: 'find_pay_invoice' }, { confirmed: true, idempotencyKey: 'k1' }))
    const b = await invokeAction(state, inv('resolve_find', { findId: 'find_pay_invoice' }, { confirmed: true, idempotencyKey: 'k1' }))
    expect(a.replayed).toBe(false)
    expect(b.replayed).toBe(true)
  })
})

describe('the mock reads stay consistent with the mutations', () => {
  it('summary counts open Finds and Unresolved from state', async () => {
    const before = summary(state)
    await invokeAction(state, inv('dismiss_find', { findId: 'find_review_412' }, { confirmed: true }))
    const after = summary(state)
    expect(Number(after.findsOpen)).toBe(Number(before.findsOpen) - 1)
    const unresolved = listMessages(state, { filter: { disposition: 'unresolved' }, page: { cursor: null, limit: 50 } })
    expect(Number(unresolved.total)).toBe(Number(before.unresolved))
  })

  it('simulation of the current version changes nothing', () => {
    const r = simulate(state, { meshId: 'mesh_github', version: 7 })
    expect(Number(r.changed)).toBe(0)
    expect(Number(r.unchanged)).toBe(Number(r.historicalMessages))
  })
})

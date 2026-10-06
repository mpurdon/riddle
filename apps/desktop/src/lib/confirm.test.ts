import { describe, expect, it } from 'vitest'
import type { PendingAction } from '@/bindings/PendingAction'
import { confirmsImmediately, expectedCount, pendingFrom, realRecipients, tierFor } from './confirm'

const pending = (over: Partial<PendingAction>): PendingAction => ({ id: 'p1', action: 'send_message', args: {}, realArguments: null, summary: '', confirmation: 'two_step', reason: '', principal: { kind: 'user' }, createdAt: '', ...over })

describe('confirmation tiers route to a real control', () => {
  it('maps every policy to its component', () => {
    expect(tierFor('none')).toBe('auto')
    expect(tierFor('in_place_undo')).toBe('ConfirmInPlace')
    expect(tierFor('two_step')).toBe('TwoStepDialog')
    expect(tierFor('type_count')).toBe('TypeCountDialog')
    expect(tierFor('press_hold')).toBe('HoldButton')
    expect(tierFor('approval')).toBe('ApprovalDialog')
  })
  it('only the in-place tier proceeds without a dialog', () => {
    expect(confirmsImmediately('in_place_undo')).toBe(true)
    expect(confirmsImmediately('none')).toBe(true)
    for (const p of ['two_step', 'type_count', 'press_hold', 'approval'] as const) expect(confirmsImmediately(p)).toBe(false)
  })
})

describe('a needs_confirmation error carries the PendingAction', () => {
  it('extracts the pending action from the error detail', () => {
    const p = pending({})
    expect(pendingFrom({ kind: 'needs_confirmation', message: 'x', detail: p })).toEqual(p)
  })
  it('ignores other errors and malformed details', () => {
    expect(pendingFrom({ kind: 'policy_denied', message: 'x', detail: pending({}) })).toBeNull()
    expect(pendingFrom({ kind: 'needs_confirmation', message: 'x', detail: { nope: true } })).toBeNull()
    expect(pendingFrom(new Error('boom'))).toBeNull()
  })
})

describe('the type-count and two-step dialogs read the Action layer, not the caller', () => {
  it('takes the count from realArguments before args', () => {
    expect(expectedCount(pending({ realArguments: { count: 8 }, args: { messageIds: ['a', 'b'] } }))).toBe(8)
    expect(expectedCount(pending({ realArguments: null, args: { messageIds: ['a', 'b'] } }))).toBe(2)
    expect(expectedCount(pending({ realArguments: {}, args: {} }))).toBeNull()
    expect(expectedCount(pending({ realArguments: { digId: 'd', name: 'Work', count: 1100 } }))).toBe(1100)
  })
  it('reads real recipients and subject for a send', () => {
    const r = realRecipients(pending({ realArguments: { to: [{ email: 'a@x.example', name: 'A' }], cc: ['b@x.example'], bcc: [], subject: 'Hi' } }))
    expect(r?.to).toEqual([{ email: 'a@x.example', name: 'A' }])
    expect(r?.cc).toEqual([{ email: 'b@x.example', name: null }])
    expect(r?.subject).toBe('Hi')
    expect(realRecipients(pending({ realArguments: { count: 3 } }))).toBeNull()
  })
})

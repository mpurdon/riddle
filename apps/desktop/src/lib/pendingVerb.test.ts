import { describe, expect, it } from 'vitest'
import type { PendingAction } from '@/bindings/PendingAction'
import type { SimulationReport } from '@/bindings/SimulationReport'
import { u64 } from '@/lib/format'
import { argLabel, impactLine, meshText, pendingVerb } from './pendingVerb'

const base: PendingAction = {
  id: 'pend_1',
  action: 'update_mesh',
  args: {},
  realArguments: {},
  summary: 'Update Mesh: GitHub Notifications v7 → v8',
  confirmation: 'approval',
  reason: 'The Director proposes; you apply.',
  principal: { kind: 'director', conversation_id: 'conv_1', read_untrusted: true },
  createdAt: '2026-09-25T09:00:00Z',
}

describe('the verb on a pending Action', () => {
  it('an update_mesh proposal says Apply the Mesh and names the versions', () => {
    const p = { ...base, realArguments: { meshId: 'mesh_github', name: 'GitHub Notifications', fromVersion: 7, toVersion: 8, text: 'Mesh: GitHub Notifications v8\n' } }
    expect(pendingVerb(p)).toEqual({ button: 'Apply the Mesh', line: 'Update Mesh GitHub Notifications v7 → v8' })
  })

  it('a create_mesh proposal names the Mesh from the real arguments, and says a Mesh when they carry no name', () => {
    const p = { ...base, action: 'create_mesh', realArguments: { name: 'Receipts', text: 'Mesh: Receipts v1\n\nScope:\nPersonal' } }
    expect(pendingVerb(p).line).toBe('Create Mesh Receipts')
    expect(meshText(p)).toContain('Receipts')
    expect(pendingVerb({ ...base, action: 'create_mesh', realArguments: { text: 'Mesh: Receipts v1' } }).line).toBe('Create a Mesh')
  })

  it('a send says Send and counts the real recipients, not the summary', () => {
    const p = { ...base, action: 'send_message', summary: 'Send to everyone', realArguments: { to: [{ email: 'a@x', name: 'A' }, 'b@x'], subject: 'Hello' } }
    expect(pendingVerb(p)).toEqual({ button: 'Send', line: 'Send "Hello" to 2 recipients' })
  })

  it('never hidden says the verb and the entry', () => {
    const p = { ...base, action: 'add_never_hidden', realArguments: { entry: 'lena@example.com' } }
    expect(pendingVerb(p)).toEqual({ button: 'Add to Never Hidden', line: 'Add lena@example.com to Never Hidden' })
  })

  it('an unmapped action falls back to the given word and the summary', () => {
    const p = { ...base, action: 'rebuild_index', summary: 'Rebuild the index' }
    expect(pendingVerb(p)).toEqual({ button: 'Approve', line: 'Rebuild the index' })
    expect(pendingVerb(p, 'Confirm').button).toBe('Confirm')
  })

  it('a non-Mesh action carries no Mesh text even when the arguments have a text field', () => {
    expect(meshText({ action: 'send_message', realArguments: { text: 'body' } })).toBeNull()
  })
})

describe('the impact headline', () => {
  const report = (over: Partial<SimulationReport>): SimulationReport => ({
    meshId: null,
    meshName: 'GitHub Notifications',
    fromVersion: 7,
    toVersion: 8,
    historicalMessages: u64(412),
    unchanged: u64(398),
    changed: u64(14),
    transitions: [
      { from: 'find', to: 'filed', count: u64(8) },
      { from: 'spoil', to: 'find', count: u64(4) },
      { from: 'filed', to: 'find', count: u64(2) },
    ],
    newJudgeCallsRequired: u64(0),
    joinsSeries: u64(0),
    recordedChanges: u64(0),
    takenFirstBy: [],
    takesFrom: [],
    rescore: u64(0),
    estimatedCostUsd: 0,
    sampleChanged: [],
    editChanged: u64(14),
    editTransitions: [
      { from: 'find', to: 'filed', count: u64(8) },
      { from: 'spoil', to: 'find', count: u64(4) },
      { from: 'filed', to: 'find', count: u64(2) },
    ],
    sampleEditChanged: [],
    drift: u64(0),
    windowDays: 90,
    ...over,
  })

  it('groups the transitions by where messages would go, largest first', () => {
    expect(impactLine(report({}))).toBe('Would change 14 messages in the last 90 days: 8 to Filed, 6 to Find.')
  })

  it('says so plainly when nothing would change', () => {
    expect(impactLine(report({ changed: u64(0), transitions: [], editChanged: u64(0), editTransitions: [] }))).toBe('Would change nothing in the last 90 days (412 messages).')
  })

  it('calls Unresolved Escalate, as the Meshes page does', () => {
    expect(impactLine(report({ editChanged: u64(1), editTransitions: [{ from: 'find', to: 'unresolved', count: u64(1) }] }))).toBe('Would change 1 message in the last 90 days: 1 to Escalate.')
  })

  it("counts the edit's own changes, and names the mail re-decided anyway apart", () => {
    const r = report({ changed: u64(20), editChanged: u64(4), editTransitions: [{ from: 'find', to: 'spoil', count: u64(4) }], drift: u64(16) })
    expect(impactLine(r)).toBe("Would change 4 messages in the last 90 days: 4 to Spoil. Applying to history also re-decides 16 that today's rules already decide differently.")
  })
})

describe('argument labels', () => {
  it('names ids by their noun and reads the rest as words', () => {
    expect(argLabel('digId')).toBe('Account id')
    expect(argLabel('fromVersion')).toBe('From version')
    expect(argLabel('bodyPreview')).toBe('Body preview')
    expect(argLabel('some_other_key')).toBe('Some other key')
  })
})

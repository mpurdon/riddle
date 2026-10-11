/**
 * Mock `invoke_action`: the same shape as the Rust runtime (§8, §51). It
 * checks the confirmation tier, returns `needs_confirmation` with a
 * PendingAction whose realArguments are what the user must see, executes on
 * confirm, appends Activity, records undo information and emits events.
 */
import type { Importance } from '@/bindings/Importance'
import type { ActionInvocation } from '@/bindings/ActionInvocation'
import type { ActionMeta } from '@/bindings/ActionMeta'
import type { ActionOutcome } from '@/bindings/ActionOutcome'
import type { Activity } from '@/bindings/Activity'
import type { ConfirmationPolicy } from '@/bindings/ConfirmationPolicy'
import type { Disposition } from '@/bindings/Disposition'
import type { EntityRef } from '@/bindings/EntityRef'
import type { Find } from '@/bindings/Find'
import type { FindType } from '@/bindings/FindType'
import type { Mesh } from '@/bindings/Mesh'
import type { MeshStatus } from '@/bindings/MeshStatus'
import type { PendingAction } from '@/bindings/PendingAction'
import type { SimulationReport } from '@/bindings/SimulationReport'
import type { UndoInfo } from '@/bindings/UndoInfo'
import type { Draft } from '@/bindings/Draft'
import type { Signal } from '@/bindings/Signal'
import type { Stratum } from '@/bindings/Stratum'
import type { Provenance } from '@/bindings/Provenance'
import type { ActivityResult } from '@/bindings/ActivityResult'
import { emitMock } from '../events'
import { plural, u64 } from '../format'
import { MeshSyntaxError, parseMeshText } from './dsl'
import { buildMessage, buildTrace, DIG_WORK, ME, iso, NOW, type MessageSpec } from './fixtures'
import type { Job } from '@/bindings/Job'
import { nextId, type MockState } from './state'
import { err, simulate, findTitleFor } from './queries'
import type { Settings, SignalTestReport } from '../types'

type Meta = Omit<ActionMeta, 'name' | 'description'> & { description: string }
const base: Meta = { description: '', mutating: true, reversible: false, externalSideEffect: false, projected: false, risk: 'low', confirmation: 'none', minimumAutonomy: 'act', egress: false, readOnly: false, userOnly: false }
const m = (description: string, o: Partial<Meta> = {}): Meta => ({ ...base, description, ...o })

export const ACTIONS: Record<string, Meta> = {
  archive_messages: m('Archive messages', { reversible: true, projected: true, confirmation: 'in_place_undo' }),
  move_to_spoil: m('Move messages to Spoil', { reversible: true, projected: true, risk: 'medium', confirmation: 'in_place_undo' }),
  mark_contamination: m('Mark messages as Contamination', { reversible: true, projected: true, risk: 'medium', confirmation: 'in_place_undo' }),
  restore_messages: m('Restore messages from Spoil or Contamination', { reversible: true, projected: true, confirmation: 'in_place_undo' }),
  set_disposition: m('Correct the disposition of one message', { reversible: true, projected: true, confirmation: 'in_place_undo' }),
  label_messages: m('Add Strata to messages', { reversible: true, projected: true, confirmation: 'in_place_undo' }),
  snooze_messages: m('Snooze messages', { reversible: true, confirmation: 'in_place_undo' }),
  permanent_delete: m('Delete messages permanently', { risk: 'critical', externalSideEffect: true, confirmation: 'type_count', minimumAutonomy: 'trusted' }),
  resolve_find: m('Resolve a Find', { reversible: true, confirmation: 'in_place_undo' }),
  dismiss_find: m('Dismiss a Find: not important', { reversible: true, confirmation: 'in_place_undo' }),
  snooze_find: m('Snooze a Find', { reversible: true, confirmation: 'in_place_undo' }),
  retype_find: m('Change the type of a Find', { reversible: true, confirmation: 'in_place_undo' }),
  undo_activity: m('Undo an Activity entry', { externalSideEffect: true }),
  create_mesh: m('Create a Mesh', { reversible: true, risk: 'medium', confirmation: 'none', minimumAutonomy: 'suggest', egress: true }),
  update_mesh: m('Update a Mesh (new version)', { reversible: true, risk: 'medium', confirmation: 'none', minimumAutonomy: 'suggest', egress: true }),
  set_mesh_status: m('Change a Mesh status', { reversible: true, risk: 'medium' }),
  set_mesh_fallback: m('Mark a Mesh a fallback', { reversible: true }),
  remember_for_sender: m('Always decide a sender this way', { reversible: true }),
  install_pack: m('Install or update a rule pack', { reversible: true }),
  remove_pack: m('Remove an installed rule pack'),
  import_pack: m('Add a rule pack from a pack file'),
  export_pack: m('Write a rule pack as a pack file'),
  create_pack: m('Make a rule pack from your Meshes'),
  save_list: m('Create or replace a named list', { reversible: true }),
  delete_list: m('Delete a named list', { reversible: true }),
  grade_decision: m('Grade a rule decision', { reversible: true }),
  delete_mesh: m('Move a Mesh to Recently Deleted', { reversible: true, confirmation: 'in_place_undo' }),
  restore_mesh: m('Restore a Mesh from Recently Deleted', { reversible: true }),
  propose_signal: m('Propose a Signal', { minimumAutonomy: 'suggest' }),
  test_signal: m('Test a Signal against a sample', { mutating: false, readOnly: true }),
  promote_signal: m('Promote a Signal version', { risk: 'high', confirmation: 'approval' }),
  evaluate_signal: m('Evaluate a Signal on one message', { mutating: false, readOnly: true }),
  rename_stratum: m('Rename a Stratum', { reversible: true, confirmation: 'in_place_undo' }),
  merge_strata: m('Merge one Stratum into another', { reversible: true, risk: 'medium', confirmation: 'two_step' }),
  create_stratum: m('Create a Stratum', { reversible: true }),
  create_draft: m('Create a draft'),
  update_draft: m('Update a draft'),
  delete_draft: m('Delete a draft', { reversible: true, confirmation: 'in_place_undo' }),
  send_message: m('Send a message', { externalSideEffect: true, risk: 'high', confirmation: 'two_step', egress: true, minimumAutonomy: 'trusted' }),
  start_expedition: m('Start an Expedition', { risk: 'medium', confirmation: 'two_step' }),
  cancel_expedition: m('Cancel an Expedition'),
  add_never_hidden: m('Add to Never Hidden', { reversible: true, egress: true, confirmation: 'in_place_undo' }),
  remove_never_hidden: m('Remove from Never Hidden', { reversible: true, confirmation: 'in_place_undo' }),
  save_user_profile: m('Save the User Profile'),
  save_settings: m('Save settings'),
  set_secret: m('Store a key in the OS keychain', { risk: 'medium', userOnly: true }),
  clear_secret: m('Remove a key from the OS keychain', { risk: 'medium', userOnly: true }),
  import_gmail_client: m('Import the Gmail OAuth client file from Google', { risk: 'medium', userOnly: true }),
  add_fixture_dig: m('Add a Fixture Dig'),
  connect_gmail: m('Connect a Gmail account, or sign in again', { risk: 'high' }),
  dismiss_pending: m('Decline a pending proposal', { userOnly: true }),
  review_shadow: m('Agree or disagree with a Shadow proposal', { reversible: true, projected: true }),
  remove_dig: m('Remove a Dig and its store', { risk: 'critical', confirmation: 'type_count' }),
  update_dig: m('Update a Dig'),
  set_dig_enabled: m('Disable or re-enable an account'),
  save_dig_group: m('Save a Dig Group'),
  rename_dig_group: m('Rename a Dig Group', { reversible: true }),
  delete_dig_group: m('Delete a Dig Group (its accounts stay)', { reversible: true }),
  apply_mesh_to_history: m('Re-decide recent mail under a Mesh\'s current version', { reversible: true, risk: 'medium', confirmation: 'two_step' }),
  decide_again: m('Decide messages again under the current rules', { reversible: true }),
  set_mailbox_writes: m('Choose what this dev build may write to the mailbox: hold, ask or allow', { reversible: true, risk: 'high', confirmation: 'press_hold' }),
  send_held_changes: m('Write the mailbox changes waiting in the outbox now', { externalSideEffect: true, risk: 'medium', confirmation: 'approval', minimumAutonomy: 'trusted' }),
  discard_held_changes: m('Drop the mailbox changes waiting in the outbox; Riddle keeps its decisions', { risk: 'medium', confirmation: 'approval', minimumAutonomy: 'trusted' }),
  refresh_now: m('Refresh now', { mutating: false }),
  cancel_job: m('Cancel a job'),
  pause_job: m('Pause a job'),
  resume_job: m('Resume a job'),
  inject_fixture_message: m('Inject a fixture message (Demo Mode)'),
  apply_update_preview: m('Apply or keep the Update Preview', { risk: 'medium', confirmation: 'two_step' }),
}

export function listActions(): ActionMeta[] {
  return Object.entries(ACTIONS)
    .map(([name, meta]) => ({ name, ...meta }))
    .sort((a, b) => a.name.localeCompare(b.name))
}

// ---------------------------------------------------------------------------

type Args = Record<string, unknown>

interface Preview {
  summary: string
  realArguments: unknown
}

interface Exec {
  result: unknown
  summary: string
  why?: string | null
  affected?: EntityRef[]
  undo?: UndoInfo
  provenance?: Provenance | null
  inputs?: unknown
  activityResult?: ActivityResult
  digId?: string | null
  /** Skip the Activity entry (drafts, settings that are not events). */
  silent?: boolean
}

const str = (a: Args, k: string): string => {
  const v = a[k]
  if (typeof v !== 'string' || !v) throw err('invalid', `invalid arguments: missing ${k}`)
  return v
}
const strList = (a: Args, k: string): string[] => {
  const v = a[k]
  if (!Array.isArray(v)) throw err('invalid', `invalid arguments: ${k} must be a list`)
  return v.map(String)
}

function dispositionLabel(d: Disposition | null): string {
  if (!d) return 'Unsifted'
  if (d.kind === 'find') return `Find: ${d.find_type}`
  return d.kind[0].toUpperCase() + d.kind.slice(1)
}

export function dynamicTier(state: MockState, action: string, args: Args): ConfirmationPolicy {
  const meta = ACTIONS[action]
  if (!meta) return 'none'
  if (action === 'set_mesh_status' && args.status === 'active') {
    const mesh = latestMesh(state, String(args.meshId))
    if (mesh && mesh.routes.some((r) => r.output.disposition?.kind === 'spoil' || r.output.disposition?.kind === 'contamination')) return 'press_hold'
    return 'none'
  }
  if (action === 'save_settings') {
    const next = args.settings as Settings | undefined
    if (next) {
      const raised = Object.entries(next.autonomy?.perDig ?? {}).some(([d, a]) => a === 'trusted' && state.settings.autonomy.perDig[d] !== 'trusted')
      if (raised || (next.autonomy?.default === 'trusted' && state.settings.autonomy.default !== 'trusted')) return 'press_hold'
    }
    return 'none'
  }
  if (action === 'update_dig') {
    return 'none'
  }
  return meta.confirmation
}

function latestMesh(state: MockState, meshId: string): Mesh | null {
  const vs = state.meshes.filter((x) => x.id === meshId)
  if (!vs.length) return null
  return vs.reduce((a, b) => (a.version > b.version ? a : b))
}

function messageTitle(state: MockState, id: string): string {
  return state.messages.find((x) => x.id === id)?.subject ?? id
}

/** What apply_mesh_to_history would do: the Mesh's latest version over the window. The preview and the execution describe the same plan. */
function applyPlan(state: MockState, args: Args): { mesh: Mesh; windowDays: number; report: SimulationReport } {
  const mesh = latestMesh(state, str(args, 'meshId'))
  if (!mesh) throw err('not_found', 'Mesh not found')
  const windowDays = Number(args.windowDays ?? 30)
  return { mesh, windowDays, report: simulate(state, { meshId: mesh.id, windowDays }) }
}

function preview(state: MockState, action: string, args: Args): Preview {
  switch (action) {
    case 'send_message': {
      const draft = state.drafts.find((d) => d.id === args.draftId)
      if (!draft) throw err('not_found', 'draft not found')
      return {
        summary: `Send "${draft.subject || '(no subject)'}" to ${draft.to.map((a) => a.email).join(', ') || 'nobody'}`,
        realArguments: { to: draft.to, cc: draft.cc, bcc: draft.bcc, subject: draft.subject, digId: draft.digId, from: ME[draft.digId]?.email ?? null, bodyPreview: draft.bodyText.slice(0, 200) },
      }
    }
    case 'permanent_delete': {
      const ids = strList(args, 'messageIds')
      return { summary: `Permanently delete ${ids.length} messages`, realArguments: { count: ids.length, messageIds: ids, digId: args.digId } }
    }
    case 'remove_dig': {
      const dig = state.digs.find((d) => d.id === args.digId)
      const count = state.messages.filter((x) => x.digId === args.digId).length
      return { summary: `Remove account ${dig?.name ?? args.digId} and its ${count} messages`, realArguments: { count, digId: args.digId, name: dig?.name ?? null, email: dig?.email ?? null } }
    }
    case 'apply_mesh_to_history': {
      const { mesh, windowDays, report } = applyPlan(state, args)
      return {
        summary: `Re-decide ${Number(report.changed)} messages from the last ${windowDays} days under Mesh ${mesh.name} v${mesh.version}`,
        realArguments: { meshId: mesh.id, meshName: mesh.name, version: mesh.version, windowDays, count: Number(report.changed), keptYours: 0, transitions: report.transitions.map((t) => `${Number(t.count)} ${t.from} → ${t.to}`) },
      }
    }
    case 'set_mesh_status': {
      const mesh = latestMesh(state, String(args.meshId))
      return { summary: `Set Mesh ${mesh?.name ?? args.meshId} to ${String(args.status)}. It proposes Spoil, so it will start hiding mail on its own.`, realArguments: { meshId: args.meshId, name: mesh?.name, from: mesh?.status, to: args.status, routes: mesh?.routes.map((r) => r.text) ?? [] } }
    }
    case 'save_settings': {
      const next = args.settings as Settings
      const raised = Object.entries(next.autonomy.perDig).filter(([d, a]) => a === 'trusted' && state.settings.autonomy.perDig[d] !== 'trusted').map(([d]) => state.digs.find((x) => x.id === d)?.name ?? d)
      return { summary: `Raise ${raised.join(', ') || 'the default'} to Trusted: Riddle will act without asking, including on external side effects.`, realArguments: { digs: raised, autonomy: next.autonomy } }
    }
    case 'promote_signal':
      return { summary: `Promote Signal ${String(args.name)} v${String(args.version)} to active. Meshes pin versions; this triggers re-simulation and a proposal, never a silent change.`, realArguments: { name: args.name, version: args.version } }
    case 'set_mailbox_writes':
      return { summary: `Mailbox writes: ${String(args.writes)}`, realArguments: { writes: args.writes } }
    case 'send_held_changes':
      return { summary: `Write ${plural(state.held, 'waiting change')} to the mailbox`, realArguments: { held: state.held } }
    case 'discard_held_changes':
      return { summary: `Drop ${plural(state.held, 'waiting change')}; the mailbox stays as it is`, realArguments: { held: state.held } }
    case 'merge_strata': {
      const from = state.strata.find((s) => s.id === args.fromId)
      const into = state.strata.find((s) => s.id === args.intoId)
      return { summary: `Merge Stratum ${from?.name} (${Number(from?.messageCount ?? 0)} messages) into ${into?.name}`, realArguments: { from: from?.name, into: into?.name, count: Number(from?.messageCount ?? 0) } }
    }
    case 'start_expedition':
      return { summary: `Start an Expedition: "${String(args.goal)}"`, realArguments: { goal: args.goal, scope: args.scope, budget: args.budget } }
    case 'apply_update_preview':
      return { summary: args.keep ? 'Keep the previous behaviour' : 'Apply the updated Signals and Meshes to your mail', realArguments: { keep: args.keep } }
    default:
      return { summary: `${ACTIONS[action]?.description ?? action}`, realArguments: args }
  }
}

// ---------------------------------------------------------------------------

export async function invokeAction(state: MockState, raw: Args): Promise<ActionOutcome> {
  const inv = raw.invocation as ActionInvocation | undefined
  if (!inv || typeof inv.action !== 'string') throw err('invalid', 'invalid arguments: invocation')
  const meta = ACTIONS[inv.action]
  if (!meta) throw err('not_found', `no Action named ${inv.action}`)
  const args = (inv.args ?? {}) as Args

  if (inv.idempotencyKey && state.idempotent.has(inv.idempotencyKey)) {
    return { result: state.idempotent.get(inv.idempotencyKey), activityId: null, replayed: true, summary: '', affected: [] }
  }

  let confirmed = false
  if (inv.pendingId) {
    const p = state.pending.get(inv.pendingId)
    if (!p) throw err('invalid', 'that confirmation has expired; try again')
    if (p.action !== inv.action) throw err('invalid', 'confirmation does not match the action')
    state.pending.delete(inv.pendingId)
    confirmed = true
  } else {
    confirmed = !!inv.confirmed && inv.principal.kind === 'user'
  }

  // Dev builds start on Hold (riddle-actions `policy::decide`): an Action
  // that calls the provider is refused, and on Ask it asks first. Projected
  // Actions run; their mailbox change waits (`state.held`).
  if (state.devInfo.dev && meta.externalSideEffect && inv.action !== 'undo_activity' && state.writes === 'hold') {
    throw err('policy_denied', 'this dev build holds mailbox changes; choose Ask or Allow under Developer → Mailbox writes')
  }
  const askFirst = state.devInfo.dev && meta.externalSideEffect && state.writes === 'ask'

  const baseTier = dynamicTier(state, inv.action, args)
  const tier: ConfirmationPolicy = askFirst && (baseTier === 'none' || baseTier === 'in_place_undo') ? 'approval' : baseTier
  if (!meta.readOnly && !confirmed && tier !== 'none') {
    const pv = preview(state, inv.action, args)
    const pending: PendingAction = {
      id: nextId(state, 'pend'),
      action: inv.action,
      args,
      realArguments: pv.realArguments,
      summary: pv.summary,
      confirmation: tier,
      reason: `${inv.action} requires confirmation`,
      principal: inv.principal,
      createdAt: iso(new Date()),
    }
    state.pending.set(pending.id, pending)
    throw err('needs_confirmation', pending.summary, pending)
  }

  await new Promise((r) => setTimeout(r, 40 + Math.random() * 80))
  const exec = execute(state, inv.action, args)
  if (meta.projected && state.devInfo.dev && state.writes !== 'allow') state.held += Array.isArray(args.messageIds) ? args.messageIds.length : 1
  const activityId = nextId(state, 'act')
  if (!exec.silent && meta.mutating) {
    const activity: Activity = {
      id: activityId,
      actor: 'user',
      action: inv.action,
      summary: exec.summary,
      why: exec.why ?? null,
      inputs: exec.inputs ?? redactInputs(args),
      affected: exec.affected ?? [],
      result: exec.activityResult ?? { status: 'ok' },
      provenance: exec.provenance ?? null,
      undo: exec.undo ?? { state: 'not_undoable', reason: 'this action has no undo' },
      digId: exec.digId ?? inv.digId ?? null,
      traceId: exec.provenance?.traceId ?? null,
      undoes: inv.action === 'undo_activity' ? String(args.activityId) : null,
      createdAt: iso(new Date()),
    }
    state.activity.unshift(activity)
    emitMock({ type: 'activity_appended', activity })
  }
  const outcome: ActionOutcome = { result: exec.result, activityId: exec.silent ? null : activityId, replayed: false, summary: exec.silent ? '' : exec.summary, affected: exec.affected ?? [] }
  if (inv.idempotencyKey) state.idempotent.set(inv.idempotencyKey, exec.result)
  return outcome
}

function redactInputs(args: Args): unknown {
  const out: Args = {}
  for (const [k, v] of Object.entries(args)) {
    if (Array.isArray(v)) out[k] = { count: v.length }
    else if (typeof v === 'string' || typeof v === 'number' || typeof v === 'boolean') out[k] = k.toLowerCase().includes('body') || k === 'text' ? '[redacted]' : v
    else out[k] = '[object]'
  }
  return out
}

// ---------------------------------------------------------------------------
// Execution

function setDisposition(state: MockState, digId: string, messageId: string, d: Disposition | null, byUser = true, findType?: FindType | null, dueAt?: string | null): { before: Disposition | null } {
  const c = state.classifications.get(messageId)
  if (!c) throw err('not_found', 'message not found')
  const before = c.disposition
  c.disposition = d
  if (byUser) c.handledByUserAt = iso(new Date())
  const msg = state.messages.find((x) => x.id === messageId)
  if (d?.kind === 'find' && msg) {
    // Join or create the Find.
    let find = c.findId ? state.finds.find((f) => f.id === c.findId) : state.finds.find((f) => f.sources.some((s) => s.threadId === msg.threadId) && f.status === 'open')
    if (!find) {
      find = {
        id: nextId(state, 'find'),
        findType: findType ?? d.find_type,
        title: msg.subject,
        summary: msg.snippet,
        priority: 60,
        status: 'open',
        dueAt: dueAt ?? null,
        snoozedUntil: null,
        createdAt: iso(new Date()),
        updatedAt: iso(new Date()),
        resolvedAt: null,
        resolvedBy: null,
        sources: [{ messageId, threadId: msg.threadId, digId }],
        digIds: [digId],
        identityKey: msg.threadId,
        reason: 'You said this is a Find.',
        confidence: null,
        classificationId: `cls_${messageId}`,
        traceId: `tr_${messageId}`,
        supersededBy: null, notify: false,
      }
      state.finds.unshift(find)
      emitMock({ type: 'find_created', find })
    } else {
      find.status = 'open'
      find.findType = findType ?? d.find_type
      // A date the user picked replaces the one it had; no date keeps it.
      if (dueAt) find.dueAt = dueAt
      find.updatedAt = iso(new Date())
      if (!find.sources.some((s) => s.messageId === messageId)) find.sources.push({ messageId, threadId: msg.threadId, digId })
      emitMock({ type: 'find_changed', find_id: find.id })
    }
    c.findId = find.id
  } else if (before?.kind === 'find' && c.findId) {
    const find = state.finds.find((f) => f.id === c.findId)
    if (find && find.status === 'open' && find.sources.every((s) => s.messageId === messageId || state.classifications.get(s.messageId)?.disposition?.kind !== 'find')) {
      find.status = 'dismissed'
      find.resolvedAt = iso(new Date())
      find.resolvedBy = 'user'
      emitMock({ type: 'find_changed', find_id: find.id })
    }
  }
  // Keep the trace honest: the conclusion is what the user decided.
  const trace = state.traces.get(`tr_${messageId}`)
  if (trace && d) {
    trace.conclusion = d
    trace.mechanism = byUser ? 'user' : trace.mechanism
  }
  emitMock({ type: 'message_classified', message_id: messageId, dig_id: digId, disposition: d ?? { kind: 'unresolved' }, shadow: false })
  emitMock({ type: 'summary_changed' })
  return { before }
}

function bulkMove(state: MockState, args: Args, to: Disposition, verb: string): Exec {
  const digId = str(args, 'digId')
  const ids = strList(args, 'messageIds')
  const items = ids.map((id) => ({ digId, messageId: id, disposition: state.classifications.get(id)?.disposition ?? null }))
  ids.forEach((id) => setDisposition(state, digId, id, to))
  const label = ids.length === 1 ? `"${messageTitle(state, ids[0])}"` : `${ids.length} messages`
  return {
    result: { count: ids.length },
    summary: `You ${verb} ${label}${verb.includes('to') ? '' : ` to ${dispositionLabel(to)}`}`,
    affected: ids.map((id) => ({ kind: 'message', id })),
    undo: { state: 'undoable', restore: { kind: 'dispositions', items }, label: `Restore ${label}` },
    digId,
  }
}

function findById(state: MockState, id: string): Find {
  const f = state.finds.find((x) => x.id === id)
  if (!f) throw err('not_found', 'Find not found')
  return f
}

function findStatusExec(_state: MockState, f: Find, status: Find['status'], summary: string, extra: Partial<Find> = {}): Exec {
  const before = { status: f.status, snoozedUntil: f.snoozedUntil, findType: f.findType, resolvedAt: f.resolvedAt, resolvedBy: f.resolvedBy }
  Object.assign(f, { status, updatedAt: iso(new Date()) }, extra)
  emitMock({ type: 'find_changed', find_id: f.id })
  emitMock({ type: 'summary_changed' })
  return {
    result: { findId: f.id, status },
    summary,
    affected: [{ kind: 'find', id: f.id }],
    undo: { state: 'undoable', restore: { kind: 'find_state', findId: f.id, ...before }, label: `Reopen "${f.title}"` },
    digId: f.digIds[0] ?? null,
  }
}

function newMeshVersion(state: MockState, meshId: string | null, text: string, note: string | null, status: MeshStatus, createdBy: 'user' | 'director'): Mesh {
  const digIdByName = (n: string) => state.digs.find((d) => d.name.toLowerCase() === n.toLowerCase())?.id ?? null
  let parsed
  try {
    parsed = parseMeshText(text, { digIdByName })
  } catch (e) {
    if (e instanceof MeshSyntaxError) throw e.toRiddleError()
    throw e
  }
  const prev = meshId ? latestMesh(state, meshId) : null
  const version = prev ? prev.version + 1 : 1
  const mesh: Mesh = {
    id: meshId ?? nextId(state, 'mesh'),
    name: parsed.name,
    version,
    scope: parsed.scope,
    priority: parsed.priority ?? prev?.priority ?? 50,
    routes: parsed.routes,
    guards: parsed.guards,
    status,
    sourceText: text.replace(/^Mesh:\s*(.+?)\s*(?:v\d+)?$/m, `Mesh: ${parsed.name} v${version}`),
    createdAt: iso(new Date()),
    createdBy,
    changeNote: note,
    shadowStartedAt: status === 'shadow' ? iso(new Date()) : prev?.shadowStartedAt ?? null,
    fallback: prev?.fallback ?? false,
    shadowAgreed: 0,
    shadowProposed: 0,
    deletedAt: null,
    autonomy: parsed.autonomy ?? prev?.autonomy,
  }
  if (prev) for (const v of state.meshes.filter((x) => x.id === mesh.id)) v.status = status
  state.meshes.push(mesh)
  return mesh
}

function execute(state: MockState, action: string, args: Args): Exec {
  const now = iso(new Date())
  switch (action) {
    case 'archive_messages': {
      const digId = str(args, 'digId')
      const ids = strList(args, 'messageIds')
      for (const id of ids) {
        const msg = state.messages.find((x) => x.id === id)
        if (msg) msg.labels = msg.labels.filter((l) => l !== 'INBOX')
        const c = state.classifications.get(id)
        if (c) c.handledByUserAt = now
      }
      emitMock({ type: 'messages_changed', dig_id: digId, count: u64(ids.length) })
      return {
        result: { count: ids.length },
        summary: `You archived ${ids.length === 1 ? `"${messageTitle(state, ids[0])}"` : `${ids.length} messages`}`,
        affected: ids.map((id) => ({ kind: 'message', id })),
        undo: { state: 'undoable', restore: { kind: 'labels_add', digId, messageIds: ids, labels: ['INBOX'] }, label: 'Move back to the inbox' },
        digId,
      }
    }
    case 'move_to_spoil':
      return bulkMove(state, args, { kind: 'spoil' }, 'moved')
    case 'mark_contamination':
      return bulkMove(state, args, { kind: 'contamination' }, 'marked')
    case 'restore_messages': {
      const ex = bulkMove(state, args, { kind: 'unresolved' }, 'restored')
      ex.why = 'Recorded as a correction: restore.'
      ex.summary = ex.summary.replace(' to Unresolved', ' to Unresolved for a decision')
      return ex
    }
    case 'set_disposition': {
      const digId = str(args, 'digId')
      const messageId = str(args, 'messageId')
      const d = args.disposition as Disposition
      if (!d || typeof d !== 'object') throw err('invalid', 'invalid arguments: disposition')
      const findType = (args.findType as FindType | undefined) ?? (d.kind === 'find' ? d.find_type : undefined)
      const dueAt = typeof args.dueAt === 'string' ? args.dueAt : null
      if (dueAt && !(new Date(dueAt).getTime() > Date.now())) throw err('invalid', 'dueAt must be in the future')
      if (dueAt && d.kind !== 'find') throw err('invalid', 'dueAt applies to a Find')
      const { before } = setDisposition(state, digId, messageId, d.kind === 'find' ? { kind: 'find', find_type: findType ?? d.find_type } : d, true, findType, dueAt)
      const followUp = d.kind === 'find' && (findType ?? d.find_type) === 'follow_up'
      return {
        result: { messageId, disposition: d },
        summary: followUp
          ? `You marked "${messageTitle(state, messageId)}" for follow-up${dueAt ? ` by ${new Date(dueAt).toLocaleDateString([], { weekday: 'short', month: 'short', day: 'numeric' })}` : ''}`
          : `You changed "${messageTitle(state, messageId)}" from ${dispositionLabel(before)} to ${dispositionLabel(d)}`,
        why: 'Recorded as a correction. Repeated corrections of the same kind lead to a Mesh proposal.',
        affected: [{ kind: 'message', id: messageId }],
        undo: { state: 'undoable', restore: { kind: 'dispositions', items: [{ digId, messageId, disposition: before }] }, label: `Put it back to ${dispositionLabel(before)} and retract the correction` },
        provenance: { mechanism: 'user', meshId: null, meshVersion: null, meshName: null, traceId: `tr_${messageId}`, note: `Correction recorded: move (${dispositionLabel(before)} → ${dispositionLabel(d)}).` },
        digId,
      }
    }
    case 'set_importance': {
      const digId = str(args, 'digId')
      const messageId = str(args, 'messageId')
      const level = args.importance as Importance | null | undefined
      if (level != null && !['low', 'normal', 'high', 'critical'].includes(level)) throw err('invalid', 'importance is low, normal, high, critical or null')
      const prior = state.importance.get(messageId) ?? null
      if (level == null) state.importance.delete(messageId)
      else state.importance.set(messageId, level)
      emitMock({ type: 'messages_changed', dig_id: digId, count: u64(1) })
      return {
        result: { importance: level ?? 'normal' },
        summary: level ? `You set "${messageTitle(state, messageId)}" to ${level} importance` : `You cleared your importance for "${messageTitle(state, messageId)}"`,
        affected: [{ kind: 'message', id: messageId }],
        undo: { state: 'undoable', restore: { kind: 'importance', messageId, prior }, label: 'Put the importance back' },
        digId,
      }
    }
    case 'label_messages': {
      const digId = str(args, 'digId')
      const ids = strList(args, 'messageIds')
      const strata = strList(args, 'strata')
      for (const id of ids) {
        const c = state.classifications.get(id)
        if (c) c.strata = [...new Set([...c.strata, ...strata])]
      }
      for (const name of strata) {
        const st = state.strata.find((s) => s.name === name)
        if (st) st.messageCount = u64(Number(st.messageCount) + ids.length)
      }
      emitMock({ type: 'messages_changed', dig_id: digId, count: u64(ids.length) })
      return {
        result: { count: ids.length },
        summary: `You added Stratum: ${strata.join(', ')} to ${ids.length === 1 ? `"${messageTitle(state, ids[0])}"` : `${ids.length} messages`}`,
        affected: ids.map((id) => ({ kind: 'message', id })),
        undo: { state: 'undoable', restore: { kind: 'strata_remove', digId, messageIds: ids, strata }, label: `Remove ${strata.join(', ')}` },
        digId,
      }
    }
    case 'snooze_messages': {
      const digId = str(args, 'digId')
      const ids = strList(args, 'messageIds')
      for (const id of ids) {
        const msg = state.messages.find((x) => x.id === id)
        if (msg && !msg.labels.includes('SNOOZED')) msg.labels.push('SNOOZED')
      }
      emitMock({ type: 'messages_changed', dig_id: digId, count: u64(ids.length) })
      return { result: { count: ids.length, until: args.until }, summary: `You snoozed ${plural(ids.length, 'message')} until ${String(args.until)}`, affected: ids.map((id) => ({ kind: 'message', id })), undo: { state: 'undoable', restore: { kind: 'labels_remove', digId, messageIds: ids, labels: ['SNOOZED'] }, label: 'Unsnooze' }, digId }
    }
    case 'permanent_delete': {
      const digId = str(args, 'digId')
      const ids = strList(args, 'messageIds')
      state.messages = state.messages.filter((x) => !ids.includes(x.id))
      ids.forEach((id) => state.classifications.delete(id))
      emitMock({ type: 'messages_changed', dig_id: digId, count: u64(ids.length) })
      emitMock({ type: 'summary_changed' })
      return { result: { count: ids.length }, summary: `You permanently deleted ${ids.length} messages`, affected: ids.map((id) => ({ kind: 'message', id })), undo: { state: 'not_undoable', reason: 'permanently deleted; there is nothing to restore' }, digId }
    }
    case 'resolve_find': {
      const f = findById(state, str(args, 'findId'))
      return findStatusExec(state, f, 'resolved', `You resolved "${f.title}"`, { resolvedAt: now, resolvedBy: 'user' })
    }
    case 'dismiss_find': {
      const f = findById(state, str(args, 'findId'))
      const ex = findStatusExec(state, f, 'dismissed', `You dismissed "${f.title}": not important`, { resolvedAt: now, resolvedBy: 'user' })
      ex.why = 'Recorded as a correction: dismiss.'
      return ex
    }
    case 'snooze_find': {
      const f = findById(state, str(args, 'findId'))
      const until = str(args, 'until')
      return findStatusExec(state, f, 'snoozed', `You snoozed "${f.title}" until ${new Date(until).toLocaleString([], { weekday: 'short', hour: 'numeric', minute: '2-digit' })}`, { snoozedUntil: until })
    }
    case 'retype_find': {
      const f = findById(state, str(args, 'findId'))
      const t = str(args, 'findType') as FindType
      const from = f.findType
      const ex = findStatusExec(state, f, f.status, `You changed "${f.title}" from ${from} to ${t}`, { findType: t })
      ex.why = 'Recorded as a correction: retype.'
      ex.undo = { state: 'undoable', restore: { kind: 'find_state', findId: f.id, status: f.status, snoozedUntil: f.snoozedUntil, findType: from, resolvedAt: f.resolvedAt, resolvedBy: f.resolvedBy }, label: `Back to ${from}` }
      return ex
    }
    case 'undo_activity':
      return undoActivity(state, str(args, 'activityId'))
    case 'create_mesh': {
      const text = str(args, 'text')
      const status = (args.status as MeshStatus | undefined) ?? 'draft'
      const mesh = newMeshVersion(state, null, text, (args.changeNote as string | null) ?? 'Created.', status, 'user')
      const report = simulate(state, { text })
      return {
        result: { mesh, report } satisfies { mesh: Mesh; report: SimulationReport },
        summary: `You created Mesh: ${mesh.name} v1 (${status})`,
        affected: [{ kind: 'mesh', id: mesh.id }],
        undo: { state: 'undoable', restore: { kind: 'mesh_deleted', meshId: mesh.id }, label: 'Move the Mesh to Recently Deleted' },
        provenance: { mechanism: 'user', meshId: mesh.id, meshVersion: 1, meshName: mesh.name, traceId: null, note: `Simulated against ${Number(report.historicalMessages)} messages: ${Number(report.changed)} would change.` },
      }
    }
    case 'update_mesh': {
      const meshId = str(args, 'meshId')
      const text = str(args, 'text')
      const prev = latestMesh(state, meshId)
      if (!prev) throw err('not_found', 'Mesh not found')
      const report = simulate(state, { text, meshId })
      // A guard-only Mesh (the user's own sender rules) keeps Active: guards veto and never hide mail.
      const guardOnly = /^\s*GUARDS\s*$/m.test(text) && !/^\s*ROUTES\s*$/m.test(text)
      const mesh = newMeshVersion(state, meshId, text, (args.changeNote as string | null) ?? null, prev.status === 'active' && !guardOnly ? 'shadow' : prev.status, 'user')
      return {
        result: { mesh, report } satisfies { mesh: Mesh; report: SimulationReport },
        summary: `You updated Mesh: ${mesh.name} v${prev.version} → v${mesh.version}`,
        why: mesh.status === 'shadow' && prev.status === 'active' ? 'A changed Mesh starts by suggesting; it acts again after a Shadow Period.' : null,
        affected: [{ kind: 'mesh', id: mesh.id }],
        undo: { state: 'undoable', restore: { kind: 'mesh_version', meshId: mesh.id, version: prev.version, status: prev.status }, label: `Go back to v${prev.version}` },
        provenance: { mechanism: 'user', meshId: mesh.id, meshVersion: mesh.version, meshName: mesh.name, traceId: null, note: `Simulated against ${Number(report.historicalMessages)} messages before applying: ${Number(report.changed)} changed, ${Number(report.newJudgeCallsRequired)} new judge calls.` },
      }
    }
    case 'set_mesh_status': {
      const meshId = str(args, 'meshId')
      const status = str(args, 'status') as MeshStatus
      const prev = latestMesh(state, meshId)
      if (!prev) throw err('not_found', 'Mesh not found')
      const from = prev.status
      for (const v of state.meshes.filter((x) => x.id === meshId)) {
        v.status = status
        if (status === 'shadow' && !v.shadowStartedAt) v.shadowStartedAt = now
      }
      return { result: { meshId, status }, summary: `You set Mesh: ${prev.name} from ${from} to ${status}`, affected: [{ kind: 'mesh', id: meshId }], undo: { state: 'undoable', restore: { kind: 'mesh_status', meshId, status: from }, label: `Back to ${from}` } }
    }
    case 'grade_decision': {
      state.grades[`${str(args, 'meshId')}:${str(args, 'messageId')}`] = str(args, 'verdict')
      return { result: { verdict: args.verdict }, summary: `Marked ${String(args.verdict)}`, affected: [], undo: { state: 'not_undoable', reason: 'grade it again' } }
    }
    case 'save_list': {
      const name = str(args, 'name')
      const entries = ((args.entries as string[] | undefined) ?? []).map((e) => e.trim().replace(/^@/, '').toLowerCase()).filter(Boolean)
      const i = state.namedLists.findIndex((l) => l.name === name)
      const list = { name, description: (args.description as string | undefined) ?? state.namedLists[i]?.description ?? '', entries: [...new Set(entries)], pack: state.namedLists[i]?.pack ?? null }
      if (i >= 0) state.namedLists[i] = list
      else state.namedLists.push(list)
      return { result: list, summary: `Saved list:${name} (${list.entries.length} entries)`, affected: [], undo: { state: 'not_undoable', reason: 'edit the list again' } }
    }
    case 'install_pack': {
      const id = str(args, 'packId')
      const lists = (args.lists as Record<string, string[]> | undefined) ?? {}
      for (const [name, entries] of Object.entries(lists)) {
        const existing = state.namedLists.find((l) => l.name === name)
        const merged = [...new Set([...(existing?.entries ?? []), ...entries.map((e) => e.trim().replace(/^@/, '').toLowerCase()).filter(Boolean)])]
        if (existing) existing.entries = merged
        else state.namedLists.push({ name, description: '', entries: merged, pack: id })
      }
      state.installedPacks = [...state.installedPacks.filter((p) => p.id !== id), { id, version: 1, meshes: {}, installedAt: new Date().toISOString() }]
      return { result: { packId: id }, summary: `Installed ${id}`, affected: [], undo: { state: 'not_undoable', reason: 'remove the pack' } }
    }
    case 'remove_pack': {
      const id = str(args, 'packId')
      state.installedPacks = state.installedPacks.filter((p) => p.id !== id)
      return { result: { packId: id }, summary: `Removed pack ${id}`, affected: [], undo: { state: 'not_undoable', reason: 'install it again' } }
    }
    case 'export_pack':
      return { result: { packId: str(args, 'packId'), fileName: `${str(args, 'packId')}.riddlepack.toml`, text: `id = "${str(args, 'packId')}"\n` }, summary: 'Pack file', affected: [], undo: { state: 'not_undoable', reason: 'nothing changed' } }
    case 'import_pack':
      return { result: { packId: 'imported' }, summary: 'Added the pack to the catalogue', affected: [], undo: { state: 'not_undoable', reason: 'remove it' } }
    case 'remember_for_sender': {
      const scope = str(args, 'scope') === 'domain' ? 'domain' : 'sender'
      const summary = `From now on, ${scope === 'domain' ? 'everything from this domain' : 'this sender'} is decided the way you decided this message`
      return { result: { scope }, summary, affected: [], undo: { state: 'not_undoable', reason: 'edit Your corrections' } }
    }
    case 'set_mesh_fallback': {
      const meshId = str(args, 'meshId')
      const prev = latestMesh(state, meshId)
      if (!prev) throw err('not_found', 'Mesh not found')
      prev.fallback = Boolean(args.fallback)
      return { result: prev, summary: prev.fallback ? `Mesh ${prev.name} is a fallback` : `Mesh ${prev.name} is a specific rule`, affected: [{ kind: 'mesh', id: meshId }], undo: { state: 'not_undoable', reason: 'mark it again' } }
    }
    case 'delete_mesh': {
      const meshId = str(args, 'meshId')
      const prev = latestMesh(state, meshId)
      if (!prev) throw err('not_found', 'Mesh not found')
      for (const v of state.meshes.filter((x) => x.id === meshId)) {
        v.deletedAt = now
        v.status = 'deleted'
      }
      return { result: { meshId }, summary: `You moved Mesh: ${prev.name} to Recently Deleted`, affected: [{ kind: 'mesh', id: meshId }], undo: { state: 'undoable', restore: { kind: 'mesh_restore', meshId }, label: 'Restore the Mesh' } }
    }
    case 'restore_mesh': {
      const meshId = str(args, 'meshId')
      const prev = latestMesh(state, meshId)
      if (!prev) throw err('not_found', 'Mesh not found')
      if (state.meshes.some((x) => x.id !== meshId && !x.deletedAt && x.name === prev.name)) throw err('conflict', `a Mesh named "${prev.name}" already exists; rename it first`)
      for (const v of state.meshes.filter((x) => x.id === meshId)) {
        v.deletedAt = null
        v.status = 'disabled'
      }
      return { result: { meshId }, summary: `You restored Mesh: ${prev.name} (disabled)`, affected: [{ kind: 'mesh', id: meshId }], undo: { state: 'undoable', restore: { kind: 'mesh_deleted', meshId }, label: 'Delete again' } }
    }
    case 'propose_signal': {
      const sig = args.signal as Partial<Signal>
      if (!sig?.name || !sig.definition) throw err('invalid', 'a Signal needs a name and a definition')
      if (!/^[a-z][a-z0-9_]*$/.test(sig.name)) throw err('invalid', 'Signal names are snake_case: letters, digits and underscores')
      const existing = state.signals.filter((s) => s.name === sig.name)
      const version = existing.length ? Math.max(...existing.map((s) => s.version)) + 1 : 1
      const s: Signal = {
        id: existing[0]?.id ?? `sig_${sig.name}`,
        name: sig.name,
        version,
        primitive: sig.primitive ?? 'noul',
        definition: sig.definition,
        choices: sig.choices?.length ? sig.choices : [{ name: 'YES', definition: `Yes: ${sig.definition}` }, { name: 'NO', definition: 'No.' }],
        inputs: sig.inputs ?? { message: true, thread: false, contextSlices: [] },
        languageScope: [],
        status: 'proposed',
        calibration: null,
        createdAt: now,
        createdBy: 'user',
        changeNote: sig.changeNote ?? (version === 1 ? 'Initial definition.' : 'Proposed revision.'),
      }
      state.signals.push(s)
      return { result: s, summary: `You proposed Signal: ${s.name} v${version}`, affected: [{ kind: 'signal', id: s.id }], undo: { state: 'not_undoable', reason: 'reject the proposal instead' } }
    }
    case 'test_signal': {
      const name = str(args, 'name')
      const version = (args.version as number | undefined) ?? Math.max(...state.signals.filter((s) => s.name === name).map((s) => s.version), 1)
      const sampleSize = (args.sampleSize as number | undefined) ?? 40
      const samples = [...state.messages].filter((mm) => state.traces.get(`tr_${mm.id}`)?.signals.some((e) => e.signalName === name)).slice(0, Math.min(sampleSize, 12)).map((mm) => {
        const ev = state.traces.get(`tr_${mm.id}`)!.signals.find((e) => e.signalName === name)!
        const drift = version > ev.signalVersion ? 0.04 : version < ev.signalVersion ? -0.05 : 0
        const p = Math.min(0.99, Math.max(0.01, (ev.distribution.YES ?? ev.confidence) + drift))
        const got = p >= 0.5 ? 'YES' : 'NO'
        return { messageId: mm.id, digId: mm.digId, expected: ev.selected, got, confidence: Math.max(p, 1 - p) }
      })
      const agree = samples.filter((s) => s.expected === s.got).length
      const report: SignalTestReport = {
        name,
        version,
        sampleSize: samples.length,
        agreement: samples.length ? agree / samples.length : 0,
        falsePositives: samples.filter((s) => s.expected === 'NO' && s.got === 'YES').length,
        falseNegatives: samples.filter((s) => s.expected === 'YES' && s.got === 'NO').length,
        uncertain: samples.filter((s) => s.confidence < 0.7).length,
        latencyMs: 172,
        costUsd: Math.round(samples.length * 1900 * 0.042) / 1_000_000,
        distribution: { YES: samples.filter((s) => s.got === 'YES').length, NO: samples.filter((s) => s.got === 'NO').length },
        samples,
      }
      return { result: report, summary: `Tested ${name} v${version} on ${samples.length} messages`, silent: true }
    }
    case 'promote_signal': {
      const name = str(args, 'name')
      const version = Number(args.version)
      const target = state.signals.find((s) => s.name === name && s.version === version)
      if (!target) throw err('not_found', 'Signal version not found')
      const prevActive = state.signals.find((s) => s.name === name && s.status === 'active')
      for (const s of state.signals.filter((x) => x.name === name)) if (s.version !== version && s.status === 'active') s.status = 'deprecated'
      target.status = 'active'
      target.calibration = target.calibration ?? { method: 'platt', labelledCount: 400, engineIdentity: 'jev:jev-1.13.0', calibratedAt: now, plattA: 1.1, plattB: -0.05, heldOutAgreement: 0.94 }
      const users = state.meshes.filter((mm) => !mm.deletedAt && mm.routes.some((r) => JSON.stringify(r.condition).includes(`"signal":"${name}"`)))
      if (users.length) emitMock({ type: 'proposal_created', pending_id: nextId(state, 'pend'), summary: `Re-simulate ${users.map((u) => u.name).join(', ')} against ${name} v${version}` })
      return { result: target, summary: `You promoted Signal: ${name} v${version} to active`, why: users.length ? `${plural(users.length, 'Mesh pins', 'Meshes pin')} an older version; re-simulation proposed.` : null, affected: [{ kind: 'signal', id: target.id }], undo: { state: 'undoable', restore: { kind: 'signal_status', name, version, prevActive: prevActive?.version ?? null }, label: 'Demote again' } }
    }
    case 'evaluate_signal': {
      const digId = str(args, 'digId')
      const messageId = str(args, 'messageId')
      const name = str(args, 'name')
      const version = Number(args.version ?? 1)
      const msg = state.messages.find((x) => x.id === messageId && x.digId === digId)
      if (!msg) throw err('not_found', 'message not found')
      const cached = state.traces.get(`tr_${messageId}`)?.signals.find((e) => e.signalName === name)
      const p = cached ? (cached.distribution.YES ?? cached.confidence) : 0.3 + ((msg.subject.length * 7 + name.length) % 60) / 100
      const sig = state.signals.find((s) => s.name === name && s.version === version)
      const distribution = sig?.primitive === 'choice' ? { NONE: Math.round((1 - p) * 100) / 100, SOFT: Math.round(p * 0.6 * 100) / 100, HARD: Math.round(p * 0.4 * 100) / 100 } : { YES: Math.round(p * 100) / 100, NO: Math.round((1 - p) * 100) / 100 }
      const selected = Object.entries(distribution).sort((a, b) => b[1] - a[1])[0][0]
      const ev = { messageId, signalName: name, signalVersion: version, engineIdentity: 'jev:jev-1.13.0', selected, distribution, calibrated: distribution, confidence: Math.max(...Object.values(distribution)), latencyMs: u64(150 + (name.length * 13) % 80), inputTokens: u64(1800 + msg.bodyText!.length), cached: false, deterministic: false, contextHash: `ctx_${messageId.slice(-6)}`, requestHash: `req_${name}_${version}_${messageId.slice(-4)}`, evaluatedAt: now }
      state.logs.push({ at: now, level: 'debug', category: 'judge', message: `playground: jev request 1 question, ${Number(ev.inputTokens)} input tokens, ${Number(ev.latencyMs)}ms`, payload: { experimental: true, signal: `${name}@v${version}` } })
      return { result: ev, summary: `Evaluated ${name} v${version}`, silent: true }
    }
    case 'rename_stratum': {
      const st = state.strata.find((s) => s.id === args.stratumId)
      if (!st) throw err('not_found', 'Stratum not found')
      const name = str(args, 'name')
      const from = st.name
      if (state.strata.some((s) => s.id !== st.id && !s.deletedAt && s.name.toLowerCase() === name.toLowerCase())) throw err('conflict', `a Stratum named "${name}" already exists`)
      st.name = name
      st.version += 1
      for (const c of state.classifications.values()) c.strata = c.strata.map((x) => (x === from ? name : x))
      return { result: st, summary: `You renamed Stratum: ${from} to ${name}`, affected: [{ kind: 'stratum', id: st.id }], undo: { state: 'undoable', restore: { kind: 'stratum_name', stratumId: st.id, name: from }, label: `Rename back to ${from}` } }
    }
    case 'merge_strata': {
      const from = state.strata.find((s) => s.id === args.fromId)
      const into = state.strata.find((s) => s.id === args.intoId)
      if (!from || !into) throw err('not_found', 'Stratum not found')
      for (const c of state.classifications.values()) if (c.strata.includes(from.name)) c.strata = [...new Set(c.strata.filter((x) => x !== from.name).concat(into.name))]
      into.messageCount = u64(Number(into.messageCount) + Number(from.messageCount))
      into.version += 1
      from.deletedAt = now
      return { result: into, summary: `You merged Stratum: ${from.name} into ${into.name}`, affected: [{ kind: 'stratum', id: from.id }, { kind: 'stratum', id: into.id }], undo: { state: 'undoable', restore: { kind: 'stratum_unmerge', fromId: from.id, intoId: into.id, count: Number(from.messageCount) }, label: `Split ${from.name} out again` } }
    }
    case 'create_stratum': {
      const name = str(args, 'name')
      const clash = state.strata.find((s) => !s.deletedAt && s.name.toLowerCase() === name.toLowerCase())
      if (clash) throw err('conflict', `a Stratum named "${clash.name}" already exists`)
      const st: Stratum = { id: nextId(state, 'st'), name, version: 1, scopeGroup: (args.scopeGroup as string | null) ?? null, description: (args.description as string | undefined) ?? '', proposedByRiddle: false, messageCount: u64(0), createdAt: now, deletedAt: null }
      state.strata.push(st)
      return { result: st, summary: `You created Stratum: ${name}`, affected: [{ kind: 'stratum', id: st.id }], undo: { state: 'undoable', restore: { kind: 'stratum_deleted', stratumId: st.id }, label: 'Delete it' } }
    }
    case 'create_draft': {
      const digId = str(args, 'digId')
      const d = args.draft as Partial<Draft>
      const draft: Draft = { id: nextId(state, 'draft'), digId, providerId: null, threadId: d.threadId ?? null, inReplyTo: d.inReplyTo ?? null, to: d.to ?? [], cc: d.cc ?? [], bcc: d.bcc ?? [], subject: d.subject ?? '', bodyText: d.bodyText ?? '', updatedAt: now }
      state.drafts.push(draft)
      return { result: draft, summary: 'Draft created', silent: true }
    }
    case 'update_draft': {
      const d = args.draft as Draft
      const idx = state.drafts.findIndex((x) => x.id === d.id)
      if (idx < 0) throw err('not_found', 'draft not found')
      state.drafts[idx] = { ...state.drafts[idx], ...d, updatedAt: now }
      return { result: state.drafts[idx], summary: 'Draft saved', silent: true }
    }
    case 'delete_draft': {
      const id = str(args, 'draftId')
      const d = state.drafts.find((x) => x.id === id)
      if (!d) throw err('not_found', 'draft not found')
      state.drafts = state.drafts.filter((x) => x.id !== id)
      return { result: { draftId: id }, summary: `You deleted the draft "${d.subject || '(no subject)'}"`, affected: [{ kind: 'draft', id }], undo: { state: 'undoable', restore: { kind: 'draft_restore', draft: d }, label: 'Restore the draft' } }
    }
    case 'send_message': {
      const digId = str(args, 'digId')
      const draft = state.drafts.find((x) => x.id === args.draftId)
      if (!draft) throw err('not_found', 'draft not found')
      state.drafts = state.drafts.filter((x) => x.id !== draft.id)
      const spec: MessageSpec = {
        id: nextId(state, 'm_sent'),
        dig: digId,
        thread: draft.threadId ?? undefined,
        from: ME[digId],
        to: draft.to,
        cc: draft.cc,
        subject: draft.subject,
        text: draft.bodyText,
        hoursAgo: 0,
        labels: ['SENT'],
        sent: true,
        disposition: { kind: 'filed' },
        strata: [],
        facts: { isSent: true },
        signals: { commitment_made: /\bwill\b|\bby (monday|tuesday|wednesday|thursday|friday|tomorrow)\b/i.test(draft.bodyText) ? 0.8 : 0.1 },
        mechanism: 'deterministic',
        reason: 'Sent mail. Thread State updated.',
      }
      const msg = buildMessage(spec)
      state.messages.unshift(msg)
      state.specs.set(msg.id, spec)
      state.classifications.set(msg.id, { disposition: spec.disposition, strata: [], findId: null, handledByUserAt: now, shadow: false })
      const tr = buildTrace(spec, msg, state.signals, state.meshes)
      if (tr) state.traces.set(tr.id, tr)
      // A reply resolves the Find whose thread this is (README "Finds").
      if (draft.threadId) {
        for (const f of state.finds) {
          if (f.status === 'open' && f.sources.some((s) => s.threadId === draft.threadId)) {
            f.status = 'resolved'
            f.resolvedAt = now
            f.resolvedBy = 'thread_state'
            emitMock({ type: 'find_changed', find_id: f.id })
          }
        }
      }
      emitMock({ type: 'messages_changed', dig_id: digId, count: u64(1) })
      emitMock({ type: 'summary_changed' })
      return { result: { messageId: msg.id }, summary: `You sent "${draft.subject || '(no subject)'}" to ${draft.to.map((a) => a.name || a.email).join(', ')}`, affected: [{ kind: 'message', id: msg.id }], undo: { state: 'not_undoable', reason: 'sent mail cannot be unsent' }, digId }
    }
    case 'start_expedition': {
      const goal = str(args, 'goal')
      const id = nextId(state, 'exp')
      const scope = (args.scope as { digIds: string[]; since: string | null; until: string | null; toolsAllowed: string[] } | undefined) ?? { digIds: state.digs.map((d) => d.id), since: null, until: null, toolsAllowed: ['search', 'get_thread'] }
      const budget = (args.budget as { costUsd: number; seconds: number; toolCalls: number } | undefined) ?? { costUsd: 1, seconds: 900, toolCalls: 60 }
      const exp = {
        id,
        goal,
        scope,
        budget: { costUsd: budget.costUsd, seconds: u64(budget.seconds), toolCalls: budget.toolCalls },
        spent: { costUsd: 0, seconds: u64(0), toolCalls: 0 },
        status: 'planned' as const,
        plan: [{ n: 1, text: 'Search the Digs in scope for relevant threads.', done: false }, { n: 2, text: 'Read the candidates and extract evidence.', done: false }, { n: 3, text: 'Report findings as Finds.', done: false }],
        checkpoint: null,
        evidence: [],
        findings: [],
        error: null,
        createdAt: now,
        updatedAt: now,
        finishedAt: null,
      }
      state.expeditions.unshift(exp)
      state.jobs.unshift({ id: `job_${id}`, kind: 'expedition', priority: 'interactive', status: { state: 'queued' }, digId: null, label: `Expedition: ${goal.slice(0, 40)}`, progress: { done: u64(0), total: u64(3), unit: 'steps', step: 'Queued', etaSeconds: null }, checkpoint: null, cancellable: true, attempts: 0, createdAt: now, startedAt: null, finishedAt: null, updatedAt: now })
      emitMock({ type: 'expedition_changed', expedition_id: id })
      emitMock({ type: 'job_changed', job: state.jobs[0] })
      scheduleExpedition(state, id)
      return { result: exp, summary: `You started an Expedition: "${goal}"`, affected: [{ kind: 'expedition', id }], undo: { state: 'not_undoable', reason: 'cancel the Expedition instead' } }
    }
    case 'cancel_expedition': {
      const id = str(args, 'expeditionId')
      const exp = state.expeditions.find((e) => e.id === id)
      if (!exp) throw err('not_found', 'Expedition not found')
      exp.status = 'cancelled'
      exp.finishedAt = now
      exp.updatedAt = now
      const job = state.jobs.find((j) => j.kind === 'expedition' && (j.id === `job_${id}` || (id === 'exp_subs' && j.id === 'job_expedition_subs')))
      if (job) {
        job.status = { state: 'cancelled' }
        job.finishedAt = now
        emitMock({ type: 'job_changed', job })
      }
      emitMock({ type: 'expedition_changed', expedition_id: id })
      return { result: { expeditionId: id }, summary: `You cancelled the Expedition "${exp.goal.slice(0, 60)}"`, affected: [{ kind: 'expedition', id }], undo: { state: 'not_undoable', reason: 'a cancelled Expedition keeps its checkpoint; start a new one to continue' } }
    }
    case 'review_shadow': {
      const digId = str(args, 'digId')
      const messageId = str(args, 'messageId')
      const msg = state.messages.find((x) => x.id === messageId && x.digId === digId)
      const cls = state.classifications.get(messageId)
      if (!msg || !(cls?.shadow || state.shadowProposals.has(messageId))) throw err('invalid', 'this message has no Shadow proposal to review')
      const agree = args.agree !== false
      state.shadowReviews.set(messageId, agree ? 'agreed' : 'disagreed')
      const proposed = state.shadowProposals.get(messageId) ?? cls?.disposition
      if (!proposed) throw err('invalid', 'this message has no Shadow proposal to review')
      const mesh = state.meshes.filter((mm) => mm.id === state.specs.get(messageId)?.meshId).sort((a, b) => b.version - a.version)[0]
      if (mesh) {
        if (agree) mesh.shadowAgreed = Math.min(mesh.shadowProposed, mesh.shadowAgreed + 1)
        else mesh.shadowProposed = Math.max(1, mesh.shadowProposed)
      }
      if (agree) {
        return { result: { review: 'agreed', proposed }, summary: `Agreed with the Shadow proposal: ${dispositionLabel(proposed)}`, affected: [{ kind: 'message', id: messageId }], undo: { state: 'not_undoable', reason: 'agreement is a count the Mesh earns; disagree from the same row to record the opposite' }, digId }
      }
      const to = args.disposition as Disposition
      const current = cls ?? { disposition: proposed, strata: [], findId: null, handledByUserAt: null, shadow: true }
      const before = current.disposition
      // The proposal stays on the review list beside the decision, as the backend's shadow row does.
      state.shadowProposals.set(messageId, proposed)
      state.classifications.set(messageId, { ...current, disposition: to, shadow: false, handledByUserAt: now })
      emitMock({ type: 'message_classified', dig_id: digId, message_id: messageId, disposition: to, shadow: false })
      return {
        result: { review: 'disagreed' },
        summary: `Disagreed with the Shadow proposal (${dispositionLabel(proposed)}): ${dispositionLabel(to)} instead`,
        affected: [{ kind: 'message', id: messageId }],
        undo: { state: 'undoable', restore: { kind: 'dispositions', items: [{ digId, messageId, disposition: before }] }, label: 'Undo' },
        digId,
      }
    }
    case 'decide_again': {
      const digId = str(args, 'digId')
      const ids = strList(args, 'messageIds')
      const closed: string[] = []
      let count = 0
      for (const id of ids) {
        const cls = state.classifications.get(id)
        if (!cls || cls.handledByUserAt) continue
        count += 1
        for (const f of state.finds) {
          if ((f.status === 'open' || f.status === 'snoozed') && f.sources.some((s) => s.messageId === id)) {
            f.status = 'superseded'
            f.updatedAt = now
            closed.push(f.id)
          }
        }
      }
      return { result: { count, keptYours: ids.length - count, findsSuperseded: closed.length }, summary: `Sent ${plural(count, 'message')} back to Sift`, affected: ids.map((id) => ({ kind: 'message' as const, id })), undo: { state: 'undoable', restore: { kind: 'finds_reopen', findIds: closed }, label: 'Put the previous decisions back' }, digId }
    }
    case 'apply_mesh_to_history': {
      const { mesh, report } = applyPlan(state, args)
      return { result: { count: Number(report.changed), keptYours: 0, findsSuperseded: 0 }, summary: `Sent ${Number(report.changed)} messages back to Sift under Mesh ${mesh.name} v${mesh.version}`, affected: [{ kind: 'mesh' as const, id: mesh.id }], undo: { state: 'undoable', restore: { kind: 'finds_reopen', findIds: [] }, label: 'Put the previous decisions back' } }
    }
    case 'resift': {
      const digs = args.digId ? state.digs.filter((d) => d.id === args.digId) : state.digs
      const ids = new Set(digs.map((d) => d.id))
      const count = state.messages.filter((m) => ids.has(m.digId) && state.classifications.get(m.id)?.handledByUserAt === null).length
      return { result: { digs: digs.length, messages: count }, summary: `Sifting ${plural(count, 'message')} again across ${plural(digs.length, 'account')}`, affected: digs.map((d) => ({ kind: 'dig' as const, id: d.id })), undo: { state: 'not_undoable', reason: 'the earlier decisions stay as history; correct any message from its row' } }
    }
    case 'dismiss_pending': {
      const id = str(args, 'pendingId')
      const p = state.pending.get(id)
      if (!p) throw err('not_found', 'that proposal was already applied, declined or expired')
      state.pending.delete(id)
      const reason = typeof args.reason === 'string' && args.reason.trim() ? args.reason.trim() : null
      return { result: { pendingId: id }, summary: `Declined: ${p.summary}`, why: reason, inputs: { pendingId: id, action: p.action }, undo: { state: 'not_undoable', reason: 'the Director can propose it again' } }
    }
    case 'add_never_hidden': {
      const entry = str(args, 'entry').trim().toLowerCase()
      if (!state.profile.neverHidden.includes(entry)) state.profile.neverHidden.push(entry)
      emitMock({ type: 'settings_changed' })
      return { result: state.profile.neverHidden, summary: `You added ${entry} to Never Hidden`, affected: [{ kind: 'person', id: entry }], undo: { state: 'undoable', restore: { kind: 'never_hidden_remove', entry }, label: 'Remove from Never Hidden' } }
    }
    case 'remove_never_hidden': {
      const entry = str(args, 'entry').trim().toLowerCase()
      state.profile.neverHidden = state.profile.neverHidden.filter((x) => x !== entry)
      emitMock({ type: 'settings_changed' })
      return { result: state.profile.neverHidden, summary: `You removed ${entry} from Never Hidden`, affected: [{ kind: 'person', id: entry }], undo: { state: 'undoable', restore: { kind: 'never_hidden_add', entry }, label: 'Add back to Never Hidden' } }
    }
    case 'save_user_profile': {
      state.profile = { ...state.profile, ...(args.profile as object) }
      emitMock({ type: 'settings_changed' })
      return { result: state.profile, summary: 'You updated your User Profile', undo: { state: 'not_undoable', reason: 'edit it again' } }
    }
    case 'import_gmail_client': {
      let installed: { client_id?: string; client_secret?: string } | undefined
      try {
        installed = (JSON.parse(str(args, 'text')) as { installed?: typeof installed }).installed
      } catch {
        throw err('invalid', 'that file is not the OAuth client JSON Google Auth Platform downloads')
      }
      const clientId = installed?.client_id?.trim() ?? ''
      if (!clientId.endsWith('.apps.googleusercontent.com')) throw err('invalid', 'that file has no Desktop app client in it')
      const clientSecretStored = !!installed?.client_secret?.trim()
      state.settings = { ...state.settings, gmail: { clientId, clientSecretPresent: clientSecretStored } }
      emitMock({ type: 'settings_changed' })
      const projectId = (installed as { project_id?: string } | undefined)?.project_id ?? null
      return { result: { clientId, projectId, clientSecretStored, keychainService: 'riddle-dev' }, summary: 'You imported the Gmail client', inputs: { clientId, clientSecret: clientSecretStored }, undo: { state: 'not_undoable', reason: 'the previous client is not kept' } }
    }
    case 'set_secret':
    case 'clear_secret': {
      const secret = str(args, 'secret')
      const present = action === 'set_secret'
      if (secret === 'jev_api_key') state.settings = { ...state.settings, judge: { ...state.settings.judge, apiKeyPresent: present } }
      else if (secret === 'gmail_client_secret') state.settings = { ...state.settings, gmail: { ...state.settings.gmail, clientSecretPresent: present } }
      else throw err('invalid', `${JSON.stringify(secret)} is not a secret Settings can store`)
      emitMock({ type: 'settings_changed' })
      return { result: { secret }, summary: present ? `You stored ${secret} in the keychain` : `You removed ${secret} from the keychain`, inputs: { secret }, undo: { state: 'not_undoable', reason: 'the previous value is not kept' } }
    }
    case 'save_settings': {
      const before = state.settings
      state.settings = { ...state.settings, ...(args.settings as object) }
      emitMock({ type: 'settings_changed' })
      return { result: state.settings, summary: 'You changed settings', affected: [], undo: { state: 'undoable', restore: { kind: 'settings', settings: before }, label: 'Restore the previous settings' } }
    }
    case 'connect_gmail': {
      // Stands in for Google: the consent URL comes back at once and the
      // account appears a moment later, as the real callback does.
      const signedIn = (digId: string, body: string) => {
        emitMock({ type: 'notify', title: 'Gmail connected', body, find_id: null })
        emitMock({ type: 'gmail_sign_in', dig_id: digId, connected: true, message: null })
        emitMock({ type: 'settings_changed' })
      }
      if (typeof args.digId === 'string') {
        const dig = state.digs.find((d) => d.id === args.digId)
        if (!dig) throw err('not_found', `dig ${String(args.digId)}`)
        if (dig.provider !== 'gmail') throw err('invalid', 'only a Gmail account can sign in again; a test mailbox has no sign-in')
        setTimeout(() => {
          dig.status = { state: 'catching_up' as const, remaining: u64(0), total: u64(0) }
          emitMock({ type: 'dig_status_changed', dig_id: dig.id, status: dig.status })
          signedIn(dig.id, `${dig.email} is signed in again and catching up.`)
        }, 2500)
        // No consent page to open in browser mode: the UI treats a null URL as nothing to open.
        return { result: { digId: dig.id, authUrl: null, reconnect: true }, summary: 'Started signing in to Gmail again', inputs: { digId: dig.id, reconnect: true }, undo: { state: 'not_undoable', reason: 'the sign-in completes in the browser' } }
      }
      const id = nextId(state, 'dig')
      const name = typeof args.name === 'string' && args.name.trim() ? args.name.trim() : 'Gmail'
      const email = `${name.toLowerCase().replace(/[^a-z0-9]+/g, '.')}@gmail.example`
      setTimeout(() => {
        const at = new Date().toISOString()
        state.digs.push({ id, name, email, provider: 'gmail' as const, group: null, privacy: (args.privacy as typeof state.settings.privacyDefault) ?? state.settings.privacyDefault, projection: (args.projection as typeof state.settings.projectionDefault) ?? state.settings.projectionDefault, status: { state: 'catching_up' as const, remaining: u64(0), total: u64(0) }, shadowStartedAt: null, disabled: false, createdAt: at })
        ME[id] = { email, name }
        signedIn(id, `${email} is syncing now.`)
      }, 2500)
      return { result: { digId: id, authUrl: null }, summary: 'Started a Gmail sign-in', inputs: { digId: id }, undo: { state: 'not_undoable', reason: 'remove the account instead' } }
    }
    case 'add_fixture_dig': {
      const name = str(args, 'name')
      const email = str(args, 'email')
      const id = nextId(state, 'dig')
      const dig = { id, name, email, provider: 'fixture' as const, group: null, privacy: (args.privacy as typeof state.settings.privacyDefault) ?? state.settings.privacyDefault, projection: (args.projection as typeof state.settings.projectionDefault) ?? state.settings.projectionDefault, status: { state: 'catching_up' as const, remaining: u64(Number(args.count ?? 120)), total: u64(Number(args.count ?? 120)) }, shadowStartedAt: now, disabled: false, createdAt: now }
      state.digs.push(dig)
      ME[id] = { email, name }
      const job = { id: `job_initial_${id}`, kind: 'initial_sync' as const, priority: 'catch_up' as const, status: { state: 'running' as const }, digId: id, label: `Initial sync: ${name}`, progress: { done: u64(0), total: u64(Number(args.count ?? 120)), unit: 'messages', step: 'Fetching headers', etaSeconds: u64(40) }, checkpoint: null, cancellable: true, attempts: 1, createdAt: now, startedAt: now, finishedAt: null, updatedAt: now }
      state.jobs.unshift(job)
      emitMock({ type: 'dig_status_changed', dig_id: id, status: dig.status })
      emitMock({ type: 'job_changed', job })
      emitMock({ type: 'settings_changed' })
      return { result: dig, summary: `You added Fixture Dig: ${name}`, affected: [{ kind: 'dig', id }], undo: { state: 'not_undoable', reason: 'remove the Dig instead' } }
    }
    case 'remove_dig': {
      const digId = str(args, 'digId')
      const dig = state.digs.find((d) => d.id === digId)
      if (!dig) throw err('not_found', 'Dig not found')
      state.digs = state.digs.filter((d) => d.id !== digId)
      const removed = state.messages.filter((x) => x.digId === digId).map((x) => x.id)
      state.messages = state.messages.filter((x) => x.digId !== digId)
      removed.forEach((id) => state.classifications.delete(id))
      emitMock({ type: 'settings_changed' })
      emitMock({ type: 'summary_changed' })
      return { result: { digId }, summary: `You removed Dig: ${dig.name} (${removed.length} messages)`, affected: [{ kind: 'dig', id: digId }], undo: { state: 'not_undoable', reason: 'the store was deleted; connect the Dig again to resync' } }
    }
    case 'update_dig': {
      const d = args.dig as { id: string } & Partial<typeof state.digs[number]>
      const idx = state.digs.findIndex((x) => x.id === d.id)
      if (idx < 0) throw err('not_found', 'Dig not found')
      const before = state.digs[idx]
      // As the backend: only set_dig_enabled switches an account.
      state.digs[idx] = { ...before, ...d, disabled: before.disabled }
      emitMock({ type: 'settings_changed' })
      return { result: state.digs[idx], summary: `You updated Dig: ${state.digs[idx].name}`, affected: [{ kind: 'dig', id: d.id }], undo: { state: 'undoable', restore: { kind: 'dig', dig: before }, label: 'Restore the previous Dig settings' } }
    }
    case 'set_dig_enabled': {
      const idx = state.digs.findIndex((x) => x.id === args.digId)
      if (idx < 0) throw err('not_found', 'Dig not found')
      const before = state.digs[idx]
      const enabled = Boolean(args.enabled)
      state.digs[idx] = { ...before, disabled: !enabled }
      emitMock({ type: 'settings_changed' })
      return { result: { digId: before.id, enabled }, summary: enabled ? `Enabled "${before.name}"; it syncs and sifts again` : `Disabled "${before.name}"; its mail is kept`, affected: [{ kind: 'dig', id: before.id }], undo: { state: 'undoable', restore: { kind: 'dig', dig: before }, label: enabled ? 'Disable it again' : 'Enable it again' } }
    }
    case 'save_dig_group': {
      const g = { ...(args.group as { name: string; digs: string[] }), usedBy: [] as string[] }
      const idx = state.digGroups.findIndex((x) => x.name === g.name)
      if (idx >= 0) state.digGroups[idx] = g
      else state.digGroups.push(g)
      for (const d of state.digs) {
        if (g.digs.includes(d.id)) d.group = g.name
        else if (d.group === g.name) d.group = null
      }
      emitMock({ type: 'settings_changed' })
      return { result: g, summary: `You saved Dig Group: ${g.name}`, affected: g.digs.map((id) => ({ kind: 'dig' as const, id })), undo: { state: 'not_undoable', reason: 'edit the group again' } }
    }
    case 'rename_dig_group':
    case 'delete_dig_group': {
      const name = str(args, action === 'rename_dig_group' ? 'from' : 'name')
      const g = state.digGroups.find((x) => x.name === name)
      if (!g) throw err('not_found', `no group "${name}"`)
      const users = [...state.meshes.filter((m) => m.scope.kind === 'dig_group' && m.scope.group === name).map((m) => `Mesh "${m.name}"`), ...state.strata.filter((s) => s.scopeGroup === name).map((s) => `Stratum "${s.name}"`)]
      if (users.length) throw err('invalid', `${users.join(', ')} ${users.length === 1 ? 'is' : 'are'} scoped to "${name}"; change the scope first`)
      const to = action === 'rename_dig_group' ? str(args, 'to').trim() : null
      if (to !== null && state.digGroups.some((x) => x !== g && x.name.toLowerCase() === to.toLowerCase())) throw err('invalid', `a group called "${to}" exists`)
      state.digGroups = to !== null ? state.digGroups.map((x) => (x === g ? { ...g, name: to } : x)) : state.digGroups.filter((x) => x !== g)
      for (const d of state.digs) if (d.group === name) d.group = to
      emitMock({ type: 'settings_changed' })
      return { result: { name: to ?? name }, summary: to !== null ? `You renamed group ${name} to ${to}` : `You deleted group ${name}`, undo: { state: 'not_undoable', reason: 'mock' } }
    }
    case 'set_mailbox_writes': {
      const w = args.writes
      if (w !== 'hold' && w !== 'ask' && w !== 'allow') throw err('invalid', 'writes is hold, ask or allow')
      state.writes = w
      state.armed = w === 'allow'
      if (w === 'allow') state.held = 0
      return { result: { writes: w }, summary: `Mailbox writes: ${w}`, undo: { state: 'not_undoable', reason: 'choose again under Developer' } }
    }
    case 'send_held_changes': {
      const n = state.held
      state.held = 0
      return { result: { applied: n, failed: 0, conflicts: 0 }, summary: `Wrote ${plural(n, 'change')} to the mailbox`, undo: { state: 'not_undoable', reason: 'mock' } }
    }
    case 'discard_held_changes': {
      const n = state.held
      state.held = 0
      return { result: { discarded: n }, summary: `Dropped ${plural(n, 'waiting mailbox change')}`, undo: { state: 'not_undoable', reason: 'mock' } }
    }
    case 'refresh_now': {
      const digs = args.digId ? state.digs.filter((d) => d.id === args.digId) : state.digs
      // A short sync per account: fetch, then sift, then synced, with the job row moving alongside.
      for (const d of digs) {
        const job: Job = { id: nextId(state, 'job'), kind: 'incremental_sync', priority: 'interactive', status: { state: 'running' }, digId: d.id, label: `Refresh ${d.name}`, progress: { done: u64(0), total: u64(12), unit: 'messages', step: 'Fetching headers', etaSeconds: null }, checkpoint: null, cancellable: true, attempts: 1, createdAt: now, startedAt: now, finishedAt: null, updatedAt: now }
        state.jobs.unshift(job)
        const before = d.status
        d.status = { state: 'catching_up', remaining: u64(12), total: u64(12) }
        emitMock({ type: 'job_changed', job })
        emitMock({ type: 'dig_status_changed', dig_id: d.id, status: d.status })
        setTimeout(() => {
          job.progress = { ...job.progress, done: u64(12), step: 'sifting' }
          d.status = { state: 'sifting', remaining: u64(12) }
          emitMock({ type: 'job_progress', job_id: job.id, done: u64(12), total: u64(12), step: 'sifting' })
          emitMock({ type: 'dig_status_changed', dig_id: d.id, status: d.status })
        }, 400)
        setTimeout(() => {
          job.status = { state: 'completed' }
          job.finishedAt = iso(new Date())
          d.status = before.state === 'catching_up' || before.state === 'sifting' ? { state: 'synced' } : before
          emitMock({ type: 'job_changed', job })
          emitMock({ type: 'dig_status_changed', dig_id: d.id, status: d.status })
        }, 900)
      }
      state.logs.push({ at: now, level: 'info', category: 'sync', message: `refresh requested for ${digs.length} dig(s)`, payload: null })
      return { result: { digId: args.digId ?? null, heard: true }, summary: args.digId ? 'Syncing now' : 'Syncing all accounts now', affected: digs.map((d) => ({ kind: 'dig' as const, id: d.id })), silent: true }
    }
    case 'cancel_job':
    case 'pause_job':
    case 'resume_job': {
      const job = state.jobs.find((j) => j.id === args.jobId)
      if (!job) throw err('not_found', 'job not found')
      job.status = action === 'cancel_job' ? { state: 'cancelled' } : action === 'pause_job' ? { state: 'paused', reason: 'paused by you' } : { state: 'running' }
      if (action === 'cancel_job') job.finishedAt = now
      job.updatedAt = now
      emitMock({ type: 'job_changed', job })
      return { result: job, summary: `You ${action.replace('_job', '')}${action === 'cancel_job' ? 'led' : 'd'} the job "${job.label}"`, affected: [{ kind: 'job', id: job.id }], undo: { state: 'not_undoable', reason: 'jobs resume from their checkpoint' } }
    }
    case 'inject_fixture_message':
      return injectFixture(state, args)
    case 'apply_update_preview':
      return { result: { keep: !!args.keep }, summary: args.keep ? 'You kept the previous Signal and Mesh versions' : 'You applied the updated Signals and Meshes', undo: { state: 'not_undoable', reason: 'choose again from Settings → Update' } }
    default:
      throw err('not_found', `no Action named ${action}`)
  }
}

// ---------------------------------------------------------------------------
// Undo

interface RestoreDispositions {
  kind: 'dispositions'
  items: { digId: string; messageId: string; disposition: Disposition | null }[]
}

function undoActivity(state: MockState, activityId: string): Exec {
  const target = state.activity.find((a) => a.id === activityId)
  if (!target) throw err('not_found', 'Activity not found')
  if (target.undo.state === 'undone') throw err('conflict', 'already undone')
  if (target.undo.state !== 'undoable') throw err('policy_denied', `cannot undo: ${target.undo.reason}`)
  const restore = target.undo.restore as { kind: string } & Record<string, unknown>
  const skipped: { entity: EntityRef; reason: string }[] = []
  const affected: EntityRef[] = []
  let detail = ''
  switch (restore.kind) {
    case 'dispositions': {
      const r = restore as unknown as RestoreDispositions
      for (const it of r.items) {
        const c = state.classifications.get(it.messageId)
        if (!c) {
          skipped.push({ entity: { kind: 'message', id: it.messageId }, reason: 'no longer in the store' })
          continue
        }
        // Undo All skips what the user handled by hand since (README "Undo").
        if (target.actor !== 'user' && c.handledByUserAt && c.handledByUserAt > target.createdAt) {
          skipped.push({ entity: { kind: 'message', id: it.messageId }, reason: 'you handled it by hand since' })
          continue
        }
        setDisposition(state, it.digId, it.messageId, it.disposition, false)
        affected.push({ kind: 'message', id: it.messageId })
      }
      detail = `${affected.length} restored${skipped.length ? `, ${skipped.length} skipped` : ''}.`
      break
    }
    case 'find_state': {
      const f = state.finds.find((x) => x.id === restore.findId)
      if (f) {
        Object.assign(f, { status: restore.status, snoozedUntil: restore.snoozedUntil, findType: restore.findType, resolvedAt: restore.resolvedAt, resolvedBy: restore.resolvedBy, updatedAt: iso(new Date()) })
        emitMock({ type: 'find_changed', find_id: f.id })
        affected.push({ kind: 'find', id: f.id })
      }
      break
    }
    case 'mesh_version': {
      const meshId = String(restore.meshId)
      const version = Number(restore.version)
      state.meshes = state.meshes.filter((mm) => !(mm.id === meshId && mm.version > version))
      if (restore.status) for (const v of state.meshes.filter((x) => x.id === meshId)) v.status = restore.status as MeshStatus
      affected.push({ kind: 'mesh', id: meshId })
      break
    }
    case 'finds_reopen': {
      for (const id of (restore.findIds as string[]) ?? []) {
        const f = state.finds.find((x) => x.id === id)
        if (f) {
          f.status = 'open'
          affected.push({ kind: 'find', id })
        }
      }
      break
    }
    case 'mesh_status': {
      for (const v of state.meshes.filter((x) => x.id === restore.meshId)) v.status = restore.status as MeshStatus
      affected.push({ kind: 'mesh', id: String(restore.meshId) })
      break
    }
    case 'mesh_deleted': {
      for (const v of state.meshes.filter((x) => x.id === restore.meshId)) {
        v.deletedAt = iso(new Date())
        v.status = 'deleted'
      }
      affected.push({ kind: 'mesh', id: String(restore.meshId) })
      break
    }
    case 'mesh_restore': {
      for (const v of state.meshes.filter((x) => x.id === restore.meshId)) {
        v.deletedAt = null
        v.status = 'disabled'
      }
      affected.push({ kind: 'mesh', id: String(restore.meshId) })
      break
    }
    case 'strata_remove': {
      const ids = restore.messageIds as string[]
      const strata = restore.strata as string[]
      for (const id of ids) {
        const c = state.classifications.get(id)
        if (c) c.strata = c.strata.filter((s) => !strata.includes(s))
        affected.push({ kind: 'message', id })
      }
      for (const name of strata) {
        const st = state.strata.find((s) => s.name === name)
        if (st) st.messageCount = u64(Math.max(0, Number(st.messageCount) - ids.length))
      }
      emitMock({ type: 'messages_changed', dig_id: String(restore.digId), count: u64(ids.length) })
      break
    }
    case 'labels_add':
    case 'labels_remove': {
      const ids = restore.messageIds as string[]
      const labels = restore.labels as string[]
      for (const id of ids) {
        const msg = state.messages.find((x) => x.id === id)
        if (msg) msg.labels = restore.kind === 'labels_add' ? [...new Set([...msg.labels, ...labels])] : msg.labels.filter((l) => !labels.includes(l))
        affected.push({ kind: 'message', id })
      }
      emitMock({ type: 'messages_changed', dig_id: String(restore.digId), count: u64(ids.length) })
      break
    }
    case 'never_hidden_remove':
      state.profile.neverHidden = state.profile.neverHidden.filter((x) => x !== restore.entry)
      emitMock({ type: 'settings_changed' })
      break
    case 'never_hidden_add':
      if (!state.profile.neverHidden.includes(String(restore.entry))) state.profile.neverHidden.push(String(restore.entry))
      emitMock({ type: 'settings_changed' })
      break
    case 'settings':
      state.settings = restore.settings as Settings
      emitMock({ type: 'settings_changed' })
      break
    case 'dig': {
      const dig = restore.dig as typeof state.digs[number]
      const idx = state.digs.findIndex((d) => d.id === dig.id)
      if (idx >= 0) state.digs[idx] = dig
      emitMock({ type: 'settings_changed' })
      break
    }
    case 'stratum_name': {
      const st = state.strata.find((s) => s.id === restore.stratumId)
      if (st) {
        const cur = st.name
        st.name = String(restore.name)
        for (const c of state.classifications.values()) c.strata = c.strata.map((x) => (x === cur ? st.name : x))
        affected.push({ kind: 'stratum', id: st.id })
      }
      break
    }
    case 'stratum_deleted': {
      const st = state.strata.find((s) => s.id === restore.stratumId)
      if (st) st.deletedAt = iso(new Date())
      break
    }
    case 'stratum_unmerge': {
      const from = state.strata.find((s) => s.id === restore.fromId)
      const into = state.strata.find((s) => s.id === restore.intoId)
      if (from && into) {
        from.deletedAt = null
        into.messageCount = u64(Math.max(0, Number(into.messageCount) - Number(restore.count)))
      }
      break
    }
    case 'signal_status': {
      const name = String(restore.name)
      for (const s of state.signals.filter((x) => x.name === name)) {
        if (s.version === Number(restore.version)) s.status = 'candidate'
        if (restore.prevActive !== null && s.version === Number(restore.prevActive)) s.status = 'active'
      }
      break
    }
    case 'draft_restore':
      state.drafts.push(restore.draft as Draft)
      break
    case 'importance': {
      const id = String(restore.messageId)
      if (restore.prior) state.importance.set(id, restore.prior as Importance)
      else state.importance.delete(id)
      affected.push({ kind: 'message', id })
      break
    }
    default:
      throw err('internal', `unknown restore kind ${restore.kind}`)
  }
  target.undo = { state: 'undone', at: iso(new Date()), by: 'user', undo_activity_id: 'pending' }
  const summary = `You undid: ${target.summary.replace(/^You /, '').replace(/^(Sift|Director|Mesh|Automation|System) /, (w) => `${w.trim()} `)}`
  const exec: Exec = {
    result: { activityId, restored: affected.length, skipped: skipped.length },
    summary,
    why: [detail, target.actor === 'user' ? 'The correction it recorded was retracted.' : null].filter(Boolean).join(' ') || null,
    affected,
    activityResult: skipped.length ? { status: 'partial', skipped } : { status: 'ok' },
    undo: { state: 'not_undoable', reason: 'an undo is not undone; redo the action by hand' },
    digId: target.digId,
  }
  // Link the undone entry to the undo entry once it exists.
  queueMicrotask(() => {
    const undoEntry = state.activity.find((a) => a.undoes === activityId)
    if (undoEntry && target.undo.state === 'undone') target.undo = { ...target.undo, undo_activity_id: undoEntry.id }
  })
  return exec
}

// ---------------------------------------------------------------------------
// Demo Mode: inject a fixture message and run it through a visible pipeline.

function injectFixture(state: MockState, args: Args): Exec {
  const digId = (args.digId as string | undefined) ?? DIG_WORK
  const template = (args.template as string | undefined) ?? 'review_request'
  const id = nextId(state, 'm_demo')
  const templates: Record<string, MessageSpec> = {
    review_request: {
      id,
      dig: digId,
      from: { email: 'notifications@github.example', name: 'Jamie Okafor' },
      subject: (args.subject as string) ?? '[northwind/platform] Review requested: Tray icon at 2x (PR #418)',
      text: (args.body as string) ?? '@matthew requested your review on #418.\n\nRenders the tray icon from the 2x asset on Retina displays.',
      hoursAgo: 0,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'find', find_type: 'review' },
      strata: ['Engineering'],
      facts: { senderDomain: 'github.example', directlyAddressed: true, autoSubmitted: true, gmailCategory: 'updates' },
      signals: { review_requested: 0.94, ci_failure: 0.03, personally_relevant: 0.89, routine: 0.55, requires_response: 0.12 },
      meshId: 'mesh_github',
      meshVersion: 7,
      matchedRule: 'IF review_requested@v3 > .80\n    → Find: Review',
      reason: 'Jamie requested your review directly.',
    },
    ci_success: {
      id,
      dig: digId,
      from: { email: 'notifications@github.example', name: 'GitHub' },
      subject: (args.subject as string) ?? '[northwind/platform] Run succeeded: CI on feature/tray-2x',
      text: (args.body as string) ?? 'All 414 tests passed in 3m 52s.',
      hoursAgo: 0,
      labels: ['INBOX', 'CATEGORY_UPDATES'],
      disposition: { kind: 'spoil' },
      strata: ['Engineering', 'Notifications'],
      facts: { senderDomain: 'github.example', autoSubmitted: true, precedenceBulk: true, gmailCategory: 'updates' },
      signals: { review_requested: 0.02, ci_failure: 0.03, personally_relevant: 0.1, routine: 0.96, requires_response: 0.03 },
      meshId: 'mesh_github',
      meshVersion: 7,
      matchedRule: 'ELSE IF routine@v1 > .90\n     AND personally_relevant@v3 < .30\n    → Spoil',
      reason: 'Routine GitHub notification not addressed to you.',
    },
    ambiguous: {
      id,
      dig: digId,
      from: { email: (args.from as string) ?? 'lee.n@partner-co.example', name: 'Lee Nakamura' },
      subject: (args.subject as string) ?? 'Following up',
      text: (args.body as string) ?? 'Did you get a chance to look at what I sent? Would be good to close this out this week.',
      hoursAgo: 0,
      labels: ['INBOX'],
      disposition: { kind: 'unresolved' },
      strata: [],
      facts: { senderDomain: 'partner-co.example', firstTimeSender: true, directlyAddressed: true },
      signals: { requires_response: 0.52, personally_relevant: 0.41, routine: 0.08, urgency: 0.35 },
      meshId: null,
      matchedRule: null,
      escalated: { reason: 'requires_response .52 below the .80 auto threshold', rationale: 'A first-time sender refers to something previously sent that is not in the mailbox. Not enough context to decide; Unresolved.' },
      reason: 'Too little context: a first-time sender about something Riddle has not seen.',
      mechanism: 'llm',
    },
  }
  const spec = templates[template] ?? templates.review_request
  spec.fresh = true
  if (args.from && template !== 'ambiguous') spec.from = { email: String(args.from), name: null }
  const msg = buildMessage(spec)
  state.messages.unshift(msg)
  state.specs.set(msg.id, spec)
  state.classifications.set(msg.id, { disposition: null, strata: [], findId: null, handledByUserAt: null, shadow: false })
  emitMock({ type: 'messages_changed', dig_id: digId, count: u64(1) })
  emitMock({ type: 'pipeline_stage', message_id: msg.id, stage: 'Message received', ms: u64(0) })

  // The pipeline unfolds over real time so the Live Pipeline and Demo Mode
  // can watch it; each stage writes the state a later read would see.
  const steps: [number, string, () => void][] = [
    [300, 'Facts extracted', () => {}],
    [900, 'Jev evaluating', () => {}],
    [1500, 'Jev 184ms', () => {}],
    [1600, 'Signals persisted', () => {}],
    [1700, spec.meshId ? `Mesh ${state.meshes.find((mm) => mm.id === spec.meshId)?.name} v${spec.meshVersion} evaluated` : 'No Mesh matched; confidence policy', () => {}],
  ]
  if (spec.escalated) steps.push([2600, 'LLM escalation 1240ms', () => {}])
  steps.push([
    spec.escalated ? 2800 : 1900,
    `Disposition: ${dispositionLabel(spec.disposition)}`,
    () => {
      const tr = buildTrace(spec, msg, state.signals, state.meshes)
      if (tr) state.traces.set(tr.id, tr)
      const c = state.classifications.get(msg.id)!
      c.disposition = spec.disposition
      c.strata = spec.strata ?? []
      if (spec.disposition?.kind === 'find') {
        const find: Find = { id: nextId(state, 'find'), findType: spec.disposition.find_type, title: msg.subject.replace(/^\[.*?\]\s*/, ''), summary: msg.snippet, priority: 78, status: 'open', dueAt: null, snoozedUntil: null, createdAt: iso(new Date()), updatedAt: iso(new Date()), resolvedAt: null, resolvedBy: null, sources: [{ messageId: msg.id, threadId: msg.threadId, digId }], digIds: [digId], identityKey: msg.threadId, reason: spec.reason ?? '', confidence: 0.94, classificationId: `cls_${msg.id}`, traceId: `tr_${msg.id}`, supersededBy: null, notify: false }
        state.finds.unshift(find)
        c.findId = find.id
        emitMock({ type: 'find_created', find })
        if (find.priority >= state.settings.notifyPriorityThreshold) emitMock({ type: 'notify', title: `Find: ${find.findType}`, body: find.title, find_id: find.id })
      }
      emitMock({ type: 'message_classified', message_id: msg.id, dig_id: digId, disposition: spec.disposition ?? { kind: 'unresolved' }, shadow: false })
      emitMock({ type: 'summary_changed' })
      if (spec.disposition?.kind === 'spoil') {
        const activity: Activity = {
          id: nextId(state, 'act'),
          actor: 'sift',
          action: 'move_to_spoil',
          summary: `Sift moved "${msg.subject}" to Spoil`,
          why: spec.reason ?? null,
          inputs: { digId, count: 1 },
          affected: [{ kind: 'message', id: msg.id }],
          result: { status: 'ok' },
          provenance: { mechanism: 'mesh', meshId: spec.meshId ?? null, meshVersion: spec.meshVersion ?? null, meshName: state.meshes.find((mm) => mm.id === spec.meshId)?.name ?? null, traceId: `tr_${msg.id}`, note: '1 needed a fresh evaluation.' },
          undo: { state: 'undoable', restore: { kind: 'dispositions', items: [{ digId, messageId: msg.id, disposition: { kind: 'unresolved' } }] }, label: 'Restore to Unresolved' },
          digId,
          traceId: `tr_${msg.id}`,
          undoes: null,
          createdAt: iso(new Date()),
        }
        state.activity.unshift(activity)
        emitMock({ type: 'activity_appended', activity })
      }
    },
  ])
  steps.push([spec.escalated ? 3000 : 2100, 'Activity persisted', () => {}])
  let last = 0
  for (const [at, stage, fn] of steps) {
    setTimeout(() => {
      fn()
      emitMock({ type: 'pipeline_stage', message_id: msg.id, stage, ms: u64(at - last) })
      last = at
    }, at)
  }
  return { result: { messageId: msg.id, digId, traceId: `tr_${msg.id}` }, summary: `You injected a fixture message into ${state.digs.find((d) => d.id === digId)?.name ?? digId}`, affected: [{ kind: 'message', id: msg.id }], undo: { state: 'not_undoable', reason: 'fixture messages are removed on the next resync' }, digId }
}

function scheduleExpedition(state: MockState, id: string) {
  const tick = (n: number) => {
    const exp = state.expeditions.find((e) => e.id === id)
    const job = state.jobs.find((j) => j.id === `job_${id}`)
    if (!exp || !job || exp.status === 'cancelled') return
    const now = iso(new Date())
    if (n === 0) {
      exp.status = 'running'
      job.status = { state: 'running' }
      job.startedAt = now
    }
    if (n >= 1 && n <= 3) {
      exp.plan[n - 1].done = true
      exp.spent = { costUsd: Math.round(exp.spent.costUsd * 100 + 11) / 100, seconds: u64(Number(exp.spent.seconds) + 40), toolCalls: exp.spent.toolCalls + 4 }
      exp.evidence.push({ kind: 'message', id: state.messages[n * 3]?.id ?? 'm_pers_amazon' })
      job.progress = { done: u64(n), total: u64(3), unit: 'steps', step: exp.plan[n]?.text ?? 'Reporting', etaSeconds: u64((3 - n) * 4) }
    }
    if (n === 4) {
      exp.status = 'completed'
      exp.finishedAt = now
      exp.findings = [`Looked at ${exp.evidence.length} threads. Nothing needed a Find; see the evidence list.`]
      job.status = { state: 'completed' }
      job.finishedAt = now
    }
    exp.updatedAt = now
    job.updatedAt = now
    emitMock({ type: 'expedition_changed', expedition_id: id })
    emitMock({ type: 'job_changed', job })
    if (n < 4) setTimeout(() => tick(n + 1), 4000)
  }
  setTimeout(() => tick(0), 800)
}

export const _internal = { dispositionLabel, findTitleFor, NOW }

/**
 * The verb and noun for a PendingAction, and the headline for a simulation.
 *
 * The UX study found people approving Director proposals on the Director's
 * description of them: the dialog said "Approve", the arguments were a JSON
 * blob, and the change itself was nowhere in plain words. So the primary
 * button now says what will happen ("Apply the Mesh", "Send", "Add to Never
 * Hidden") and every proposal carries one verb-plus-noun line derived from
 * the real arguments, never from the summary alone. "Approve" survives only
 * as the fallback for an Action nobody has mapped yet.
 */
import type { PendingAction } from '@/bindings/PendingAction'
import type { SimulationReport } from '@/bindings/SimulationReport'
import { fmtInt, num, titleCase } from '@/lib/format'

export interface PendingVerb {
  /** The primary button: "Apply the Mesh", "Send", "Add to Never Hidden". */
  button: string
  /** The change in one line: "Update Mesh GitHub Notifications v7 → v8". */
  line: string
}

export function realArgs(p: Pick<PendingAction, 'realArguments'>): Record<string, unknown> {
  const r = p.realArguments
  return r && typeof r === 'object' && !Array.isArray(r) ? (r as Record<string, unknown>) : {}
}

const str = (v: unknown): string | null => (typeof v === 'string' && v.trim() ? v.trim() : null)
const numOrNull = (v: unknown): number | null => (typeof v === 'number' ? v : typeof v === 'bigint' ? Number(v) : null)

/** The Mesh text a proposal carries, when it is a Mesh proposal. */
export function meshText(p: Pick<PendingAction, 'action' | 'realArguments'>): string | null {
  if (p.action !== 'create_mesh' && p.action !== 'update_mesh') return null
  return str(realArgs(p).text)
}

/** The Mesh a proposal names; the backend and the mock both put `name` in the real arguments. */
export function meshName(p: Pick<PendingAction, 'realArguments'>): string | null {
  return str(realArgs(p).name)
}

function recipientCount(v: unknown): number {
  return Array.isArray(v) ? v.length : 0
}

export function pendingVerb(p: PendingAction, fallback = 'Approve'): PendingVerb {
  const real = realArgs(p)
  switch (p.action) {
    case 'create_mesh': {
      const name = meshName(p)
      return { button: 'Apply the Mesh', line: name ? `Create Mesh ${name}` : 'Create a Mesh' }
    }
    case 'update_mesh': {
      const name = meshName(p) ?? 'this Mesh'
      const from = numOrNull(real.fromVersion)
      const to = numOrNull(real.toVersion)
      return { button: 'Apply the Mesh', line: from !== null && to !== null ? `Update Mesh ${name} v${from} → v${to}` : `Update Mesh ${name}` }
    }
    case 'set_mesh_status': {
      const to = str(real.to)
      const name = str(real.name) ?? 'this Mesh'
      return { button: to ? `Set to ${to}` : 'Change the status', line: to ? `Set Mesh ${name} to ${to}` : `Change the status of Mesh ${name}` }
    }
    case 'delete_mesh':
      return { button: 'Delete the Mesh', line: `Delete Mesh ${str(real.name) ?? ''}`.trim() }
    case 'send_message': {
      const subject = str(real.subject) ?? '(no subject)'
      const n = recipientCount(real.to)
      return { button: 'Send', line: `Send "${subject}" to ${n === 1 ? '1 recipient' : `${n} recipients`}` }
    }
    case 'forward_message':
      return { button: 'Forward', line: `Forward "${str(real.subject) ?? '(no subject)'}"` }
    case 'create_draft':
    case 'update_draft':
      return { button: p.action === 'create_draft' ? 'Create the draft' : 'Update the draft', line: p.action === 'create_draft' ? 'Create a draft' : 'Update the draft' }
    case 'add_never_hidden':
      return { button: 'Add to Never Hidden', line: `Add ${str(real.entry) ?? 'this sender'} to Never Hidden` }
    case 'remove_never_hidden':
      return { button: 'Remove from Never Hidden', line: `Remove ${str(real.entry) ?? 'this sender'} from Never Hidden` }
    case 'start_expedition':
      return { button: 'Start the Expedition', line: `Start an Expedition: "${str(real.goal) ?? p.summary}"` }
    case 'cancel_expedition':
      return { button: 'Cancel the Expedition', line: 'Cancel the Expedition' }
    case 'promote_signal':
      return { button: 'Promote the Signal', line: `Promote Signal ${str(real.name) ?? ''} v${numOrNull(real.version) ?? '?'}`.replace('  ', ' ') }
    case 'propose_signal':
      return { button: 'Propose the Signal', line: `Propose Signal ${str(real.name) ?? ''}`.trim() }
    case 'move_to_spoil':
      return { button: 'Move to Spoil', line: p.summary }
    case 'archive_messages':
      return { button: 'Archive', line: p.summary }
    case 'mark_contamination':
      return { button: 'Mark as Contamination', line: p.summary }
    case 'restore_messages':
      return { button: 'Restore', line: p.summary }
    case 'label_messages':
      return { button: 'Add the Stratum', line: p.summary }
    case 'snooze_messages':
    case 'snooze_find':
      return { button: 'Snooze', line: p.summary }
    case 'permanent_delete':
      return { button: "Move to Gmail's Trash", line: p.summary }
    case 'remove_dig':
      return { button: 'Remove the account', line: p.summary }
    case 'merge_strata':
      return { button: 'Merge the Strata', line: p.summary }
    case 'save_settings':
      return { button: 'Save the settings', line: p.summary }
    case 'set_mailbox_writes':
      return { button: real.writes === 'allow' ? 'Allow mailbox writes' : 'Change mailbox writes', line: p.summary }
    case 'send_held_changes':
      return { button: 'Write them to the mailbox', line: p.summary }
    case 'discard_held_changes':
      return { button: 'Drop them', line: p.summary }
    case 'apply_update_preview':
      return { button: real.keep ? 'Keep the previous behaviour' : 'Apply the update', line: p.summary }
    default:
      return { button: fallback, line: p.summary }
  }
}

/** "Escalate" is the word for Unresolved on the Meshes page; keep them in step. */
function dispositionWord(k: string): string {
  return k === 'unresolved' ? 'Escalate' : titleCase(k)
}

/** "Would change 14 messages: 6 to Find, 8 to Filed." from a SimulationReport. */
export function impactLine(r: SimulationReport): string {
  const changed = num(r.changed)
  if (changed === 0) return `Would change nothing in the last ${r.windowDays} days (${fmtInt(r.historicalMessages)} messages).`
  const byTarget = new Map<string, number>()
  for (const t of r.transitions) byTarget.set(t.to, (byTarget.get(t.to) ?? 0) + num(t.count))
  const parts = [...byTarget.entries()].sort((a, b) => b[1] - a[1]).map(([to, n]) => `${fmtInt(n)} to ${dispositionWord(to)}`)
  return `Would change ${fmtInt(changed)} ${changed === 1 ? 'message' : 'messages'} in the last ${r.windowDays} days${parts.length ? `: ${parts.join(', ')}` : ''}.`
}

/** A label for a real-argument key: known ids get their noun, the rest read as words. */
export function argLabel(key: string): string {
  const known: Record<string, string> = {
    digId: 'Account id',
    digs: 'Accounts',
    digIds: 'Account ids',
    meshId: 'Mesh id',
    fromVersion: 'From version',
    toVersion: 'To version',
    changeNote: 'Change note',
    bodyPreview: 'Body preview',
    messageIds: 'Message ids',
    threadId: 'Thread id',
    draftId: 'Draft id',
    inReplyTo: 'In reply to',
    to: 'To',
    cc: 'Cc',
    bcc: 'Bcc',
  }
  if (known[key]) return known[key]
  const words = key.replace(/([a-z0-9])([A-Z])/g, '$1 $2').replace(/[_-]+/g, ' ').toLowerCase()
  return words.charAt(0).toUpperCase() + words.slice(1)
}

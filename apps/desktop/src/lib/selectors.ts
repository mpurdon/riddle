/**
 * Pure derivations the UI renders from. Kept free of React so they can be
 * tested as functions: status chips, the summary's honesty about
 * catching up, Attention Zero evidence, Undo All's skipped list.
 */
import type { DigStatus } from '@/bindings/DigStatus'
import type { TaskCentre } from '@/bindings/TaskCentre'
import type { Job } from '@/bindings/Job'
import type { Summary } from '@/bindings/Summary'
import type { Activity } from '@/bindings/Activity'
import type { Disposition } from '@/bindings/Disposition'
import type { FindType } from '@/bindings/FindType'
import type { RiddleError } from '@/bindings/RiddleError'
import { fmtInt, fmtRelative, num, plural, titleCase, fmtClock } from './format'

// ---------------------------------------------------------------------------
// Per-Dig status chips (ARCHITECTURE.md §5)

export type Tone = 'neutral' | 'ok' | 'busy' | 'warn' | 'bad'

export interface Chip {
  label: string
  tone: Tone
  title: string
}

export function digStatusChip(s: DigStatus): Chip {
  switch (s.state) {
    case 'synced':
      return { label: 'Synced', tone: 'ok', title: 'Cursor current' }
    case 'catching_up':
      return { label: `Catching up ${fmtInt(s.remaining)}`, tone: 'busy', title: `${fmtInt(s.remaining)} of ${fmtInt(s.total)} messages still to fetch` }
    case 'sifting':
      return { label: `Sifting ${fmtInt(s.remaining)}`, tone: 'busy', title: `${fmtInt(s.remaining)} messages waiting for the Sift` }
    case 'projection':
      return { label: `Projection ${fmtInt(s.pending)}`, tone: 'busy', title: `${fmtInt(s.pending)} decisions not yet written to Gmail` }
    case 'degraded':
      return { label: 'Degraded', tone: 'bad', title: `${s.reason}. Nothing is hidden while degraded.` }
    case 'shadow':
      return { label: `Shadow day ${s.day}`, tone: 'neutral', title: `Shadow Period day ${s.day} of ${s.of}: Riddle proposes, the inbox is untouched` }
    case 'not_connected':
      return { label: 'Not connected', tone: 'warn', title: 'This Dig is not connected' }
  }
}

// ---------------------------------------------------------------------------
// Task Centre indicator (§55)

export interface TaskCentreIndicator {
  label: 'Caught up' | 'Catching up' | 'Degraded'
  tone: Tone
  detail: string
  running: number
  queued: number
  paused: number
}

export function isJobActive(j: Job): boolean {
  return j.status.state === 'running' || j.status.state === 'queued' || j.status.state === 'paused'
}

export function taskCentreIndicator(tc: TaskCentre | undefined): TaskCentreIndicator {
  if (!tc) return { label: 'Catching up', tone: 'busy', detail: 'Connecting…', running: 0, queued: 0, paused: 0 }
  const running = tc.jobs.filter((j) => j.status.state === 'running').length
  const queued = tc.jobs.filter((j) => j.status.state === 'queued').length
  const paused = tc.jobs.filter((j) => j.status.state === 'paused').length
  if (tc.overall === 'degraded' || tc.storeFault) {
    const reasons = tc.digs.filter((d) => d.status.state === 'degraded').map((d) => (d.status.state === 'degraded' ? d.status.reason : ''))
    const detail = tc.storeFault ? `Store fault: ${tc.storeFault}` : reasons[0] ? `${reasons[0]}. Nothing hidden.` : 'Nothing hidden.'
    return { label: 'Degraded', tone: 'bad', detail, running, queued, paused }
  }
  if (tc.overall === 'catching_up') {
    const catching = tc.digs.filter((d) => d.status.state === 'catching_up' || d.status.state === 'sifting')
    const remaining = catching.reduce((acc, d) => acc + (d.status.state === 'catching_up' ? num(d.status.remaining) : d.status.state === 'sifting' ? num(d.status.remaining) : 0), 0)
    const detail = remaining ? `${fmtInt(remaining)} messages to go` : running ? `${plural(running, 'job')} running` : 'Working'
    return { label: 'Catching up', tone: 'busy', detail, running, queued, paused }
  }
  return { label: 'Caught up', tone: 'ok', detail: `as of ${fmtClock(tc.asOf)}`, running, queued, paused }
}

export function jobProgressPct(j: Job): number | null {
  const total = j.progress.total
  if (total === null || total === undefined || num(total) === 0) return null
  return Math.min(100, Math.round((num(j.progress.done) / num(total)) * 100))
}

export function jobKindLabel(kind: Job['kind']): string {
  const names: Record<Job['kind'], string> = {
    initial_sync: 'Initial sync',
    catch_up: 'Catch-up',
    incremental_sync: 'Incremental sync',
    sift: 'Sift',
    historical_sift: 'Historical Sift',
    signal_backfill: 'Signal backfill',
    mesh_simulation: 'Mesh simulation',
    projection: 'Projection',
    expedition: 'Expedition',
    embeddings: 'Embeddings',
    bulk_action: 'Bulk action',
    replay: 'Replay',
    shadow_evaluation: 'Shadow evaluation',
    temporal_sift: 'Temporal Sift',
    maintenance: 'Maintenance',
  }
  return names[kind] ?? titleCase(kind)
}

// ---------------------------------------------------------------------------
// The morning summary's copy (README "The Core Experience")

export interface SummaryCopy {
  headline: string
  status: string
  finds: string
  unresolved: string
  arrived: string
  attentionZero: boolean
  evidence: string | null
}

export function greeting(now: Date = new Date()): string {
  const h = now.getHours()
  if (h < 5) return 'Good night.'
  if (h < 12) return 'Good morning.'
  if (h < 18) return 'Good afternoon.'
  return 'Good evening.'
}

export function summaryCopy(s: Summary, now: Date = new Date()): SummaryCopy {
  const findsOpen = num(s.findsOpen)
  const unresolved = num(s.unresolved)
  if (!s.caughtUp) {
    const remaining = num(s.remainingToSift)
    const total = num(s.totalToSift)
    return {
      headline: 'Catching up.',
      status: `${plural(findsOpen, 'Find')} so far. ${fmtInt(remaining)} of ${fmtInt(total)} messages still to sift.`,
      finds: `${plural(findsOpen, 'Find')} so far`,
      unresolved: unresolved === 1 ? '1 is Unresolved so far.' : `${fmtInt(unresolved)} are Unresolved so far.`,
      arrived: `${fmtInt(s.arrived)} messages arrived across your ${plural(s.digCount, 'Dig')} so far.`,
      attentionZero: false,
      evidence: null,
    }
  }
  const attentionZero = findsOpen === 0 && unresolved === 0
  return {
    headline: attentionZero ? 'Nothing needs your attention.' : greeting(now),
    status: `Caught up as of ${fmtClock(s.asOf)}.`,
    finds: findsOpen === 1 ? '1 Find needs your attention.' : `${fmtInt(findsOpen)} Finds need your attention.`,
    unresolved: unresolved === 1 ? '1 is Unresolved.' : `${fmtInt(unresolved)} are Unresolved.`,
    arrived: `${fmtInt(s.arrived)} messages arrived across your ${plural(s.digCount, 'Dig')}.`,
    attentionZero,
    evidence: attentionZero
      ? `Synced ${fmtRelative(s.lastSyncedAt ?? s.asOf, now)}. ${fmtInt(s.filed)} messages Filed, ${fmtInt(s.spoil)} in Spoil since ${sinceLabel(s.since, now)}.`
      : null,
  }
}

function sinceLabel(iso: string, now: Date): string {
  const d = new Date(iso)
  const diff = now.getTime() - d.getTime()
  if (diff < 36 * 3600_000 && d.toDateString() !== now.toDateString()) return 'yesterday'
  if (d.toDateString() === now.toDateString()) return fmtClock(iso)
  return d.toLocaleDateString([], { month: 'short', day: 'numeric' })
}

// ---------------------------------------------------------------------------
// Activity rendering helpers (README "Activity & Provenance", "Undo")

export function actorLabel(a: Activity['actor']): string {
  switch (a) {
    case 'user':
      return 'You'
    case 'director':
      return 'Director'
    case 'sift':
      return 'Sift'
    case 'mesh':
      return 'Mesh'
    case 'automation':
      return 'Automation'
    case 'system':
      return 'System'
  }
}

export function provenanceLine(a: Activity): string | null {
  const p = a.provenance
  if (!p) return null
  if (p.meshName) return `Triggered by Mesh: ${p.meshName}${p.meshVersion !== null ? ` v${p.meshVersion}` : ''}`
  switch (p.mechanism) {
    case 'temporal':
      return 'Triggered by Temporal Sift'
    case 'director':
      return 'Triggered by the Director'
    case 'llm':
      return 'Decided by LLM escalation'
    case 'jev':
      return 'Decided on Signals'
    case 'gate':
      return 'Decided by a deterministic gate'
    case 'provider':
      return 'Observed from Gmail'
    case 'user':
      return null
    default:
      return `Triggered by ${titleCase(p.mechanism)}`
  }
}

/** True when the entry can be undone now (not already undone, not irreversible). */
export function canUndo(a: Activity): boolean {
  return a.undo.state === 'undoable'
}

export function affectedCount(a: Activity): number {
  return a.affected.length
}

export function isBulk(a: Activity): boolean {
  return a.affected.filter((e) => e.kind === 'message').length > 1
}

/** The lines Undo All prints for what it skipped: "1 skipped — handled by you since". */
export function skippedLines(a: Activity): string[] {
  if (a.result.status !== 'partial') return []
  return a.result.skipped.map((s) => `${titleCase(s.entity.kind)} ${s.entity.id}: ${s.reason}`)
}

export function skippedSummary(a: Activity): string | null {
  if (a.result.status !== 'partial') return null
  const n = a.result.skipped.length
  return `${n} skipped: ${n === 1 ? 'it was' : 'they were'} handled by hand since.`
}

// ---------------------------------------------------------------------------
// Dispositions and Find types

export function dispositionLabel(d: Disposition | null | undefined): string {
  if (!d) return 'Unsifted'
  switch (d.kind) {
    case 'find':
      return `Find: ${findTypeLabel(d.find_type)}`
    case 'filed':
      return 'Filed'
    case 'spoil':
      return 'Spoil'
    case 'contamination':
      return 'Contamination'
    case 'unresolved':
      return 'Unresolved'
  }
}

export function dispositionTone(d: Disposition | null | undefined): Tone {
  if (!d) return 'neutral'
  switch (d.kind) {
    case 'find':
      return 'ok'
    case 'filed':
      return 'neutral'
    case 'spoil':
      return 'neutral'
    case 'contamination':
      return 'bad'
    case 'unresolved':
      return 'warn'
  }
}

export const FIND_TYPES: FindType[] = ['respond', 'decide', 'review', 'do', 'pay', 'attend', 'follow_up', 'know']

export function findTypeLabel(t: FindType): string {
  const names: Record<FindType, string> = {
    respond: 'Respond',
    decide: 'Decide',
    review: 'Review',
    do: 'Do',
    pay: 'Pay',
    attend: 'Attend',
    follow_up: 'Follow Up',
    know: 'Know',
  }
  return names[t]
}

// ---------------------------------------------------------------------------
// Errors → affordances (ARCHITECTURE.md §8)

export interface ErrorAffordance {
  title: string
  body: string
  action: 'sign_in' | 'task_centre' | 'reconcile' | 'retry' | 'none' | 'arm_dev'
  actionLabel: string | null
  degraded: boolean
}

export function errorAffordance(e: RiddleError): ErrorAffordance {
  switch (e.kind) {
    case 'provider_auth':
      return { title: 'Sign in needed', body: e.message || 'Gmail needs you to sign in again.', action: 'sign_in', actionLabel: 'Sign in', degraded: false }
    case 'provider_admin_blocked':
      return {
        title: 'Blocked by your Workspace administrator',
        body: `${e.message || 'A Google Workspace administrator has not allowed Riddle for this account.'} Ask them to allow the Riddle OAuth client with the gmail.modify scope.`,
        action: 'none',
        actionLabel: null,
        degraded: false,
      }
    case 'provider_rate_limited':
      return { title: 'Gmail is rate limiting', body: `${e.message}. The job is parked and will resume.`, action: 'task_centre', actionLabel: 'Open Task Centre', degraded: false }
    case 'provider_history_expired':
      return { title: 'Gmail history expired', body: 'A full resync is queued. Nothing is lost.', action: 'task_centre', actionLabel: 'Open Task Centre', degraded: false }
    case 'judge_unavailable':
      return { title: 'Judge unavailable', body: `${e.message}. Mail stays Unresolved; nothing is hidden.`, action: 'retry', actionLabel: 'Retry', degraded: true }
    case 'llm_unavailable':
      return { title: 'LLM unavailable', body: `${e.message}. Mail stays Unresolved; nothing is hidden.`, action: 'retry', actionLabel: 'Retry', degraded: true }
    case 'budget_exhausted':
      return { title: 'Budget exhausted', body: `${e.message}. Background work is paused until tomorrow or a higher budget.`, action: 'task_centre', actionLabel: 'Open Task Centre', degraded: false }
    case 'policy_denied':
      return { title: 'Not permitted', body: e.message, action: e.message.includes('armed') ? 'arm_dev' : 'none', actionLabel: e.message.includes('armed') ? 'Open Developer Mode' : null, degraded: false }
    case 'conflict':
      return { title: 'Gmail changed underneath', body: `${e.message}. Reconcile before retrying.`, action: 'reconcile', actionLabel: 'Reconcile', degraded: false }
    case 'not_found':
      return { title: 'Not found', body: e.message, action: 'none', actionLabel: null, degraded: false }
    case 'invalid':
      return { title: 'Invalid', body: e.message, action: 'none', actionLabel: null, degraded: false }
    case 'store':
      return { title: 'Store fault', body: `${e.message}. Riddle stops writing until this clears.`, action: 'none', actionLabel: null, degraded: true }
    case 'needs_confirmation':
      return { title: 'Confirmation needed', body: e.message, action: 'none', actionLabel: null, degraded: false }
    default:
      return { title: 'Something failed', body: e.message, action: 'retry', actionLabel: 'Retry', degraded: false }
  }
}

/**
 * Pure derivations the UI renders from. Kept free of React so they can be
 * tested as functions: status chips, the summary's honesty about
 * catching up, Attention Zero evidence, Undo All's skipped list.
 */
import type { SyncFailure } from '@/bindings/SyncFailure'
import type { Importance } from '@/bindings/Importance'
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

/** Why an account is not syncing, in one line (`SyncFailure::sentence`). */
export function syncFailureLine(f: SyncFailure): string {
  switch (f) {
    case 'sign_in':
      return 'the sign-in has expired; sign in again to resume syncing'
    case 'not_set_up':
      return 'Gmail sign-in is not set up: the OAuth client id is missing; add it under Settings → Accounts → Google sign-in'
    case 'admin_blocked':
      return 'a Workspace administrator blocks Riddle from this account'
    case 'rate_limited':
      return 'Gmail is limiting requests; Riddle keeps retrying'
    case 'unreachable':
      return 'Gmail cannot be reached; Riddle keeps retrying'
    case 'other':
      return 'syncing fails; see Developer → Logs'
  }
}

/** "last synced 27 Sep", or that it never has. */
export function lastSyncedPhrase(at: string | null | undefined): string {
  return at ? `last synced ${fmtRelative(at)}` : 'not synced yet'
}

export function digStatusChip(s: DigStatus): Chip {
  switch (s.state) {
    case 'synced':
      return { label: 'Synced', tone: 'ok', title: 'Cursor current' }
    case 'catching_up':
      if (Number(s.total) === 0) return { label: 'Starting first sync', tone: 'busy', title: 'Signed in; the first sync is listing the mailbox' }
      return { label: `Catching up ${fmtInt(s.remaining)}`, tone: 'busy', title: `${fmtInt(s.remaining)} of ${fmtInt(s.total)} messages still to fetch` }
    case 'sifting':
      return { label: `Sifting ${fmtInt(s.remaining)}`, tone: 'busy', title: `${fmtInt(s.remaining)} messages waiting for the Sift` }
    case 'projection':
      return { label: `${fmtInt(s.pending)} waiting for Gmail`, tone: 'busy', title: `${fmtInt(s.pending)} changes waiting to be written to Gmail` }
    case 'degraded':
      return { label: 'Degraded', tone: 'bad', title: `${s.reason}. Nothing is hidden while degraded.` }
    case 'sync_failing':
      return { label: s.failure === 'sign_in' ? 'Sign-in expired' : s.failure === 'not_set_up' ? 'Sign-in not set up' : 'Not syncing', tone: 'bad', title: `Not syncing: ${syncFailureLine(s.failure)}; ${lastSyncedPhrase(s.last_synced_at)}.` }
    case 'shadow':
      return { label: `Shadow day ${s.day}`, tone: 'neutral', title: `Shadow Period day ${s.day} of ${s.of}: Riddle proposes, the inbox is untouched` }
    case 'not_connected':
      return { label: 'Not signed in', tone: 'warn', title: 'Not signed in. Open the account in Settings and choose Sign in again.' }
  }
}

/** One account's place in the pipeline: mail is fetched, then sifted, then written back. */
export type StageState = 'done' | 'active' | 'todo' | 'off'
export interface PipelineStage {
  key: 'sync' | 'sift' | 'project'
  label: 'Sync' | 'Sift' | 'Project'
  state: StageState
}
export interface DigPipeline {
  stages: PipelineStage[]
  /** What the active stage is doing, or why nothing is. */
  detail: string
  /** Whether a request to sync now would be a repeat of work already running. */
  busy: boolean
}

/**
 * The account's status as three stages, with the running job's own step as
 * the detail when one is running for it. "Catching up 120" on a chip said
 * which stage but not that two more follow; the strip says the whole path.
 */
export function digPipeline(s: DigStatus, job?: Job | null): DigPipeline {
  const stage = (active: 0 | 1 | 2 | 3): PipelineStage[] =>
    (['sync', 'sift', 'project'] as const).map((key, i) => ({
      key,
      label: key === 'sync' ? 'Sync' : key === 'sift' ? 'Sift' : 'Project',
      state: i < active ? 'done' : i === active ? 'active' : 'todo',
    }))
  const stepOf = (fallback: string) => (job && job.status.state === 'running' && job.progress.step ? `${job.progress.step}${job.progress.total !== null ? ` · ${fmtInt(job.progress.done)} of ${fmtInt(job.progress.total)} ${job.progress.unit}` : ''}` : fallback)
  const running = !!job && (job.status.state === 'running' || job.status.state === 'queued')
  switch (s.state) {
    case 'catching_up':
      return { stages: stage(0), detail: stepOf(Number(s.total) === 0 ? 'Listing the mailbox' : `${fmtInt(s.remaining)} of ${fmtInt(s.total)} messages to fetch`), busy: true }
    case 'sifting':
      return { stages: stage(1), detail: stepOf(`${fmtInt(s.remaining)} messages waiting for the Sift`), busy: true }
    case 'projection':
      return { stages: stage(2), detail: stepOf(`${fmtInt(s.pending)} changes waiting to be written to Gmail`), busy: true }
    case 'synced':
      return { stages: running ? stage(0) : stage(3), detail: running ? stepOf('Checking for new mail') : 'Up to date', busy: running }
    case 'shadow':
      return { stages: running ? stage(0) : stage(3), detail: running ? stepOf('Checking for new mail') : `Up to date · Shadow day ${s.day} of ${s.of}`, busy: running }
    case 'degraded':
      return { stages: stage(0).map((st) => ({ ...st, state: 'off' })), detail: `Degraded: ${s.reason}`, busy: false }
    case 'sync_failing':
      return { stages: stage(0).map((st) => ({ ...st, state: 'off' })), detail: `Not syncing: ${syncFailureLine(s.failure)}; ${lastSyncedPhrase(s.last_synced_at)}`, busy: false }
    case 'not_connected':
      return { stages: stage(0).map((st) => ({ ...st, state: 'off' })), detail: 'Not signed in', busy: false }
  }
}

// ---------------------------------------------------------------------------
// Task Centre indicator (§55)

export interface TaskCentreIndicator {
  label: 'Caught up' | 'Catching up' | 'Degraded' | 'Nothing connected'
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
  // With no account there is nothing to be caught up with. "Caught up" here
  // read as praise on a first run that had not started.
  if (tc.digs.length === 0 && !tc.storeFault) {
    return { label: 'Nothing connected', tone: 'neutral', detail: 'Connect an account to start', running, queued, paused }
  }
  if (tc.overall === 'degraded' || tc.storeFault) {
    const reasons = tc.digs.flatMap((d) => (d.status.state === 'sync_failing' ? [`${d.name} is not syncing: ${syncFailureLine(d.status.failure)}`] : d.status.state === 'degraded' ? [d.status.reason] : []))
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
    projection: 'Writing to Gmail',
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

/**
 * The scheduling class in plain words. The raw values (`catch_up`,
 * `interactive`) leaked into the Task Centre row under each job.
 */
export function jobPriorityLabel(p: Job['priority']): string {
  const names: Record<Job['priority'], string> = {
    interactive: 'For you now',
    incoming: 'New mail',
    catch_up: 'Catching up',
    projection: 'Writing to Gmail',
    backfill: 'Backfill',
    experiment: 'Experiment',
  }
  return names[p] ?? titleCase(p)
}

// ---------------------------------------------------------------------------
// The morning summary's copy (README "The Core Experience")

export interface SummaryCopy {
  headline: string
  status: string
  finds: string
  unresolved: string
  arrived: string
  /** No account yet: the page is an invitation, not a report. */
  firstRun: boolean
  attentionZero: boolean
  evidence: string | null
  /** "Since yesterday", "Since 07:42", "Since Sep 21": the brief's opening. */
  since: string
  /** Where Start review should go, or null when there is nothing to review. */
  reviewRoute: '/finds' | '/unresolved' | null
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
  const accounts = plural(s.digCount, 'account')
  const since = `Since ${sinceLabel(s.since, now)}`
  const reviewRoute = findsOpen > 0 ? '/finds' : unresolved > 0 ? '/unresolved' : null
  // Before the first account there is nothing to be caught up with, and
  // "Nothing needs your attention" claimed a result Riddle had not earned.
  if (s.digCount === 0) {
    return {
      headline: 'Connect an account to start.',
      status: 'Riddle has no mailbox yet. Connect Gmail, or try it with sample mail first.',
      finds: 'No Finds yet.',
      unresolved: 'Nothing Unresolved yet.',
      arrived: 'No mail has arrived yet.',
      firstRun: true,
      attentionZero: false,
      evidence: null,
      since,
      reviewRoute: null,
    }
  }
  if (!s.caughtUp) {
    const remaining = num(s.remainingToSift)
    const total = num(s.totalToSift)
    return {
      headline: 'Catching up.',
      status: `${plural(findsOpen, 'Find')} so far. ${fmtInt(remaining)} of ${fmtInt(total)} messages still to sift.`,
      finds: `${plural(findsOpen, 'Find')} so far`,
      unresolved: unresolved === 1 ? '1 is Unresolved so far.' : `${fmtInt(unresolved)} are Unresolved so far.`,
      arrived: `${fmtInt(s.arrived)} messages arrived across your ${accounts} so far.`,
      firstRun: false,
      attentionZero: false,
      evidence: null,
      since,
      reviewRoute,
    }
  }
  const attentionZero = findsOpen === 0 && unresolved === 0
  return {
    headline: attentionZero ? 'Nothing needs your attention.' : greeting(now),
    status: `Caught up as of ${fmtClock(s.asOf)}.`,
    finds: findsOpen === 1 ? '1 Find needs your attention.' : `${fmtInt(findsOpen)} Finds need your attention.`,
    unresolved: unresolved === 1 ? '1 is Unresolved.' : `${fmtInt(unresolved)} are Unresolved.`,
    arrived: `${fmtInt(s.arrived)} messages arrived across your ${accounts}.`,
    firstRun: false,
    attentionZero,
    evidence: attentionZero
      ? `Synced ${fmtRelative(s.lastSyncedAt ?? s.asOf, now)}. ${fmtInt(s.filed)} messages Filed, ${fmtInt(s.spoil)} in Spoil since ${sinceLabel(s.since, now)}.`
      : null,
    since,
    reviewRoute,
  }
}

export function sinceLabel(iso: string, now: Date = new Date()): string {
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

/** Same kind, and the same Find type when both are Finds. */
export function sameDisposition(a: Disposition | null | undefined, b: Disposition | null | undefined): boolean {
  if (!a || !b) return false
  if (a.kind !== b.kind) return false
  return a.kind === 'find' && b.kind === 'find' ? a.find_type === b.find_type : true
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

/** What "Follow up by…" records: a Find of type Follow Up. */
export const FOLLOW_UP: Disposition = { kind: 'find', find_type: 'follow_up' }

/** The `set_disposition` arguments for a follow-up, due `dueAt` when a date was picked. */
export function followUpArgs(digId: string, messageId: string, dueAt: string | null): Record<string, unknown> {
  return { digId, messageId, disposition: FOLLOW_UP, findType: 'follow_up', ...(dueAt ? { dueAt } : {}) }
}

/** Lowest first, as the backend ranks them. */
export const IMPORTANCES: Importance[] = ['low', 'normal', 'high', 'critical']

/** The level a Find priority (0–100) falls in, as `Importance::from_priority`. */
export function importanceOfPriority(p: number): Importance {
  return p >= 90 ? 'critical' : p >= 70 ? 'high' : p >= 35 ? 'normal' : 'low'
}

export function importanceLabel(i: Importance): string {
  return i[0].toUpperCase() + i.slice(1)
}

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
    {
      // A dev build on Hold (or, from an older build, unarmed): the fix is under Developer.
      const held = e.message.includes('holds mailbox changes') || e.message.includes('armed')
      return { title: held ? 'Held by this dev build' : 'Not permitted', body: e.message, action: held ? 'arm_dev' : 'none', actionLabel: held ? 'Open Developer Mode' : null, degraded: false }
    }
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


/**
 * The opening message for "Talk to the Director about this": the question
 * the button stands for, naming the message the way a person would (subject,
 * sender, account) and never by id. It is prefilled, not sent: the user
 * reads it, edits it, and presses Enter. The Director finds the message by
 * searching, which costs one tool call and keeps identifiers out of the
 * user's own words.
 */
export function directorAbout(what: { label: string; title?: string; from?: string; digName?: string }): string {
  const named = what.title ? `: "${what.title}"` : ''
  const sender = what.from ? ` from ${what.from}` : ''
  const account = what.digName ? ` (in ${what.digName})` : ''
  return `Let's talk about this ${what.label}${named}${sender}${account}. Why did Riddle decide it this way, and should a Mesh change so mail like this is handled differently? If you suggest a change, show me what it would do to recent mail before anything is applied.`
}

/**
 * What an account's settings mean, in one place, so the list, the detail
 * panel and the Add account wizard describe them the same way.
 *
 * The sentence that matters is `gmailEffect`: Sift's changes reach the
 * mailbox only from Act, with projection on, and in a dev build only once
 * armed. The old account cards showed the three settings side by side and
 * left the user to work out that "Full" at "Observe" wrote nothing, or, before
 * the backend gate, that it did. The backend now answers the question itself
 * (`Dig.mailbox`, from the same functions that gate projection); the
 * derivation here is only the fallback for a Dig that has not been read
 * with it, such as the one the Add account wizard previews before it exists.
 */
import type { Autonomy } from '@/bindings/Autonomy'
import type { Dig } from '@/bindings/Dig'
import type { HeldReason } from '@/bindings/HeldReason'
import type { MailboxEffect } from '@/bindings/MailboxEffect'
import type { MailboxWrites } from '@/bindings/MailboxWrites'
import type { Mesh } from '@/bindings/Mesh'
import type { MeshScope } from '@/bindings/MeshScope'
import type { PrivacyMode } from '@/bindings/PrivacyMode'
import type { ProjectionMode } from '@/bindings/ProjectionMode'
import { mailboxName } from '@/lib/gmailEffect'
import type { Settings } from '@/lib/types'

export interface Option<T extends string> {
  value: T
  label: string
  detail: string
}

export const AUTONOMY_OPTIONS: Option<Autonomy>[] = [
  { value: 'observe', label: 'Observe', detail: 'Classifies inside Riddle. Never changes the mailbox.' },
  { value: 'suggest', label: 'Suggest', detail: 'Also proposes moves for you to apply. Never changes the mailbox on its own.' },
  { value: 'act', label: 'Act', detail: 'Applies its decisions to the mailbox: labels, and archiving where the account allows it.' },
  { value: 'trusted', label: 'Trusted', detail: 'Also takes riskier actions, such as sending, without asking. Hold to confirm.' },
]

export const PROJECTION_OPTIONS: Option<ProjectionMode>[] = [
  { value: 'none', label: 'None', detail: 'Riddle only. The mailbox is never touched.' },
  { value: 'labels_only', label: 'Labels', detail: 'Adds Riddle labels. Nothing leaves the inbox.' },
  { value: 'full', label: 'Labels and archiving', detail: 'Adds labels and archives Spoil and Filed mail.' },
]

export const PRIVACY_OPTIONS: Option<PrivacyMode>[] = [
  { value: 'local_only', label: 'Local only', detail: 'Nothing leaves this Mac. Uncertain mail stays Unresolved.' },
  { value: 'cloud_judgment', label: 'Cloud judgment', detail: 'Jev reads message features to classify. No LLM.' },
  { value: 'cloud_reasoning', label: 'Cloud reasoning', detail: 'Jev, plus an LLM second opinion when Jev is unsure.' },
]

export const labelOf = <T extends string>(options: Option<T>[], v: T): string => options.find((o) => o.value === v)?.label ?? v
export const detailOf = <T extends string>(options: Option<T>[], v: T): string => options.find((o) => o.value === v)?.detail ?? ''

/**
 * The one set of words for an autonomy level. The Meshes page, the trust
 * card and the account switcher each restated these before this.
 */
export const autonomyLabel = (a: Autonomy): string => labelOf(AUTONOMY_OPTIONS, a)

export const AUTONOMY_RANK: Record<Autonomy, number> = { observe: 0, suggest: 1, act: 2, trusted: 3 }

/** True when the level keeps decisions inside Riddle; a missing level is not below anything. */
export function belowAct(a: Autonomy | null | undefined): a is 'observe' | 'suggest' {
  return !!a && AUTONOMY_RANK[a] < AUTONOMY_RANK.act
}

/** The level that governs this account: its own, or the default. */
export function autonomyOf(settings: Settings, digId: string): { level: Autonomy; own: boolean } {
  const own = settings.autonomy.perDig[digId]
  return own ? { level: own, own: true } : { level: settings.autonomy.default, own: false }
}

export type GmailEffect = {
  writes: MailboxWrites
  /** Why nothing is written although the projection alone would write something. */
  held: HeldReason | null
  /** `held` as a clause: "autonomy is observe", "this dev build is not armed". */
  heldBecause: string | null
  sentence: string
}

export interface GmailEffectOptions {
  /** The backend's answer for the account, which wins over the derivation below. */
  mailbox?: MailboxEffect | null
  dev?: boolean
  armed?: boolean
  provider?: Dig['provider']
}

/**
 * What Sift will do to the mailbox for these settings, taken from the
 * backend's `Dig.mailbox` when the caller has it and derived from the
 * settings otherwise. `level` and `projection` still phrase the sentence:
 * the backend says a write is held for autonomy, the level says which.
 */
export function gmailEffect(level: Autonomy, projection: ProjectionMode, opts: GmailEffectOptions = {}): GmailEffect {
  const provider = opts.provider ?? 'gmail'
  const mailbox = mailboxName(provider)
  const { writes, held } = opts.mailbox ?? deriveMailboxEffect(level, projection, opts)
  const would = projection === 'full' ? 'labels and archives' : 'labels'
  if (held === 'autonomy') {
    return { writes, held, heldBecause: `autonomy is ${level}`, sentence: `Riddle classifies only. At Act it would add ${projection === 'full' ? 'labels and archive mail' : 'labels'} in ${mailbox}.` }
  }
  if (held === 'dev_unarmed') {
    return { writes, held, heldBecause: 'this dev build holds mailbox writes', sentence: `Riddle ${would} mail in ${mailbox} once you send the waiting changes or allow mailbox writes. Until then changes wait.` }
  }
  if (writes === 'nothing') return { writes, held: null, heldBecause: null, sentence: `Riddle classifies only. ${mailboxName(provider, { capital: true })} is never changed.` }
  return { writes, held: null, heldBecause: null, sentence: `Riddle ${writes === 'labels_and_archive' ? 'labels and archives' : 'labels'} mail in ${mailbox} as it classifies.` }
}

/** The projection gate's rule, for a Dig read without the backend's answer. Keep it in step with `mailbox_effect` in riddle-app. */
function deriveMailboxEffect(level: Autonomy, projection: ProjectionMode, opts: Pick<GmailEffectOptions, 'dev' | 'armed'>): MailboxEffect {
  if (projection === 'none') return { writes: 'nothing', held: null }
  if (belowAct(level)) return { writes: 'nothing', held: 'autonomy' }
  if (opts.dev && !opts.armed) return { writes: 'nothing', held: 'dev_unarmed' }
  return { writes: projection === 'full' ? 'labels_and_archive' : 'labels', held: null }
}

/**
 * The Meshes in this account's scope whose own `Autonomy:` cap keeps their
 * decisions inside Riddle whatever the account allows (Flow 7, per-rule
 * autonomy). Named on the account page so "Act" never reads as "everything
 * is written to Gmail" when a rule holds part of it back.
 */
export function cappedRules(meshes: Mesh[], dig: Pick<Dig, 'id' | 'group'>): Mesh[] {
  return meshes.filter((m) => !m.deletedAt && m.status !== 'deleted' && belowAct(m.autonomy) && scopeCovers(m.scope, dig))
}

function scopeCovers(scope: MeshScope, dig: Pick<Dig, 'id' | 'group'>): boolean {
  switch (scope.kind) {
    case 'all_digs':
      return true
    case 'dig':
      return scope.dig_id === dig.id
    case 'dig_group':
      return scope.group === dig.group
  }
}

/** A stable tone per account, so an avatar keeps its colour across renders. */
export function avatarTone(id: string): number {
  let h = 0
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0
  return h % 5
}

/** Accounts grouped for the list: named groups first, alphabetically, then the rest. */
export function groupAccounts(digs: Dig[], query: string): { group: string | null; digs: Dig[] }[] {
  const q = query.trim().toLowerCase()
  const hits = q ? digs.filter((d) => `${d.name} ${d.email} ${d.group ?? ''}`.toLowerCase().includes(q)) : digs
  const byGroup = new Map<string | null, Dig[]>()
  for (const d of [...hits].sort((a, b) => a.name.localeCompare(b.name))) {
    const k = d.group ?? null
    byGroup.set(k, [...(byGroup.get(k) ?? []), d])
  }
  const named = [...byGroup.keys()].filter((k): k is string => k !== null).sort((a, b) => a.localeCompare(b))
  const out = named.map((g) => ({ group: g as string | null, digs: byGroup.get(g)! }))
  if (byGroup.has(null)) out.push({ group: null, digs: byGroup.get(null)! })
  return out
}

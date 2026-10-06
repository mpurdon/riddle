/**
 * The mutable browser-mode state the mock commands read and the mock
 * actions change. Built fresh per session (and per test).
 */
import type { ProviderWrites } from '@/bindings/ProviderWrites'
import type { Importance } from '@/bindings/Importance'
import type { InstalledPack } from '@/bindings/InstalledPack'
import type { NamedListDef } from '@/bindings/NamedListDef'
import type { Activity } from '@/bindings/Activity'
import type { Conversation } from '@/bindings/Conversation'
import type { DecisionTrace } from '@/bindings/DecisionTrace'
import type { Dig } from '@/bindings/Dig'
import type { DigGroup } from '@/bindings/DigGroup'
import type { Disposition } from '@/bindings/Disposition'
import type { Draft } from '@/bindings/Draft'
import type { Expedition } from '@/bindings/Expedition'
import type { Find } from '@/bindings/Find'
import type { Job } from '@/bindings/Job'
import type { Mesh } from '@/bindings/Mesh'
import type { Message } from '@/bindings/Message'
import type { PendingAction } from '@/bindings/PendingAction'
import type { Signal } from '@/bindings/Signal'
import type { Stratum } from '@/bindings/Stratum'
import type { UserProfile } from '@/bindings/UserProfile'
import type { DevInfo, LogLine, Settings } from '../types'
import {
  buildActivity,
  buildConversations,
  buildDevInfo,
  buildDigGroups,
  buildDigs,
  buildDrafts,
  buildExpeditions,
  buildFinds,
  buildJobs,
  buildLogs,
  buildMeshes,
  buildMessage,
  buildProfile,
  buildSettings,
  buildSignals,
  buildStrata,
  buildTrace,
  messageSpecs,
  type MessageSpec,
} from './fixtures'

export interface Classification {
  disposition: Disposition | null
  strata: string[]
  findId: string | null
  /** Set when a user action changed it, for Undo All's skip rule. */
  handledByUserAt: string | null
  shadow: boolean
}

export interface MockState {
  namedLists: NamedListDef[]
  /** Spot-check grades, `meshId:messageId` → right | wrong. */
  grades: Record<string, string>
  installedPacks: InstalledPack[]
  digs: Dig[]
  digGroups: DigGroup[]
  profile: UserProfile
  messages: Message[]
  specs: Map<string, MessageSpec>
  classifications: Map<string, Classification>
  traces: Map<string, DecisionTrace>
  finds: Find[]
  activity: Activity[]
  meshes: Mesh[]
  signals: Signal[]
  strata: Stratum[]
  jobs: Job[]
  conversations: Conversation[]
  expeditions: Expedition[]
  drafts: Draft[]
  settings: Settings
  devInfo: DevInfo
  logs: LogLine[]
  pending: Map<string, PendingAction>
  /** Agree/Disagree marks on Shadow proposals, message id → 'agreed' | 'disagreed'. */
  shadowReviews: Map<string, 'agreed' | 'disagreed'>
  /** The user's own Importance per message (`set_importance`). */
  importance: Map<string, Importance>
  /** Shadow proposals the user has since decided on themselves; the backend keeps both rows. */
  shadowProposals: Map<string, Disposition>
  idempotent: Map<string, unknown>
  armed: boolean
  /** The dev build's mailbox writes, and the projected changes waiting on them. */
  writes: ProviderWrites
  held: number
  seq: number
}

/**
 * `?scenario=empty` in browser mode builds a mailbox with no account, so the
 * first-run Summary and the Task Centre's "Nothing connected" can be seen
 * without deleting the three fixture accounts by hand.
 */
function scenario(): string | null {
  try {
    return typeof location !== 'undefined' ? new URLSearchParams(location.search).get('scenario') : null
  } catch {
    return null
  }
}

export function buildState(): MockState {
  const empty = scenario() === 'empty'
  const digs = empty ? [] : buildDigs()
  const digIdByName = (n: string) => digs.find((d) => d.name.toLowerCase() === n.toLowerCase())?.id ?? null
  const specs = empty ? [] : messageSpecs()
  const messages = linkReplies(specs.map(buildMessage))
  const signals = buildSignals()
  // The fixture Meshes name the fixture accounts in their scope, so an empty mailbox has none.
  const meshes = empty ? [] : buildMeshes(digIdByName)
  const traces = new Map<string, DecisionTrace>()
  const classifications = new Map<string, Classification>()
  const specMap = new Map<string, MessageSpec>()
  specs.forEach((s, i) => {
    specMap.set(s.id, s)
    const t = buildTrace(s, messages[i], signals, meshes)
    if (t) traces.set(t.id, t)
    classifications.set(s.id, { disposition: s.disposition, strata: s.strata ?? [], findId: s.findId ?? null, handledByUserAt: s.mechanism === 'user' ? messages[i].date : null, shadow: !!s.shadow })
  })
  const finds = empty ? [] : buildFinds(messages, specs)
  return {
    digs,
    digGroups: buildDigGroups(),
    profile: buildProfile(),
    messages,
    specs: specMap,
    classifications,
    traces,
    finds,
    activity: empty ? [] : buildActivity(specs),
    meshes,
    namedLists: [],
    grades: {},
    installedPacks: [],
    signals,
    strata: buildStrata(),
    jobs: empty ? [] : buildJobs(),
    conversations: buildConversations(),
    expeditions: buildExpeditions(),
    drafts: buildDrafts(),
    settings: buildSettings(),
    devInfo: buildDevInfo(),
    logs: buildLogs(),
    pending: new Map(),
    shadowReviews: new Map(),
    importance: new Map(),
    shadowProposals: new Map(),
    idempotent: new Map(),
    armed: false,
    writes: 'hold',
    held: 0,
    seq: 1000,
  }
}

export function nextId(state: MockState, prefix: string): string {
  state.seq += 1
  return `${prefix}_${state.seq.toString(36)}`
}


/**
 * Real mail carries In-Reply-To and References; the specs do not, so the
 * mock threads each message onto the one before it in its thread. Without
 * this the threaded message view had nothing to nest in the browser.
 */
function linkReplies(messages: Message[]): Message[] {
  const byThread = new Map<string, Message[]>()
  for (const m of messages) byThread.set(m.threadId, [...(byThread.get(m.threadId) ?? []), m])
  for (const list of byThread.values()) {
    list.sort((a, b) => a.date.localeCompare(b.date))
    list.forEach((m, i) => {
      if (i === 0) return
      const parent = list[i - 1]
      m.headers.inReplyTo = parent.headers.messageId
      m.headers.references = [...parent.headers.references, parent.headers.messageId ?? ''].filter(Boolean)
    })
  }
  return messages
}
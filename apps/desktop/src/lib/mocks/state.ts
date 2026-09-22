/**
 * The mutable browser-mode state the mock commands read and the mock
 * actions change. Built fresh per session (and per test).
 */
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
  idempotent: Map<string, unknown>
  armed: boolean
  seq: number
}

export function buildState(): MockState {
  const digs = buildDigs()
  const digIdByName = (n: string) => digs.find((d) => d.name.toLowerCase() === n.toLowerCase())?.id ?? null
  const specs = messageSpecs()
  const messages = specs.map(buildMessage)
  const signals = buildSignals()
  const meshes = buildMeshes(digIdByName)
  const traces = new Map<string, DecisionTrace>()
  const classifications = new Map<string, Classification>()
  const specMap = new Map<string, MessageSpec>()
  specs.forEach((s, i) => {
    specMap.set(s.id, s)
    const t = buildTrace(s, messages[i], signals, meshes)
    if (t) traces.set(t.id, t)
    classifications.set(s.id, { disposition: s.disposition, strata: s.strata ?? [], findId: s.findId ?? null, handledByUserAt: s.mechanism === 'user' ? messages[i].date : null, shadow: !!s.shadow })
  })
  const finds = buildFinds(messages, specs)
  return {
    digs,
    digGroups: buildDigGroups(),
    profile: buildProfile(),
    messages,
    specs: specMap,
    classifications,
    traces,
    finds,
    activity: buildActivity(specs),
    meshes,
    signals,
    strata: buildStrata(),
    jobs: buildJobs(),
    conversations: buildConversations(),
    expeditions: buildExpeditions(),
    drafts: buildDrafts(),
    settings: buildSettings(),
    devInfo: buildDevInfo(),
    logs: buildLogs(),
    pending: new Map(),
    idempotent: new Map(),
    armed: false,
    seq: 1000,
  }
}

export function nextId(state: MockState, prefix: string): string {
  state.seq += 1
  return `${prefix}_${state.seq.toString(36)}`
}

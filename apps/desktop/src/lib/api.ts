/**
 * The typed command surface (docs/COMMANDS.md). One function per read
 * command; every mutation goes through `invoke`, which wraps the generic
 * `invoke_action` command so the Action layer stays the only path that
 * changes state (ARCHITECTURE.md §8). `confirm` re-invokes a PendingAction
 * with its id, which is how every confirmation tier completes.
 */
import { call } from './ipc'
import type { ActionInvocation } from '@/bindings/ActionInvocation'
import type { ActionMeta } from '@/bindings/ActionMeta'
import type { ActionOutcome } from '@/bindings/ActionOutcome'
import type { Activity } from '@/bindings/Activity'
import type { Conversation } from '@/bindings/Conversation'
import type { DecisionTrace } from '@/bindings/DecisionTrace'
import type { Dig } from '@/bindings/Dig'
import type { DigGroup } from '@/bindings/DigGroup'
import type { Draft } from '@/bindings/Draft'
import type { Expedition } from '@/bindings/Expedition'
import type { Explanation } from '@/bindings/Explanation'
import type { Find } from '@/bindings/Find'
import type { FindStatus } from '@/bindings/FindStatus'
import type { FindType } from '@/bindings/FindType'
import type { Mesh } from '@/bindings/Mesh'
import type { Message } from '@/bindings/Message'
import type { MessageSummary } from '@/bindings/MessageSummary'
import type { Page } from '@/bindings/Page'
import type { PageRequest } from '@/bindings/PageRequest'
import type { PendingAction } from '@/bindings/PendingAction'
import type { Signal } from '@/bindings/Signal'
import type { SignalEvaluation } from '@/bindings/SignalEvaluation'
import type { SimulationReport } from '@/bindings/SimulationReport'
import type { Stratum } from '@/bindings/Stratum'
import type { Summary } from '@/bindings/Summary'
import type { TaskCentre } from '@/bindings/TaskCentre'
import type { Thread } from '@/bindings/Thread'
import type { UsageSummary } from '@/bindings/UsageSummary'
import type { UserProfile } from '@/bindings/UserProfile'
import type {
  DevInfo,
  DirectorSendResult,
  InFlightCall,
  LogLine,
  MessageFilter,
  ProbeResult,
  Settings,
  TraceStats,
  UpdatePreview,
} from './types'

// ---------------------------------------------------------------------------
// Mutations

export interface InvokeOptions {
  digId?: string | null
  idempotencyKey?: string | null
  /** The user has already confirmed inline (a held button); the Action layer re-validates. */
  confirmed?: boolean
}

export function invoke<T = unknown>(action: string, args: Record<string, unknown> = {}, opts: InvokeOptions = {}): Promise<ActionOutcome & { result: T }> {
  const invocation: ActionInvocation = {
    action,
    args,
    principal: { kind: 'user' },
    digId: opts.digId ?? (typeof args.digId === 'string' ? args.digId : null),
    idempotencyKey: opts.idempotencyKey ?? null,
    confirmed: opts.confirmed ?? false,
    pendingId: null,
  }
  return call<ActionOutcome & { result: T }>('invoke_action', { invocation })
}

/** Re-invoke a pending action with its id: the Action layer re-validates and executes. */
export function confirm<T = unknown>(pending: PendingAction, opts: InvokeOptions = {}): Promise<ActionOutcome & { result: T }> {
  const args = (pending.args ?? {}) as Record<string, unknown>
  const invocation: ActionInvocation = {
    action: pending.action,
    args,
    principal: { kind: 'user' },
    digId: opts.digId ?? (typeof args.digId === 'string' ? args.digId : null),
    idempotencyKey: opts.idempotencyKey ?? null,
    confirmed: true,
    pendingId: pending.id,
  }
  return call<ActionOutcome & { result: T }>('invoke_action', { invocation })
}

export const listActions = () => call<ActionMeta[]>('list_actions')
export const listPending = () => call<PendingAction[]>('list_pending')

// ---------------------------------------------------------------------------
// Reads

export const page = (cursor: string | null = null, limit = 50): PageRequest => ({ cursor, limit })

export const summary = () => call<Summary>('summary')
export const taskCentre = () => call<TaskCentre>('task_centre')
export const listDigs = () => call<Dig[]>('list_digs')
export const listDigGroups = () => call<DigGroup[]>('list_dig_groups')

export const listMessages = (args: { digId?: string | null; filter: MessageFilter; page: PageRequest }) =>
  call<Page<MessageSummary>>('list_messages', { digId: args.digId ?? null, filter: args.filter, page: args.page })
export const getMessage = (digId: string, messageId: string) => call<Message>('get_message', { digId, messageId })
export const getThread = (digId: string, threadId: string) => call<Thread>('get_thread', { digId, threadId })
export const explain = (digId: string, messageId: string) => call<Explanation>('explain', { digId, messageId })
export const search = (args: { query: string; digId?: string | null; page: PageRequest }) =>
  call<Page<MessageSummary>>('search', { query: args.query, digId: args.digId ?? null, page: args.page })

export const listFinds = (args: { status?: FindStatus; findType?: FindType; digId?: string | null; page: PageRequest }) =>
  call<Page<Find>>('list_finds', { status: args.status ?? null, findType: args.findType ?? null, digId: args.digId ?? null, page: args.page })
export const getFind = (findId: string) => call<Find>('get_find', { findId })

export interface ActivityQuery {
  digId?: string | null
  actor?: string | null
  action?: string | null
  since?: string | null
  until?: string | null
  page: PageRequest
}
export const listActivity = (args: ActivityQuery) =>
  call<Page<Activity>>('list_activity', {
    digId: args.digId ?? null,
    actor: args.actor ?? null,
    action: args.action ?? null,
    since: args.since ?? null,
    until: args.until ?? null,
    page: args.page,
  })
export const getActivity = (activityId: string) => call<Activity>('get_activity', { activityId })

export const listMeshes = (includeDeleted = false) => call<Mesh[]>('list_meshes', { includeDeleted })
export const getMesh = (meshId: string, version?: number | null) => call<Mesh>('get_mesh', { meshId, version: version ?? null })
export const meshVersions = (meshId: string) => call<Mesh[]>('mesh_versions', { meshId })
export const parseMesh = (text: string) => call<Mesh>('parse_mesh', { text })
export const simulateMesh = (args: { text?: string; meshId?: string; version?: number; windowDays?: number }) =>
  call<SimulationReport>('simulate_mesh', { text: args.text ?? null, meshId: args.meshId ?? null, version: args.version ?? null, windowDays: args.windowDays ?? null })

export const listSignals = () => call<Signal[]>('list_signals')
export const signalVersions = (name: string) => call<Signal[]>('signal_versions', { name })
export const signalEvaluations = (digId: string, messageId: string) => call<SignalEvaluation[]>('signal_evaluations', { digId, messageId })

export const listStrata = () => call<Stratum[]>('list_strata')

export const listConversations = () => call<Conversation[]>('list_conversations')
export const getConversation = (conversationId: string) => call<Conversation>('get_conversation', { conversationId })
export const directorSend = (args: { conversationId?: string | null; message: string; digIds?: string[] | null }) =>
  call<DirectorSendResult>('director_send', { conversationId: args.conversationId ?? null, message: args.message, digIds: args.digIds ?? null })

export const listExpeditions = () => call<Expedition[]>('list_expeditions')
export const getExpedition = (expeditionId: string) => call<Expedition>('get_expedition', { expeditionId })

export const listDrafts = (digId: string) => call<Draft[]>('list_drafts', { digId })

export const getSettings = () => call<Settings>('get_settings')
export const getUserProfile = () => call<UserProfile>('get_user_profile')
export const privacyStatement = (digId?: string | null) => call<string>('privacy_statement', { digId: digId ?? null })

export const readTrace = (digId: string, traceId: string) => call<DecisionTrace>('read_trace', { digId, traceId })
export const recentTraces = (args: { digId?: string | null; limit?: number } = {}) =>
  call<DecisionTrace[]>('recent_traces', { digId: args.digId ?? null, limit: args.limit ?? null })
export const usageSummary = (sinceDays?: number) => call<UsageSummary>('usage_summary', { sinceDays: sinceDays ?? null })
export const traceStats = (args: { digId?: string | null; sinceDays?: number } = {}) =>
  call<TraceStats>('trace_stats', { digId: args.digId ?? null, sinceDays: args.sinceDays ?? null })

export const devInfo = () => call<DevInfo>('dev_info')
export const devArmed = () => call<boolean>('dev_armed')
export const probeJudge = () => call<ProbeResult>('probe_judge')
export const probeLlm = () => call<ProbeResult>('probe_llm')
export const probeProvider = (digId: string) => call<ProbeResult>('probe_provider', { digId })
export const readAppLogs = (args: { limit?: number; category?: string | null; level?: string | null; text?: string | null } = {}) =>
  call<LogLine[]>('read_app_logs', { limit: args.limit ?? null, category: args.category ?? null, level: args.level ?? null, text: args.text ?? null })
export const logCategories = () => call<string[]>('log_categories')
export const inFlightCalls = () => call<InFlightCall[]>('in_flight_calls')
export const exportDiagnostics = () => call<string>('export_diagnostics')
export const updatePreview = () => call<UpdatePreview | null>('update_preview')

// ---------------------------------------------------------------------------
// Query keys, so events and pages agree on what to invalidate.

export const qk = {
  summary: ['summary'] as const,
  taskCentre: ['task_centre'] as const,
  digs: ['digs'] as const,
  digGroups: ['dig_groups'] as const,
  messages: ['messages'] as const,
  message: (digId: string, messageId: string) => ['message', digId, messageId] as const,
  thread: (digId: string, threadId: string) => ['thread', digId, threadId] as const,
  explain: (digId: string, messageId: string) => ['explain', digId, messageId] as const,
  search: (query: string, digId: string | null) => ['search', query, digId] as const,
  finds: ['finds'] as const,
  find: (id: string) => ['find', id] as const,
  activity: ['activity'] as const,
  meshes: ['meshes'] as const,
  mesh: (id: string, version: number | null) => ['mesh', id, version] as const,
  meshVersions: (id: string) => ['mesh_versions', id] as const,
  signals: ['signals'] as const,
  signalVersions: (name: string) => ['signal_versions', name] as const,
  signalEvaluations: (digId: string, messageId: string) => ['signal_evaluations', digId, messageId] as const,
  strata: ['strata'] as const,
  conversations: ['conversations'] as const,
  conversation: (id: string) => ['conversation', id] as const,
  expeditions: ['expeditions'] as const,
  expedition: (id: string) => ['expedition', id] as const,
  drafts: (digId: string) => ['drafts', digId] as const,
  settings: ['settings'] as const,
  profile: ['profile'] as const,
  privacy: (digId: string | null) => ['privacy', digId] as const,
  trace: (digId: string, traceId: string) => ['trace', digId, traceId] as const,
  recentTraces: ['recent_traces'] as const,
  usage: ['usage'] as const,
  traceStats: ['trace_stats'] as const,
  devInfo: ['dev_info'] as const,
  devArmed: ['dev_armed'] as const,
  logs: ['logs'] as const,
  logCategories: ['log_categories'] as const,
  inFlight: ['in_flight'] as const,
  actions: ['actions'] as const,
  pending: ['pending'] as const,
  updatePreview: ['update_preview'] as const,
}

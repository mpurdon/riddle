/**
 * Spec-only shapes from docs/COMMANDS.md. These mirror the shapes the
 * backend will expose until a ts-rs binding exists; when one lands in
 * `src/bindings/`, the import here is swapped and nothing else moves.
 */
import type { BuildStamp } from '@/bindings/BuildStamp'
import type { FindType } from '@/bindings/FindType'
import type { SimulationReport } from '@/bindings/SimulationReport'

export type DispositionFilter = 'find' | 'filed' | 'spoil' | 'contamination' | 'unresolved'

export interface MessageFilter {
  disposition?: DispositionFilter
  findType?: FindType
  label?: string
  query?: string
  from?: string
  threadId?: string
  since?: string
  until?: string
  stratum?: string
  unclassifiedOnly?: boolean
  /** The Shadow review list: only messages with a proposal, shown as the proposal. */
  shadowOnly?: boolean
  /** false hides mail a newer one of its `+ Replaces` series superseded; true lists only that. */
  superseded?: boolean
  /** A place in the mail column (`label:Receipts`, `topic:Receipts`, `mailbox:INBOX`). */
  place?: string
}

export type { LogLevel as LogLevelSetting } from '@/bindings/LogLevel'
export type { JudgeSettings } from '@/bindings/JudgeSettings'
export type { LlmSettings } from '@/bindings/LlmSettings'
export type { NotificationMode } from '@/bindings/NotificationMode'
export type { NotificationSettings } from '@/bindings/NotificationSettings'
export type { Settings } from '@/bindings/Settings'

export interface SignalTestSample {
  messageId: string
  digId: string
  expected?: string
  got: string
  confidence: number
}

export interface SignalTestReport {
  name: string
  version: number
  sampleSize: number
  agreement: number
  falsePositives: number
  falseNegatives: number
  uncertain: number
  latencyMs: number
  costUsd: number | null
  distribution: Record<string, number>
  samples: SignalTestSample[]
}

export interface TraceStats {
  count: number
  escalated: number
  unresolved: number
  avgTotalMs: number
  avgJudgeMs: number
  judgeTokens: number
  judgeCostUsd: number | null
  llmCostUsd: number | null
}

export interface UpdatePreview {
  fromVersion: string
  toVersion: string
  changedSignals: number
  changedMeshes: number
  report: SimulationReport
}

export interface StoreInfo {
  label: string
  path: string
  sizeBytes: number
}

export interface DevInfo {
  build: BuildStamp
  dev: boolean
  os: string
  arch: string
  uptimeSeconds: number
  dataDir: string
  logDir: string
  configDir: string
  cacheDir: string
  stores: StoreInfo[]
  storeFault: string | null
  logLevel: string
}

export interface LogLine {
  at: string
  level: string
  category: string
  message: string
  payload: unknown | null
}

export interface InFlightCall {
  command: string
  elapsedMs: number
}

export interface ProbeResult {
  ok: boolean
  latencyMs: number
  message: string
}

export interface DirectorSendResult {
  conversationId: string
}

/** The line information `parse_mesh` returns in the detail of an `invalid` error. */
export interface MeshParseIssue {
  line: number
  column?: number
  message: string
}

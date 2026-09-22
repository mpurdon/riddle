/**
 * Spec-only shapes from docs/COMMANDS.md. These mirror the shapes the
 * backend will expose until a ts-rs binding exists; when one lands in
 * `src/bindings/`, the import here is swapped and nothing else moves.
 */
import type { Autonomy } from '@/bindings/Autonomy'
import type { BuildStamp } from '@/bindings/BuildStamp'
import type { FindType } from '@/bindings/FindType'
import type { PrivacyMode } from '@/bindings/PrivacyMode'
import type { ProjectionMode } from '@/bindings/ProjectionMode'
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
}

export type LogLevelSetting = 'error' | 'warn' | 'info' | 'debug' | 'trace'

export interface JudgeSettings {
  provider: 'jev' | 'null' | 'scripted'
  model: string
  keySource: 'env' | 'keychain' | 'ssm'
  ssmProfile?: string
  ssmParameter?: string
  apiKeyPresent: boolean
}

export interface LlmSettings {
  provider: 'bedrock' | 'null' | 'scripted'
  profile: string
  region: string
  escalationModel: string
  directorModel: string
}

export interface Settings {
  schemaVersion: number
  developerMode: boolean
  logLevel: LogLevelSetting
  notifyPriorityThreshold: number
  shadowPeriodDays: number
  judge: JudgeSettings
  llm: LlmSettings
  budgets: { dailyJudgeUsd: number; dailyLlmUsd: number; escalationPerDigUsd: number }
  gmail: { clientId: string; clientSecretPresent: boolean }
  autonomy: { default: Autonomy; perDig: Record<string, Autonomy>; perAction: Record<string, Autonomy> }
  projectionDefault: ProjectionMode
  privacyDefault: PrivacyMode
  launchAtLogin: boolean
  closeToHide: boolean
  /** Spoil digest cadence (README "Spoil"). Not in the spec table; the mock carries it. */
  spoilDigest?: 'daily' | 'weekly' | 'off'
}

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

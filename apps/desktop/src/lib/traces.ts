/**
 * Trace helpers for Developer Mode: the Jev request/response view derived
 * from a DecisionTrace, and the redacted export (§69, §89).
 */
import type { DecisionTrace } from '@/bindings/DecisionTrace'
import type { MeshOutcome } from '@/bindings/MeshOutcome'
import type { SignalEvaluation } from '@/bindings/SignalEvaluation'
import { num } from './format'

export interface JevRequestView {
  model: string | null
  signalSet: string[]
  signalVersions: Record<string, number>
  inputState: { messageId: string; digId: string; contextHash: string | null; facts: string[] }
  questions: { name: string; version: number; choices: string[] }[]
  configuration: { engine: string | null; cachedQuestions: number; freshQuestions: number }
  inputTokens: number
  timestamp: string
}

export interface JevResponseView {
  raw: { name: string; version: number; distribution: Record<string, number>; selected: string; cached: boolean }[]
  calibrated: Record<string, Record<string, number>>
  selected: Record<string, string>
  latencyMs: number
  tokenUsage: { input: number }
  costUsd: number | null
  requestHashes: string[]
}

const trueFacts = (t: DecisionTrace) =>
  Object.entries(t.facts)
    .filter(([, v]) => v === true)
    .map(([k]) => k)

export function jevRequest(t: DecisionTrace): JevRequestView {
  const fresh = t.signals.filter((s) => !s.cached)
  return {
    model: t.judgeIdentity,
    signalSet: t.signals.map((s) => s.signalName),
    signalVersions: Object.fromEntries(t.signals.map((s) => [s.signalName, s.signalVersion])),
    inputState: { messageId: t.messageId, digId: t.digId, contextHash: t.signals[0]?.contextHash ?? null, facts: trueFacts(t) },
    questions: t.signals.map((s) => ({ name: s.signalName, version: s.signalVersion, choices: Object.keys(s.distribution) })),
    configuration: { engine: t.judgeIdentity, cachedQuestions: t.signals.length - fresh.length, freshQuestions: fresh.length },
    inputTokens: num(t.cost.judgeInputTokens),
    timestamp: t.createdAt,
  }
}

export function jevResponse(t: DecisionTrace): JevResponseView {
  return {
    raw: t.signals.map((s) => ({ name: s.signalName, version: s.signalVersion, distribution: s.distribution, selected: s.selected, cached: s.cached })),
    calibrated: Object.fromEntries(t.signals.map((s) => [`${s.signalName}@v${s.signalVersion}`, s.calibrated])),
    selected: Object.fromEntries(t.signals.map((s) => [s.signalName, s.selected])),
    latencyMs: num(t.timing.judgeMs),
    tokenUsage: { input: num(t.cost.judgeInputTokens) },
    costUsd: t.cost.judgeCostUsd,
    requestHashes: t.signals.map((s) => s.requestHash).filter((x): x is string => !!x),
  }
}

/** Redacted export: identifiers, facts, distributions, timings and costs; never content, addresses or tokens. */
export function redactTrace(t: DecisionTrace): Record<string, unknown> {
  const sig = (s: SignalEvaluation) => ({ name: s.signalName, version: s.signalVersion, engine: s.engineIdentity, selected: s.selected, distribution: s.distribution, calibrated: s.calibrated, confidence: s.confidence, latencyMs: num(s.latencyMs), inputTokens: num(s.inputTokens), cached: s.cached, requestHash: s.requestHash })
  const { senderDomain: _domain, ...facts } = t.facts
  return {
    exportVersion: 1,
    redaction: 'default: no subjects, bodies, addresses or tokens; sender domain removed',
    trace: t.id,
    message: t.messageId,
    dig: t.digId,
    createdAt: t.createdAt,
    build: t.buildStamp,
    appVersion: t.appVersion,
    objective: t.objective,
    strategy: t.strategy,
    facts: { ...facts, deadlines: t.facts.deadlines.length },
    gate: t.gate,
    signals: t.signals.map(sig),
    mesh: t.meshId ? { id: t.meshId, version: t.meshVersion, name: t.meshName, matchedRule: t.matchedRule, guardVetoes: t.guardVetoes } : null,
    escalation: t.escalation ? { reason: t.escalation.reason, modelId: t.escalation.modelId, refined: t.escalation.refined.map(sig), latencyMs: num(t.escalation.latencyMs), rationale: '[redacted: model text]' } : null,
    conclusion: t.conclusion,
    confidence: t.confidence,
    mechanism: t.mechanism,
    timing: Object.fromEntries(Object.entries(t.timing).map(([k, v]) => [k, num(v)])),
    cost: { ...t.cost, judgeInputTokens: num(t.cost.judgeInputTokens), llmInputTokens: num(t.cost.llmInputTokens), llmOutputTokens: num(t.cost.llmOutputTokens) },
    shadow: t.shadow,
  }
}

/** `0.62` as the rule syntax writes it: `.62`. */
const two = (n: number) => n.toFixed(2).replace(/^0/, '')

/**
 * One Mesh's outcome in one line, the same words the backend's "Why?"
 * answer uses for its earlier Meshes: "Bills v3: payment_related .62,
 * needed .80". A view of the stored structure; the backend's line is the
 * reference and this one follows it.
 */
export function outcomeLine(o: MeshOutcome): string {
  const key = `${o.name} v${o.version}`
  const k = o.outcome
  switch (k.kind) {
    case 'matched':
      return `${key}: decided`
    case 'no_route_matched':
      return `${key}: no route matched`
    case 'margin_shortfall':
      return k.signal && k.score !== null && k.needed_score !== null
        ? `${key}: ${k.signal.split('@')[0]} ${two(k.score)}, needed ${two(k.needed_score)}`
        : `${key}: margin ${two(k.margin)}, needed ${two(k.needed)}`
    case 'unknown':
      return k.signals.length ? `${key}: ${k.signals.join(', ')} not scored` : `${key}: a Signal was not scored`
    case 'held_for_find':
      return `${key}: matched, set aside so an earlier borderline Find is not hidden`
    case 'not_reached':
      return `${key}: not reached`
  }
}

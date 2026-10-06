/**
 * Mock read commands. Each one derives from the mock state the way the
 * Rust read services would from the store, so the UI sees the same data
 * from every angle: a message's disposition, its Find, its trace and its
 * explanation all agree.
 */
import type { Importance } from '@/bindings/Importance'
import type { Disposition } from '@/bindings/Disposition'
import type { Activity } from '@/bindings/Activity'
import type { DecisionTrace } from '@/bindings/DecisionTrace'
import type { ClauseCheck } from '@/bindings/ClauseCheck'
import type { CmpOp } from '@/bindings/CmpOp'
import type { Explanation } from '@/bindings/Explanation'
import type { Expr } from '@/bindings/Expr'
import type { RouteCheck } from '@/bindings/RouteCheck'
import type { Find } from '@/bindings/Find'
import type { Dig } from '@/bindings/Dig'
import type { Mesh } from '@/bindings/Mesh'
import type { MeshOrder } from '@/bindings/MeshOrder'
import type { Pack } from '@/bindings/Pack'
import type { PackInfo } from '@/bindings/PackInfo'
import type { RuleCheckRow } from '@/bindings/RuleCheckRow'
import type { RuleSample } from '@/bindings/RuleSample'
import type { Message } from '@/bindings/Message'
import type { MessageSummary } from '@/bindings/MessageSummary'
import type { Page } from '@/bindings/Page'
import type { PageRequest } from '@/bindings/PageRequest'
import type { RiddleError } from '@/bindings/RiddleError'
import type { SimulationReport } from '@/bindings/SimulationReport'
import type { Summary } from '@/bindings/Summary'
import type { TaskCentre } from '@/bindings/TaskCentre'
import type { Thread } from '@/bindings/Thread'
import type { Transition } from '@/bindings/Transition'
import type { DispositionKind } from '@/bindings/DispositionKind'
import type { UsageSummary } from '@/bindings/UsageSummary'
import type { TrustStats } from '@/bindings/TrustStats'
import type { NamedList } from '@/bindings/NamedList'
import { belowAct } from '@/features/settings/accounts/model'
import { u64 } from '../format'
import type { MessageFilter, TraceStats, UpdatePreview, LogLine } from '../types'
import { evalExpr, evalMesh, MeshSyntaxError, parseMeshText, type EvalInput } from './dsl'
import { buildFacts, DIG_WORK } from './fixtures'
import type { MockState } from './state'

/** A RiddleError to throw, the shape the IPC layer would deliver; every mock module throws through it. */
export const err = (kind: RiddleError['kind'], message: string, detail: unknown = null): RiddleError => ({ kind, message, detail })

function pageOf<T>(items: T[], page: PageRequest | undefined): Page<T> {
  const limit = page?.limit ?? 50
  const offset = page?.cursor ? Number(page.cursor) : 0
  const slice = items.slice(offset, offset + limit)
  const next = offset + limit < items.length ? String(offset + limit) : null
  return { items: slice, nextCursor: next, total: u64(items.length) }
}

/**
 * A list row. As in the backend, a message whose only classification is a
 * Shadow proposal reads as unsifted in the plain list and as the proposal
 * in the Shadow review list (`shadowOnly`).
 */
/** As the backend derives it when no rule says: hidden mail is Low. */
function importanceOf(d: Disposition | null): Importance {
  return d && (d.kind === 'spoil' || d.kind === 'filed' || d.kind === 'contamination') ? 'low' : 'normal'
}

export function toSummary(state: MockState, m: Message, shadowOnly = false): MessageSummary {
  const raw = state.classifications.get(m.id)
  // The proposal outlives a disagreement, as the backend's shadow row does.
  const proposal = state.shadowProposals.get(m.id) ?? (raw?.shadow ? raw.disposition : undefined)
  const c = shadowOnly ? (proposal ? { ...(raw ?? { strata: [], findId: null, handledByUserAt: null, shadow: true }), disposition: proposal, shadow: true } : undefined) : raw?.shadow ? undefined : raw
  return {
    id: m.id,
    digId: m.digId,
    threadId: m.threadId,
    subject: m.subject,
    from: m.from,
    date: m.date,
    snippet: m.snippet,
    labels: m.labels,
    isSent: m.isSent,
    hasAttachments: m.attachments.length > 0,
    disposition: c?.disposition ?? null,
    strata: c?.strata ?? [],
    findId: c?.findId ?? null,
    shadow: shadowOnly && !!c?.shadow,
    shadowReview: shadowOnly ? (state.shadowReviews.get(m.id) ?? null) : null,
    importance: state.importance.get(m.id) ?? importanceOf(c?.disposition ?? null),
    importanceSetByYou: state.importance.has(m.id),
  }
}

/** What Sift does to this account's mailbox now, the way the backend's projection gate answers it. */
export function withMailbox(state: MockState, d: Dig): Dig {
  const level = state.settings.autonomy.perDig[d.id] ?? state.settings.autonomy.default
  const writes = d.projection === 'none' ? 'nothing' : d.projection === 'labels_only' ? 'labels' : 'labels_and_archive'
  if (writes === 'nothing') return { ...d, mailbox: { writes, held: null } }
  if (belowAct(level)) return { ...d, mailbox: { writes: 'nothing', held: 'autonomy' } }
  if (state.devInfo.dev && !state.armed) return { ...d, mailbox: { writes: 'nothing', held: 'dev_unarmed' } }
  return { ...d, mailbox: { writes, held: null } }
}

/** The Meshes and Strata that name a group, as the backend lists them. */
export function groupUsedBy(state: MockState, name: string): string[] {
  const latest = new Map<string, Mesh>()
  for (const m of state.meshes) {
    const cur = latest.get(m.id)
    if (!cur || m.version > cur.version) latest.set(m.id, m)
  }
  const meshes = [...latest.values()].filter((m) => m.status !== 'deleted' && m.scope.kind === 'dig_group' && m.scope.group === name).map((m) => `Mesh "${m.name}"`)
  const strata = state.strata.filter((s) => !s.deletedAt && s.scopeGroup === name).map((s) => `Stratum "${s.name}"`)
  return [...meshes, ...strata]
}

export function findTitleFor(state: MockState, messageId: string): string | null {
  const c = state.classifications.get(messageId)
  if (!c?.findId) return null
  return state.finds.find((f) => f.id === c.findId)?.title ?? null
}

const byDateDesc = (a: Message, b: Message) => (a.date < b.date ? 1 : a.date > b.date ? -1 : 0)

export function listMessages(state: MockState, args: { digId?: string | null; filter?: MessageFilter; page?: PageRequest }): Page<MessageSummary> {
  const f = args.filter ?? {}
  let items = state.messages.filter((m) => {
    if (args.digId && m.digId !== args.digId) return false
    const c = state.classifications.get(m.id)
    if (f.unclassifiedOnly && c?.disposition) return false
    // The Shadow review list: only messages with a proposal, shown as the proposal.
    if (f.shadowOnly && !(c?.shadow || state.shadowProposals.has(m.id))) return false
    if (f.disposition && c?.disposition?.kind !== f.disposition) return false
    if (f.findType && !(c?.disposition?.kind === 'find' && c.disposition.find_type === f.findType)) return false
    if (f.label && !m.labels.includes(f.label)) return false
    if (f.stratum && !(c?.strata ?? []).includes(f.stratum)) return false
    if (f.from && !(m.from.email.toLowerCase().includes(f.from.toLowerCase()) || (m.from.name ?? '').toLowerCase().includes(f.from.toLowerCase()))) return false
    if (f.threadId && m.threadId !== f.threadId) return false
    // The mocks keep no `+ Replaces` series, so nothing in them is superseded.
    if (f.superseded === true) return false
    if (f.since && m.date < f.since) return false
    if (f.until && m.date > f.until) return false
    if (f.query) {
      const q = f.query.toLowerCase()
      if (!(m.subject.toLowerCase().includes(q) || m.snippet.toLowerCase().includes(q) || m.from.email.toLowerCase().includes(q) || (m.from.name ?? '').toLowerCase().includes(q))) return false
    }
    return true
  })
  items = items.sort(byDateDesc)
  return pageOf(items.map((m) => toSummary(state, m, !!f.shadowOnly)), args.page)
}

/**
 * The Priority inbox, as the backend derives it: a chosen label, a chosen
 * Stratum, or a source of an open Find at or above the chosen priority (of
 * the chosen types; none means any), OR-combined.
 */
export function priorityInbox(state: MockState, args: { digId?: string | null; page?: PageRequest }): Page<MessageSummary> {
  const p = state.settings.priorityInbox
  const min = p.minFindPriority ?? 0
  const found = new Set(state.finds.filter((f) => f.status === 'open' && f.priority >= min && (!p.findTypes.length || p.findTypes.includes(f.findType))).flatMap((f) => f.sources.map((s) => s.messageId)))
  const items = state.messages
    .filter((m) => {
      if (args.digId && m.digId !== args.digId) return false
      const c = state.classifications.get(m.id)
      const strata = c && !c.shadow ? c.strata : []
      return p.labels.some((l) => m.labels.includes(l)) || strata.some((s) => p.strata.includes(s)) || found.has(m.id)
    })
    .sort(byDateDesc)
  return pageOf(items.map((m) => toSummary(state, m)), args.page)
}

export function search(state: MockState, args: { query: string; digId?: string | null; page?: PageRequest; shadowOnly?: boolean }): Page<MessageSummary> {
  const filter: MessageFilter = { shadowOnly: args.shadowOnly || undefined }
  const words: string[] = []
  for (const tok of args.query.split(/\s+/).filter(Boolean)) {
    const m = /^(from|is|stratum|label|dig|has):(.+)$/i.exec(tok)
    if (!m) {
      words.push(tok)
      continue
    }
    const [, k, v] = m
    if (k === 'from') filter.from = v
    else if (k === 'is' && ['find', 'filed', 'spoil', 'contamination', 'unresolved'].includes(v)) filter.disposition = v as MessageFilter['disposition']
    else if (k === 'stratum') filter.stratum = v
    else if (k === 'label') filter.label = v.toUpperCase()
    else words.push(tok)
  }
  filter.query = words.join(' ') || undefined
  return listMessages(state, { digId: args.digId, filter, page: args.page })
}

export function getMessage(state: MockState, digId: string, messageId: string): Message {
  const m = state.messages.find((x) => x.id === messageId && x.digId === digId)
  if (!m) throw err('not_found', 'message not found')
  return m
}

export function getThread(state: MockState, digId: string, threadId: string): Thread {
  const messages = state.messages.filter((x) => x.threadId === threadId && x.digId === digId).sort((a, b) => (a.date < b.date ? -1 : 1))
  if (!messages.length) throw err('not_found', 'thread not found')
  return { id: threadId, digId, providerId: `p_${threadId}`, subject: messages[0].subject, messages }
}

export function explain(state: MockState, digId: string, messageId: string): Explanation {
  const m = getMessage(state, digId, messageId)
  const c = state.classifications.get(m.id)
  const t = state.traces.get(`tr_${m.id}`)
  if (!c?.disposition || !t) throw err('not_found', 'this message has not been sifted yet; there is nothing to explain')
  const spec = state.specs.get(m.id)
  return {
    messageId: m.id,
    disposition: c.disposition,
    mechanism: t.mechanism,
    meshName: t.meshName,
    meshVersion: t.meshVersion,
    matched: t.matchedRule,
    signals: t.signals.map((e) => ({ name: e.signalName, version: e.signalVersion, score: e.distribution.YES ?? e.confidence, selected: e.selected, cached: e.cached })),
    gate: t.gate,
    escalated: !!t.escalation,
    llmRationale: t.escalation?.rationale ?? null,
    traceId: t.id,
    shadow: c.shadow,
    reason: spec?.reason ?? t.matchedRule ?? '',
    routes: mockRouteChecks(state, m, t),
    importance: state.importance.get(m.id) ?? importanceOf(c.disposition),
    importanceSetByYou: state.importance.has(m.id),
  }
}

const OP: Record<CmpOp, string> = { gt: '>', gte: '>=', lt: '<', lte: '<=', eq: '=', neq: '!=', contains: 'CONTAINS' }
const num2 = (n: number) => (n < 1 ? String(n).replace(/^0/, '') : String(n))
function exprText(e: Expr): string {
  switch (e.kind) {
    case 'signal':
      return `${e.signal}@v${e.version}${e.choice ? `:${e.choice}` : ''} ${OP[e.op]} ${num2(e.threshold)}`
    case 'fact':
      return e.op === 'eq' && e.value.type === 'bool' && e.value.value === true ? e.fact : `${e.fact} ${OP[e.op]} ${String(e.value.value)}`
    case 'in_list':
      return `${e.subject} IN ${e.list}`
    case 'in_named_list':
      return `${e.subject} IN list:${e.list}`
    case 'not':
      return `NOT ${exprText(e.item)}`
    case 'and':
      return e.items.map(exprText).join(' AND ')
    case 'or':
      return `(${e.items.map(exprText).join(' OR ')})`
    case 'always':
      return 'ELSE'
  }
}

/** The mock's stand-in for the backend's route replay. */
function mockRouteChecks(state: MockState, m: Message, t: DecisionTrace): RouteCheck[] {
  const mesh = state.meshes.find((x) => x.name === t.meshName && x.version === t.meshVersion)
  if (!mesh) return []
  const input = evalInputFor(state, m)
  const signals = input.signals
  const check = (e: Expr): ClauseCheck => {
    const missing: string[] = []
    const ok = evalExpr(e, input, missing)
    const signal = e.kind === 'signal' ? `${e.signal}@v${e.version}` : null
    const actual = signal ? signals[signal] : e.kind === 'fact' ? input.facts[e.fact] : undefined
    return { text: exprText(e), actual: actual === undefined ? null : typeof actual === 'number' ? actual.toFixed(2) : String(actual), result: missing.length ? 'unknown' : ok ? 'pass' : 'fail', signal }
  }
  let done = false
  return mesh.routes.map((r) => {
    const clauses = r.condition.kind === 'always' ? [] : r.condition.kind === 'and' ? r.condition.items.map(check) : [check(r.condition)]
    let outcome: RouteCheck['outcome'] = 'not_reached'
    if (!done) {
      const missing: string[] = []
      const ok = evalExpr(r.condition, input, missing)
      outcome = ok ? 'matched' : missing.length ? 'unknown' : 'no_match'
      done = outcome !== 'no_match'
    }
    const o = r.output
    const output = [o.disposition ? (o.disposition.kind === 'find' ? `Find: ${o.disposition.find_type}` : o.disposition.kind) : o.escalate ? 'Escalate' : '', ...o.strata.map((x) => `Stratum: ${x}`)].filter(Boolean).join(' + ')
    return { when: null, clauses, output, outcome }
  })
}

export function listFinds(state: MockState, args: { status?: string | null; findType?: string | null; digId?: string | null; page?: PageRequest }): Page<Find> {
  const items = state.finds
    .filter((f) => (!args.status || f.status === args.status) && (!args.findType || f.findType === args.findType) && (!args.digId || f.digIds.includes(args.digId)))
    .sort((a, b) => b.priority - a.priority || (a.createdAt < b.createdAt ? 1 : -1))
  return pageOf(items, args.page)
}

export function listActivity(state: MockState, args: { digId?: string | null; actor?: string | null; action?: string | null; since?: string | null; until?: string | null; page?: PageRequest }): Page<Activity> {
  const items = state.activity
    .filter((a) => (!args.digId || a.digId === args.digId || a.digId === null) && (!args.actor || a.actor === args.actor) && (!args.action || a.action === args.action) && (!args.since || a.createdAt >= args.since) && (!args.until || a.createdAt <= args.until))
    .sort((a, b) => (a.createdAt < b.createdAt ? 1 : -1))
  return pageOf(items, args.page)
}

export function summary(state: MockState, digId: string | null = null): Summary {
  const digs = digId ? state.digs.filter((d) => d.id === digId) : state.digs
  const inScope = (id: string) => !digId || id === digId
  const messageIds = new Set(state.messages.filter((m) => inScope(m.digId)).map((m) => m.id))
  const cls = [...state.classifications.entries()].filter(([id]) => messageIds.has(id)).map(([, c]) => c)
  const catching = digs.find((d) => d.status.state === 'catching_up')
  const job = state.jobs.find((j) => j.kind === 'catch_up' && j.status.state === 'running' && (!j.digId || inScope(j.digId)))
  const caughtUp = !catching && !job
  const shadowDig = digs.find((d) => d.status.state === 'shadow')
  const degraded = digs.filter((d) => d.status.state === 'degraded').map((d) => (d.status.state === 'degraded' ? `${d.name}: ${d.status.reason}` : ''))
  const openFinds = state.finds.filter((f) => f.status === 'open' && (!digId || f.digIds.includes(digId))).length
  // The fixture mailbox is a sample of a larger history; the constants stand in for the part not modelled.
  const scale = digId ? 1 / Math.max(1, state.digs.length) : 1
  const base = (n: number) => Math.round(n * scale)
  return {
    asOf: new Date().toISOString(),
    caughtUp,
    since: new Date(Date.now() - 22 * 3600_000).toISOString(),
    arrived: u64(base(312) + messageIds.size - base(62)),
    digCount: digs.length,
    findsOpen: u64(openFinds),
    unresolved: u64(cls.filter((c) => c.disposition?.kind === 'unresolved').length),
    filed: u64(Math.max(0, base(41) + cls.filter((c) => c.disposition?.kind === 'filed').length - base(10))),
    spoil: u64(Math.max(0, base(236) + cls.filter((c) => c.disposition?.kind === 'spoil').length - base(37))),
    contamination: u64(Math.max(0, base(26) + cls.filter((c) => c.disposition?.kind === 'contamination').length - base(3))),
    remainingToSift: u64(job ? Number(job.progress.total) - Number(job.progress.done) : 0),
    totalToSift: u64(job ? Number(job.progress.total) : 0),
    directorNote: openFinds ? (caughtUp ? 'Two of the Finds are probably worth handling before your first meeting: the Jeep brakes and PR #412.' : 'Adam and PR #412 are worth handling first; the rest may change while catch-up finishes.') : null,
    shadow: shadowDig ? { day: shadowDig.status.state === 'shadow' ? shadowDig.status.day : 0, of: 7, wouldSpoil: u64(412), wouldFind: u64(19), disagreements: u64(6) } : null,
    lastSyncedAt: new Date(Date.now() - 2 * 60_000).toISOString(),
    projectionPending: u64(base(41)),
    degraded,
  }
}

export function taskCentre(state: MockState): TaskCentre {
  const degraded = state.digs.some((d) => d.status.state === 'degraded') || !!state.devInfo.storeFault
  const catching = state.digs.some((d) => d.status.state === 'catching_up' || d.status.state === 'sifting') || state.jobs.some((j) => (j.kind === 'catch_up' || j.kind === 'initial_sync') && j.status.state === 'running')
  return {
    overall: degraded ? 'degraded' : catching ? 'catching_up' : 'caught_up',
    digs: state.digs.map((d) => ({ digId: d.id, name: d.name, status: d.status })),
    jobs: [...state.jobs].sort((a, b) => rank(a.status.state) - rank(b.status.state) || (a.updatedAt < b.updatedAt ? 1 : -1)),
    storeFault: state.devInfo.storeFault,
    asOf: new Date().toISOString(),
  }
}

const rank = (s: string) => ({ running: 0, paused: 1, queued: 2, failed: 3, completed: 4, cancelled: 5 })[s] ?? 9

// ---------------------------------------------------------------------------
// Meshes and simulation

export function latestMeshes(state: MockState, includeDeleted: boolean): Mesh[] {
  const byId = new Map<string, Mesh>()
  for (const m of state.meshes) {
    const cur = byId.get(m.id)
    if (!cur || m.version > cur.version) byId.set(m.id, m)
  }
  return [...byId.values()].filter((m) => includeDeleted || !m.deletedAt).sort((a, b) => a.priority - b.priority || a.name.localeCompare(b.name))
}

export function getMesh(state: MockState, meshId: string, version: number | null): Mesh {
  const vs = state.meshes.filter((m) => m.id === meshId)
  if (!vs.length) throw err('not_found', 'Mesh not found')
  if (version !== null && version !== undefined) {
    const v = vs.find((m) => m.version === version)
    if (!v) throw err('not_found', `Mesh version ${version} not found`)
    return v
  }
  return vs.reduce((a, b) => (a.version > b.version ? a : b))
}

export function parseMesh(state: MockState, text: string): Mesh {
  const digIdByName = (n: string) => state.digs.find((d) => d.name.toLowerCase() === n.toLowerCase())?.id ?? null
  try {
    const p = parseMeshText(text, { digIdByName })
    return { id: 'unsaved', name: p.name, version: p.version, scope: p.scope, priority: p.priority ?? 50, routes: p.routes, guards: p.guards, autonomy: p.autonomy, status: 'draft', fallback: false, sourceText: text, createdAt: new Date().toISOString(), createdBy: 'user', changeNote: null, shadowStartedAt: null, shadowAgreed: 0, shadowProposed: 0, deletedAt: null }
  } catch (e) {
    if (e instanceof MeshSyntaxError) throw e.toRiddleError()
    throw e
  }
}

function inScope(state: MockState, mesh: { scope: Mesh['scope'] }, m: Message): boolean {
  const s = mesh.scope
  if (s.kind === 'all_digs') return true
  if (s.kind === 'dig') return m.digId === s.dig_id
  const dig = state.digs.find((d) => d.id === m.digId)
  return dig?.group === s.group
}

export function evalInputFor(state: MockState, m: Message): EvalInput {
  const spec = state.specs.get(m.id)
  const trace = state.traces.get(`tr_${m.id}`)
  const signals: Record<string, number> = {}
  for (const e of trace?.signals ?? []) {
    const p = e.distribution.YES ?? e.confidence
    signals[`${e.signalName}@v${e.signalVersion}`] = p
    for (const [choice, v] of Object.entries(e.distribution)) signals[`${e.signalName}@v${e.signalVersion}:${choice}`] = v
  }
  const facts = spec ? buildFacts(spec, m.from) : trace?.facts
  const flat: Record<string, boolean | number | string> = {}
  if (facts) {
    for (const [k, v] of Object.entries(facts)) {
      const snake = k.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`)
      if (typeof v === 'boolean' || typeof v === 'number' || typeof v === 'string') flat[snake] = v
      else if (v === null) flat[snake] = ''
    }
    flat.deadline_count = facts.deadlines.length
  }
  const email = m.from.email.toLowerCase()
  const domain = `@${email.split('@')[1]}`
  const nh = state.profile.neverHidden.map((x) => x.toLowerCase())
  const lists: Record<NamedList, boolean> = {
    never_hidden: nh.includes(email) || nh.includes(domain) || !!facts?.senderInNeverHidden,
    contacts: !!facts?.senderInContacts,
    vips: state.profile.vips.includes(email) || !!facts?.senderIsVip,
    organisation: state.profile.organisationDomains.some((d) => email.endsWith(`@${d}`)) || !!facts?.sameOrganisation,
    family: state.profile.family.includes(email),
  }
  return { signals, facts: flat, lists }
}

export function simulate(state: MockState, args: { text?: string | null; meshId?: string | null; version?: number | null; windowDays?: number | null }): SimulationReport {
  const windowDays = args.windowDays ?? 90
  const digIdByName = (n: string) => state.digs.find((d) => d.name.toLowerCase() === n.toLowerCase())?.id ?? null
  let proposed: { name: string; version: number; scope: Mesh['scope']; routes: Mesh['routes']; guards: Mesh['guards'] }
  let from: Mesh | null = null
  if (args.text) {
    try {
      proposed = parseMeshText(args.text, { digIdByName })
    } catch (e) {
      if (e instanceof MeshSyntaxError) throw e.toRiddleError()
      throw e
    }
    if (args.meshId) from = getMesh(state, args.meshId, null)
  } else if (args.meshId) {
    proposed = getMesh(state, args.meshId, args.version ?? null)
    const vs = state.meshes.filter((m) => m.id === args.meshId && m.version < proposed.version)
    from = vs.length ? vs.reduce((a, b) => (a.version > b.version ? a : b)) : null
  } else throw err('invalid', 'simulate_mesh needs text or a meshId')

  const since = new Date(Date.now() - windowDays * 86_400_000).toISOString()
  // Gates run before any Mesh and user decisions stand, so only messages a
  // Mesh or the confidence policy decided can change under a new Mesh.
  const meshDecided = (m: Message) => {
    const t = state.traces.get(`tr_${m.id}`)
    return !t || !['gate', 'deterministic', 'user', 'provider', 'temporal'].includes(t.mechanism)
  }
  const candidates = state.messages.filter((m) => m.date >= since && inScope(state, proposed, m) && state.classifications.get(m.id)?.disposition && meshDecided(m))
  const counts = new Map<string, number>()
  let unchanged = 0
  let judgeCalls = 0
  const sample: string[] = []
  const proposedId = args.meshId ?? null
  for (const m of candidates) {
    const input = evalInputFor(state, m)
    const before = state.classifications.get(m.id)!.disposition!.kind
    const r = evalMesh(proposed, input)
    // A Mesh claims a message through a specific route. The catch-all ELSE
    // only applies to mail this Mesh already decided; it does not swallow
    // the rest of the Dig from other Meshes or the confidence policy.
    const decidedByThis = proposedId !== null && state.traces.get(`tr_${m.id}`)?.meshId === proposedId
    if (r.matchedAlways && !decidedByThis) {
      unchanged += 1
      continue
    }
    if (r.missingSignals.length) judgeCalls += r.missingSignals.length
    const after: DispositionKind = r.disposition ? r.disposition.kind : r.escalate ? 'unresolved' : before
    if (after === before) unchanged += 1
    else {
      const key = `${before}>${after}`
      counts.set(key, (counts.get(key) ?? 0) + 1)
      if (sample.length < 10) sample.push(m.id)
    }
  }
  const transitions: Transition[] = [...counts.entries()].map(([k, count]) => {
    const [f, t] = k.split('>') as [DispositionKind, DispositionKind]
    return { from: f, to: t, count: u64(count) }
  })
  const fromVersion = from?.version ?? null
  return {
    meshId: args.meshId ?? null,
    meshName: proposed.name,
    fromVersion,
    toVersion: args.text ? (from ? from.version + 1 : proposed.version) : proposed.version,
    historicalMessages: u64(candidates.length),
    unchanged: u64(unchanged),
    changed: u64(candidates.length - unchanged),
    transitions: transitions.sort((a, b) => Number(b.count) - Number(a.count)),
    newJudgeCallsRequired: u64(judgeCalls),
    joinsSeries: u64(0),
    recordedChanges: u64(0),
    rescore: u64(0),
    takenFirstBy: [],
    takesFrom: [],
    estimatedCostUsd: judgeCalls ? Math.round(judgeCalls * 1900 * 0.042) / 1_000_000 : 0,
    sampleChanged: sample,
    windowDays,
  }
}

/** Running Meshes with the grades given. The mock does not record which Mesh decided a message, so every Mesh offers the same samples. */
export function ruleCheck(state: MockState): RuleCheckRow[] {
  const decided = [...state.classifications.values()].filter((k) => k.disposition).length
  return latestMeshes(state, false)
    .filter((m) => m.status === 'active' || m.status === 'shadow')
    .sort((a, b) => a.priority - b.priority || a.name.localeCompare(b.name))
    .map((m) => {
      const grades = Object.entries(state.grades).filter(([k]) => k.startsWith(`${m.id}:`)).map(([, v]) => v)
      return { meshId: m.id, meshName: m.name, priority: m.priority, status: m.status, fallback: m.fallback, decided: u64(decided), right: u64(grades.filter((g) => g === 'right').length), wrong: u64(grades.filter((g) => g === 'wrong').length) }
    })
}

export function ruleSamples(state: MockState, meshId: string, limit: number): RuleSample[] {
  return [...state.classifications.entries()]
    .flatMap(([messageId, k]) => {
      const m = state.messages.find((x) => x.id === messageId)
      return m && k.disposition ? [{ digId: m.digId, messageId: m.id, from: m.from, subject: m.subject, date: m.date, disposition: k.disposition, strata: k.strata, grade: state.grades[`${meshId}:${m.id}`] ?? null }] : []
    })
    .slice(0, limit)
}

/** A small catalogue: the mock does not ship the real pack files. */
export function packCatalogue(state: MockState): PackInfo[] {
  const shopping: Pack = {
    id: 'shopping',
    name: 'Shopping and promotions',
    version: 1,
    area: 'Shopping',
    summary: 'Promotions from stores that write daily or weekly are set aside; stores you name keep showing.',
    band: 'general',
    detect: { sender_domains: [], gmail_categories: ['promotions'], min_messages: 20, always: false },
    lists: [{ name: 'keep_promotions', description: 'Stores whose promotions you still want to see', question: "Which stores' or services' promotions do you still want to see?", default: [] }],
    meshes: [{ text: 'Mesh: Frequent promotions\nROUTES\nIF marketing@v1 > .70 AND NOT sender_domain IN list:keep_promotions → Spoil', band: null, fallback: false, rank: 7 }],
  }
  const installed = state.installedPacks.find((i) => i.id === shopping.id) ?? null
  const keep = state.namedLists.find((l) => l.name === 'keep_promotions')
  return [{ pack: shopping, builtin: true, installed, meshNames: ['Frequent promotions'], conflicts: [], listEntries: { keep_promotions: keep?.entries ?? [] } }]
}

/** The running Meshes in run order; the mock does not replay mail, so counts are zero. */
export function meshOrder(state: MockState, windowDays: number): MeshOrder {
  const specificity = (m: Mesh) => (m.scope.kind === 'dig' ? 0 : m.scope.kind === 'dig_group' ? 1 : 2)
  const running = latestMeshes(state, false)
    .filter((m) => m.status === 'active' || m.status === 'shadow')
    .sort((a, b) => specificity(a) - specificity(b) || a.priority - b.priority || a.name.localeCompare(b.name))
  return {
    windowDays,
    messages: u64(0),
    rows: running.map((m, i) => ({ position: i + 1, meshId: m.id, meshName: m.name, version: m.version, status: m.status, scope: m.scope, priority: m.priority, fallback: m.fallback, decided: u64(0), takenFirstBy: [] })),
  }
}

// ---------------------------------------------------------------------------
// Traces, usage, dev

export function recentTraces(state: MockState, args: { digId?: string | null; limit?: number | null }): DecisionTrace[] {
  return [...state.traces.values()]
    .filter((t) => !args.digId || t.digId === args.digId)
    .sort((a, b) => (a.createdAt < b.createdAt ? 1 : -1))
    .slice(0, args.limit ?? 50)
}

export function readTrace(state: MockState, digId: string, traceId: string): DecisionTrace {
  const t = state.traces.get(traceId)
  if (!t || t.digId !== digId) throw err('not_found', 'trace not found')
  return t
}

export function usageSummary(state: MockState, sinceDays: number | null): UsageSummary {
  const days = sinceDays ?? 1
  const traces = [...state.traces.values()]
  const judgeCalls = traces.reduce((a, t) => a + t.cost.judgeQuestions, 0)
  const judgeTokens = traces.reduce((a, t) => a + Number(t.cost.judgeInputTokens), 0)
  const judgeUsd = traces.reduce((a, t) => a + (t.cost.judgeCostUsd ?? 0), 0)
  const llmCalls = traces.filter((t) => t.escalation).length
  const llmIn = traces.reduce((a, t) => a + Number(t.cost.llmInputTokens), 0)
  const llmOut = traces.reduce((a, t) => a + Number(t.cost.llmOutputTokens), 0)
  const llmUsd = traces.reduce((a, t) => a + (t.cost.llmCostUsd ?? 0), 0)
  const director = state.conversations.flatMap((c) => c.turns.filter((t) => t.role === 'director'))
  const directorUsd = director.reduce((a, t) => a + (t.costUsd ?? 0), 0)
  const directorIn = director.reduce((a, t) => a + Number(t.inputTokens), 0)
  const directorOut = director.reduce((a, t) => a + Number(t.outputTokens), 0)
  const expUsd = state.expeditions.reduce((a, e) => a + e.spent.costUsd, 0)
  const expCalls = state.expeditions.reduce((a, e) => a + e.spent.toolCalls, 0)
  const scale = Math.max(1, days)
  return {
    since: new Date(Date.now() - days * 86_400_000).toISOString(),
    calls: u64((judgeCalls + llmCalls + director.length + expCalls) * scale),
    inputTokens: u64((judgeTokens + llmIn + directorIn) * scale),
    outputTokens: u64((llmOut + directorOut) * scale),
    pricedUsd: Math.round((judgeUsd + llmUsd + directorUsd + expUsd) * scale * 10000) / 10000,
    unpricedCalls: u64(3),
    byTask: [
      ['sift', Math.round((judgeUsd + llmUsd) * scale * 10000) / 10000, u64((judgeCalls + llmCalls) * scale)],
      ['director', Math.round(directorUsd * scale * 10000) / 10000, u64(director.length * scale)],
      ['expedition', Math.round(expUsd * scale * 10000) / 10000, u64(expCalls * scale)],
      ['simulation', 0, u64(2 * scale)],
      ['probe', 0, u64(3)],
    ],
    byProvider: [
      ['jev', Math.round(judgeUsd * scale * 10000) / 10000, u64(judgeCalls * scale)],
      ['bedrock', Math.round((llmUsd + directorUsd + expUsd) * scale * 10000) / 10000, u64((llmCalls + director.length + expCalls) * scale)],
      ['null', 0, u64(3)],
    ],
  }
}

export function traceStats(state: MockState, args: { digId?: string | null; sinceDays?: number | null }): TraceStats {
  const traces = [...state.traces.values()].filter((t) => !args.digId || t.digId === args.digId)
  const n = traces.length || 1
  return {
    count: traces.length,
    escalated: traces.filter((t) => t.escalation).length,
    unresolved: traces.filter((t) => t.conclusion.kind === 'unresolved').length,
    avgTotalMs: Math.round(traces.reduce((a, t) => a + Number(t.timing.totalMs), 0) / n),
    avgJudgeMs: Math.round(traces.reduce((a, t) => a + Number(t.timing.judgeMs), 0) / n),
    judgeTokens: traces.reduce((a, t) => a + Number(t.cost.judgeInputTokens), 0),
    judgeCostUsd: Math.round(traces.reduce((a, t) => a + (t.cost.judgeCostUsd ?? 0), 0) * 10000) / 10000,
    llmCostUsd: Math.round(traces.reduce((a, t) => a + (t.cost.llmCostUsd ?? 0), 0) * 10000) / 10000,
  }
}

export function privacyStatement(state: MockState, digId: string | null): string {
  const dig = digId ? state.digs.find((d) => d.id === digId) : null
  const lines = [
    'Your mail stays on this machine. Riddle stores it in a local SQLite database in your Application Support directory.',
    '',
  ]
  const mode = dig?.privacy ?? state.settings.privacyDefault
  if (mode === 'local_only') lines.push(`${dig ? `The ${dig.name} account` : 'By default, each account'} is local-only: deterministic rules and on-device processing. Nothing from its mail leaves the device.`)
  else {
    lines.push(`${dig ? `For the ${dig.name} account, c` : 'C'}lassification sends a bounded state (sender, subject, a redacted body excerpt, thread facts) to TypeSafe AI's Jev decision model (jev-1.13.0) to answer Signal questions. Jev returns probabilities, not text, and does not retain state.`)
    if (mode === 'cloud_reasoning') lines.push('When Signals cannot decide, the message and its Signals go to Anthropic Claude on Amazon Bedrock (us-east-1, profile dev) for one bounded judgment. The Director and Expeditions use the same model and see only the mail they retrieve for your request. Contamination never enters their context unless you open it.')
    else lines.push('LLM reasoning is off for this account: undecided mail stays Unresolved rather than leaving the device.')
  }
  lines.push('', 'Nothing in your mail is used to train any model. Riddle uses Gmail under Google\'s Limited Use requirements. Logs carry identifiers only; exports and diagnostics are redacted by default. Credentials live in the macOS keychain and are never written to disk by Riddle.')
  return lines.join('\n')
}

export function updatePreview(state: MockState): UpdatePreview | null {
  const report = simulate(state, { meshId: 'mesh_github', version: 7 })
  return {
    fromVersion: '0.1.0',
    toVersion: '0.2.0',
    changedSignals: 2,
    changedMeshes: 1,
    report: { ...report, historicalMessages: u64(18_102 + 39), unchanged: u64(18_102), changed: u64(39), transitions: [{ from: 'spoil', to: 'find', count: u64(14) }, { from: 'find', to: 'spoil', count: u64(3) }, { from: 'filed', to: 'spoil', count: u64(22) }], newJudgeCallsRequired: u64(0), estimatedCostUsd: 0 },
  }
}

export function exportDiagnostics(state: MockState): string {
  const tc = taskCentre(state)
  return [
    `Riddle ${state.devInfo.build.version} (${state.devInfo.build.gitSha}${state.devInfo.build.dirty ? '-dirty' : ''}) ${state.devInfo.build.profile}`,
    `OS ${state.devInfo.os} ${state.devInfo.arch}; uptime ${state.devInfo.uptimeSeconds}s; log level ${state.devInfo.logLevel}`,
    `Data dir: ${state.devInfo.dataDir}`,
    `Stores: ${state.devInfo.stores.map((s) => `${s.label} ${(s.sizeBytes / 1024 / 1024).toFixed(1)} MB`).join(', ')}`,
    `Digs: ${state.digs.map((d) => `${d.id} provider=${d.provider} privacy=${d.privacy} projection=${d.projection} status=${d.status.state}`).join('; ')}`,
    `Task Centre: ${tc.overall}; jobs ${tc.jobs.map((j) => `${j.kind}:${j.status.state}`).join(', ')}`,
    `Judge: ${state.settings.judge.provider} ${state.settings.judge.model} key=${state.settings.judge.apiKeyPresent ? 'present' : 'absent'} (${state.settings.judge.keySource})`,
    `LLM: ${state.settings.llm.provider} ${state.settings.llm.region} escalation=${state.settings.llm.escalationModel} director=${state.settings.llm.directorModel}`,
    `Gmail client: ${state.settings.gmail.clientId ? 'configured' : 'absent'}; secret ${state.settings.gmail.clientSecretPresent ? 'present' : 'absent'}`,
    `Store fault: ${state.devInfo.storeFault ?? 'none'}`,
    `Recent errors: ${state.logs.filter((l) => l.level === 'error').length}`,
    '[redacted: no subjects, bodies, addresses or tokens]',
  ].join('\n')
}

export function readLogs(state: MockState, args: { limit?: number | null; category?: string | null; level?: string | null; text?: string | null }): LogLine[] {
  const order = ['error', 'warn', 'info', 'debug', 'trace']
  const min = args.level ? order.indexOf(args.level) : 4
  return state.logs
    .filter((l) => (!args.category || l.category === args.category) && order.indexOf(l.level) <= min && (!args.text || l.message.toLowerCase().includes(args.text.toLowerCase())))
    .slice(-(args.limit ?? 500))
}

export const LOG_CATEGORIES = ['sync', 'projection', 'sift', 'judge', 'llm', 'mesh', 'director', 'expedition', 'store', 'jobs', 'ui', 'actions', 'credentials', 'app']

export { DIG_WORK }

/**
 * The trust card's evidence, from the fixture the same way the core computes
 * it: Sift decisions in the window, corrections that stand, undone entries,
 * Spoil decisions held below Act on accounts that would archive, per-rule
 * precision, and one offer when an account's last Spoil calls all stood.
 */
export function trustStats(state: MockState, digId: string | null, sinceDays: number): TrustStats {
  const since = new Date(Date.now() - sinceDays * 86_400_000)
  const digs = digId ? state.digs.filter((d) => d.id === digId) : state.digs
  const level = (id: string) => state.settings.autonomy.perDig[id] ?? state.settings.autonomy.default
  let decisions = 0
  let wouldArchive = 0
  const perRule = new Map<string, { decisions: number; reversed: number }>()
  let offer: TrustStats['offer'] = null
  for (const dig of digs) {
    const held = belowAct(level(dig.id))
    let streak = 0
    let streakOpen = true
    for (const m of state.messages.filter((x) => x.digId === dig.id).sort((a, b) => (a.date < b.date ? 1 : -1))) {
      const spec = state.specs.get(m.id)
      const cls = state.classifications.get(m.id)
      if (!spec || !cls?.disposition || cls.shadow) continue
      const bySift = spec.mechanism !== 'user' && !cls.handledByUserAt
      if (new Date(m.date) >= since && spec.mechanism !== 'user') {
        decisions += 1
        if (held && dig.projection === 'full' && spec.disposition?.kind === 'spoil') wouldArchive += 1
        if (spec.meshId) {
          const mesh = state.meshes.filter((mm) => mm.id === spec.meshId).sort((a, b) => b.version - a.version)[0]
          const key = mesh ? `${mesh.id}@${mesh.version}` : null
          if (key) {
            const r = perRule.get(key) ?? { decisions: 0, reversed: 0 }
            r.decisions += 1
            if (cls.handledByUserAt) r.reversed += 1
            perRule.set(key, r)
          }
        }
      }
      if (streakOpen && spec.disposition?.kind === 'spoil') {
        if (bySift) streak += 1
        else if (cls.handledByUserAt) streakOpen = false
      }
    }
    if (held && dig.projection !== 'none' && streak >= 40 && (!offer || streak > Number(offer.spoilCalls))) {
      offer = { digId: dig.id, digName: dig.name, spoilCalls: u64(streak), projection: dig.projection, autonomy: level(dig.id) }
    }
  }
  const inScope = (a: Activity) => !digId || a.digId === digId
  const corrected = state.activity.filter((a) => inScope(a) && a.actor === 'user' && new Date(a.createdAt) >= since && ['set_disposition', 'restore_messages', 'move_to_spoil', 'dismiss_find', 'retype_find', 'review_shadow'].includes(a.action) && a.undo.state !== 'undone').length
  const undone = state.activity.filter((a) => inScope(a) && new Date(a.createdAt) >= since && a.undo.state === 'undone').length
  const rules = [...perRule.entries()]
    .map(([key, r]) => {
      const [id, v] = key.split('@')
      const mesh = state.meshes.find((mm) => mm.id === id && mm.version === Number(v))!
      return { meshId: id, name: mesh.name, version: Number(v), status: mesh.status, decisions: u64(r.decisions), reversed: u64(r.reversed), shadowProposed: mesh.shadowProposed, shadowAgreed: mesh.shadowAgreed, reversedOften: r.decisions >= 20 && r.reversed * 4 >= r.decisions }
    })
    .sort((a, b) => Number(b.decisions) - Number(a.decisions) || a.name.localeCompare(b.name))
  return { since: since.toISOString(), digCount: digs.length, decisions: u64(decisions), corrected: u64(corrected), undone: u64(undone), wouldArchive: u64(wouldArchive), rules, offer }
}

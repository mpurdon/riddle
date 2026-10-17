/**
 * Browser-mode mocks: one handler per command in docs/COMMANDS.md, over a
 * fixture mailbox that behaves like the real core. Never imported in Tauri
 * mode except through the ipc wrapper's registry, which only consults it
 * when `!isTauri`.
 */
import { mailPlaces } from './places'
import { comparisonOverview, comparisonRows, directorComparisons } from './comparison'
import { mock, clearMocks, isTauri, inFlightCalls } from '../ipc'
import { emitMock } from '../events'
import { u64 } from '../format'
import type { PageRequest } from '@/bindings/PageRequest'
import type { MessageFilter } from '../types'
import { buildState, type MockState } from './state'
import { invokeAction, listActions } from './actions'
import { directorSend, directorRetry } from './director'
import * as q from './queries'

let state: MockState = buildState()

export function mockState(): MockState {
  return state
}

export function resetMockState(): MockState {
  state = buildState()
  return state
}

const str = (a: Record<string, unknown>, k: string) => String(a[k] ?? '')
const opt = (a: Record<string, unknown>, k: string) => (a[k] === undefined || a[k] === null ? null : a[k])

export function registerMocks(): void {
  clearMocks()
  mock('invoke_action', (a) => invokeAction(state, a))
  mock('list_actions', () => listActions())
  mock('list_pending', () => [...state.pending.values()])

  mock('summary', (a) => q.summary(state, opt(a, 'digId') as string | null))
  mock('trust_stats', (a) => q.trustStats(state, opt(a, 'digId') as string | null, (opt(a, 'sinceDays') as number | null) ?? 7))
  mock('task_centre', () => q.taskCentre(state))
  mock('list_digs', () => state.digs.map((d) => q.withMailbox(state, d)))
  mock('list_dig_groups', () => state.digGroups.map((g) => ({ ...g, usedBy: q.groupUsedBy(state, g.name) })))
  mock('list_messages', (a) => q.listMessages(state, { digId: opt(a, 'digId') as string | null, filter: (a.filter ?? {}) as MessageFilter, page: a.page as PageRequest }))
  mock('get_message', (a) => q.getMessage(state, str(a, 'digId'), str(a, 'messageId')))
  mock('get_thread', (a) => q.getThread(state, str(a, 'digId'), str(a, 'threadId')))
  mock('explain', (a) => q.explain(state, str(a, 'digId'), str(a, 'messageId')))
  mock('search', (a) => q.search(state, { query: str(a, 'query'), digId: opt(a, 'digId') as string | null, page: a.page as PageRequest, shadowOnly: !!a.shadowOnly }))
  mock('priority_inbox', (a) => q.priorityInbox(state, { digId: opt(a, 'digId') as string | null, page: a.page as PageRequest }))
  mock('list_finds', (a) => q.listFinds(state, { status: opt(a, 'status') as string | null, findType: opt(a, 'findType') as string | null, digId: opt(a, 'digId') as string | null, page: a.page as PageRequest }))
  mock('get_find', (a) => {
    const f = state.finds.find((x) => x.id === a.findId)
    if (!f) throw { kind: 'not_found', message: 'Find not found', detail: null }
    return f
  })
  mock('list_activity', (a) => q.listActivity(state, { digId: opt(a, 'digId') as string | null, actor: opt(a, 'actor') as string | null, action: opt(a, 'action') as string | null, since: opt(a, 'since') as string | null, until: opt(a, 'until') as string | null, page: a.page as PageRequest }))
  mock('get_activity', (a) => {
    const x = state.activity.find((y) => y.id === a.activityId)
    if (!x) throw { kind: 'not_found', message: 'Activity not found', detail: null }
    return x
  })
  mock('list_meshes', (a) => q.latestMeshes(state, !!a.includeDeleted))
  mock('get_mesh', (a) => q.getMesh(state, str(a, 'meshId'), (opt(a, 'version') as number | null)))
  mock('mesh_versions', (a) => state.meshes.filter((m) => m.id === a.meshId).sort((x, y) => y.version - x.version))
  mock('parse_mesh', (a) => q.parseMesh(state, str(a, 'text')))
  mock('rule_check', () => q.ruleCheck(state))
  mock('rule_samples', (a) => q.ruleSamples(state, str(a, 'meshId'), (opt(a, 'limit') as number | null) ?? 10))
  mock('list_packs', () => q.packCatalogue(state))
  mock('list_named_lists', () => state.namedLists)
  mock('mesh_order', (a) => q.meshOrder(state, (opt(a, 'windowDays') as number | null) ?? 30))
  mock('simulate_mesh', async (a) => {
    await new Promise((r) => setTimeout(r, 350))
    return q.simulate(state, { text: opt(a, 'text') as string | null, meshId: opt(a, 'meshId') as string | null, version: opt(a, 'version') as number | null, windowDays: opt(a, 'windowDays') as number | null })
  })
  mock('list_signals', () => {
    const latest = new Map<string, (typeof state.signals)[number]>()
    for (const s of state.signals) {
      const cur = latest.get(s.name)
      if (!cur || s.version > cur.version) latest.set(s.name, s)
    }
    return [...latest.values()].sort((x, y) => x.name.localeCompare(y.name))
  })
  mock('signal_versions', (a) => state.signals.filter((s) => s.name === a.name).sort((x, y) => y.version - x.version))
  mock('signal_evaluations', (a) => state.traces.get(`tr_${str(a, 'messageId')}`)?.signals ?? [])
  mock('mail_places', (a) => mailPlaces(state, opt(a, 'digId') as string | null))
  mock('list_labels', (a) => {
    // The fixture's labels are their own names; counts come from the mail.
    const out: { digId: string; id: string; name: string; kind: string; messages: number; unread: number }[] = []
    for (const d of state.digs.filter((x) => !a.digId || x.id === a.digId)) {
      const counts = new Map<string, [number, number]>()
      for (const m of state.messages.filter((x) => x.digId === d.id)) {
        for (const l of m.labels) {
          const c = counts.get(l) ?? [0, 0]
          c[0]++
          if (m.labels.includes('UNREAD')) c[1]++
          counts.set(l, c)
        }
      }
      for (const [id, [messages, unread]] of counts) {
        if (id === 'UNREAD' || id === 'CHAT') continue
        const kind = id.startsWith('CATEGORY_') ? 'category' : id.startsWith('Riddle/') ? 'riddle' : /^[A-Z_]+$/.test(id) ? 'system' : 'user'
        out.push({ digId: d.id, id, name: id, kind, messages, unread })
      }
    }
    return out.sort((x, y) => x.name.localeCompare(y.name))
  })
  mock('list_strata', () => state.strata.filter((s) => !s.deletedAt).sort((x, y) => Number(y.messageCount) - Number(x.messageCount)))
  mock('list_conversations', () => state.conversations.map((c) => ({ ...c, turns: [] })))
  mock('list_suggestions', () => q.seriesSuggestions(state))
  mock('get_conversation', (a) => {
    const c = state.conversations.find((x) => x.id === a.conversationId)
    if (!c) throw { kind: 'not_found', message: 'conversation not found', detail: null }
    return c
  })
  mock('director_send', (a) => directorSend(state, { conversationId: opt(a, 'conversationId') as string | null, message: str(a, 'message'), digIds: opt(a, 'digIds') as string[] | null, title: opt(a, 'title') as string | null }))
  mock('director_retry', (a) => directorRetry(state, { conversationId: str(a, 'conversationId') }))
  mock('list_expeditions', () => state.expeditions)
  mock('get_expedition', (a) => {
    const e = state.expeditions.find((x) => x.id === a.expeditionId)
    if (!e) throw { kind: 'not_found', message: 'Expedition not found', detail: null }
    return e
  })
  mock('list_drafts', (a) => state.drafts.filter((d) => d.digId === a.digId))
  mock('get_settings', () => state.settings)
  mock('get_user_profile', () => state.profile)
  mock('privacy_statement', (a) => q.privacyStatement(state, opt(a, 'digId') as string | null))
  mock('read_trace', (a) => q.readTrace(state, str(a, 'digId'), str(a, 'traceId')))
  mock('recent_traces', (a) => q.recentTraces(state, { digId: opt(a, 'digId') as string | null, limit: opt(a, 'limit') as number | null }))
  mock('usage_summary', (a) => q.usageSummary(state, opt(a, 'sinceDays') as number | null))
  mock('trace_stats', (a) => q.traceStats(state, { digId: opt(a, 'digId') as string | null, sinceDays: opt(a, 'sinceDays') as number | null }))
  mock('dev_info', () => ({ ...state.devInfo, uptimeSeconds: state.devInfo.uptimeSeconds + Math.round(performance.now() / 1000) }))
  mock('dev_armed', () => state.armed)
  mock('comparison_overview', () => comparisonOverview(state))
  mock('comparison_rows', () => comparisonRows(state))
  mock('director_comparisons', () => directorComparisons(state))
  mock('mailbox_writes', () => ({ dev: state.devInfo.dev, writes: state.writes, held: u64(state.held) }))
  mock('probe_judge', async () => {
    await new Promise((r) => setTimeout(r, 420))
    return { ok: true, latencyMs: 184, message: 'jev-1.13.0 answered 1 question (2 choices); key from ssm dev' }
  })
  mock('probe_llm', async () => {
    await new Promise((r) => setTimeout(r, 900))
    return { ok: true, latencyMs: 812, message: 'bedrock us-east-1 us.anthropic.claude-haiku-4-5 responded; profile dev' }
  })
  mock('test_notification', () => ({ sent: true }))
  mock('probe_provider', async (a) => {
    await new Promise((r) => setTimeout(r, 300))
    const dig = state.digs.find((d) => d.id === a.digId)
    return dig ? { ok: true, latencyMs: 12, message: `fixture provider for ${dig.name}: ${state.messages.filter((m) => m.digId === dig.id).length} messages` } : { ok: false, latencyMs: 0, message: 'Dig not found' }
  })
  mock('read_app_logs', (a) => q.readLogs(state, { limit: opt(a, 'limit') as number | null, category: opt(a, 'category') as string | null, level: opt(a, 'level') as string | null, text: opt(a, 'text') as string | null }))
  mock('log_categories', () => q.LOG_CATEGORIES)
  mock('in_flight_calls', () => inFlightCalls())
  mock('export_diagnostics', () => q.exportDiagnostics(state))
  mock('update_preview', () => q.updatePreview(state))
  mock('ui_log', () => undefined)
}

let ticker: ReturnType<typeof setInterval> | null = null

/**
 * Advance the running catch-up job on real ticks so progress bars move only
 * because something happened, and flip the Dig to Synced when it finishes.
 */
export function startMockTicker(): void {
  if (isTauri || ticker) return
  ticker = setInterval(() => {
    const job = state.jobs.find((j) => j.id === 'job_catchup_work')
    if (job && job.status.state === 'running') {
      const done = Math.min(Number(job.progress.total), Number(job.progress.done) + 150 + Math.round(Math.random() * 80))
      const total = Number(job.progress.total)
      job.progress = { ...job.progress, done: u64(done), etaSeconds: u64(Math.round(((total - done) / 200) * 3)) }
      job.updatedAt = new Date().toISOString()
      const dig = state.digs.find((d) => d.id === job.digId)
      if (dig) dig.status = { state: 'catching_up', remaining: u64(total - done), total: u64(total) }
      state.logs.push({ at: job.updatedAt, level: 'debug', category: 'jobs', message: `job_catchup_work progress ${done}/${total}`, payload: { etaSeconds: Number(job.progress.etaSeconds) } })
      emitMock({ type: 'job_progress', job_id: job.id, done: u64(done), total: u64(total), step: job.progress.step })
      if (dig) emitMock({ type: 'dig_status_changed', dig_id: dig.id, status: dig.status })
      if (done >= total) {
        job.status = { state: 'completed' }
        job.finishedAt = job.updatedAt
        if (dig) {
          dig.status = { state: 'synced' }
          emitMock({ type: 'dig_status_changed', dig_id: dig.id, status: dig.status })
        }
        const proj = state.jobs.find((j) => j.id === 'job_projection_work')
        if (proj) {
          proj.status = { state: 'running' }
          proj.progress = { ...proj.progress, step: 'Writing labels' }
          emitMock({ type: 'job_changed', job: proj })
        }
        state.logs.push({ at: job.updatedAt, level: 'info', category: 'sync', message: 'catch-up finished for dig_work', payload: { fetched: total } })
        emitMock({ type: 'job_changed', job })
        emitMock({ type: 'summary_changed' })
        emitMock({ type: 'notify', title: 'Caught up', body: 'Work is synced. 7 Finds need your attention.', find_id: null })
      }
    }
    const proj = state.jobs.find((j) => j.id === 'job_projection_work')
    if (proj && proj.status.state === 'running') {
      const done = Math.min(Number(proj.progress.total), Number(proj.progress.done) + 7)
      proj.progress = { ...proj.progress, done: u64(done) }
      if (done >= Number(proj.progress.total)) {
        proj.status = { state: 'completed' }
        proj.finishedAt = new Date().toISOString()
        emitMock({ type: 'job_changed', job: proj })
      } else emitMock({ type: 'job_progress', job_id: proj.id, done: u64(done), total: proj.progress.total, step: proj.progress.step })
    }
    if (state.logs.length > 2000) state.logs.splice(0, state.logs.length - 2000)
  }, 2500)
}

export function stopMockTicker(): void {
  if (ticker) clearInterval(ticker)
  ticker = null
}

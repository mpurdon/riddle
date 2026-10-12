/**
 * A scripted Director for browser mode. It streams deltas, records tool
 * calls (marking the ones that read mail content as untrusted), and turns
 * some requests into proposals or Expeditions so the surfaces that consume
 * those events can be exercised end to end.
 */
import type { Conversation } from '@/bindings/Conversation'
import type { ToolCall } from '@/bindings/ToolCall'
import type { PendingAction } from '@/bindings/PendingAction'
import { emitMock } from '../events'
import { plural, u64 } from '../format'
import { iso } from './fixtures'
import { err } from './queries'
import { nextId, type MockState } from './state'
import { MESH_GITHUB_V7 } from './fixtures'

interface Script {
  tools: (Omit<ToolCall, 'id' | 'input'> & { delayMs: number })[]
  text: string
  proposal?: { action: string; args: Record<string, unknown>; realArguments: unknown; summary: string; reason: string }
  expedition?: boolean
  draft?: boolean
}

function script(state: MockState, message: string): Script {
  const q = message.toLowerCase()
  const open = state.finds.filter((f) => f.status === 'open')
  const unresolved = [...state.classifications.values()].filter((c) => c.disposition?.kind === 'unresolved').length
  if (/why .*(find|spoil|contamination|unresolved)|why did/.test(q)) {
    const find = open[0]
    return {
      tools: [
        { action: 'explain', args: { findId: find?.id ?? null }, ok: true, summary: find ? `${find.findType}: ${find.reason.slice(0, 40)}…` : 'no Find', activityId: null, latencyMs: u64(6), untrusted: false, delayMs: 300 },
        { action: 'read_trace', args: { traceId: find?.traceId ?? null }, ok: true, summary: 'Signals and Mesh match read', activityId: null, latencyMs: u64(4), untrusted: false, delayMs: 700 },
      ],
      text: find
        ? `**${find.title}** became a Find: ${find.findType} because ${find.reason.charAt(0).toLowerCase()}${find.reason.slice(1)}${find.confidence ? ` The Signals were confident (${(find.confidence * 100).toFixed(0)}%).` : ''}\n\nIf that is wrong, dismiss it and I will record the correction; three of the same kind and I will propose a Mesh change.`
        : 'There are no open Finds to explain right now.',
    }
  }
  if (/github|notification/.test(q) && /(unless|only|fail|spoil|handle)/.test(q)) {
    const mesh = state.meshes.filter((m) => m.id === 'mesh_github').reduce((a, b) => (a.version > b.version ? a : b))
    const text = MESH_GITHUB_V7.replace(' v7', ` v${mesh.version + 1}`).replace('ELSE IF routine@v1 > .90\n     AND personally_relevant@v3 < .30\n    → Spoil', 'ELSE IF routine@v1 > .80\n    → Spoil')
    return {
      tools: [
        { action: 'list_meshes', args: {}, ok: true, summary: `GitHub Notifications v${mesh.version} covers Work Digs`, activityId: null, latencyMs: u64(3), untrusted: false, delayMs: 300 },
        { action: 'simulate_mesh', args: { meshId: 'mesh_github' }, ok: true, summary: 'Simulated against 90 days', activityId: null, latencyMs: u64(212), untrusted: false, delayMs: 900 },
      ],
      text: `Understood: GitHub mail is only worth surfacing when something fails or you are asked to review. I have drafted **GitHub Notifications v${mesh.version + 1}**: everything routine goes to Spoil, CI failures on your work and review requests stay Finds, and the never-hidden guard stays.\n\nI have not applied it. Preview shows what would have changed over the last 90 days; Apply creates the new version, which starts in Shadow.`,
      proposal: {
        action: 'update_mesh',
        args: { meshId: 'mesh_github', text, changeNote: 'Director: Spoil all routine GitHub mail unless it fails or asks for review.' },
        realArguments: { meshId: 'mesh_github', name: 'GitHub Notifications', fromVersion: mesh.version, toVersion: mesh.version + 1, text },
        summary: `Update Mesh: GitHub Notifications v${mesh.version} → v${mesh.version + 1}`,
        reason: 'The Director proposes; you apply.',
      },
    }
  }
  if (/subscription|paying for|renovation|go through everything|find every|find all/.test(q)) {
    return {
      tools: [{ action: 'start_expedition', args: { goal: message }, ok: true, summary: 'Expedition planned', activityId: null, latencyMs: u64(20), untrusted: false, delayMs: 400 }],
      text: `That needs more than a search, so I have started an Expedition: "${message}". It has a budget of $1.00, 15 minutes and 60 tool calls, reports progress in the Task Centre, and can be cancelled. Its results will become Finds.`,
      expedition: true,
    }
  }
  if (/draft|reply to everything|replies/.test(q)) {
    return {
      tools: [
        { action: 'list_finds', args: { findType: 'respond' }, ok: true, summary: `${open.filter((f) => f.findType === 'respond').length} Respond Finds`, activityId: null, latencyMs: u64(5), untrusted: false, delayMs: 300 },
        { action: 'get_thread', args: { threadId: 't_pers_jeep' }, ok: true, summary: 'Read 3 messages', activityId: null, latencyMs: u64(8), untrusted: true, delayMs: 800 },
        { action: 'create_draft', args: { digId: 'dig_personal', threadId: 't_pers_jeep' }, ok: true, summary: 'Draft created', activityId: null, latencyMs: u64(12), untrusted: false, delayMs: 1300 },
      ],
      text: 'I drafted a reply to Adam choosing the £310 rebuild and asking him to book Thursday. It is a normal Riddle draft in your Personal Dig; open the Jeep thread to edit or send it. Sending needs your confirmation, and because I read the thread in this task, it also needs your approval.',
      draft: true,
    }
  }
  if (/attention|morning|important|away|what.*need/.test(q)) {
    return {
      tools: [
        { action: 'list_finds', args: { status: 'open' }, ok: true, summary: `${open.length} open Finds`, activityId: null, latencyMs: u64(9), untrusted: false, delayMs: 300 },
        { action: 'task_centre', args: {}, ok: true, summary: state.digs.some((d) => d.status.state === 'catching_up') ? 'Work catching up' : 'Caught up', activityId: null, latencyMs: u64(3), untrusted: false, delayMs: 600 },
      ],
      text: `${open.length} Finds are open and ${plural(unresolved, 'message is', 'messages are')} Unresolved.\n\n${open
        .slice(0, 3)
        .map((f, i) => `${i + 1}. **${f.title}** — ${f.summary}`)
        .join('\n')}${state.digs.some((d) => d.status.state === 'catching_up') ? '\n\nRiddle is still catching up on Work, so this may change.' : ''}`,
    }
  }
  if (/waiting/.test(q)) {
    return {
      tools: [{ action: 'list_finds', args: { findType: 'follow_up' }, ok: true, summary: `${open.filter((f) => f.findType === 'follow_up').length} Follow Up Finds`, activityId: null, latencyMs: u64(6), untrusted: false, delayMs: 300 }],
      text: open.filter((f) => f.findType === 'follow_up').length ? `You are waiting on:\n\n${open.filter((f) => f.findType === 'follow_up').map((f) => `- **${f.title}**: ${f.summary}`).join('\n')}` : 'Nothing is waiting on other people right now.',
    }
  }
  // Default: a search that reads mail content.
  const hits = state.messages.filter((m) => q.split(/\s+/).some((w) => w.length > 3 && (m.subject.toLowerCase().includes(w) || m.snippet.toLowerCase().includes(w)))).slice(0, 3)
  return {
    tools: [
      { action: 'search', args: { query: message, page: { cursor: null, limit: 20 } }, ok: true, summary: `${hits.length} messages`, activityId: null, latencyMs: u64(31), untrusted: true, delayMs: 400 },
      ...(hits[0] ? [{ action: 'get_message', args: { digId: hits[0].digId, messageId: hits[0].id }, ok: true, summary: 'Read 1 message', activityId: null, latencyMs: u64(7), untrusted: true, delayMs: 900 }] : []),
    ],
    text: hits.length ? `I found ${plural(hits.length, 'message')} that fit:\n\n${hits.map((h) => `- **${h.subject}** from ${h.from.name ?? h.from.email} (${new Date(h.date).toLocaleDateString()})`).join('\n')}\n\nI read mail content to answer this, so sending or creating a Mesh in this conversation now needs your approval.` : 'I searched your Digs and found nothing that matches. Try a sender, a subject word, or ask me to look further with an Expedition.',
  }
}

/** Play the scripted answer for `message` into conversation `c`: tool calls, deltas, the turn, then done. */
function runScript(state: MockState, c: Conversation, message: string) {
  const s = script(state, message)
  const calls: ToolCall[] = []
  for (const t of s.tools) {
    const { delayMs, ...rest } = t
    // The mock's arguments carry no mail, so the record's full input is the same.
    const call: ToolCall = { id: nextId(state, 'tc'), input: rest.args, ...rest }
    calls.push(call)
    setTimeout(() => {
      if (call.untrusted) c.readUntrusted = true
      emitMock({ type: 'director_tool_call', conversation_id: c.id, call })
    }, delayMs)
  }
  const start = (s.tools.at(-1)?.delayMs ?? 200) + 300
  const words = s.text.split(/(?<=\s)/)
  let i = 0
  const chunk = () => {
    if (i >= words.length) {
      finish()
      return
    }
    const piece = words.slice(i, i + 3).join('')
    i += 3
    emitMock({ type: 'director_delta', conversation_id: c.id, text: piece })
    setTimeout(chunk, 35)
  }
  setTimeout(chunk, start)
  const finish = () => {
    const tokensIn = 2400 + message.length * 4 + calls.length * 900
    const tokensOut = Math.round(s.text.length / 4)
    c.turns.push({ role: 'director', content: s.text, toolCalls: calls, createdAt: iso(new Date()), inputTokens: u64(tokensIn), outputTokens: u64(tokensOut), costUsd: Math.round(((tokensIn * 3 + tokensOut * 15) / 1_000_000) * 10000) / 10000 })
    c.updatedAt = iso(new Date())
    state.logs.push({ at: c.updatedAt, level: 'info', category: 'director', message: `conversation ${c.id}: ${calls.length} tool calls, ${tokensIn} input tokens`, payload: { untrusted: c.readUntrusted } })
    if (s.proposal) {
      const pending: PendingAction = {
        id: nextId(state, 'pend'),
        action: s.proposal.action,
        args: s.proposal.args,
        realArguments: s.proposal.realArguments,
        summary: s.proposal.summary,
        confirmation: 'approval',
        reason: s.proposal.reason,
        principal: { kind: 'director', conversation_id: c.id, read_untrusted: c.readUntrusted },
        createdAt: iso(new Date()),
      }
      state.pending.set(pending.id, pending)
      emitMock({ type: 'proposal_created', pending_id: pending.id, summary: pending.summary })
    }
    if (s.expedition) {
      const id = nextId(state, 'exp')
      state.expeditions.unshift({ id, goal: message, scope: { digIds: c.digIds, since: null, until: null, toolsAllowed: ['search', 'get_thread', 'get_message'] }, budget: { costUsd: 1, seconds: u64(900), toolCalls: 60 }, spent: { costUsd: 0.04, seconds: u64(12), toolCalls: 2 }, status: 'running', plan: [{ n: 1, text: 'Search the Digs in scope for relevant threads.', done: true }, { n: 2, text: 'Read the candidates and extract evidence.', done: false }, { n: 3, text: 'Report findings as Finds.', done: false }], checkpoint: { step: 1 }, evidence: [], findings: [], error: null, createdAt: iso(new Date()), updatedAt: iso(new Date()), finishedAt: null })
      const job = { id: `job_${id}`, kind: 'expedition' as const, priority: 'interactive' as const, status: { state: 'running' as const }, digId: null, label: `Expedition: ${message.slice(0, 40)}`, progress: { done: u64(1), total: u64(3), unit: 'steps', step: 'Reading candidates', etaSeconds: u64(90) }, checkpoint: null, cancellable: true, attempts: 1, createdAt: iso(new Date()), startedAt: iso(new Date()), finishedAt: null, updatedAt: iso(new Date()) }
      state.jobs.unshift(job)
      emitMock({ type: 'expedition_changed', expedition_id: id })
      emitMock({ type: 'job_changed', job })
    }
    if (s.draft) {
      state.drafts.push({ id: nextId(state, 'draft'), digId: 'dig_personal', providerId: null, threadId: 't_pers_jeep', inReplyTo: 'm_pers_adam_3', to: [{ email: 'adam.k@mechanics.example', name: 'Adam Kowalski' }], cc: [], bcc: [], subject: 'Re: Jeep brakes — what we found', bodyText: 'Hi Adam,\n\nLet us go with option A, the pads and caliper rebuild at £310. Thursday works; please order the parts.\n\nThanks,\nMatt', updatedAt: iso(new Date()) })
    }
    emitMock({ type: 'director_done', conversation_id: c.id, error: null })
  }
}

export function directorSend(state: MockState, args: { conversationId?: string | null; message: string; digIds?: string[] | null; title?: string | null }): { conversationId: string } {
  let conv = args.conversationId ? state.conversations.find((c) => c.id === args.conversationId) : undefined
  const now = iso(new Date())
  if (!conv) {
    conv = { id: nextId(state, 'conv'), title: (args.title || args.message.split('\n')[0]).slice(0, 60), turns: [], readUntrusted: false, digIds: args.digIds ?? state.digs.map((d) => d.id), modelId: 'bedrock:us.anthropic.claude-sonnet-5', promptVersion: 'director/v3', createdAt: now, updatedAt: now }
    state.conversations.unshift(conv)
  }
  const c: Conversation = conv
  c.turns.push({ role: 'user', content: args.message, toolCalls: [], createdAt: now, inputTokens: u64(0), outputTokens: u64(0), costUsd: null })
  c.updatedAt = now
  runScript(state, c, args.message)
  return { conversationId: c.id }
}

/** The failed Director turns at the end come off and the last message runs again. */
export function directorRetry(state: MockState, args: { conversationId: string }): { conversationId: string } {
  const c = state.conversations.find((x) => x.id === args.conversationId)
  if (!c) throw err('not_found', 'no such conversation')
  while (c.turns.length && c.turns.at(-1)!.role === 'director' && c.turns.at(-1)!.failure) c.turns.pop()
  const last = c.turns.at(-1)
  if (!last || last.role !== 'user') throw err('invalid', 'nothing to retry: the last turn is not a message of yours that failed')
  c.updatedAt = iso(new Date())
  runScript(state, c, last.content)
  return { conversationId: c.id }
}

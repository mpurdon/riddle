/**
 * Model comparison in the mock backend: the roster from settings with
 * access the check sets, recorded outputs, scores against the person's own
 * decisions, and Director picks. Mirrors `riddle_app::comparison` closely
 * enough for the Models view and its tests.
 */
import type { BenchModel } from '@/bindings/BenchModel'
import type { BenchOutput } from '@/bindings/BenchOutput'
import type { BenchOverview } from '@/bindings/BenchOverview'
import type { BenchRow } from '@/bindings/BenchRow'
import type { BenchScore } from '@/bindings/BenchScore'
import type { BenchTask } from '@/bindings/BenchTask'
import type { DirectorComparison } from '@/bindings/DirectorComparison'
import type { Disposition } from '@/bindings/Disposition'
import type { MockState } from './state'

/** `us.anthropic.claude-haiku-4-5-20251001-v1:0` → `Claude Haiku 4.5`. */
export function modelLabel(id: string): string {
  const base = id.replace(/^.*?(anthropic|openai)\./, '').split(':')[0]
  const words = base.split('-').filter((w) => !(w.length === 8 && /^\d+$/.test(w)) && w !== 'v1')
  const out: string[] = []
  let version: string[] = []
  for (const w of words) {
    if (/^[\d.]+$/.test(w)) version.push(w)
    else {
      if (version.length) out.push(version.join('.'))
      version = []
      out.push(w === 'gpt' ? 'GPT' : w[0].toUpperCase() + w.slice(1))
    }
  }
  if (version.length) out.push(version.join('.'))
  return out.join(' ').replace('GPT ', 'GPT-')
}

export function comparisonModels(state: MockState): BenchModel[] {
  const llm = state.settings.llm
  const roster = [...state.settings.comparison.roster]
  for (const m of [llm.escalationModel, llm.directorModel]) if (!roster.includes(m)) roster.unshift(m)
  return roster.map((id) => {
    const access = state.modelAccess[id] ?? 'unchecked'
    return {
      modelId: id,
      label: modelLabel(id),
      vendor: id.includes('openai') ? 'OpenAI' : 'Anthropic',
      access,
      accessDetail: access === 'not_enabled' ? `${id} is not available for this account` : null,
      checkedAt: access === 'unchecked' ? null : new Date().toISOString(),
      probeLatencyMs: access === 'usable' ? 640 : null,
      priceInPerMtok: id.includes('haiku-4-5') ? 1 : id.includes('sonnet-4') ? 3 : null,
      priceOutPerMtok: id.includes('haiku-4-5') ? 5 : id.includes('sonnet-4') ? 15 : null,
      usedFor: [...(id === llm.escalationModel ? (['escalation'] as BenchTask[]) : []), ...(id === llm.directorModel ? (['director'] as BenchTask[]) : [])],
    }
  })
}

const same = (a: Disposition | null | undefined, b: Disposition | null | undefined) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null)

/** The person's own decision for a message, when they made one. */
function truthOf(state: MockState, messageId: string): Disposition | null {
  const c = state.classifications.get(messageId)
  return c?.handledByUserAt && c.disposition ? c.disposition : null
}

function scores(state: MockState): BenchScore[] {
  const out: BenchScore[] = []
  for (const task of ['escalation', 'director'] as BenchTask[]) {
    const outputs = state.comparisonOutputs.filter((o) => o.task === task)
    for (const model of new Set(outputs.map((o) => o.modelId))) {
      const mine = outputs.filter((o) => o.modelId === model)
      const ok = mine.filter((o) => !o.error)
      let judged = 0
      let correct = 0
      let agrees = 0
      for (const o of ok) {
        if (!o.messageId) continue
        const truth = truthOf(state, o.messageId)
        if (truth) {
          judged++
          if (same(truth, o.disposition)) correct++
        }
        if (same(state.classifications.get(o.messageId)?.disposition, o.disposition)) agrees++
      }
      const priced = ok.every((o) => o.costUsd != null)
      out.push({
        modelId: model,
        task,
        runs: mine.length,
        errors: mine.length - ok.length,
        judged: judged,
        correct: correct,
        agreesWithCurrent: agrees,
        preferred: task === 'director' ? Object.values(state.comparisonPicks).filter((m) => m === model).length : 0,
        avgLatencyMs: ok.length ? Math.round(ok.reduce((a, o) => a + Number(o.latencyMs), 0) / ok.length) : 0,
        costPer1k: priced && ok.length ? (ok.reduce((a, o) => a + (o.costUsd ?? 0), 0) / ok.length) * 1000 : null,
      })
    }
  }
  return out
}

export function comparisonOverview(state: MockState): BenchOverview {
  return {
    models: comparisonModels(state),
    scores: scores(state),
    shadow: state.settings.comparison.challengers,
    spentTodayUsd: state.comparisonOutputs.reduce((a, o) => a + (o.costUsd ?? 0), 0),
    dailyCapUsd: state.settings.comparison.dailyUsd,
    running: false,
  }
}

export function comparisonRows(state: MockState): BenchRow[] {
  const rows: BenchRow[] = []
  for (const o of state.comparisonOutputs.filter((x) => x.task === 'escalation')) {
    if (!o.messageId || !o.digId) continue
    const row = rows.find((r) => r.messageId === o.messageId)
    if (row) {
      if (!row.outputs.some((x) => x.modelId === o.modelId)) row.outputs.push(o)
      continue
    }
    const m = state.messages.find((x) => x.id === o.messageId)
    if (!m) continue
    rows.push({ digId: o.digId, messageId: o.messageId, subject: m.subject, from: m.from.email, truth: truthOf(state, o.messageId), current: state.classifications.get(o.messageId)?.disposition ?? null, outputs: [o] })
  }
  return rows
}

export function directorComparisons(state: MockState): DirectorComparison[] {
  const rows: DirectorComparison[] = []
  for (const o of state.comparisonOutputs.filter((x) => x.task === 'director')) {
    if (!o.questionKey || !o.question) continue
    const row = rows.find((r) => r.questionKey === o.questionKey)
    if (row) {
      if (!row.outputs.some((x) => x.modelId === o.modelId)) row.outputs.push(o)
      continue
    }
    rows.push({ questionKey: o.questionKey, question: o.question, outputs: [o], preferred: state.comparisonPicks[o.questionKey] ?? null })
  }
  return rows
}

function output(state: MockState, partial: Partial<BenchOutput> & Pick<BenchOutput, 'task' | 'modelId'>): BenchOutput {
  state.seq++
  return {
    id: `cmp_${state.seq}`,
    source: 'benchmark',
    digId: null,
    messageId: null,
    question: null,
    questionKey: null,
    disposition: null,
    text: '',
    latencyMs: 900,
    inputTokens: 1200,
    outputTokens: 80,
    costUsd: partial.modelId.includes('haiku-4-5') ? 0.0016 : partial.modelId.includes('sonnet-4') ? 0.0048 : null,
    error: null,
    createdAt: new Date().toISOString(),
    ...partial,
  }
}

const usable = (state: MockState) => comparisonModels(state).filter((m) => m.access === 'usable').map((m) => m.modelId)

/** `check_model_access`: the Claude 4.x models answer; the rest are not enabled. */
export function checkModelAccess(state: MockState): BenchModel[] {
  for (const m of comparisonModels(state)) state.modelAccess[m.modelId] = /claude-(haiku-4-5|sonnet-4|opus-4)/.test(m.modelId) ? 'usable' : 'not_enabled'
  return comparisonModels(state)
}

/** `run_model_benchmark`: each usable model on the messages the person decided. */
export function runBenchmark(state: MockState, models: string[], count: number): number {
  const chosen = models.length ? models : usable(state)
  const judged = state.messages.filter((m) => truthOf(state, m.id)).slice(0, count)
  for (const m of judged) {
    const truth = truthOf(state, m.id)
    chosen.forEach((model, i) => {
      // The first model agrees with the person; the others differ on every other message.
      const disposition: Disposition = i === 0 || judged.indexOf(m) % 2 === 0 ? truth! : { kind: 'spoil' }
      state.comparisonOutputs.unshift(output(state, { task: 'escalation', modelId: model, digId: m.digId, messageId: m.id, disposition, text: `${modelLabel(model)} read it as ${disposition.kind}.` }))
    })
  }
  return judged.length
}

/** `run_director_comparison`: the person's past questions on each usable model. */
export function runDirectorComparison(state: MockState, models: string[], count: number): number {
  const chosen = models.length ? models : usable(state)
  const questions = [...new Set(state.conversations.flatMap((c) => c.turns.filter((t) => t.role === 'user').map((t) => t.content)))].slice(0, count)
  const fallback = questions.length ? questions : ['What needs me today?']
  for (const q of fallback) {
    const key = `q_${q.length}_${q.slice(0, 6)}`
    for (const model of chosen) state.comparisonOutputs.unshift(output(state, { task: 'director', modelId: model, question: q, questionKey: key, text: `${modelLabel(model)}: three things need you today.` }))
  }
  return fallback.length
}

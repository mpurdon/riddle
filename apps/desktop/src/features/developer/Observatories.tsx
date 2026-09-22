/**
 * Director Inspector (§78), Action Inspector (§79), Activity Timeline (§80),
 * Live Pipeline (§82), Cost Observatory (§84), Decision Quality (§85),
 * Performance (§87) and Failure Inspector (§88).
 */
import { useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { getConversation, listActivity, listConversations, page, qk, readAppLogs, traceStats, usageSummary } from '@/lib/api'
import { cx, fmtClockMs, fmtInt, fmtMs, fmtPct, fmtUsd, num, safeJson, titleCase } from '@/lib/format'
import { useActionsMeta, useDigContext, useTaskCentre } from '@/lib/queries'
import { dispositionLabel } from '@/lib/selectors'
import { useEventLog, usePipeline } from '@/lib/stores'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Select } from '@/components/ui/Select'
import { ActivityEntry } from '@/features/activity/ActivityPage'
import { JsonBlock, KV, Section, useRecentTraces } from './shared'

export function DirectorInspector() {
  const convs = useQuery({ queryKey: qk.conversations, queryFn: listConversations })
  const [id, setId] = useState<string | null>(null)
  const cid = id ?? convs.data?.[0]?.id ?? null
  const conv = useQuery({ queryKey: cid ? qk.conversation(cid) : ['conversation', 'none'], queryFn: () => getConversation(cid!), enabled: !!cid })
  const c = conv.data
  const totals = useMemo(() => {
    const turns = c?.turns.filter((t) => t.role === 'director') ?? []
    return { calls: turns.reduce((a, t) => a + t.toolCalls.length, 0), inTok: turns.reduce((a, t) => a + num(t.inputTokens), 0), outTok: turns.reduce((a, t) => a + num(t.outputTokens), 0), cost: turns.reduce((a, t) => a + (t.costUsd ?? 0), 0), unpriced: turns.some((t) => t.costUsd === null) }
  }, [c])
  return (
    <div className="flex min-h-0 flex-1">
      <div className="w-72 shrink-0 overflow-auto border-r border-line" role="listbox" aria-label="Conversations">
        {(convs.data ?? []).map((x) => (
          <div key={x.id} role="option" aria-selected={x.id === cid} onClick={() => setId(x.id)} className={cx('cursor-default border-b border-line/60 px-3 py-2 text-xs', x.id === cid ? 'row-selected' : 'hover:bg-sand/60')}>
            <div className="truncate font-medium">{x.title}</div>
            <div className="text-clay">{x.modelId} · {x.promptVersion}{x.readUntrusted ? ' · read untrusted' : ''}</div>
          </div>
        ))}
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-5">
        {c ? (
          <>
            <Section title="Goal">
              <div className="text-sm">{c.turns.find((t) => t.role === 'user')?.content}</div>
              <div className="mt-1"><KV rows={[['Model', c.modelId], ['Prompt version', c.promptVersion], ['Digs', c.digIds.join(', ')], ['Read untrusted', c.readUntrusted ? 'yes: egress approval-gated' : 'no'], ['Tool calls', String(totals.calls)], ['Tokens', `${fmtInt(totals.inTok)} in / ${fmtInt(totals.outTok)} out`], ['Cost', totals.unpriced ? `${fmtUsd(totals.cost)} + unpriced` : fmtUsd(totals.cost)]]} /></div>
            </Section>
            <Section title="Execution">
              <ol className="flex flex-col gap-2">
                {c.turns.map((t, i) => (
                  <li key={i} className="rounded border border-line p-2 text-xs">
                    <div className="flex items-center gap-2"><Chip tone={t.role === 'user' ? 'neutral' : 'ok'}>{t.role}</Chip><span className="text-clay">{fmtClockMs(t.createdAt)}</span>{t.role === 'director' ? <span className="text-clay">· {fmtInt(t.inputTokens)} in / {fmtInt(t.outputTokens)} out · {fmtUsd(t.costUsd)}</span> : null}</div>
                    {t.toolCalls.map((tc) => (
                      <div key={tc.id} className="mt-1 ml-3 border-l-2 border-line pl-2">
                        <div className="mono">ACTION {tc.action}({safeJson(tc.args, 0)}) {tc.ok ? '✓' : '✗'} {fmtMs(tc.latencyMs)}{tc.untrusted ? <Chip tone="warn" className="ml-1">untrusted</Chip> : null}</div>
                        <div className="text-ink-2">{tc.summary}</div>
                      </div>
                    ))}
                    <div className="mt-1 whitespace-pre-wrap text-ink-2">{t.content}</div>
                  </li>
                ))}
              </ol>
              <p className="mt-2 text-xs text-clay">Structured plans, Actions, evidence and decisions; never hidden model scratch reasoning.</p>
            </Section>
          </>
        ) : (
          <span className="text-sm text-clay">No conversations yet.</span>
        )}
      </div>
    </div>
  )
}

export function ActionInspector() {
  const metas = useActionsMeta()
  const [name, setName] = useState<string | null>(null)
  const recent = useQuery({ queryKey: [...qk.activity, 'by-action', name], queryFn: () => listActivity({ action: name, page: page(null, 20) }), enabled: !!name })
  const { digName } = useDigContext()
  return (
    <div className="flex min-h-0 flex-1">
      <div className="min-h-0 w-[560px] shrink-0 overflow-auto border-r border-line">
        <table className="w-full text-xs">
          <thead className="sticky top-0 bg-sand text-left text-clay">
            <tr>{['Action', 'Risk', 'Confirmation', 'Min autonomy', 'Flags'].map((h) => <th key={h} className="px-3 py-1 font-semibold">{h}</th>)}</tr>
          </thead>
          <tbody>
            {(metas.data ?? []).map((m) => (
              <tr key={m.name} onClick={() => setName(m.name)} className={cx('cursor-default border-b border-line/50', name === m.name ? 'row-selected' : 'hover:bg-sand/60')}>
                <td className="px-3 py-1 mono font-medium" title={m.description}>{m.name}</td>
                <td className="px-3 py-1"><Chip tone={m.risk === 'critical' || m.risk === 'high' ? 'bad' : m.risk === 'medium' ? 'warn' : 'neutral'}>{m.risk}</Chip></td>
                <td className="px-3 py-1">{m.confirmation}</td>
                <td className="px-3 py-1">{m.minimumAutonomy}</td>
                <td className="px-3 py-1 text-clay">{[m.mutating && 'mutating', m.reversible && 'reversible', m.externalSideEffect && 'external', m.egress && 'egress', m.readOnly && 'read-only'].filter(Boolean).join(' · ')}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-5">
        {name ? (
          <>
            <Section title={`${name} · recent invocations`}>
              {recent.data?.items.length ? <ol className="flex flex-col gap-2">{recent.data.items.map((a) => <ActivityEntry key={a.id} a={a} digName={digName} />)}</ol> : <span className="text-xs text-clay">No Activity for this Action yet.</span>}
            </Section>
            <Section title="Metadata"><JsonBlock value={metas.data?.find((m) => m.name === name)} label="action metadata" /></Section>
          </>
        ) : (
          <span className="text-sm text-clay">Every Action owns its contract, validation, authorization, risk metadata, idempotency, audit and undo behaviour. Pick one.</span>
        )}
      </div>
    </div>
  )
}

export function ActivityTimeline() {
  const events = useEventLog((s) => s.events)
  const pipeline = usePipeline((s) => s.entries)
  const [open, setOpen] = useState<number | null>(null)
  const rows = useMemo(() => {
    const all = [...events.map((e) => ({ at: e.at, kind: 'event' as const, label: e.event.type, detail: e.event })), ...pipeline.map((p) => ({ at: p.at, kind: 'stage' as const, label: `${p.stage} (${p.messageId})`, detail: p }))]
    return all.sort((a, b) => b.at - a.at).slice(0, 300)
  }, [events, pipeline])
  return (
    <div className="min-h-0 flex-1 overflow-auto p-5">
      <p className="mb-2 text-xs text-ink-2">System-wide timeline from real events received by this window: {rows.length} entries. Each is expandable.</p>
      <ol className="mono text-xs">
        {rows.map((r, i) => (
          <li key={i} className="border-b border-line/40">
            <button type="button" className="flex w-full gap-3 py-0.5 text-left hover:bg-sand/60" onClick={() => setOpen(open === i ? null : i)} aria-expanded={open === i}>
              <span className="text-clay">{fmtClockMs(new Date(r.at).toISOString())}</span>
              <span className={r.kind === 'stage' ? 'text-ochre' : ''}>{r.label}</span>
            </button>
            {open === i ? <pre className="bg-sand px-4 py-1">{safeJson(r.detail)}</pre> : null}
          </li>
        ))}
        {!rows.length ? <li className="text-clay">Nothing yet. Inject a fixture message from Demo Mode to watch a chain unfold.</li> : null}
      </ol>
    </div>
  )
}

export function LivePipeline() {
  const entries = usePipeline((s) => s.entries)
  const clear = usePipeline((s) => s.clear)
  const byMessage = useMemo(() => {
    const g = new Map<string, typeof entries>()
    for (const e of entries) g.set(e.messageId, [...(g.get(e.messageId) ?? []), e])
    return [...g.entries()].reverse()
  }, [entries])
  return (
    <div className="min-h-0 flex-1 overflow-auto p-5">
      <div className="mb-3 flex items-center justify-between">
        <p className="text-xs text-ink-2">PipelineStage events as they arrive. This is real telemetry; nothing animates unless a stage was reported.</p>
        <Button size="sm" variant="ghost" onClick={clear}>Clear</Button>
      </div>
      {!byMessage.length ? <div className="text-sm text-clay">Waiting for messages. Use Demo Mode to inject one.</div> : null}
      <div className="flex flex-col gap-4">
        {byMessage.map(([id, stages]) => (
          <div key={id} className="rounded border border-line p-3">
            <div className="mono text-xs font-medium">{id}</div>
            <ol className="mt-1 mono text-xs">
              {stages.map((s, i) => (
                <li key={i} className="flex gap-3">
                  <span className="text-clay">{fmtClockMs(new Date(s.at).toISOString())}</span>
                  <span className="w-3 text-clay">{i < stages.length - 1 ? '│' : '└'}</span>
                  <span>{s.stage}</span>
                  <span className="text-clay">+{s.ms}ms</span>
                </li>
              ))}
            </ol>
          </div>
        ))}
      </div>
    </div>
  )
}

export function CostObservatory() {
  const [days, setDays] = useState('1')
  const q = useQuery({ queryKey: [...qk.usage, days], queryFn: () => usageSummary(Number(days)) })
  const u = q.data
  return (
    <div className="min-h-0 flex-1 overflow-auto p-5">
      <div className="mb-3 flex items-center gap-2 text-xs">
        <Select label="Window" size="sm" value={days} onChange={setDays} options={[{ value: '1', label: 'Today' }, { value: '7', label: '7 days' }, { value: '30', label: '30 days' }]} />
        {u ? <span className="text-clay">since {new Date(u.since).toLocaleString()}</span> : null}
      </div>
      {u ? (
        <div className="grid grid-cols-3 gap-6">
          <Section title="Totals">
            <KV rows={[['Calls', fmtInt(u.calls)], ['Input tokens', fmtInt(u.inputTokens)], ['Output tokens', fmtInt(u.outputTokens)], ['Priced cost', fmtUsd(u.pricedUsd)], ['Unpriced calls', `${fmtInt(u.unpricedCalls)} (unpriced, never free)`]]} />
          </Section>
          <Section title="By task">
            <table className="w-full text-xs"><tbody>{u.byTask.map(([task, usd, calls]) => <tr key={task} className="border-b border-line/40"><td className="py-0.5">{titleCase(task)}</td><td className="text-right tabular-nums">{fmtInt(calls)} calls</td><td className="text-right tabular-nums">{fmtUsd(usd)}</td></tr>)}</tbody></table>
          </Section>
          <Section title="By provider">
            <table className="w-full text-xs"><tbody>{u.byProvider.map(([p, usd, calls]) => <tr key={p} className="border-b border-line/40"><td className="py-0.5 mono">{p}</td><td className="text-right tabular-nums">{fmtInt(calls)} calls</td><td className="text-right tabular-nums">{p === 'null' ? 'unpriced' : fmtUsd(usd)}</td></tr>)}</tbody></table>
          </Section>
        </div>
      ) : null}
      <p className="text-xs text-clay">Reads the usage ledger. A provider without a price list reports unpriced rather than $0.</p>
    </div>
  )
}

export function DecisionQuality() {
  const traces = useRecentTraces(300)
  const corrections = useQuery({ queryKey: [...qk.activity, 'corrections'], queryFn: () => listActivity({ actor: 'user', page: page(null, 200) }) })
  const stats = useMemo(() => {
    const ts = traces.data ?? []
    const corr = (corrections.data?.items ?? []).filter((a) => ['set_disposition', 'restore_messages', 'dismiss_find', 'retype_find'].includes(a.action) && a.undo.state !== 'undone')
    const byMech = (m: string) => ts.filter((t) => t.mechanism === m).length
    const correctedIds = new Set(corr.flatMap((a) => a.affected.filter((e) => e.kind === 'message').map((e) => e.id)))
    const signalCounts = new Map<string, number>()
    for (const t of ts) if (correctedIds.has(t.messageId)) for (const s of t.signals) signalCounts.set(`${s.signalName}:v${s.signalVersion}`, (signalCounts.get(`${s.signalName}:v${s.signalVersion}`) ?? 0) + 1)
    return { decisions: ts.length, corrected: corr.length, agreement: ts.length ? 1 - Math.min(ts.length, corr.length) / ts.length : null, jev: byMech('jev') + byMech('mesh'), llm: byMech('llm'), gate: byMech('gate') + byMech('deterministic'), user: byMech('user'), signals: [...signalCounts.entries()].sort((a, b) => b[1] - a[1]).slice(0, 5) }
  }, [traces.data, corrections.data])
  return (
    <div className="min-h-0 flex-1 overflow-auto p-5">
      <p className="mb-3 text-xs text-ink-2">Dataset: the {stats.decisions} most recent traces and your corrections in Activity. Observed product outcomes, not claims of model accuracy.</p>
      <div className="grid grid-cols-2 gap-8">
        <Section title="Sift decisions"><KV rows={[['Decisions', fmtInt(stats.decisions)], ['Corrected', fmtInt(stats.corrected)], ['Observed agreement', fmtPct(stats.agreement)], ['Jev + Mesh', fmtInt(stats.jev)], ['Jev → LLM', fmtInt(stats.llm)], ['Deterministic / gate', fmtInt(stats.gate)], ['Decided by you', fmtInt(stats.user)]]} /></Section>
        <Section title="Most corrected Signals">
          {stats.signals.length ? <table className="text-xs"><tbody>{stats.signals.map(([k, v]) => <tr key={k}><td className="pr-4 mono">{k}</td><td className="tabular-nums">{v}</td></tr>)}</tbody></table> : <span className="text-xs text-clay">No corrections touch a Signal yet.</span>}
        </Section>
      </div>
    </div>
  )
}

export function PerformanceObservatory() {
  const { digId } = useDigContext()
  const stats = useQuery({ queryKey: [...qk.traceStats, digId], queryFn: () => traceStats({ digId }), refetchInterval: 5000 })
  const tc = useTaskCentre()
  const s = stats.data
  const jobs = tc.data?.jobs ?? []
  return (
    <div className="min-h-0 flex-1 overflow-auto p-5">
      <div className="grid grid-cols-3 gap-6">
        <Section title="Sift">
          {s ? <KV rows={[['Traces', fmtInt(s.count)], ['Escalated', `${fmtInt(s.escalated)} (${fmtPct(s.count ? s.escalated / s.count : null, 1)})`], ['Unresolved', fmtInt(s.unresolved)], ['Avg total', fmtMs(s.avgTotalMs)], ['Avg judge', fmtMs(s.avgJudgeMs)], ['Judge tokens', fmtInt(s.judgeTokens)], ['Judge cost', fmtUsd(s.judgeCostUsd)], ['LLM cost', fmtUsd(s.llmCostUsd)]]} /> : null}
        </Section>
        <Section title="Queues">
          <KV rows={[['Jobs running', String(jobs.filter((j) => j.status.state === 'running').length)], ['Jobs queued', String(jobs.filter((j) => j.status.state === 'queued').length)], ['Jobs paused', String(jobs.filter((j) => j.status.state === 'paused').length)], ['Sift queue depth', fmtInt(tc.data?.digs.reduce((a, d) => a + (d.status.state === 'sifting' ? num(d.status.remaining) : 0), 0) ?? 0)], ['Catch-up backlog', fmtInt(tc.data?.digs.reduce((a, d) => a + (d.status.state === 'catching_up' ? num(d.status.remaining) : 0), 0) ?? 0)], ['Projection pending', fmtInt(tc.data?.digs.reduce((a, d) => a + (d.status.state === 'projection' ? num(d.status.pending) : 0), 0) ?? 0)]]} />
        </Section>
        <Section title="Limits">
          <KV rows={[['Gmail', '~300 fetches / min / Dig'], ['Judge', '1,200 requests / min shared'], ['SQLite', 'one writer, pooled readers'], ['Signal cache hit rate', s && s.count ? fmtPct(1 - Math.min(1, s.judgeTokens / Math.max(1, s.count * 1900)), 0) : '—']]} />
        </Section>
      </div>
      <p className="text-xs text-clay">Model performance and application performance are different questions; this page is the second one.</p>
    </div>
  )
}

export function FailureInspector() {
  const traces = useRecentTraces(300)
  const logs = useQuery({ queryKey: [...qk.logs, 'errors'], queryFn: () => readAppLogs({ limit: 100, level: 'warn' }), refetchInterval: 5000 })
  const failed = (traces.data ?? []).filter((t) => t.escalation || t.conclusion.kind === 'unresolved')
  return (
    <div className="min-h-0 flex-1 overflow-auto p-5">
      <div className="grid grid-cols-2 gap-8">
        <Section title={`Escalations and Unresolved (${failed.length})`}>
          <ol className="flex flex-col gap-2">
            {failed.map((t) => (
              <li key={t.id} className="rounded border border-line p-2 text-xs">
                <div className="flex items-center gap-2"><span className="mono">{t.messageId}</span><Chip tone={t.conclusion.kind === 'unresolved' ? 'warn' : 'neutral'}>{dispositionLabel(t.conclusion)}</Chip>{t.escalation ? <Chip tone="busy">escalated</Chip> : null}</div>
                <KV rows={[['Signal Set', t.signals.map((s) => `${s.signalName}:v${s.signalVersion}`).join(', ') || '—'], ['Fallback', t.escalation ? `LLM ${t.escalation.modelId}` : 'none'], ['Disposition', `${dispositionLabel(t.conclusion).toUpperCase()}${t.conclusion.kind === 'unresolved' ? ' (never hidden)' : ''}`], ['Mailbox mutation', t.conclusion.kind === 'unresolved' ? 'NONE' : 'per projection mode']]} />
                {t.escalation ? <div className="mt-1 text-ink-2">{t.escalation.reason}</div> : null}
              </li>
            ))}
            {!failed.length ? <li className="text-xs text-clay">No escalations or Unresolved decisions in the recent traces.</li> : null}
          </ol>
        </Section>
        <Section title="Warnings and errors in the log">
          <ol className="mono text-[11px]">
            {(logs.data ?? []).map((l, i) => (
              <li key={i} className={cx('border-b border-line/40 py-0.5', l.level === 'error' ? 'text-rust' : 'text-ochre')}>
                <span className="text-clay">{fmtClockMs(l.at)}</span> {l.level.toUpperCase()} [{l.category}] {l.message}
                {l.payload ? <pre className="text-ink-2">{safeJson(l.payload, 0)}</pre> : null}
              </li>
            ))}
            {!logs.data?.length ? <li className="text-clay">No warnings or errors.</li> : null}
          </ol>
          <p className="mt-2 text-xs text-clay">Failures are first-class pipeline events: a judge outage becomes Unresolved with a queued retry, never a hidden message.</p>
        </Section>
      </div>
    </div>
  )
}

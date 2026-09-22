/**
 * Meshes (README "Mesh", ARCHITECTURE §36-38, §51): the list with status
 * and Shadow agreement, an editor on the DSL text with live validation,
 * a structural view, version history with diff, Preview impact, Apply or
 * Keep, status controls (Shadow → Active holds when the Mesh proposes
 * Spoil), soft delete with Recently Deleted and restore.
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Network, Plus, RotateCcw, Trash2 } from 'lucide-react'
import type { Mesh } from '@/bindings/Mesh'
import type { MeshStatus } from '@/bindings/MeshStatus'
import type { SimulationReport } from '@/bindings/SimulationReport'
import { getMesh, meshVersions, parseMesh, qk, simulateMesh } from '@/lib/api'
import { cx, fmtPct, fmtRelative } from '@/lib/format'
import { diffLines } from '@/lib/diff'
import { issuesFrom } from '@/lib/meshIssues'
import { useDigContext, useMeshes } from '@/lib/queries'
import { dispositionLabel } from '@/lib/selectors'
import type { MeshParseIssue } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { useShortcuts } from '@/lib/useShortcuts'
import { toRiddleError } from '@/lib/ipc'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { EmptyState } from '@/components/ui/EmptyState'
import { Note } from '@/components/ui/Note'
import { Tabs, TabPanel } from '@/components/ui/Tabs'
import { ErrorCard } from '@/components/ErrorCard'
import { HoldButton } from '@/components/confirm/HoldButton'
import { PageHeader } from '@/components/Bits'
import { SimulationTable } from './SimulationTable'
import type { Tone } from '@/lib/selectors'

const STATUS_TONE: Record<MeshStatus, Tone> = { draft: 'neutral', shadow: 'warn', active: 'ok', disabled: 'neutral', deleted: 'bad' }

const NEW_MESH_TEMPLATE = `Mesh: New Mesh v1

Scope:
All Digs

ROUTES

IF routine@v1 > .90
    → Spoil

ELSE
    → Escalate

GUARDS

NEVER Spoil IF sender IN never_hidden
`

export function scopeLabel(m: Mesh, digName: (id: string) => string): string {
  switch (m.scope.kind) {
    case 'all_digs':
      return 'All Digs'
    case 'dig_group':
      return `${m.scope.group} Digs`
    case 'dig':
      return `Dig: ${digName(m.scope.dig_id)}`
  }
}

export function proposesSpoil(m: Mesh): boolean {
  return m.routes.some((r) => r.output.disposition?.kind === 'spoil' || r.output.disposition?.kind === 'contamination')
}

export default function MeshesPage() {
  const { meshId } = useParams()
  const [params] = useSearchParams()
  const nav = useNavigate()
  const { digName } = useDigContext()
  const [showDeleted, setShowDeleted] = useState(false)
  const list = useMeshes(true)
  const meshes = useMemo(() => (list.data ?? []).filter((m) => showDeleted || !m.deletedAt), [list.data, showDeleted])
  const deletedCount = (list.data ?? []).filter((m) => m.deletedAt).length
  const byName = params.get('name')
  useEffect(() => {
    if (byName && list.data) {
      const m = list.data.find((x) => x.name === byName)
      if (m) nav(`/meshes/${m.id}`, { replace: true })
    }
  }, [byName, list.data, nav])
  const [creating, setCreating] = useState(false)
  if (list.error) return <ErrorCard error={list.error} onRetry={() => list.refetch()} />
  return (
    <div className="flex min-h-0 flex-1">
      <section className="flex w-[360px] shrink-0 flex-col border-r border-line">
        <PageHeader title="Meshes" subtitle="Policy as versioned text. A new or changed Mesh suggests until it has earned the right to act." actions={<Button size="sm" icon={<Plus />} onClick={() => { setCreating(true); nav('/meshes') }}>New</Button>} />
        <div className="min-h-0 flex-1 overflow-auto" role="listbox" aria-label="Meshes">
          {list.data && !meshes.length ? <EmptyState icon={<Network />} title="No Meshes yet." body="Correct a few classifications and Riddle proposes one; or write one here. Every Mesh starts in Shadow and is previewed against history before it takes effect." actions={<Button size="sm" onClick={() => setCreating(true)}>Write a Mesh</Button>} /> : null}
          {meshes.map((m) => {
            const active = m.id === meshId
            const agree = m.shadowProposed ? m.shadowAgreed / m.shadowProposed : null
            return (
              <div key={m.id} role="option" aria-selected={active} onClick={() => { setCreating(false); nav(`/meshes/${m.id}`) }} className={cx('cursor-default border-b border-line/60 px-4 py-2', active ? 'row-selected' : 'hover:bg-sand/60', m.deletedAt && 'opacity-60')}>
                <div className="flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-sm font-medium">{m.name}</span>
                  <span className="text-xs text-clay">v{m.version}</span>
                  <Chip tone={STATUS_TONE[m.status]}>{m.status}</Chip>
                </div>
                <div className="mt-0.5 flex items-center gap-2 text-xs text-ink-2">
                  <span>{scopeLabel(m, digName)}</span>
                  {agree !== null ? <span title={`${m.shadowAgreed} of ${m.shadowProposed} proposals matched what you did`}>· agreement {fmtPct(agree, 0)}</span> : null}
                  {m.deletedAt ? <span>· deleted {fmtRelative(m.deletedAt)}</span> : null}
                </div>
              </div>
            )
          })}
        </div>
        <div className="border-t border-line px-4 py-2 text-xs">
          <button type="button" className="text-ink-2 hover:text-ink" onClick={() => setShowDeleted((v) => !v)}>
            {showDeleted ? 'Hide' : 'Show'} Recently Deleted ({deletedCount})
          </button>
        </div>
      </section>
      <section className="flex min-w-0 flex-1 flex-col">
        {creating ? <MeshEditor key="new" mesh={null} onSaved={(id) => { setCreating(false); nav(`/meshes/${id}`) }} /> : meshId ? <MeshDetail key={meshId} meshId={meshId} /> : <EmptyState title="Select a Mesh" body="Or write a new one. The text is the Mesh: routes decide where mail goes; guards veto after every route." />}
      </section>
    </div>
  )
}

function MeshDetail({ meshId }: { meshId: string }) {
  const { digName } = useDigContext()
  const { run } = useAction()
  const qc = useQueryClient()
  const mesh = useQuery({ queryKey: qk.mesh(meshId, null), queryFn: () => getMesh(meshId, null) })
  const versions = useQuery({ queryKey: qk.meshVersions(meshId), queryFn: () => meshVersions(meshId) })
  const [tab, setTab] = useState('editor')
  const m = mesh.data
  if (mesh.error) return <ErrorCard error={mesh.error} onRetry={() => mesh.refetch()} />
  if (!m) return <div className="p-6 text-sm text-clay">Loading Mesh…</div>
  const setStatus = (status: MeshStatus) => run('set_mesh_status', { meshId, status }, { invalidate: [qk.mesh(meshId, null), qk.meshVersions(meshId)] })
  const spoils = proposesSpoil(m)
  const agree = m.shadowProposed ? m.shadowAgreed / m.shadowProposed : null
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={`${m.name} v${m.version}`}
        subtitle={
          <span className="flex flex-wrap items-center gap-2">
            <Chip tone={STATUS_TONE[m.status]}>{m.status}</Chip>
            <span>{scopeLabel(m, digName)}</span>
            <span>· created by {m.createdBy} {fmtRelative(m.createdAt)}</span>
            {agree !== null ? <span>· Shadow agreement {fmtPct(agree, 0)} ({m.shadowAgreed}/{m.shadowProposed})</span> : null}
          </span>
        }
        actions={
          m.deletedAt ? (
            <Button size="sm" icon={<RotateCcw />} onClick={() => run('restore_mesh', { meshId }, { invalidate: [qk.mesh(meshId, null)] })}>
              Restore
            </Button>
          ) : (
            <>
              {m.status !== 'shadow' ? (
                <Button size="sm" onClick={() => setStatus('shadow')} disabledReason={m.status === 'deleted' ? 'Restore it first' : null}>
                  Shadow
                </Button>
              ) : null}
              {m.status !== 'active' && !spoils ? (
                <Button size="sm" variant="primary" onClick={() => setStatus('active')} disabledReason={m.status === 'draft' ? 'A draft starts in Shadow; it earns Active by agreeing with you' : null}>
                  Activate
                </Button>
              ) : null}
              {m.status !== 'disabled' ? (
                <Button size="sm" onClick={() => setStatus('disabled')}>
                  Disable
                </Button>
              ) : null}
              <Button size="sm" variant="danger" icon={<Trash2 />} onClick={() => run('delete_mesh', { meshId }, { invalidate: [qk.mesh(meshId, null)] })}>
                Delete
              </Button>
            </>
          )
        }
      />
      {m.status === 'shadow' && spoils && !m.deletedAt ? (
        <div className="border-b border-line bg-ochre/5 px-5 py-3">
          <div className="text-sm font-medium">This Mesh proposes Spoil. Activating lets it hide mail on its own.</div>
          <div className="mt-0.5 text-xs text-ink-2">{agree !== null && m.shadowProposed >= 20 ? `In Shadow it agreed with you ${fmtPct(agree, 0)} of the time over ${m.shadowProposed} proposals.` : `It has ${m.shadowProposed} Shadow proposals so far; Riddle would normally wait for the Shadow Period to show agreement.`}</div>
          <div className="mt-2">
            <HoldButton label="Hold to activate" coaching="Press and hold for 1.5 seconds. The Action layer re-checks this confirmation; the mail it hides stays reviewable in Spoil." onComplete={() => run('set_mesh_status', { meshId, status: 'active' }, { confirmed: true, invalidate: [qk.mesh(meshId, null), qk.meshVersions(meshId)] })} />
          </div>
        </div>
      ) : null}
      <Tabs value={tab} onChange={setTab} className="min-h-0 flex-1" tabs={[{ value: 'editor', label: 'Editor' }, { value: 'structure', label: 'Structure' }, { value: 'history', label: `History (${versions.data?.length ?? m.version})` }]}>
        <TabPanel value="editor">
          <MeshEditor key={`${meshId}-${m.version}`} mesh={m} onSaved={() => { void qc.invalidateQueries({ queryKey: qk.mesh(meshId, null) }); void qc.invalidateQueries({ queryKey: qk.meshVersions(meshId) }) }} />
        </TabPanel>
        <TabPanel value="structure">
          <Structure m={m} />
        </TabPanel>
        <TabPanel value="history">
          <History versions={versions.data ?? []} current={m} />
        </TabPanel>
      </Tabs>
    </div>
  )
}

function Structure({ m }: { m: Mesh }) {
  return (
    <div className="p-5 text-sm">
      <h3 className="text-xs font-semibold uppercase tracking-wide text-clay">Routes · first match wins</h3>
      <ol className="mt-2 flex flex-col gap-2">
        {m.routes.map((r, i) => (
          <li key={i} className="rounded-md border border-line p-3">
            <pre className="whitespace-pre-wrap mono text-xs text-ink-2">{r.text}</pre>
            <div className="mt-2 flex flex-wrap gap-1">
              {r.output.disposition ? <Chip tone={r.output.disposition.kind === 'find' ? 'ok' : r.output.disposition.kind === 'spoil' || r.output.disposition.kind === 'contamination' ? 'warn' : 'neutral'}>{dispositionLabel(r.output.disposition)}</Chip> : null}
              {r.output.strata.map((s) => (
                <Chip key={s}>Stratum: {s}</Chip>
              ))}
              {r.output.notify ? <Chip tone="busy">Notify</Chip> : null}
              {r.output.escalate ? <Chip tone="warn">Escalate to LLM</Chip> : null}
            </div>
          </li>
        ))}
        {!m.routes.length ? <li className="text-xs text-clay">No routes: this Mesh only guards.</li> : null}
      </ol>
      <h3 className="mt-6 text-xs font-semibold uppercase tracking-wide text-clay">Guards · run after every route, from every scope</h3>
      <ul className="mt-2 flex flex-col gap-1">
        {m.guards.map((g, i) => (
          <li key={i} className="rounded-md border border-rust/30 bg-rust/5 px-3 py-2 mono text-xs">
            {g.text}
          </li>
        ))}
        {!m.guards.length ? <li className="text-xs text-clay">No guards. The Never Hidden gate still runs before any Signal and no route overrides it.</li> : null}
      </ul>
    </div>
  )
}

function History({ versions, current }: { versions: Mesh[]; current: Mesh }) {
  const [sel, setSel] = useState<number | null>(null)
  const { run } = useAction()
  const chosen = versions.find((v) => v.version === sel) ?? null
  const prev = chosen ? versions.find((v) => v.version === chosen.version - 1) ?? null : null
  return (
    <div className="flex min-h-0 flex-1">
      <ol className="w-64 shrink-0 overflow-auto border-r border-line">
        {versions.map((v) => (
          <li key={v.version}>
            <button type="button" className={cx('w-full px-4 py-2 text-left text-sm', sel === v.version ? 'row-selected' : 'hover:bg-sand/60')} onClick={() => setSel(v.version)} aria-pressed={sel === v.version}>
              <div className="flex items-center justify-between">
                <span className="font-medium">v{v.version}</span>
                <span className="text-xs text-clay">{fmtRelative(v.createdAt)}</span>
              </div>
              <div className="truncate text-xs text-ink-2">{v.changeNote ?? '—'} · {v.createdBy}</div>
            </button>
          </li>
        ))}
      </ol>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        {chosen ? (
          <>
            <div className="mb-2 flex items-center justify-between text-sm">
              <span>
                {prev ? `v${prev.version} → ` : ''}v{chosen.version}
                {chosen.changeNote ? <span className="text-ink-2"> · {chosen.changeNote}</span> : null}
              </span>
              {chosen.version !== current.version ? (
                <Button size="sm" onClick={() => run('update_mesh', { meshId: current.id, text: chosen.sourceText.replace(/ v\d+/, ` v${current.version + 1}`), changeNote: `Restored from v${chosen.version}` })}>
                  Restore as v{current.version + 1}
                </Button>
              ) : null}
            </div>
            <pre className="rounded border border-line bg-sand p-3 mono text-xs">
              {(prev ? diffLines(prev.sourceText, chosen.sourceText) : chosen.sourceText.split('\n').map((t) => ({ kind: 'same' as const, text: t }))).map((l, i) => (
                <div key={i} className={cx(l.kind === 'add' && 'bg-moss/15 text-moss', l.kind === 'del' && 'bg-rust/15 text-rust line-through')}>
                  <span className="inline-block w-4 select-none text-clay">{l.kind === 'add' ? '+' : l.kind === 'del' ? '−' : ' '}</span>
                  {l.text || ' '}
                </div>
              ))}
            </pre>
          </>
        ) : (
          <div className="text-sm text-clay">Select a version to see its diff against the previous one. Every decision remembers which version made it.</div>
        )}
      </div>
    </div>
  )
}

export function MeshEditor({ mesh, onSaved }: { mesh: Mesh | null; onSaved: (id: string) => void }) {
  const { run } = useAction()
  const [text, setText] = useState(mesh?.sourceText ?? NEW_MESH_TEMPLATE)
  const [issues, setIssues] = useState<MeshParseIssue[]>([])
  const [parsed, setParsed] = useState<Mesh | null>(null)
  const [report, setReport] = useState<SimulationReport | null>(null)
  const [simulating, setSimulating] = useState(false)
  const [simError, setSimError] = useState<unknown>(null)
  const [note, setNote] = useState('')
  const [status, setStatus] = useState<'draft' | 'shadow'>('shadow')
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const dirty = mesh ? text !== mesh.sourceText : true
  const lines = text.split('\n').length

  useEffect(() => {
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(async () => {
      try {
        const p = await parseMesh(text)
        setParsed(p)
        setIssues([])
      } catch (e) {
        setParsed(null)
        setIssues(issuesFrom(e))
      }
    }, 250)
    return () => {
      if (timer.current) clearTimeout(timer.current)
    }
  }, [text])

  const preview = async () => {
    setSimulating(true)
    setSimError(null)
    try {
      setReport(await simulateMesh({ text, meshId: mesh?.id }))
    } catch (e) {
      setSimError(e)
    } finally {
      setSimulating(false)
    }
  }
  const apply = async () => {
    const out = mesh ? await run<{ mesh: Mesh; report: SimulationReport }>('update_mesh', { meshId: mesh.id, text, changeNote: note || null }) : await run<{ mesh: Mesh; report: SimulationReport }>('create_mesh', { text, status, changeNote: note || null })
    if (out) {
      setReport(out.result.report)
      onSaved(out.result.mesh.id)
    }
  }
  useShortcuts('meshes', { mesh_preview: () => void preview(), mesh_save: () => !issues.length && void apply() })
  const issueLines = new Set(issues.map((i) => i.line))
  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col p-4">
        <div className="relative flex min-h-0 flex-1 rounded-md border border-line bg-sand">
          <div aria-hidden className="w-9 shrink-0 select-none border-r border-line py-2 text-right mono text-xs leading-5 text-clay">
            {Array.from({ length: lines }, (_, i) => (
              <div key={i} className={cx('pr-1.5', issueLines.has(i + 1) && 'bg-rust/20 text-rust font-semibold')}>
                {i + 1}
              </div>
            ))}
          </div>
          <textarea
            value={text}
            onChange={(e) => setText(e.target.value)}
            spellCheck={false}
            aria-label="Mesh text"
            aria-invalid={issues.length > 0}
            className="min-h-0 flex-1 resize-none bg-transparent p-2 mono text-xs leading-5 text-ink focus:outline-none"
          />
        </div>
        <div className="mt-2 min-h-6 text-xs">
          {issues.length ? (
            <ul className="flex flex-col gap-0.5 text-rust" role="alert">
              {issues.map((i, k) => (
                <li key={k}>
                  line {i.line}: {i.message}
                </li>
              ))}
            </ul>
          ) : parsed ? (
            <span className="text-moss">Valid: {parsed.routes.length} route{parsed.routes.length === 1 ? '' : 's'}, {parsed.guards.length} guard{parsed.guards.length === 1 ? '' : 's'}, scope {parsed.scope.kind === 'all_digs' ? 'All Digs' : parsed.scope.kind === 'dig_group' ? `${parsed.scope.group} Digs` : 'one Dig'}.</span>
          ) : (
            <span className="text-clay">Checking…</span>
          )}
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-2">
          <input value={note} onChange={(e) => setNote(e.target.value)} placeholder="Change note (why this version exists)" aria-label="Change note" className="h-7 min-w-64 flex-1 rounded-md border border-line bg-paper px-2 text-xs" />
          {!mesh ? (
            <label className="flex items-center gap-1 text-xs text-ink-2">
              Start as
              <select value={status} onChange={(e) => setStatus(e.target.value as 'draft' | 'shadow')} className="rounded border border-line bg-paper px-1 py-0.5" aria-label="Initial status">
                <option value="shadow">Shadow (proposes)</option>
                <option value="draft">Draft (off)</option>
              </select>
            </label>
          ) : null}
          <Button size="sm" busy={simulating} onClick={preview} disabledReason={issues.length ? 'Fix the errors first' : null} title="Preview impact (⌘↵)">
            Preview impact
          </Button>
          <Button size="sm" variant="primary" onClick={apply} disabledReason={issues.length ? 'Fix the errors first' : !dirty ? 'No changes' : null} title="Apply (⌘S)">
            {mesh ? `Apply as v${mesh.version + 1}` : 'Create'}
          </Button>
          {mesh && dirty ? (
            <Button size="sm" variant="ghost" onClick={() => { setText(mesh.sourceText); setReport(null) }}>
              Keep v{mesh.version}
            </Button>
          ) : null}
        </div>
      </div>
      <aside className="w-80 shrink-0 overflow-auto border-l border-line p-4">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-clay">Preview impact</h3>
        {simError ? <Note tone="bad" className="mt-2">{toRiddleError(simError).message}</Note> : null}
        {report ? (
          <div className="mt-2">
            <SimulationTable r={report} compact />
            {mesh && dirty ? <p className="mt-3 text-xs text-ink-2">Apply creates v{mesh.version + 1}{mesh.status === 'active' ? ', which starts in Shadow' : ''}. Keep discards the edit.</p> : null}
          </div>
        ) : (
          <p className="mt-2 text-xs text-ink-2">Run Preview impact to execute this text against your history: how many messages would change, which way, and whether any Signal needs a fresh judge call. Nothing changes until you Apply.</p>
        )}
        <h3 className="mt-6 text-xs font-semibold uppercase tracking-wide text-clay">Syntax</h3>
        <pre className="mt-1 whitespace-pre-wrap mono text-[11px] leading-4 text-ink-2">{`Mesh: <name> vN
Scope:
All Digs | <Group> Digs | Dig: <name>
ROUTES
IF signal@vN > .80
     AND fact has_attachments = true
    → Find: Review | Filed | Spoil
      | Contamination | Escalate
      + Stratum: <name> + Notify
ELSE IF … / ELSE
GUARDS
NEVER Spoil IF sender IN never_hidden`}</pre>
      </aside>
    </div>
  )
}

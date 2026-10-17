/**
 * Meshes (README "Mesh", ARCHITECTURE §36-38, §51): the list with status
 * and Shadow agreement, an editor on the DSL text with live validation,
 * a structural view, version history with diff, Preview impact, Apply or
 * Keep, status controls (Shadow → Active holds when the Mesh proposes
 * Spoil), soft delete with Recently Deleted and restore.
 *
 * Editor decisions, each after a finding in the UX study:
 * - The line-number gutter follows the textarea's scroll and lines never
 *   wrap, because on a 60-line Mesh the marker for line 50 used to sit
 *   beside line 12.
 * - Leaving the editor runs a 30-day simulation of the text, cached by
 *   text, so the Valid line can say what the change would do before anyone
 *   presses Preview; Preview (⌘↵) feeds the same line from its full report
 *   rather than running a second, smaller simulation beside it. The line
 *   says only what the report's totals support; it carries no per-route
 *   counts.
 * - ⌘I inserts a header line, fact, mail header or Signal at the caret from
 *   the same list the Syntax box shows, and that list is complete (see
 *   meshSyntax.ts).
 * - A Director proposal arrives as location state (`draftText`) and opens
 *   the Mesh it names, or the New editor, with a note that the proposal is
 *   still pending. "Describe the rule" hands a description to the Director
 *   composer; nothing is sent from here.
 * - A Mesh capped by an `Autonomy:` line says so on every surface (list
 *   chip, Valid line, Structure), because the cap arrived in the backend
 *   first and a tester who typed the line could not tell it had worked
 *   (autonomy.ts).
 * - Under the header, what the Mesh did in the last 30 days and how much of
 *   it the user reversed (precision.ts). A Mesh the user kept reversing used
 *   to look like one that never missed, and the backend's step back to
 *   Shadow showed up as a bare status change.
 * - "Your corrections" (features/rules) is a Mesh like any other, listed
 *   first and labelled as the user's own, with a note that says where its
 *   lines come from and where else they can be removed.
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react'
import { Link, useLocation, useNavigate, useParams, useSearchParams } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowDownWideNarrow, History as HistoryIcon, ListPlus, Network, Plus, RotateCcw, Trash2 } from 'lucide-react'
import type { Mesh } from '@/bindings/Mesh'
import type { MeshStatus } from '@/bindings/MeshStatus'
import type { SimulationReport } from '@/bindings/SimulationReport'
import { getMesh, meshVersions, parseMesh, qk, simulateMesh } from '@/lib/api'
import { cx, fmtInt, fmtPct, fmtRelative, plural } from '@/lib/format'
import { diffLines } from '@/lib/diff'
import { issuesFrom } from '@/lib/meshIssues'
import { useDigContext, useMeshes, useSignals, useTrustStats } from '@/lib/queries'
import { dispositionLabel } from '@/lib/selectors'
import { keyHint } from '@/lib/shortcuts'
import { useDirector } from '@/lib/stores'
import type { MeshParseIssue } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { useShortcuts } from '@/lib/useShortcuts'
import { toRiddleError } from '@/lib/ipc'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { EmptyState } from '@/components/ui/EmptyState'
import { Menu } from '@/components/ui/Menu'
import { Note } from '@/components/ui/Note'
import { Tabs, TabPanel } from '@/components/ui/Tabs'
import { ErrorCard } from '@/components/ErrorCard'
import { HoldButton } from '@/components/confirm/HoldButton'
import { PageHeader } from '@/components/Bits'
import type { MeshDraftState } from '@/features/director/ProposalsPanel'
import { YOUR_CORRECTIONS_MESH } from '@/features/rules/yourCorrections'
import { belowAct } from '@/features/settings/accounts/model'
import { autonomySentence, capChipTip, capValidTail } from './autonomy'
import { liveMatchSentence } from './liveMatch'
import { factsLine, insertAt, insertMenuEntries, type Snippet } from './meshSyntax'
import { dayLabel, precisionFor, steppedBack } from './precision'
import { OrderView } from './OrderView'
import { SimulationTable } from './SimulationTable'
import type { Tone } from '@/lib/selectors'
import { Resizable } from '@/components/Resizable'

/** What each status means, for the chip and its tooltip; the enum never shows. */
const STATUS: Record<MeshStatus, { label: string; tone: Tone; tip: string }> = {
  draft: { label: 'Draft', tone: 'neutral', tip: 'Off until you start it in Shadow' },
  shadow: { label: 'Shadow', tone: 'warn', tip: 'Proposes only; you decide' },
  active: { label: 'Active', tone: 'ok', tip: 'Decides on its own' },
  disabled: { label: 'Off', tone: 'neutral', tip: 'Not evaluated' },
  deleted: { label: 'Deleted', tone: 'bad', tip: 'In Recently Deleted; restore it to use it' },
}

export function StatusChip({ status }: { status: MeshStatus }) {
  const s = STATUS[status]
  return (
    <Chip tone={s.tone} title={s.tip}>
      {s.label}
    </Chip>
  )
}

/** The window the live line checks against; short so it is cheap on blur. */
const LIVE_WINDOW_DAYS = 30
/** The window the precision line counts over; the trust card's longest. */
const PRECISION_WINDOW_DAYS = 30

/** The tooltip of the "your rules" chip, on the list row and the header. */
const YOUR_RULES_TIP = 'Built from your corrections on the Spoil and Contamination pages; each line is one sender or domain'

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

/** The scope in the app's words (accounts); the DSL itself still says Digs. */
export function scopeLabel(m: Pick<Mesh, 'scope'>, digName: (id: string) => string): string {
  switch (m.scope.kind) {
    case 'all_digs':
      return 'All accounts'
    case 'dig_group':
      return `${m.scope.group} accounts`
    case 'dig':
      return `Account: ${digName(m.scope.dig_id)}`
  }
}

export function proposesSpoil(m: Mesh): boolean {
  return m.routes.some((r) => r.output.disposition?.kind === 'spoil' || r.output.disposition?.kind === 'contamination')
}

/** The Director's "Adjust" hands the proposal over as location state (ProposalsPanel). */
function draftFrom(state: unknown): MeshDraftState | null {
  const s = state as Partial<MeshDraftState> | null | undefined
  if (!s || typeof s.draftText !== 'string' || !s.draftText.trim()) return null
  return { draftText: s.draftText, fromPendingId: typeof s.fromPendingId === 'string' ? s.fromPendingId : '', meshId: typeof s.meshId === 'string' ? s.meshId : null, meshName: typeof s.meshName === 'string' ? s.meshName : null }
}

export default function MeshesPage() {
  const { meshId } = useParams()
  const [params] = useSearchParams()
  const nav = useNavigate()
  const loc = useLocation()
  const draft = draftFrom(loc.state)
  const draftText = draft?.draftText ?? null
  const { digName } = useDigContext()
  const [showDeleted, setShowDeleted] = useState(false)
  const list = useMeshes(true)
  // The user's own rules first; the rest keep the order the store gave them.
  const meshes = useMemo(
    () =>
      (list.data ?? [])
        .filter((m) => showDeleted || !m.deletedAt)
        .sort((a, b) => Number(b.name === YOUR_CORRECTIONS_MESH) - Number(a.name === YOUR_CORRECTIONS_MESH)),
    [list.data, showDeleted],
  )
  const deletedCount = (list.data ?? []).filter((m) => m.deletedAt).length
  const byName = params.get('name')
  useEffect(() => {
    if (byName && list.data) {
      const m = list.data.find((x) => x.name === byName)
      if (m) nav(`/meshes/${m.id}`, { replace: true })
    }
  }, [byName, list.data, nav])
  const [creating, setCreating] = useState(false)
  // A draft from a Director proposal opens the Mesh it is for (an update_mesh
  // proposal names it), or the New editor. The state travels with the replace
  // so a remount still finds it; choosing another Mesh or New drops it.
  useEffect(() => {
    if (!draft || !list.data) return
    const target = draft.meshId ? list.data.find((m) => m.id === draft.meshId) : undefined
    if (target) {
      if (target.id !== meshId || creating) {
        setCreating(false)
        nav(`/meshes/${target.id}`, { replace: true, state: loc.state })
      }
    } else if (!creating) {
      setCreating(true)
      if (meshId) nav('/meshes', { replace: true, state: loc.state })
    }
  }, [draft, list.data, meshId, creating, nav, loc.state])
  if (list.error) return <ErrorCard error={list.error} onRetry={() => list.refetch()} />
  return (
    <div className="flex min-h-0 flex-1">
      <section className="flex w-[360px] shrink-0 flex-col border-r border-line">
        <PageHeader title="Meshes" subtitle="Policy as versioned text. A new or changed Mesh suggests until it has earned the right to act." actions={<><Button size="sm" variant="ghost" icon={<ArrowDownWideNarrow />} title="The order Meshes run in, and which take mail from others" onClick={() => { setCreating(false); nav('/meshes') }}>Order</Button><Button size="sm" icon={<Plus />} onClick={() => { setCreating(true); nav('/meshes') }}>New</Button></>} />
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
                  {m.name === YOUR_CORRECTIONS_MESH ? <Chip title={YOUR_RULES_TIP}>your rules</Chip> : null}
                  {m.pack ? <Chip title={`Installed from the ${m.pack.id} pack, v${m.pack.version}`}>pack</Chip> : null}
                  {belowAct(m.autonomy) ? <Chip title={capChipTip(m.autonomy)}>Riddle only</Chip> : null}
                  <StatusChip status={m.status} />
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
        {creating ? <MeshEditor key={draftText ?? 'new'} mesh={null} initialText={draftText} onSaved={(id) => { setCreating(false); nav(`/meshes/${id}`) }} /> : meshId ? <MeshDetail key={meshId} meshId={meshId} draftText={draftText} /> : <OrderView />}
      </section>
    </div>
  )
}

function MeshDetail({ meshId, draftText }: { meshId: string; draftText: string | null }) {
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
  const yours = m.name === YOUR_CORRECTIONS_MESH
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={`${m.name} v${m.version}`}
        subtitle={
          <span className="flex flex-wrap items-center gap-2">
            <StatusChip status={m.status} />
            <span>{scopeLabel(m, digName)}</span>
            {yours ? (
              <>
                <Chip title={YOUR_RULES_TIP}>your rules</Chip>
                <span>· since {fmtRelative(m.createdAt)}</span>
              </>
            ) : m.createdBy === 'system' ? (
              <>
                <Chip title="Shipped with Riddle">a Riddle default</Chip>
                <span>· since {fmtRelative(m.createdAt)}</span>
              </>
            ) : (
              <span>
                · created by {m.createdBy === 'user' ? 'you' : 'the Director'} {fmtRelative(m.createdAt)}
              </span>
            )}
            {/* In Shadow the precision line below carries the proposals; here it would say the same thing twice. */}
            {agree !== null && m.status !== 'shadow' ? <span>· Shadow agreement {fmtPct(agree, 0)} ({m.shadowAgreed}/{m.shadowProposed})</span> : null}
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
                <Button size="sm" onClick={() => setStatus('shadow')} disabledReason={m.status === 'deleted' ? 'Restore it first' : null} title="Proposes only; you decide">
                  Shadow
                </Button>
              ) : null}
              {m.status !== 'active' && !spoils ? (
                <Button size="sm" variant="primary" onClick={() => setStatus('active')} disabledReason={m.status === 'draft' ? 'A draft starts in Shadow; it earns Active by agreeing with you' : null} title="Decides on its own">
                  Activate
                </Button>
              ) : null}
              {m.status !== 'disabled' ? (
                <Button size="sm" onClick={() => setStatus('disabled')} title="Stops evaluating it; the text stays">
                  Turn off
                </Button>
              ) : null}
              {m.status === 'active' ? (
                <Menu
                  label="Apply to past mail"
                  trigger={
                    <Button size="sm" icon={<HistoryIcon />} title="Activating decides new mail only. This sends the mail this Mesh would decide differently back to Sift, after a preview you confirm; what you decided yourself is kept, and it can be undone.">
                      Apply to past mail
                    </Button>
                  }
                  items={[30, 90, 365].map((windowDays) => ({
                    label: windowDays === 365 ? 'The last year' : `The last ${windowDays} days`,
                    onSelect: () => run('apply_mesh_to_history', { meshId, windowDays }),
                  }))}
                />
              ) : null}
              <Button size="sm" variant="danger" icon={<Trash2 />} onClick={() => run('delete_mesh', { meshId }, { invalidate: [qk.mesh(meshId, null)] })} title="It can be restored from Recently Deleted">
                Move to Recently Deleted
              </Button>
            </>
          )
        }
      />
      <Precision m={m} />
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
          <MeshEditor
            key={`${meshId}-${m.version}${draftText ? '-draft' : ''}`}
            mesh={m}
            initialText={draftText}
            banner={
              yours ? (
                <Note className="mb-2 text-xs">
                  Built from your corrections on the Spoil and Contamination pages; each line is one sender or domain. Edit here, or remove lines in{' '}
                  <Link to="/settings#rules" className="underline hover:text-ink">
                    Settings → Mail rules
                  </Link>
                  .
                </Note>
              ) : null
            }
            onSaved={() => { void qc.invalidateQueries({ queryKey: qk.mesh(meshId, null) }); void qc.invalidateQueries({ queryKey: qk.meshVersions(meshId) }) }}
          />
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

/**
 * What the Mesh did in the window and how much of it the user reversed, from
 * the trust card's rows so the two pages agree. Nothing shows until the
 * stats arrive, and a failed fetch shows nothing: bookkeeping never fails
 * the page. The account in the tooltip is the one selected in the rail,
 * because that is the scope the rows were computed for. The step back to
 * Shadow is the backend's record on the Mesh, with the counts it acted on.
 */
function Precision({ m }: { m: Mesh }) {
  const { digId, digName } = useDigContext()
  const stats = useTrustStats(PRECISION_WINDOW_DAYS)
  if (!stats.data) return null
  const p = precisionFor(stats.data.rules, m)
  const demoted = steppedBack(m)
  const where = digId ? `in ${digName(digId)}` : 'across all accounts'
  const window = `the last ${PRECISION_WINDOW_DAYS} days`
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 border-b border-line px-5 py-2 text-xs text-ink-2 selectable">
      <p>
        Decided <span title={`Sift decisions any version of this Mesh made in ${window} ${where}; Shadow proposals are not decisions`}>{fmtInt(p.decisions)}</span> in {window} · you reversed{' '}
        <span title="Of those decisions, the ones you changed afterwards and did not undo">{fmtInt(p.reversed)}</span>
      </p>
      {m.status === 'shadow' ? (
        <p>
          · proposed <span title="Proposals made since it entered Shadow; a proposal changes nothing until you act on it">{fmtInt(p.shadowProposed)}</span> · you agreed{' '}
          <span title="Proposals you did not contradict">{fmtInt(p.shadowAgreed)}</span>
          {p.agreement !== null ? ` (${fmtPct(p.agreement, 0)})` : ''}
        </p>
      ) : null}
      {p.reversedOften ? (
        <Chip tone="warn" title={`You reversed a quarter or more of its decisions in ${window}. Riddle steps a Mesh back to Shadow when a quarter of its last 20 decisions are reversed.`}>
          reversed often
        </Chip>
      ) : null}
      {demoted ? (
        <p>
          Stepped back to Shadow on {dayLabel(demoted.at)}: you reversed {fmtInt(demoted.reversed)} of {fmtInt(demoted.decided)}.
        </p>
      ) : null}
    </div>
  )
}

function Structure({ m }: { m: Mesh }) {
  return (
    <div className="p-5 text-sm">
      <h3 className="text-xs font-semibold uppercase tracking-wide text-clay">Autonomy</h3>
      <p className="mt-2 text-xs text-ink-2 selectable">{autonomySentence(m)}</p>
      <h3 className="mt-6 text-xs font-semibold uppercase tracking-wide text-clay">Routes · first match wins</h3>
      <ol className="mt-2 flex flex-col gap-2">
        {m.routes.map((r, i) => (
          <li key={i} className="rounded-md border border-line p-3">
            <pre className="whitespace-pre-wrap mono text-xs text-ink-2">{r.text}</pre>
            {r.words ? <p className="mt-1.5 text-xs text-ink selectable">{r.words}</p> : null}
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
      {/* The order is what riddle-mesh's evaluator does (eval.rs §36): scope specificity, then priority, then name. */}
      <p className="mt-2 text-xs text-ink-2">
        Across Meshes, the most specific scope runs first (one account, then its group, then All accounts), then priority (this Mesh: {m.priority}; lower runs first), then name. The first route that matches decides, and every guard from every scope still vetoes after that. That order is not shown here yet.
      </p>
      <h3 className="mt-6 text-xs font-semibold uppercase tracking-wide text-clay">Guards · run after every route, from every scope</h3>
      <ul className="mt-2 flex flex-col gap-1">
        {m.guards.map((g, i) => (
          <li key={i} className="rounded-md border border-rust/30 bg-rust/5 px-3 py-2 text-xs">
            <span className="mono">{g.text}</span>
            {g.words ? <p className="mt-1 text-ink selectable">{g.words}</p> : null}
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
      <Resizable as="ol" id="mesh-versions" side="left" initial={256} min={180} max={480} label="Resize the version list" className="overflow-auto border-r border-line">
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
      </Resizable>
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

const SYNTAX = `Mesh: <name> vN
Scope:
All Digs | <Group> Digs | Dig: <name>
Priority: 0        lower runs first
Autonomy: Observe | Suggest | Act
    the most this Mesh may do in the
    mailbox, whatever the account allows;
    Trusted is an account setting
ROUTES
IF signal@vN > .80 AND has_attachments
    → Find: Review | Filed | Spoil
      | Contamination | Escalate
      + Stratum: <name> + Notify
      + Replaces    newest Find from the
                    sender closes the older
      + Replaces: same subject   one series
                    per subject, numbers aside
      + Expires: 18h | 2d   after arrival
ELSE IF … / ELSE

WHEN sender_domain = github.com
    IF sender_name CONTAINS "dependabot" → Spoil
    ELSE IF header X-GitHub-Reason = ci_activity
         AND ci_failure@v1 > .85 → Find: Do
    ELSE → Filed
END

${factsLine()}
Text: = != CONTAINS (any case)

GUARDS
NEVER Spoil IF sender IN never_hidden`

export function MeshEditor({ mesh, onSaved, initialText = null, banner = null }: { mesh: Mesh | null; onSaved: (id: string) => void; initialText?: string | null; banner?: ReactNode }) {
  const { run } = useAction()
  const nav = useNavigate()
  const { digName } = useDigContext()
  const signals = useSignals()
  const fromDirector = initialText !== null
  const [text, setText] = useState(initialText ?? mesh?.sourceText ?? NEW_MESH_TEMPLATE)
  const [issues, setIssues] = useState<MeshParseIssue[]>([])
  const [parsed, setParsed] = useState<Mesh | null>(null)
  /** The text the last successful parse was of, so a blur mid-debounce waits for it. */
  const parsedText = useRef<string | null>(null)
  const wantLive = useRef<string | null>(null)
  const [report, setReport] = useState<SimulationReport | null>(null)
  const [simulating, setSimulating] = useState(false)
  const [simError, setSimError] = useState<unknown>(null)
  const [note, setNote] = useState('')
  const [status, setStatus] = useState<'draft' | 'shadow'>('shadow')
  const [describe, setDescribe] = useState('')
  const [insertOpen, setInsertOpen] = useState(false)
  const [live, setLive] = useState<{ text: string; report: SimulationReport } | null>(null)
  const [liveBusy, setLiveBusy] = useState(false)
  const liveCache = useRef(new Map<string, SimulationReport>())
  const liveSeq = useRef(0)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const ta = useRef<HTMLTextAreaElement>(null)
  const gutter = useRef<HTMLDivElement>(null)
  const caret = useRef<[number, number] | null>(null)
  const dirty = mesh ? text !== mesh.sourceText : true
  const lines = text.split('\n').length

  // Cached by text: a blur with unchanged text costs nothing.
  const liveMatch = async (t: string) => {
    const hit = liveCache.current.get(t)
    if (hit) {
      setLive({ text: t, report: hit })
      return
    }
    const seq = ++liveSeq.current
    setLiveBusy(true)
    try {
      const r = await simulateMesh({ text: t, meshId: mesh?.id, windowDays: LIVE_WINDOW_DAYS })
      liveCache.current.set(t, r)
      setLive({ text: t, report: r })
    } catch {
      // The parse issues list already explains an invalid text; the live line stays quiet.
    } finally {
      if (seq === liveSeq.current) setLiveBusy(false)
    }
  }

  useEffect(() => {
    if (timer.current) clearTimeout(timer.current)
    timer.current = setTimeout(async () => {
      try {
        const p = await parseMesh(text)
        parsedText.current = text
        setParsed(p)
        setIssues([])
        if (wantLive.current === text) {
          wantLive.current = null
          void liveMatch(text)
        }
      } catch (e) {
        parsedText.current = null
        wantLive.current = null
        setParsed(null)
        setIssues(issuesFrom(e))
      }
    }, 250)
    return () => {
      if (timer.current) clearTimeout(timer.current)
    }
  }, [text]) // eslint-disable-line react-hooks/exhaustive-deps

  // After an insert, put the caret where the snippet wants it.
  useEffect(() => {
    const c = caret.current
    if (!c || !ta.current) return
    caret.current = null
    ta.current.focus()
    ta.current.setSelectionRange(c[0], c[1])
  }, [text])

  const onBlur = () => {
    if (parsed && parsedText.current === text) void liveMatch(text)
    else wantLive.current = text
  }
  const preview = async () => {
    if (issues.length) return
    setSimulating(true)
    setSimError(null)
    try {
      const r = await simulateMesh({ text, meshId: mesh?.id })
      setReport(r)
      // The full report covers the live line's window, so it answers the line too.
      setLive({ text, report: r })
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
  const insert = (s: Snippet) => {
    const el = ta.current
    const start = el?.selectionStart ?? text.length
    const end = el?.selectionEnd ?? start
    const r = insertAt(text, start, end, s)
    caret.current = [r.selectionStart, r.selectionEnd]
    setText(r.text)
  }
  const askDirector = () => {
    const d = describe.trim().replace(/\.$/, '')
    if (!d) return
    const what = mesh ? `Draft a new version of the Mesh "${mesh.name}" that: ${d}.` : `Draft a Mesh that: ${d}.`
    useDirector.getState().setPrefill(`${what} Show me the DSL and what it would do to recent mail before proposing it.`)
    nav('/director')
  }
  useShortcuts('meshes', { mesh_preview: () => void preview(), mesh_save: () => !issues.length && void apply(), mesh_insert: () => setInsertOpen(true) })
  const issueLines = new Set(issues.map((i) => i.line))
  const liveForThis = live && live.text === text ? live.report : null
  const cap = parsed?.autonomy ?? null
  const validLine = parsed ? `Valid: ${plural(parsed.routes.length, 'route')}, ${plural(parsed.guards.length, 'guard')}, scope ${scopeLabel(parsed, digName)}${capValidTail(cap)}.` : ''
  return (
    <div className="flex min-h-0 flex-1">
      <div className="flex min-w-0 flex-1 flex-col p-4">
        {banner}
        {fromDirector ? (
          <Note tone="busy" className="mb-2 text-xs">
            From a Director proposal; the proposal stays pending until you apply or decline it.
          </Note>
        ) : null}
        <form
          className="mb-2 flex items-center gap-2"
          onSubmit={(e) => {
            e.preventDefault()
            askDirector()
          }}
        >
          <input value={describe} onChange={(e) => setDescribe(e.target.value)} placeholder="Describe the rule in plain words, e.g. file CI failures I am not tagged on" aria-label="Describe the rule" className="h-7 min-w-0 flex-1 rounded-md border border-line bg-paper px-2 text-xs" />
          <Button size="sm" onClick={askDirector} disabledReason={describe.trim() ? null : 'Describe the rule first'} title="Opens the Director with the request written; nothing is sent until you send it">
            Ask the Director to draft it
          </Button>
        </form>
        <div className="relative flex min-h-0 flex-1 rounded-md border border-line bg-sand">
          <div ref={gutter} aria-hidden className="w-9 shrink-0 select-none overflow-hidden border-r border-line py-2 text-right mono text-xs leading-5 text-clay">
            {Array.from({ length: lines }, (_, i) => (
              <div key={i} className={cx('pr-1.5', issueLines.has(i + 1) && 'bg-rust/20 text-rust font-semibold')}>
                {i + 1}
              </div>
            ))}
          </div>
          <textarea
            ref={ta}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onScroll={(e) => {
              if (gutter.current) gutter.current.scrollTop = e.currentTarget.scrollTop
            }}
            onBlur={onBlur}
            wrap="off"
            spellCheck={false}
            aria-label="Mesh text"
            aria-invalid={issues.length > 0}
            className="min-h-0 flex-1 resize-none overflow-auto whitespace-pre bg-transparent p-2 mono text-xs leading-5 text-ink focus:outline-none"
          />
        </div>
        <div className="mt-2 min-h-6 text-xs selectable">
          {issues.length ? (
            <ul className="flex flex-col gap-0.5 text-rust" role="alert">
              {issues.map((i, k) => (
                <li key={k}>
                  line {i.line}: {i.message}
                </li>
              ))}
            </ul>
          ) : parsed ? (
            <span>
              <span className="text-moss">{validLine}</span>{' '}
              {liveForThis ? <span className="text-ink-2">{liveMatchSentence(liveForThis)}</span> : liveBusy ? <span className="text-clay">Checking the last {LIVE_WINDOW_DAYS} days…</span> : <span className="text-clay">Click away or press {keyHint('mesh_preview')} to check it against the last {LIVE_WINDOW_DAYS} days.</span>}
            </span>
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
          <Menu
            label="Insert into the Mesh text"
            align="start"
            open={insertOpen}
            onOpenChange={setInsertOpen}
            onCloseAutoFocus={(e) => {
              e.preventDefault()
              ta.current?.focus()
            }}
            items={insertMenuEntries(signals.data ?? [], insert)}
            trigger={
              <Button size="sm" icon={<ListPlus />} title={`Insert a header line, fact, mail header or Signal (${keyHint('mesh_insert')})`}>
                Insert
              </Button>
            }
          />
          <Button size="sm" busy={simulating} onClick={preview} disabledReason={issues.length ? 'Fix the errors first' : null} title={`Preview impact (${keyHint('mesh_preview')})`}>
            Preview impact
          </Button>
          <Button size="sm" variant="primary" onClick={apply} disabledReason={issues.length ? 'Fix the errors first' : !dirty ? 'No changes' : null} title={`Apply (${keyHint('mesh_save')})`}>
            {mesh ? `Apply as v${mesh.version + 1}` : 'Create'}
          </Button>
          {mesh && dirty ? (
            <Button size="sm" variant="ghost" onClick={() => { setText(mesh.sourceText); setReport(null) }}>
              Keep v{mesh.version}
            </Button>
          ) : null}
        </div>
      </div>
      <Resizable as="aside" id="mesh-preview" side="right" initial={320} min={240} max={640} label="Resize the preview" className="overflow-auto border-l border-line p-4">
        <h3 className="text-xs font-semibold uppercase tracking-wide text-clay">Preview impact</h3>
        {simError ? <Note tone="bad" className="mt-2">{toRiddleError(simError).message}</Note> : null}
        {report ? (
          <div className="mt-2">
            <SimulationTable r={report} compact fallback={mesh?.fallback} />
            {mesh && dirty ? <p className="mt-3 text-xs text-ink-2">Apply creates v{mesh.version + 1}{mesh.status === 'active' ? ', which starts in Shadow' : ''}. Keep discards the edit.</p> : null}
          </div>
        ) : (
          <p className="mt-2 text-xs text-ink-2">Run Preview impact to execute this text against your history: how many messages would change, which way, and whether any Signal needs a fresh judge call. Nothing changes until you Apply.</p>
        )}
        <h3 className="mt-6 text-xs font-semibold uppercase tracking-wide text-clay">Syntax</h3>
        <pre className="mt-1 whitespace-pre-wrap mono text-2xs text-ink-2 selectable">{SYNTAX}</pre>
      </Resizable>
    </div>
  )
}

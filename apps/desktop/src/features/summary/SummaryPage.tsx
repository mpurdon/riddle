/**
 * The home card (README "The Core Experience"): honest about whether
 * Riddle has finished, counts as links, the Director's note, the Shadow
 * Period card, and the Attention Zero state with its evidence.
 */
import { Link, useNavigate } from 'react-router-dom'
import { Sunrise } from 'lucide-react'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { ErrorCard } from '@/components/ErrorCard'
import { fmtInt, fmtRelative, num } from '@/lib/format'
import { useSummary, useDigContext } from '@/lib/queries'
import { summaryCopy } from '@/lib/selectors'
import { useAction, useLastUndoable } from '@/lib/useAction'
import { useUi } from '@/lib/stores'

export default function SummaryPage() {
  const q = useSummary()
  const nav = useNavigate()
  const { digName } = useDigContext()
  const setTaskCentreOpen = useUi((s) => s.setTaskCentreOpen)
  const { run } = useAction()
  const lastUndoable = useLastUndoable((s) => s.lastActivityId)
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  if (!q.data) return <div className="p-8 text-sm text-clay">Reading the summary…</div>
  const s = q.data
  const c = summaryCopy(s)
  const Count = ({ n, to, label, strong }: { n: number | bigint; to: string; label: string; strong?: boolean }) => (
    <Link to={to} className={`hover:underline ${strong ? 'font-semibold text-ink' : 'text-ink-2'}`} aria-label={`${label} (${fmtInt(n)})`}>
      {label}
    </Link>
  )
  return (
    <div className="flex-1 overflow-auto">
      <div className="mx-auto max-w-2xl px-8 py-10">
        <div className="flex items-center gap-2 text-clay">
          <Sunrise className="size-4" />
          <span className="text-xs uppercase tracking-wide">{digName(null)}</span>
        </div>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight">{c.headline}</h1>
        <p className="mt-2 text-base text-ink-2">
          {c.status}{' '}
          {!s.caughtUp ? (
            <button type="button" className="text-ochre hover:underline" onClick={() => setTaskCentreOpen(true)}>
              Task Centre
            </button>
          ) : null}
        </p>
        {s.degraded.length ? (
          <div className="mt-3 flex flex-wrap items-center gap-2 text-sm">
            <Chip tone="bad">Degraded</Chip>
            <span className="text-ink-2">{s.degraded.join('; ')}. Nothing is hidden: undecided mail stays Unresolved.</span>
          </div>
        ) : null}

        {c.attentionZero ? (
          <section className="mt-8 rounded-lg border border-moss/40 bg-moss/10 p-5">
            <p className="text-sm text-ink-2">{c.evidence}</p>
            <div className="mt-3 flex gap-2">
              <Button onClick={() => nav('/spoil')}>Review Spoil</Button>
              <Button variant="ghost" disabledReason={lastUndoable ? null : 'Nothing to undo yet'} onClick={() => lastUndoable && run('undo_activity', { activityId: lastUndoable }, { successToast: 'Undone' })}>
                Undo
              </Button>
            </div>
          </section>
        ) : (
          <section className="mt-8 space-y-2 text-base">
            <p className="text-ink-2">{c.arrived}</p>
            <p>
              <Count n={s.findsOpen} to="/finds" label={c.finds} strong />
            </p>
            <p>
              <Count n={s.unresolved} to="/unresolved" label={c.unresolved} strong={num(s.unresolved) > 0} />
            </p>
            <p className="text-ink-2">
              <Link to="/inbox?disposition=filed" className="hover:underline">
                {fmtInt(s.filed)} were Filed into your Strata.
              </Link>
            </p>
            <p className="text-ink-2">
              <Link to="/spoil" className="hover:underline">
                {fmtInt(s.spoil)} went to Spoil.
              </Link>
            </p>
            <p className="text-ink-2">
              <Link to="/contamination" className="hover:underline">
                {fmtInt(s.contamination)} were Contamination.
              </Link>
            </p>
            {num(s.projectionPending) > 0 ? <p className="text-xs text-clay">{fmtInt(s.projectionPending)} decisions are not yet written to Gmail (Projection).</p> : null}
          </section>
        )}

        {s.directorNote ? (
          <section className="mt-8 rounded-lg border border-line bg-sand p-4">
            <div className="text-xs font-semibold uppercase tracking-wide text-clay">Director</div>
            <p className="mt-1 text-sm">{s.directorNote}</p>
            <Button className="mt-3" size="sm" variant="ghost" onClick={() => nav('/director')}>
              Open the Director
            </Button>
          </section>
        ) : null}

        {s.shadow ? (
          <section className="mt-6 rounded-lg border border-ochre/40 bg-ochre/10 p-4">
            <div className="text-sm font-semibold">Shadow Period — day {s.shadow.day}</div>
            <p className="mt-1 text-sm text-ink-2">
              Riddle would have moved {fmtInt(s.shadow.wouldSpoil)} messages to Spoil and surfaced {fmtInt(s.shadow.wouldFind)} Finds. You disagreed with {fmtInt(s.shadow.disagreements)} of its choices. The inbox is untouched until a Mesh has earned the right to act.
            </p>
            <Button className="mt-3" size="sm" onClick={() => nav('/inbox?shadow=1')}>
              Review Proposals
            </Button>
          </section>
        ) : null}

        <p className="mt-10 text-xs text-clay">
          Last synced {fmtRelative(s.lastSyncedAt)}. Riddle processes mail while this machine is awake; nothing runs while it sleeps.
        </p>
      </div>
    </div>
  )
}

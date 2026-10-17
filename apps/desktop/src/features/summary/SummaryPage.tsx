/**
 * The home card (README "The Core Experience"): honest about whether
 * Riddle has finished, one brief with counts as links, what Riddle noticed,
 * a Start review button, the Shadow Period card, and the Attention Zero
 * state with its evidence.
 *
 * It follows the account switcher, like the pages do. Before the first
 * account it is an invitation (connect Gmail, or try sample mail) rather
 * than "Nothing needs your attention", which a UX study found asserted on
 * an empty install. Below the brief sits the trust card (`TrustCard.tsx`):
 * the evidence for letting Riddle act, and the offer to, when it has earned
 * it.
 */
import { useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { FlaskConical, Mail, RefreshCw, Sunrise } from 'lucide-react'
import type { Summary } from '@/bindings/Summary'
import type { TaskCentre } from '@/bindings/TaskCentre'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { ProgressBar } from '@/components/ui/Progress'
import { Sep } from '@/components/Bits'
import { ErrorCard } from '@/components/ErrorCard'
import { PRIVACY_OPTIONS, detailOf, labelOf } from '@/features/settings/accounts/model'
import { fmtDuration, fmtInt, fmtRelative, num } from '@/lib/format'
import { toRiddleError } from '@/lib/ipc'
import { useSummary, useDigContext, useSettings, useTaskCentre } from '@/lib/queries'
import { catchUpJob, catchUpLine, jobKindLabel, jobProgressPct, summaryCopy } from '@/lib/selectors'
import { useAction, useLastUndoable } from '@/lib/useAction'
import { useUi } from '@/lib/stores'
import { TrustCard } from './TrustCard'

/** The sample mailbox the first-run card creates: the wizard's "Test mailbox" defaults; privacy and projection come from Settings as the wizard's do. */
const SAMPLE_MAILBOX = { name: 'Sample mail', email: 'sample@riddle.example', count: 120, group: null }

export default function SummaryPage() {
  const q = useSummary()
  const tc = useTaskCentre()
  const nav = useNavigate()
  const { digId, digName } = useDigContext()
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
          <span className="text-xs uppercase tracking-wide">{digId ? digName(digId) : 'All accounts'}</span>
        </div>
        <h1 className="mt-2 text-3xl font-semibold tracking-tight">{c.headline}</h1>
        <p className="mt-2 text-base text-ink-2">
          {c.status}{' '}
          {!s.caughtUp && !c.firstRun ? (
            <button type="button" className="text-ochre hover:underline" onClick={() => setTaskCentreOpen(true)}>
              Task Centre
            </button>
          ) : null}
        </p>
        {!s.caughtUp && !c.firstRun ? <CatchUp s={s} tc={tc.data} digId={digId} /> : null}
        {s.degraded.length ? (
          <div className="mt-3 flex flex-wrap items-center gap-2 text-sm">
            <Chip tone="bad">Degraded</Chip>
            <span className="text-ink-2">{s.degraded.join('; ')}. Nothing is hidden: undecided mail stays Unresolved.</span>
          </div>
        ) : null}

        {c.firstRun ? (
          <FirstRunCards />
        ) : c.attentionZero ? (
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
          <section className="mt-8 space-y-3 text-base">
            <p className="text-ink-2">{c.arrived}</p>
            <p className="flex flex-wrap items-center gap-x-2 gap-y-1">
              <span className="text-ink-2">{c.since}:</span>
              <Count n={s.findsOpen} to="/finds" label={`${fmtInt(s.findsOpen)} need you`} strong />
              <Sep />
              <Count n={s.filed} to="/inbox?disposition=filed" label={`${fmtInt(s.filed)} filed`} />
              <Sep />
              <Count n={s.spoil} to="/spoil" label={`${fmtInt(s.spoil)} to Spoil`} />
              <Sep />
              <Count n={s.unresolved} to="/unresolved" label={`${fmtInt(s.unresolved)} Unresolved`} strong={num(s.unresolved) > 0} />
              {num(s.contamination) > 0 ? (
                <>
                  <Sep />
                  <Count n={s.contamination} to="/contamination" label={`${fmtInt(s.contamination)} Contamination`} />
                </>
              ) : null}
            </p>
            {c.reviewRoute ? (
              <div className="pt-1">
                <Button variant="primary" onClick={() => nav(c.reviewRoute!)}>
                  Start review
                </Button>
                <span className="ml-3 text-xs text-clay">{c.reviewRoute === '/finds' ? c.finds : c.unresolved}</span>
              </div>
            ) : null}
            {num(s.projectionPending) > 0 ? (
              <p className="text-xs text-clay">
                <button type="button" className="hover:underline" onClick={() => setTaskCentreOpen(true)}>
                  {fmtInt(s.projectionPending)} changes waiting to be written to Gmail
                </button>
              </p>
            ) : null}
          </section>
        )}

        {!c.firstRun ? <TrustCard /> : null}

        {s.directorNote ? (
          <section className="mt-8 rounded-lg border border-line bg-sand p-4">
            <div className="text-xs font-semibold uppercase tracking-wide text-clay">Riddle noticed</div>
            <p className="mt-1 text-sm selectable">{s.directorNote}</p>
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
              Review what Riddle would have done
            </Button>
          </section>
        ) : null}

        {!c.firstRun ? (
          <p className="mt-10 flex flex-wrap items-center gap-2 text-xs text-clay">
            <span>Last synced {fmtRelative(s.lastSyncedAt)}. Riddle processes mail while this machine is awake; nothing runs while it sleeps.</span>
            <SyncNowButton />
          </p>
        ) : null}
      </div>
    </div>
  )
}

/**
 * What Riddle is doing while it catches up: the sync job's own progress,
 * hoisted from the Task Centre so the page does not send the user there
 * for it, and one line of what has come of the work so far. Without a
 * job in the Task Centre the Summary's own sift counts drive the bar.
 */
function CatchUp({ s, tc, digId }: { s: Summary; tc: TaskCentre | undefined; digId: string | null }) {
  const job = catchUpJob(tc, digId)
  const queued = job?.status.state === 'queued'
  const total = job ? (job.progress.total === null ? null : num(job.progress.total)) : num(s.totalToSift) || null
  const done = job ? num(job.progress.done) : Math.max(0, num(s.totalToSift) - num(s.remainingToSift))
  const pct = job ? jobProgressPct(job) : total ? Math.min(100, Math.round((done / total) * 100)) : null
  const eta = job?.status.state === 'running' && job.progress.etaSeconds !== null ? `~${fmtDuration(job.progress.etaSeconds)} left` : null
  const count = job ? `${fmtInt(done)}${total !== null ? ` of ${fmtInt(total)}` : ''} ${job.progress.unit}` : total ? `${fmtInt(done)} of ${fmtInt(total)} messages` : ''
  return (
    <section aria-label="Catching up" className="mt-4 rounded-md border border-ochre/40 bg-ochre/10 p-3 text-xs">
      <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
        <span className="font-medium">{job ? `${jobKindLabel(job.kind)}: ${job.progress.step}` : 'Sifting what has arrived'}</span>
        <span className="text-ink-2">
          {count}
          {pct !== null && !queued ? ` (${pct}%)` : ''}
          {eta ? ` · ${eta}` : ''}
        </span>
      </div>
      <div className="mt-1.5">
        <ProgressBar done={done} total={queued ? null : total} label={job?.label ?? 'Catching up'} />
      </div>
      <p className="mt-1.5 text-ink-2">{catchUpLine(s, tc, digId)}</p>
    </section>
  )
}

/**
 * The two ways to start. Connect Gmail opens the Add account wizard through
 * the Settings deep link; sample mail creates the wizard's test mailbox
 * with its defaults, so the app fills without a real account. The privacy
 * line below says what a new account actually starts in (the default under
 * Settings → Accounts → Defaults); it used to promise that nothing left
 * the machine while the default was Cloud judgment.
 */
function FirstRunCards() {
  const nav = useNavigate()
  const { run } = useAction()
  const settings = useSettings().data
  const privacy = settings?.privacyDefault
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const trySample = async () => {
    setBusy(true)
    setError(null)
    try {
      await run('add_fixture_dig', { ...SAMPLE_MAILBOX, privacy: settings?.privacyDefault ?? 'local_only', projection: settings?.projectionDefault ?? 'none' }, { successToast: 'Sample mail is loading' })
    } catch (e) {
      setError(toRiddleError(e).message)
    } finally {
      setBusy(false)
    }
  }
  return (
    <section className="mt-8">
      <div className="grid grid-cols-2 gap-3">
        <button type="button" onClick={() => nav('/settings#add-account')} className="flex flex-col items-start gap-2 rounded-lg border border-line p-4 text-left hover:bg-sand focus-visible:outline focus-visible:outline-2 focus-visible:outline-ochre">
          <Mail className="size-5 text-ochre" />
          <span className="font-medium">Connect Gmail</span>
          <span className="text-xs text-ink-2">Sign in with Google in your browser. Riddle starts by observing: it changes nothing in the mailbox until you say so.</span>
        </button>
        <button type="button" onClick={trySample} disabled={busy} aria-disabled={busy || undefined} className="flex flex-col items-start gap-2 rounded-lg border border-line p-4 text-left hover:bg-sand focus-visible:outline focus-visible:outline-2 focus-visible:outline-ochre disabled:cursor-wait disabled:opacity-60">
          <FlaskConical className="size-5 text-ochre" />
          <span className="font-medium">{busy ? 'Creating the sample mailbox…' : 'Try with sample mail'}</span>
          <span className="text-xs text-ink-2">A generated mailbox of {SAMPLE_MAILBOX.count} messages, no network, nothing real. See Finds, Spoil and the Director at work, then connect Gmail when you are ready.</span>
        </button>
      </div>
      {error ? (
        <p role="alert" className="mt-2 text-xs text-rust">
          {error}
        </p>
      ) : null}
      <p className="mt-4 text-xs text-clay">
        {privacy ? `Either way, a new account starts in ${labelOf(PRIVACY_OPTIONS, privacy)}: ${detailOf(PRIVACY_OPTIONS, privacy)}` : 'Either way, a new account starts in the default privacy mode.'} Change the default under{' '}
        <Link to="/settings#new-digs" className="underline hover:text-ink">
          Settings → Accounts → Defaults
        </Link>
        , or the account's own setting afterwards.
      </p>
    </section>
  )
}

/** Every account, now; the Task Centre shows each one moving through sync, Sift and projection. */
function SyncNowButton() {
  const { run } = useAction()
  return (
    <Button size="sm" variant="ghost" icon={<RefreshCw />} title="Fetch new mail for every account now" onClick={() => run('refresh_now', {}, { successToast: 'Syncing all accounts' })}>
      Sync now
    </Button>
  )
}

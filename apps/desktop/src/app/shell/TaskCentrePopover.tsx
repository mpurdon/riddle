/**
 * The Task Centre (§55): a status indicator, per-account status, the job
 * list with honest progress, ETA, pause and cancel, and an onboarding view
 * for the multi-hour initial sync. With no account it says so, rather than
 * "Caught up", which a first run read as a result.
 */
import * as Popover from '@radix-ui/react-popover'
import { Link } from 'react-router-dom'
import { Activity as ActivityIcon, Pause, Play, RefreshCw, X } from 'lucide-react'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { ProgressBar } from '@/components/ui/Progress'
import { Tip } from '@/components/ui/Tooltip'
import { fmtDuration, fmtInt, fmtRelative, num } from '@/lib/format'
import { cx } from '@/lib/format'
import { digPipeline, digStatusChip, isJobActive, jobKindLabel, jobPriorityLabel, jobProgressPct, taskCentreIndicator, type StageState } from '@/lib/selectors'
import { useTaskCentre } from '@/lib/queries'
import { useUi } from '@/lib/stores'
import { useAction } from '@/lib/useAction'
import type { Job } from '@/bindings/Job'

const dot: Record<string, string> = { ok: 'bg-moss', busy: 'bg-slate', bad: 'bg-rust', neutral: 'bg-clay', warn: 'bg-ochre' }

export function TaskCentrePopover() {
  const tc = useTaskCentre()
  const open = useUi((s) => s.taskCentreOpen)
  const setOpen = useUi((s) => s.setTaskCentreOpen)
  const ind = taskCentreIndicator(tc.data)
  const noAccounts = !!tc.data && tc.data.digs.length === 0
  const jobs = tc.data?.jobs ?? []
  const active = jobs.filter(isJobActive)
  const finished = jobs.filter((j) => !isJobActive(j)).slice(0, 5)
  const onboarding = jobs.find((j) => j.kind === 'initial_sync' && j.status.state === 'running')
  const { run } = useAction()
  const digs = tc.data?.digs ?? []
  const allBusy = digs.length > 0 && digs.every((d) => digPipeline(d.status, jobFor(d.digId)).busy)
  /** The sync job running or queued for this account, whose step is the strip's detail. */
  function jobFor(digId: string): Job | null {
    return active.find((j) => j.digId === digId && (j.kind === 'incremental_sync' || j.kind === 'catch_up' || j.kind === 'initial_sync' || j.kind === 'sift')) ?? null
  }
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button type="button" className="inline-flex h-7 items-center gap-1.5 rounded-md border border-line bg-sand px-2 text-sm hover:bg-sand-2" aria-label={`Task Centre: ${ind.label}, ${ind.detail}`}>
          <span className={`size-2 rounded-full ${dot[ind.tone]}`} aria-hidden />
          <span>{ind.label}</span>
          {active.length ? <span className="text-xs text-clay">{active.length}</span> : null}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content align="end" sideOffset={6} className="z-40 w-[440px] rounded-lg border border-line bg-paper p-3 text-sm shadow-xl fade-in">
          <div className="flex items-center justify-between">
            <div>
              <div className="font-semibold">{ind.label}</div>
              <div className="text-xs text-ink-2">{ind.detail}</div>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-xs text-clay">{tc.data ? `as of ${fmtRelative(tc.data.asOf)}` : 'Connecting…'}</span>
              {!noAccounts ? (
                <Button size="sm" variant="ghost" icon={<RefreshCw />} disabledReason={allBusy ? 'Every account is already syncing' : null} title="Fetch new mail for every account now, then sift and write back as usual" onClick={() => run('refresh_now', {}, { successToast: 'Syncing all accounts' })}>
                  Sync all
                </Button>
              ) : null}
            </div>
          </div>
          {tc.data?.storeFault ? <div className="mt-2 rounded border border-rust/40 bg-rust/10 px-2 py-1 text-xs text-rust">Store fault: {tc.data.storeFault}. Riddle has stopped writing until it clears.</div> : null}
          {noAccounts ? (
            <p className="mt-3 text-xs text-ink-2">
              No account is connected, so there is nothing to sync.{' '}
              <Link to="/settings#add-account" className="text-ochre hover:underline" onClick={() => setOpen(false)}>
                Add an account in Settings
              </Link>
              .
            </p>
          ) : null}
          <ul className="mt-3 flex flex-col gap-2">
            {(tc.data?.digs ?? []).map((d) => {
              const c = digStatusChip(d.status)
              const p = digPipeline(d.status, jobFor(d.digId))
              return (
                <li key={d.digId} className="rounded-md border border-line/70 px-2 py-1.5" aria-label={`${d.name}: ${p.detail}`}>
                  <div className="flex items-center justify-between gap-2">
                    <span className="truncate font-medium">{d.name}</span>
                    <div className="flex shrink-0 items-center gap-1.5">
                      {d.status.state === 'not_connected' || (d.status.state === 'sync_failing' && d.status.failure === 'sign_in') ? (
                        <Link to="/settings#digs" onClick={() => setOpen(false)}>
                          <Chip tone={c.tone}>{c.label} · Sign in again</Chip>
                        </Link>
                      ) : d.status.state === 'degraded' ? (
                        <Tip text={c.title}>
                          <span>
                            <Chip tone={c.tone}>{c.label}</Chip>
                          </span>
                        </Tip>
                      ) : (
                        <PipelineStrip stages={p.stages} />
                      )}
                      <Button size="sm" variant="ghost" icon={<RefreshCw />} disabledReason={d.status.state === 'not_connected' ? 'Sign in first' : p.busy ? 'Already syncing' : null} title={`Fetch new mail for ${d.name} now`} aria-label={`Sync ${d.name}`} onClick={() => run('refresh_now', { digId: d.digId }, { successToast: `Syncing ${d.name}` })}>
                        Sync
                      </Button>
                    </div>
                  </div>
                  <div className="mt-0.5 truncate text-xs text-ink-2">{p.detail}</div>
                </li>
              )
            })}
          </ul>
          {onboarding ? (
            <div className="mt-3 rounded-md border border-ochre/40 bg-ochre/10 p-2">
              <div className="text-xs font-medium">First sync in progress</div>
              <div className="text-xs text-ink-2">{onboarding.label}: {onboarding.progress.step}. Riddle only works while the machine is awake; Attention Zero cannot be promised for mail it has not seen yet.</div>
              <div className="mt-1">
                <ProgressBar done={num(onboarding.progress.done)} total={onboarding.progress.total === null ? null : num(onboarding.progress.total)} label={onboarding.label} />
              </div>
            </div>
          ) : null}
          <div className="mt-3 text-xs font-semibold uppercase tracking-wide text-clay">Background work</div>
          {active.length === 0 ? <div className="mt-1 text-xs text-ink-2">{noAccounts ? 'Nothing running.' : 'Nothing running. Riddle polls Gmail every 30 to 60 seconds while awake.'}</div> : null}
          <ul className="mt-1 flex flex-col gap-2">
            {active.map((j) => (
              <JobRow key={j.id} job={j} />
            ))}
          </ul>
          {finished.length ? (
            <details className="mt-2">
              <summary className="cursor-default text-xs text-clay">Recently finished ({finished.length})</summary>
              <ul className="mt-1 flex flex-col gap-1 text-xs text-ink-2">
                {finished.map((j) => (
                  <li key={j.id} className="flex items-center justify-between gap-2">
                    <span className="truncate">{j.label}</span>
                    <span className="shrink-0">{j.status.state === 'failed' ? <Chip tone="bad">failed</Chip> : j.status.state} {j.finishedAt ? fmtRelative(j.finishedAt) : ''}</span>
                  </li>
                ))}
              </ul>
            </details>
          ) : null}
          <Popover.Arrow className="fill-line" />
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  )
}

const STAGE_CLASS: Record<StageState, string> = {
  done: 'text-moss',
  active: 'rounded bg-ochre/15 px-1 font-medium text-ink',
  todo: 'text-clay',
  off: 'text-clay/60 line-through',
}

/** Sync → Sift → Project, with the stage the account is in lit. */
function PipelineStrip({ stages }: { stages: { key: string; label: string; state: StageState }[] }) {
  return (
    <span className="inline-flex items-center gap-1 text-2xs" aria-hidden>
      {stages.map((st, i) => (
        <span key={st.key} className="inline-flex items-center gap-1">
          {i > 0 ? <span className="text-clay">›</span> : null}
          <span className={cx(STAGE_CLASS[st.state])}>{st.label}</span>
        </span>
      ))}
    </span>
  )
}

function JobRow({ job }: { job: Job }) {
  const { run } = useAction()
  const pct = jobProgressPct(job)
  const total = job.progress.total === null ? null : num(job.progress.total)
  const paused = job.status.state === 'paused'
  const queued = job.status.state === 'queued'
  return (
    <li className="rounded-md border border-line p-2">
      <div className="flex items-center justify-between gap-2">
        <div className="min-w-0">
          <div className="truncate font-medium">{job.label}</div>
          <div className="truncate text-xs text-ink-2">
            {jobKindLabel(job.kind)} · {jobPriorityLabel(job.priority)}
            {queued ? ' · queued' : ''}
            {paused ? ` · paused: ${job.status.state === 'paused' ? job.status.reason : ''}` : ''}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {job.status.state === 'running' ? (
            <Button size="sm" variant="ghost" icon={<Pause />} title="Pause" onClick={() => run('pause_job', { jobId: job.id }, { successToast: false })}>
              Pause
            </Button>
          ) : paused && job.status.state === 'paused' && job.status.reason.includes('by you') ? (
            <Button size="sm" variant="ghost" icon={<Play />} title="Resume" onClick={() => run('resume_job', { jobId: job.id }, { successToast: false })}>
              Resume
            </Button>
          ) : null}
          <Button size="sm" variant="ghost" icon={<X />} title="Cancel" disabledReason={job.cancellable ? null : 'This job cannot be cancelled mid-step; it stops at its next checkpoint'} onClick={() => run('cancel_job', { jobId: job.id }, { successToast: false })}>
            Cancel
          </Button>
        </div>
      </div>
      <div className="mt-1.5">
        <ProgressBar done={num(job.progress.done)} total={queued ? null : total} label={job.label} />
      </div>
      <div className="mt-1 flex items-center justify-between text-xs text-ink-2">
        <span>
          {queued ? job.progress.step : `${job.progress.step} · ${fmtInt(job.progress.done)}${total !== null ? ` of ${fmtInt(total)}` : ''} ${job.progress.unit}`}
          {pct !== null && !queued ? ` (${pct}%)` : ''}
        </span>
        <span>{job.progress.etaSeconds !== null && job.status.state === 'running' ? `~${fmtDuration(job.progress.etaSeconds)} left` : ''}</span>
      </div>
    </li>
  )
}

export function TaskCentreIcon() {
  return <ActivityIcon className="size-4" />
}

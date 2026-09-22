/**
 * The Task Centre (§55): a status indicator, per-Dig status, the job list
 * with honest progress, ETA, pause and cancel, and an onboarding view for
 * the multi-hour initial sync.
 */
import * as Popover from '@radix-ui/react-popover'
import { Activity as ActivityIcon, Pause, Play, X } from 'lucide-react'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { ProgressBar } from '@/components/ui/Progress'
import { Tip } from '@/components/ui/Tooltip'
import { fmtDuration, fmtInt, fmtRelative, num } from '@/lib/format'
import { digStatusChip, isJobActive, jobKindLabel, jobProgressPct, taskCentreIndicator } from '@/lib/selectors'
import { useTaskCentre } from '@/lib/queries'
import { useUi } from '@/lib/stores'
import { useAction } from '@/lib/useAction'
import type { Job } from '@/bindings/Job'

const dot: Record<string, string> = { ok: 'bg-moss', busy: 'bg-ochre', bad: 'bg-rust', neutral: 'bg-clay', warn: 'bg-ochre' }

export function TaskCentrePopover() {
  const tc = useTaskCentre()
  const open = useUi((s) => s.taskCentreOpen)
  const setOpen = useUi((s) => s.setTaskCentreOpen)
  const ind = taskCentreIndicator(tc.data)
  const jobs = tc.data?.jobs ?? []
  const active = jobs.filter(isJobActive)
  const finished = jobs.filter((j) => !isJobActive(j)).slice(0, 5)
  const onboarding = jobs.find((j) => j.kind === 'initial_sync' && j.status.state === 'running')
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
            <div className="text-xs text-clay">{tc.data ? `as of ${fmtRelative(tc.data.asOf)}` : 'Connecting…'}</div>
          </div>
          {tc.data?.storeFault ? <div className="mt-2 rounded border border-rust/40 bg-rust/10 px-2 py-1 text-xs text-rust">Store fault: {tc.data.storeFault}. Riddle has stopped writing until it clears.</div> : null}
          <ul className="mt-3 flex flex-col gap-1">
            {(tc.data?.digs ?? []).map((d) => {
              const c = digStatusChip(d.status)
              return (
                <li key={d.digId} className="flex items-center justify-between">
                  <span>{d.name}</span>
                  <Tip text={c.title}>
                    <span>
                      <Chip tone={c.tone}>{c.label}</Chip>
                    </span>
                  </Tip>
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
          {active.length === 0 ? <div className="mt-1 text-xs text-ink-2">Nothing running. Riddle polls Gmail every 30 to 60 seconds while awake.</div> : null}
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
            {jobKindLabel(job.kind)} · {job.priority}
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

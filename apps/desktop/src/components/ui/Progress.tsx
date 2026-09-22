import { fmtInt } from '@/lib/format'

/** Honest progress: the bar only moves on real progress events; unknown totals are striped, not animated. */
export function ProgressBar({ done, total, label }: { done: number; total: number | null; label?: string }) {
  const pct = total && total > 0 ? Math.min(100, Math.round((done / total) * 100)) : null
  return (
    <div className="progress-track" role="progressbar" aria-label={label} aria-valuenow={pct ?? undefined} aria-valuemin={0} aria-valuemax={100} aria-valuetext={pct === null ? `${fmtInt(done)} so far` : `${pct}%`}>
      {pct === null ? <div className="progress-indeterminate" /> : <div className="progress-fill" style={{ width: `${pct}%` }} />}
    </div>
  )
}

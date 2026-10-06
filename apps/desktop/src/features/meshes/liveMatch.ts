/**
 * The one-line answer to "what would this text do?", shown beside the
 * Valid line after the editor loses focus. It says only what the
 * SimulationReport totals support: the report counts messages in the
 * window and how many would change; it does not say which route matched
 * or how many each route would take, so this line never claims that. A
 * message whose Signals are not cached is counted as unchanged by the
 * simulator, and the line says so rather than letting "none would change"
 * read as a verdict.
 */
import type { SimulationReport } from '@/bindings/SimulationReport'
import { fmtInt, num, plural } from '@/lib/format'

export function liveMatchSentence(r: SimulationReport): string {
  const total = num(r.historicalMessages)
  const changed = num(r.changed)
  const judge = num(r.newJudgeCallsRequired)
  const window = `the last ${r.windowDays} days`
  if (total === 0) return `Over ${window} there were no messages in this scope to check.`
  const head = changed === 0 ? `Over ${window}, none of the ${plural(total, 'message')} in scope would change.` : `Over ${window}, ${fmtInt(changed)} of the ${plural(total, 'message')} in scope would change.`
  if (judge === 0) return head
  return `${head} ${plural(judge, 'message')} need${judge === 1 ? 's' : ''} a fresh judge call first and count${judge === 1 ? 's' : ''} as unchanged.`
}

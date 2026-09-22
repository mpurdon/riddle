/**
 * A typed error rendered as the affordance ARCHITECTURE.md §8 asks for:
 * sign in, an explanation, a Degraded chip, a Task Centre link, the reason
 * on the control, or a reconcile prompt.
 */
import { useNavigate } from 'react-router-dom'
import { toRiddleError } from '@/lib/ipc'
import { errorAffordance } from '@/lib/selectors'
import { useUi } from '@/lib/stores'
import { Button } from './ui/Button'
import { Note } from './ui/Note'
import { Chip } from './ui/Chip'

export function ErrorCard({ error, onRetry, compact }: { error: unknown; onRetry?: () => void; compact?: boolean }) {
  const nav = useNavigate()
  const err = toRiddleError(error)
  const a = errorAffordance(err)
  const setTaskCentreOpen = useUi((s) => s.setTaskCentreOpen)
  const action = (() => {
    switch (a.action) {
      case 'sign_in':
        return <Button variant="primary" onClick={() => nav('/settings#digs')}>{a.actionLabel}</Button>
      case 'task_centre':
        return <Button onClick={() => setTaskCentreOpen(true)}>{a.actionLabel}</Button>
      case 'arm_dev':
        return <Button onClick={() => nav('/developer')}>{a.actionLabel}</Button>
      case 'reconcile':
        return <Button variant="primary" onClick={onRetry}>{a.actionLabel}</Button>
      case 'retry':
        return onRetry ? <Button onClick={onRetry}>{a.actionLabel}</Button> : null
      default:
        return null
    }
  })()
  if (compact) {
    return (
      <div className="flex items-center gap-2 text-sm text-ink-2">
        {a.degraded ? <Chip tone="bad">Degraded · nothing hidden</Chip> : null}
        <span>{a.title}</span>
        {action}
      </div>
    )
  }
  return (
    <Note tone={a.degraded ? 'bad' : err.kind === 'provider_admin_blocked' ? 'warn' : 'bad'} title={a.title} actions={action} className="m-4">
      {a.body}
      {a.degraded ? (
        <div className="mt-1">
          <Chip tone="bad">Degraded</Chip> <span className="text-xs">Nothing is hidden while a judge or LLM is unavailable: undecided mail stays Unresolved.</span>
        </div>
      ) : null}
    </Note>
  )
}

/**
 * What a dev build may write to the connected mailbox: Hold, Ask or Allow
 * (`ProviderWrites`), and the changes waiting on it. It replaced a single
 * Arm switch: unarmed, a batch of seven corrections was refused seven
 * times, though corrections only queue their mailbox change. Now they
 * always count in Riddle; Hold keeps their changes waiting, Ask sends them
 * with one confirmation for the batch, Allow writes them as they happen.
 * Sending mail and deleting ask first under Ask; Allow needs press and hold.
 */
import { useQuery } from '@tanstack/react-query'
import type { ProviderWrites } from '@/bindings/ProviderWrites'
import { mailboxWrites, qk } from '@/lib/api'
import { cx, plural } from '@/lib/format'
import { useDevInfo } from '@/lib/queries'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { HoldButton } from '@/components/confirm/HoldButton'

export const WRITES_LABEL: Record<ProviderWrites, string> = { hold: 'Hold', ask: 'Ask', allow: 'Allow' }

const WRITES_LINE: Record<ProviderWrites, string> = {
  hold: 'Nothing is written to the mailbox. Corrections still count in Riddle; their mailbox changes wait here.',
  ask: 'Waiting changes go when you send them, as one batch. Sending mail, deleting and unsubscribing ask first.',
  allow: 'Mailbox changes are written as they happen, as in a release build.',
}

/** The state, polled while a dev build runs; nothing in a release build. */
export function useMailboxWrites() {
  const dev = useDevInfo().data?.dev ?? false
  return useQuery({ queryKey: qk.mailboxWrites, queryFn: mailboxWrites, enabled: dev, refetchInterval: 5000 })
}

/** "7 mailbox changes waiting · Send · Discard": one confirmation for the batch. */
export function HeldChanges({ held, compact }: { held: number; compact?: boolean }) {
  const { run } = useAction()
  if (!held) return null
  return (
    <span className={cx('flex items-center gap-2', compact ? 'text-sm' : 'text-xs')}>
      <span>
        <span className="font-medium">{plural(held, 'mailbox change')}</span> waiting
      </span>
      <Button size="sm" variant="primary" onClick={() => void run('send_held_changes', {}, { invalidate: [qk.mailboxWrites] })} title="Write them to the mailbox now; asks once for the whole batch">
        Send
      </Button>
      <Button size="sm" variant="ghost" onClick={() => void run('discard_held_changes', {}, { invalidate: [qk.mailboxWrites] })} title="Drop them; Riddle keeps its decisions and the mailbox stays as it is">
        Discard
      </Button>
    </span>
  )
}

export function MailboxWritesControl() {
  const q = useMailboxWrites()
  const { run } = useAction()
  const state = q.data
  if (!state) return <p className="text-xs text-clay">Loading…</p>
  const set = (writes: ProviderWrites) => run('set_mailbox_writes', { writes }, { confirmed: writes !== 'allow' ? undefined : true, invalidate: [qk.mailboxWrites], successToast: `Mailbox writes: ${WRITES_LABEL[writes]}` })
  return (
    <div className="space-y-2 text-xs">
      <div role="radiogroup" aria-label="Mailbox writes" className="flex items-center gap-1.5">
        {(['hold', 'ask'] as const).map((w) => (
          <button
            key={w}
            type="button"
            role="radio"
            aria-checked={state.writes === w}
            onClick={() => void set(w)}
            className={cx('rounded border px-2.5 py-0.5 leading-5', state.writes === w ? 'border-ochre bg-ochre/15 font-medium text-ink' : 'border-line bg-sand text-ink-2 hover:bg-sand-2')}
          >
            {WRITES_LABEL[w]}
          </button>
        ))}
        {state.writes === 'allow' ? (
          <span role="radio" aria-checked className="rounded border border-rust/50 bg-rust/10 px-2.5 py-0.5 font-medium leading-5 text-rust">
            Allow
          </span>
        ) : (
          <HoldButton label="Allow" coaching="Hold for 1.5 seconds. With Gmail, real labels are written as they happen; with a fixture account nothing leaves the machine." onComplete={() => void set('allow')} />
        )}
      </div>
      <p className="text-ink-2">{WRITES_LINE[state.writes]}</p>
      <HeldChanges held={Number(state.held)} />
    </div>
  )
}

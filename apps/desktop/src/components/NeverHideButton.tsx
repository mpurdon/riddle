/**
 * "Never hide" that says when it has been done and can be undone. It used to
 * look the same before and after, so a sender already on Never Hidden still
 * offered "Never hide", and taking one off meant finding Settings. Now the
 * button shows the sender's standing: add while they can be hidden, and once
 * they are protected, "Never hidden" with a way to allow hiding again, which
 * removes the entry that covers them (their address or their domain).
 */
import type { ReactNode } from 'react'
import { ShieldCheck, ShieldOff } from 'lucide-react'
import { Button } from '@/components/ui/Button'
import { useAction } from '@/lib/useAction'
import { useNeverHiddenEntry } from '@/lib/neverHidden'

export function NeverHideButton({
  email,
  onAdd,
  children = 'Never hide',
  title,
  disabledReason = null,
  variant,
}: {
  /** The sender this button speaks for; null when the target is several senders. */
  email: string | null
  onAdd: () => void
  children?: ReactNode
  title?: string
  disabledReason?: string | null
  variant?: 'ghost'
}) {
  const entry = useNeverHiddenEntry(email)
  const { run } = useAction()
  if (entry) {
    const how = entry.startsWith('@') ? `everything from ${entry}` : entry
    return (
      <Button
        size="sm"
        variant={variant}
        icon={<ShieldCheck />}
        disabledReason={disabledReason}
        onClick={() => void run('remove_never_hidden', { entry })}
        title={`Riddle never hides ${how}. Click to allow hiding again.`}
      >
        Never hidden · Allow hiding
      </Button>
    )
  }
  return (
    <Button size="sm" variant={variant} icon={<ShieldOff />} disabledReason={disabledReason} onClick={onAdd} title={title}>
      {children}
    </Button>
  )
}

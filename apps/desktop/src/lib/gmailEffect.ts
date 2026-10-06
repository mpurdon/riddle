/**
 * What one of your own moves does to the mailbox, in three words, for the
 * toast: "Moved to Spoil · archived in Gmail". Until this, the toast for a
 * move read the same at Observe with Gmail changes off as at Act with
 * archiving on, and the only place that said what a click wrote to Gmail
 * was the "?" panel. The rules follow `actionHelp.ts` and the backend: your
 * own moves write through the account's "Changes in Gmail" setting whatever
 * the autonomy level, and an unarmed dev build holds them.
 */
import type { Dig } from '@/bindings/Dig'
import type { Disposition } from '@/bindings/Disposition'

export interface EffectContext {
  dig: Dig
  dev?: boolean
  armed?: boolean
}

/** The mailbox as the user knows it: Gmail, or the test mailbox of a fixture account. */
export function mailboxName(provider: Dig['provider'], opts: { capital?: boolean } = {}): string {
  const name = provider === 'fixture' ? 'the test mailbox' : 'Gmail'
  return opts.capital && name.startsWith('the') ? `The${name.slice(3)}` : name
}

const MAILBOX_ACTIONS = new Set(['move_to_spoil', 'archive_messages', 'mark_contamination', 'restore_messages', 'set_disposition', 'label_messages', 'permanent_delete', 'snooze_messages'])

export function isMailboxAction(action: string): boolean {
  return MAILBOX_ACTIONS.has(action)
}

/** The suffix for a toast, or null when the action never touches a mailbox. */
export function gmailEffectLine(action: string, args: Record<string, unknown>, ctx: EffectContext): string | null {
  if (!MAILBOX_ACTIONS.has(action)) return null
  const mailbox = mailboxName(ctx.dig.provider)
  if (action === 'snooze_messages') return 'Riddle only'
  if (action === 'permanent_delete') return ctx.dig.provider === 'fixture' ? `deleted from ${mailbox}` : "moved to Gmail's Trash"
  const projection = ctx.dig.projection
  if (projection === 'none') return 'Riddle only'
  // The backend answers `dev_unarmed` only at Act (autonomy is checked first), while the build
  // refuses the user's own moves at every level, so the client's armed flag stays in the OR.
  const held = ctx.dig.mailbox?.held === 'dev_unarmed' || (ctx.dev && !ctx.armed)
  if (held) return 'held: waits for you to send it (dev build)'
  const archives = projection === 'full'
  switch (action) {
    case 'archive_messages':
      return archives ? `archived in ${mailbox}` : 'Riddle only'
    case 'move_to_spoil':
      return archives ? `labelled and archived in ${mailbox}` : `labelled in ${mailbox}`
    case 'mark_contamination':
    case 'label_messages':
      return `labelled in ${mailbox}`
    case 'restore_messages':
      return archives ? `back in the ${mailbox} inbox` : `label removed in ${mailbox}`
    case 'set_disposition': {
      const d = args.disposition as Disposition | undefined
      switch (d?.kind) {
        case 'filed':
          return archives ? `archived in ${mailbox}` : 'Riddle only'
        case 'spoil':
          return archives ? `labelled and archived in ${mailbox}` : `labelled in ${mailbox}`
        case 'contamination':
          return `labelled in ${mailbox}`
        case 'find':
          return archives ? `back in the ${mailbox} inbox` : `label removed in ${mailbox}`
        default:
          return 'Riddle only'
      }
    }
    default:
      return null
  }
}

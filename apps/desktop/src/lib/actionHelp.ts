/**
 * What each action button does, in one table: the "?" panel beside a row of
 * buttons and the help sheet both render from it, so a button's
 * explanation cannot drift between the two (the same rule the shortcut
 * table follows).
 *
 * The Gmail column is computed from the account's settings where they are
 * known. The effects follow the backend: your own moves write to the
 * mailbox whatever the autonomy level, through the account's "Changes in
 * Gmail" setting (none, labels, labels and archiving), and an unarmed dev
 * build refuses them outright.
 */
import type { ProjectionMode } from '@/bindings/ProjectionMode'

export type HelpGroup = 'decide' | 'find' | 'message' | 'review'

export interface HelpContext {
  projection?: ProjectionMode
  /** A development build that has not been armed refuses mailbox changes. */
  devUnarmed?: boolean
}

export interface ActionHelp {
  id: string
  label: string
  keys?: string
  /** What happens inside Riddle. */
  riddle: string
  /** What happens in Gmail, given this account's settings. A dev build's refusal is shown once, above the table. */
  gmail: (ctx: HelpContext) => string
  /** Whether it counts as a correction Riddle learns from. */
  teaches: string
  undo: string
  /** Writes to the mailbox, so an unarmed dev build refuses it. */
  mailbox?: boolean
}

const NOTHING = 'Nothing.'
const unknown = (full: string, labels: string) => `${full} With labels only: ${labels.charAt(0).toLowerCase()}${labels.slice(1)} With Gmail changes off: nothing.`

/** Gmail effect for a projection-driven move, phrased for the account when known. */
function viaProjection(full: string, labels: string) {
  return (ctx: HelpContext) => {
    if (!ctx.projection) return unknown(full, labels)
    if (ctx.projection === 'none') return 'Nothing: this account\'s "Changes in Gmail" is set to None.'
    return ctx.projection === 'full' ? full : labels
  }
}

const CORRECTION = 'Yes, as a correction.'

/** Said once under the table rather than in every row. */
export const CORRECTION_NOTE = 'A correction never changes a rule by itself. Riddle uses it as context for later mail from the same sender or thread, and when corrections against one Mesh keep pointing the same way, the Director can show you the pattern and propose a change.'
const UNDO = 'Yes, from the toast or Activity.'

export const ACTION_HELP: Record<HelpGroup, { title: string; intro: string; items: ActionHelp[] }> = {
  decide: {
    title: 'Deciding Unresolved mail',
    intro: 'Riddle could not decide these. Pick what the message is; your answer is final for this message and teaches Riddle about mail like it.',
    items: [
      { id: 'find', label: 'Find…', keys: 'f', mailbox: true, riddle: 'Decides it needs your attention, as the type you pick (Respond, Pay, Review…), and opens a Find for it in Finds.', gmail: () => NOTHING, teaches: CORRECTION, undo: UNDO },
      { id: 'follow_up', label: 'Follow up by…', keys: 'shift+f', mailbox: true, riddle: 'Decides it is a Follow Up Find due by the date you pick. It waits in Finds and resolves by itself when a reply arrives.', gmail: () => NOTHING, teaches: CORRECTION, undo: UNDO },
      { id: 'filed', label: 'Filed', keys: 'i', mailbox: true, riddle: 'Decides it is fine to keep but needs nothing from you.', gmail: viaProjection('Archives it: out of the Inbox, still in All Mail.', 'Nothing: Filed has no label, and the account does not archive.'), teaches: CORRECTION, undo: UNDO },
      { id: 'spoil', label: 'Spoil', keys: '#', mailbox: true, riddle: 'Decides it is legitimate but low value (marketing, notifications). It moves to Spoil, stays searchable, and nothing is deleted.', gmail: viaProjection('Adds the Riddle/Spoil label and archives it.', 'Adds the Riddle/Spoil label; it stays in the Inbox.'), teaches: CORRECTION, undo: UNDO },
      { id: 'contamination', label: 'Contamination', mailbox: true, riddle: 'Decides it should not be trusted (spam, phishing, scams). It moves to Contamination and is kept out of the Director\'s view unless you open it.', gmail: viaProjection('Adds the Riddle/Contamination label. It is never archived, so you can still see it.', 'Adds the Riddle/Contamination label.'), teaches: CORRECTION, undo: UNDO },
      { id: 'never_hide', label: 'Never hide', keys: 'h', riddle: 'Adds this sender\'s address to Never Hidden: nothing from them can become Spoil or Contamination again, whatever a Mesh says. It does not decide this message; pick one of the others too.', gmail: () => NOTHING, teaches: 'Yes. Recorded as a correction on this message.', undo: 'Yes, from the toast or Activity, or remove the address in Settings → Mail rules.' },
    ],
  },
  find: {
    title: 'Acting on a Find',
    intro: 'A Find is something Riddle thinks needs you. These close it, defer it, or tell Riddle it was wrong. None of them changes anything in Gmail.',
    items: [
      { id: 'resolve', label: 'Resolve', keys: 'e', riddle: 'Marks it dealt with and takes it off your open Finds. Finds waiting on your reply resolve themselves when you reply from any client.', gmail: () => NOTHING, teaches: 'No. The Find was right; you have handled it.', undo: 'Yes: reopens it.' },
      { id: 'snooze', label: 'Snooze', keys: 's', riddle: 'Hides it until the time you pick, then brings it back.', gmail: () => NOTHING, teaches: 'No.', undo: 'Yes.' },
      { id: 'retype', label: 'Retype', keys: 'r', riddle: 'Keeps it open as a different type, for example Review instead of Respond.', gmail: () => NOTHING, teaches: CORRECTION, undo: UNDO },
      { id: 'dismiss', label: 'Not important', keys: 'd', riddle: 'Closes it as something Riddle should not have raised. The message keeps its classification; no rule changes by itself.', gmail: () => NOTHING, teaches: CORRECTION, undo: 'Yes, and undoing withdraws the correction.' },
      { id: 'director', label: 'Talk to the Director about this', riddle: 'Opens the Director on this message: why Riddle decided it, whether a Mesh should change, what a change would do to recent mail, and, if you approve, applying it to past mail.', gmail: () => 'Nothing unless you approve a proposal, and then only through the account\'s Gmail settings.', teaches: 'Only through changes you approve.', undo: 'Every approved change can be undone from Activity.' },
    ],
  },
  message: {
    title: 'Acting on a message',
    intro: 'Your own moves. They apply at every autonomy level; what reaches Gmail depends on the account\'s "Changes in Gmail" setting.',
    items: [
      { id: 'archive', label: 'Archive', keys: 'e', mailbox: true, riddle: 'Records it as Filed: dealt with.', gmail: viaProjection('Archives it: out of the Inbox, still in All Mail.', 'Nothing: the account does not archive.'), teaches: CORRECTION, undo: UNDO },
      { id: 'spoil', label: 'Spoil', keys: '#', mailbox: true, riddle: 'Moves it to Spoil: legitimate, low value, still searchable.', gmail: viaProjection('Adds the Riddle/Spoil label and archives it.', 'Adds the Riddle/Spoil label; it stays in the Inbox.'), teaches: CORRECTION, undo: UNDO },
      { id: 'stratum', label: 'Add Stratum', keys: 'l', mailbox: true, riddle: 'Adds a Stratum (a topic such as Finance or Engineering). It does not change the disposition.', gmail: viaProjection('Adds a Riddle/<Stratum> label.', 'Adds a Riddle/<Stratum> label.'), teaches: 'No.', undo: UNDO },
      { id: 'snooze', label: 'Snooze', keys: 's', riddle: 'Hides it in Riddle until the time you pick.', gmail: () => NOTHING, teaches: 'No.', undo: UNDO },
      { id: 'contamination', label: 'Contamination', keys: '!', mailbox: true, riddle: 'Marks it untrustworthy and keeps it out of the Director\'s view unless you open it.', gmail: viaProjection('Adds the Riddle/Contamination label, never archives.', 'Adds the Riddle/Contamination label.'), teaches: CORRECTION, undo: UNDO },
      { id: 'correct', label: 'Correct…', mailbox: true, riddle: 'Changes what Riddle decided: Find (with a type), Filed, Spoil or Contamination. Each works as on the Unresolved page.', gmail: viaProjection('As for the disposition you pick: Filed and Spoil archive, Spoil and Contamination get a label.', 'Spoil and Contamination get a label; nothing is archived.'), teaches: CORRECTION, undo: UNDO },
      { id: 'follow_up', label: 'Follow up by…', keys: 'shift+f', mailbox: true, riddle: 'Opens a Follow Up Find due by the date you pick, or moves the date of the one it already has. It waits in Finds and resolves by itself when a reply arrives.', gmail: viaProjection('Removes any Riddle label and, if Riddle had archived it, puts it back in the Inbox.', 'Removes any Riddle label.'), teaches: CORRECTION, undo: UNDO },
    ],
  },
  review: {
    title: 'Reviewing Spoil and Contamination',
    intro: 'Low value and untrustworthy are different. Restoring tells Riddle it was wrong about this message; Never hide exempts the sender for good.',
    items: [
      { id: 'restore', label: 'Not Spoil / Restore', mailbox: true, riddle: 'Takes it out of Spoil or Contamination and back to where it belongs.', gmail: viaProjection('Removes the Riddle label and, if Riddle archived it, puts it back in the Inbox.', 'Removes the Riddle label.'), teaches: CORRECTION, undo: 'Yes, and undoing withdraws the correction.' },
      { id: 'delete', label: 'Move to Gmail\'s Trash…', mailbox: true, riddle: 'Forgets the messages: Riddle deletes its copy and their history. You confirm by typing the count.', gmail: () => 'Moves them to Gmail\'s Trash, which Gmail empties after 30 days.', teaches: 'No.', undo: 'No. It cannot be undone from Riddle; recover from Gmail\'s Trash within 30 days.' },
      { id: 'never_hide', label: 'Never hide sender', keys: 'h', riddle: 'Adds the sender to Never Hidden: nothing from them can become Spoil or Contamination again.', gmail: () => NOTHING, teaches: 'Yes, as a correction on this message.', undo: 'Yes, or remove the address in Settings → Mail rules.' },
    ],
  },
}

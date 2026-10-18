/**
 * The words Riddle uses, defined once, for the help sheet. A UX study found
 * new users meeting Find, Spoil, Stratum and Mesh with nothing to look them
 * up in; the rail glosses help in passing, this is the place to read them
 * together. Sorted the way the product is learned, not alphabetically.
 */

import { levelShows, type UiLevel } from './level'

export interface GlossaryEntry {
  term: string
  meaning: string
  /** Shown only at the Developer level. */
  developer?: boolean
  /** The lowest level at which the word is met; Essentials when absent. */
  level?: UiLevel
}

/** The entries a person at `level` meets: a developer term needs Developer, the rest their own level. */
export function glossaryFor(level: UiLevel): GlossaryEntry[] {
  return GLOSSARY.filter((g) => levelShows(level, g.developer ? 'developer' : (g.level ?? 'essentials')))
}

export const GLOSSARY: GlossaryEntry[] = [
  { term: 'Account', meaning: 'A mailbox Riddle has signed in to. Each account is its own Dig: its own copy of the mail, its own status, its own rules. "Dig" is the name you will see in Activity and in the CLI.' },
  { term: 'Find', meaning: 'Something that needs you, pulled out of the mail with a reason and a Why? you can inspect. One of eight types: Respond, Decide, Review, Do, Pay, Attend, Follow Up, Know.' },
  { term: 'Unresolved', meaning: 'Mail Riddle could not decide about. It is never hidden; it waits for you. Deciding it here teaches Riddle for next time.' },
  { term: 'Filed', meaning: 'Mail that needed no action and was put away under a Stratum. Still searchable, still in the Inbox view.' },
  { term: 'Spoil', meaning: 'Low-value mail set aside: newsletters, notifications, receipts you did not ask for. Nothing is deleted; Spoil stays searchable and can be restored with one key.' },
  { term: 'Contamination', meaning: 'Mail that is hostile or deceptive: phishing, spoofing, prompt injection. Kept apart from Spoil so it can be reviewed differently.' },
  { term: 'Stratum', meaning: 'A category Filed mail is put under, such as Purchases or Travel. Riddle proposes Strata as patterns appear; you rename and merge them.' },
  { term: 'Mesh', meaning: 'A rule in plain text: when mail matches, route it here. Versioned, previewable against your history before it takes effect, and only trusted to act once it has earned it.' },
  { term: 'Signal', meaning: 'A named feature Riddle computes about a message, such as "sender is a contact" or "has a deadline", that Meshes and the judge read. Shown only at the Developer level.', developer: true },
  { term: 'Director', meaning: 'The assistant you talk to. It can search, explain a decision and propose Meshes or moves; it changes nothing without your approval.' },
  { term: 'Proposal', meaning: 'A change the Director wants to make. It waits in the Director until you apply, adjust or decline it.' },
  { term: 'Expedition', meaning: 'A longer job you hand the Director with a scope and a budget: "find every subscription I paid for this year". It reports progress and its results become Finds.', level: 'everything' },
  { term: 'Never Hidden', meaning: 'Addresses and domains whose mail can never become Spoil or Contamination, whatever a Mesh or Signal says. You edit the list; the Director can only propose an addition.' },
  { term: 'Autonomy', meaning: 'How much an account lets Riddle do, in rungs: Observe (classify only) < Suggest (also propose moves) < Act (apply moves in Gmail) < Trusted (also riskier actions, such as sending, without asking).' },
  { term: 'Changes in Gmail', meaning: 'Whether Riddle writes its decisions back to the mailbox as labels, or labels and archiving, or not at all. Changes not yet written wait in a queue you can see in the Task Centre.' },
  { term: 'Activity', meaning: 'The record of everything that changed: who did it, why, and Undo where it is possible. Settings changes land here too.' },
  { term: 'Shadow Period', meaning: 'The first days of an account, when Riddle says what it would have done without doing it. You review its would-be moves and it earns the right to act.' },
]

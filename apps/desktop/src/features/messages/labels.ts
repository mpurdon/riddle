/**
 * Gmail label ids as people read them. The provider hands labels over
 * verbatim (`Message.labels`), and the UX study found the Inbox filter and
 * the message page showing `INBOX`, `CATEGORY_UPDATES` and `IMPORTANT` as
 * if they were words. System labels get their Gmail names; category labels
 * drop the prefix and take title case; a user's own labels are already
 * names and pass through unchanged.
 */
import { titleCase } from '@/lib/format'

const SYSTEM: Record<string, string> = {
  INBOX: 'Inbox',
  SENT: 'Sent',
  IMPORTANT: 'Important',
  SNOOZED: 'Snoozed',
  STARRED: 'Starred',
  UNREAD: 'Unread',
  DRAFT: 'Draft',
  SPAM: 'Spam',
  TRASH: 'Trash',
}

const CATEGORY = 'CATEGORY_'

export function labelName(id: string): string {
  const fixed = SYSTEM[id]
  if (fixed) return fixed
  if (id.startsWith(CATEGORY) && id.length > CATEGORY.length) return titleCase(id.slice(CATEGORY.length).toLowerCase())
  return id
}

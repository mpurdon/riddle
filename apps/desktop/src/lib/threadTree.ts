/**
 * A thread as a reply tree, from the headers mail clients already write:
 * `In-Reply-To` names the parent, `References` the lineage when the parent
 * is missing. The thread view used to list messages flat by date, so seven
 * notifications on one pull request read as seven siblings with no way to
 * tell a reply from a new branch.
 *
 * Messages whose parent is not in the thread are roots. Order is depth
 * first, each level by date, so a reply sits under what it answers.
 */
import type { Message } from '@/bindings/Message'

export interface ThreadNode {
  message: Message
  depth: number
  parentId: string | null
  /** For each ancestor level (0..depth-1): whether a later sibling at that level follows, so its rail continues. */
  rails: boolean[]
  /** Whether a later sibling of this message follows. */
  hasNextSibling: boolean
}

const norm = (id: string | null | undefined) => (id ?? '').trim().replace(/^<|>$/g, '').toLowerCase()

export function buildThreadTree(messages: Message[]): ThreadNode[] {
  const byMsgId = new Map<string, Message>()
  for (const m of messages) {
    const k = norm(m.headers.messageId)
    if (k) byMsgId.set(k, m)
  }
  const parentOf = new Map<string, string | null>()
  for (const m of messages) {
    let parent: Message | undefined = byMsgId.get(norm(m.headers.inReplyTo))
    if (!parent) {
      for (const ref of [...m.headers.references].reverse()) {
        parent = byMsgId.get(norm(ref))
        if (parent) break
      }
    }
    // A message cannot parent itself, and a cycle (bad headers) falls back to a root.
    parentOf.set(m.id, parent && parent.id !== m.id ? parent.id : null)
  }
  for (const m of messages) {
    const seen = new Set<string>([m.id])
    let p = parentOf.get(m.id) ?? null
    while (p) {
      if (seen.has(p)) {
        parentOf.set(m.id, null)
        break
      }
      seen.add(p)
      p = parentOf.get(p) ?? null
    }
  }
  const byDate = (a: Message, b: Message) => a.date.localeCompare(b.date)
  const children = new Map<string | null, Message[]>()
  for (const m of messages) {
    const p = parentOf.get(m.id) ?? null
    children.set(p, [...(children.get(p) ?? []), m])
  }
  for (const list of children.values()) list.sort(byDate)

  const out: ThreadNode[] = []
  const walk = (parent: string | null, depth: number, rails: boolean[]) => {
    const kids = children.get(parent) ?? []
    kids.forEach((m, i) => {
      const hasNextSibling = i < kids.length - 1
      out.push({ message: m, depth, parentId: parent, rails, hasNextSibling })
      walk(m.id, depth + 1, [...rails, hasNextSibling])
    })
  }
  walk(null, 0, [])
  return out
}

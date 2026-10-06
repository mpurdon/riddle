/**
 * One message in a reply tree (`lib/threadTree`): a rail per ancestor
 * level, running on while that ancestor has later siblings, an elbow
 * joining the reply to its line, and "replying to" naming the parent.
 * Deeper replies are narrower; past MAX_DRAWN_DEPTH the indent stops
 * growing and the level is written instead. Used wherever a thread is
 * shown: the message page and a Find's sources.
 */
import type { ReactNode } from 'react'
import { CornerDownRight } from 'lucide-react'
import type { Address } from '@/bindings/Address'
import { cx } from '@/lib/format'
import type { ThreadNode } from '@/lib/threadTree'

/** How a message names its sender: the display name, else the address. Null for no message (a parent not in the thread). */
export function senderOf(m: { from: Address }): string
export function senderOf(m: { from: Address } | undefined | null): string | null
export function senderOf(m: { from: Address } | undefined | null): string | null {
  return m ? (m.from.name ?? m.from.email) : null
}

/** Rails for deep threads stop widening here; the depth is then shown as a number. */
const MAX_DRAWN_DEPTH = 6

/**
 * One message in the reply tree: a rail per ancestor level, running on
 * while that ancestor has later siblings, and an elbow joining this reply to
 * its line. Deeper replies are narrower; past MAX_DRAWN_DEPTH the indent
 * stops growing and the level is written instead.
 */
export function ThreadRow({ node, parentName, children }: { node: ThreadNode; parentName: string | null; children: ReactNode }) {
  const drawn = Math.min(node.depth, MAX_DRAWN_DEPTH)
  const columns = Array.from({ length: drawn }, (_, c) => c)
  return (
    <div className="flex">
      {columns.map((c) => {
        const own = c === drawn - 1
        // Column c joins level c to level c+1; it continues while the ancestor one level deeper has a later sibling.
        const through = !own && node.rails[c + 1 + (node.depth - drawn)]
        return (
          <div key={c} aria-hidden className="relative w-5 shrink-0">
            {through ? <span className="absolute inset-y-0 left-2 w-px bg-line" /> : null}
            {own ? (
              <>
                <span className={cx('absolute left-2 top-0 w-px bg-ochre/50', node.hasNextSibling ? 'bottom-0' : 'h-6')} />
                <span className="absolute left-2 top-6 h-px w-3 bg-ochre/50" />
              </>
            ) : null}
          </div>
        )
      })}
      <div className="min-w-0 flex-1 pb-2">
        {node.depth > 0 ? (
          <div className="mb-0.5 flex items-center gap-1 text-2xs text-clay">
            <CornerDownRight className="size-3" />
            <span className="truncate">replying to {parentName ?? 'an earlier message'}</span>
            {node.depth > MAX_DRAWN_DEPTH ? <span className="rounded bg-sand-2 px-1">depth {node.depth}</span> : null}
          </div>
        ) : null}
        {children}
      </div>
    </div>
  )
}


/**
 * The virtualised, paged message list shared by Inbox, Spoil and
 * Contamination. Rows show disposition and Strata chips; selection and
 * keyboard triage are handled by the page that owns the list. A click moves
 * the cursor (the page previews the row beside the list); Enter or a double
 * click opens it. Sender, subject and snippet are selectable text: the app
 * turns selection off by default, and copying a subject is a daily need.
 *
 * `MessageRow` is the one row component, so the by-sender Spoil view and
 * the Shadow review list look like the Inbox: a page may hang controls off
 * the right of a row (`rowTrailing`) or open something under it
 * (`rowBelow`). The rows are positioned by estimate, so the page also says
 * how tall that something is (`rowBelowHeight`, asked for every row on
 * every layout, which is why it is not the element itself) and when the
 * answer changes (`layoutKey`).
 */
import type { CSSProperties, ReactNode } from 'react'
import { Paperclip, Send } from 'lucide-react'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { cx, fmtTime } from '@/lib/format'
import { keyHint } from '@/lib/shortcuts'
import { DispositionChip, ImportanceChip, StrataChips } from '@/components/Bits'
import { Chip } from '@/components/ui/Chip'
import { senderOf } from '@/components/ThreadRow'
import { ROW_HEIGHT, rowStyle, useVirtualRows, VirtualListbox } from './useVirtualRows'

export interface MessageListProps {
  items: MessageSummary[]
  cursor: number
  selected: Set<string>
  onCursor: (i: number) => void
  onToggle: (id: string) => void
  onOpen: (m: MessageSummary) => void
  digName: (id: string) => string
  hasMore: boolean
  loadingMore: boolean
  onLoadMore: () => void
  showDisposition?: boolean
  emptyState: ReactNode
  loading: boolean
  /** Controls on the right of a row's second line. */
  rowTrailing?: (m: MessageSummary) => ReactNode
  /** Something open under a row. */
  rowBelow?: (m: MessageSummary) => ReactNode
  /** The height of what is open under a row; 0 when nothing is. */
  rowBelowHeight?: (m: MessageSummary) => number
  /** Changes whenever `rowBelowHeight` would answer differently, so the rows are laid out again. */
  layoutKey?: string
}

export function MessageList(p: MessageListProps) {
  const list = useVirtualRows({
    count: p.items.length,
    cursor: p.cursor,
    hasMore: p.hasMore,
    loadingMore: p.loadingMore,
    onLoadMore: p.onLoadMore,
    extraHeight: (i) => {
      const m = p.items[i]
      return m && p.rowBelowHeight ? p.rowBelowHeight(m) : 0
    },
    layoutKey: p.layoutKey,
  })
  return (
    <VirtualListbox list={list} label="Messages" loading={p.loading} emptyState={p.emptyState}>
      {(v) => {
        const m = p.items[v.index]
        return (
          <MessageRow
            key={m.id}
            m={m}
            index={v.index}
            active={v.index === p.cursor}
            checked={p.selected.has(m.id)}
            onCursor={() => p.onCursor(v.index)}
            onToggle={() => p.onToggle(m.id)}
            onOpen={() => p.onOpen(m)}
            digName={p.digName}
            showDisposition={p.showDisposition}
            trailing={p.rowTrailing?.(m)}
            below={p.rowBelow?.(m)}
            style={rowStyle(v.start, v.size)}
          />
        )
      }}
    </VirtualListbox>
  )
}

export interface MessageRowProps {
  m: MessageSummary
  /** The cursor index, for `data-row-index` (the list keeps it in view). */
  index: number
  active: boolean
  checked: boolean
  onCursor: () => void
  onToggle: () => void
  onOpen: () => void
  digName: (id: string) => string
  showDisposition?: boolean
  /** Indented under a sender row. */
  nested?: boolean
  trailing?: ReactNode
  below?: ReactNode
  style?: CSSProperties
}

export function MessageRow(p: MessageRowProps) {
  const { m } = p
  return (
    <div
      role="option"
      aria-selected={p.active}
      data-row-index={p.index}
      tabIndex={-1}
      style={p.style}
      className={cx('flex cursor-default flex-col border-b border-line/60', p.nested ? 'pl-9 pr-3' : 'px-3', p.active ? 'row-selected' : p.checked ? 'bg-ochre/5' : 'hover:bg-sand/60', m.supersededBy && !p.active && 'opacity-60')}
      onClick={p.onCursor}
      onDoubleClick={p.onOpen}
    >
      <div className="flex items-center gap-2" style={{ height: ROW_HEIGHT }}>
        <input type="checkbox" checked={p.checked} onChange={p.onToggle} onClick={(e) => e.stopPropagation()} aria-label={`Select ${m.subject}`} title={`Select or deselect (${keyHint('select')})`} className="shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <span className={cx('w-40 shrink-0 truncate text-sm selectable', m.disposition?.kind === 'find' ? 'font-semibold' : 'font-medium')}>
              {m.isSent ? (
                <span className="mr-1 inline-flex" role="img" aria-label="Sent by you" title="Sent by you">
                  <Send className="inline size-3 text-clay" aria-hidden />
                </span>
              ) : null}
              {m.isSent ? 'To: ' : ''}
              {senderOf(m)}
            </span>
            <span className="min-w-0 flex-1 truncate text-sm selectable">{m.subject}</span>
            {m.hasAttachments ? (
              <span className="inline-flex shrink-0" role="img" aria-label="Has attachments" title="Has attachments">
                <Paperclip className="size-3 text-clay" aria-hidden />
              </span>
            ) : null}
            <span className="shrink-0 text-xs text-clay">{fmtTime(m.date)}</span>
          </div>
          <div className="mt-0.5 flex items-center gap-2">
            <span className="min-w-0 flex-1 truncate text-xs text-ink-2 selectable">{m.snippet}</span>
            <span className="flex shrink-0 items-center gap-1">
              <span className="text-2xs text-clay">{p.digName(m.digId)}</span>
              <StrataChips strata={m.strata} max={2} />
              <ImportanceChip level={m.importance} yours={m.importanceSetByYou} />
              {m.supersededBy ? <Chip title="A newer message from this sender replaced it; it reads as read and sinks below its list">Superseded</Chip> : null}
              {/* "Would be" when the row's disposition is a Shadow proposal; the row says so itself, so a plain list never reads "Would be Unsifted". */}
              {p.showDisposition !== false ? <DispositionChip d={m.disposition} shadow={m.shadow} /> : null}
              {p.trailing}
            </span>
          </div>
        </div>
      </div>
      {p.below ? <div onClick={(e) => e.stopPropagation()}>{p.below}</div> : null}
    </div>
  )
}

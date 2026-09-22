/**
 * The virtualised, paged message list shared by Inbox, Spoil and
 * Contamination. Rows show disposition and Strata chips; selection and
 * keyboard triage are handled by the page that owns the list.
 */
import { useEffect, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { Paperclip, Send } from 'lucide-react'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { cx, fmtTime } from '@/lib/format'
import { DispositionChip, StrataChips } from '@/components/Bits'
import { Button } from '@/components/ui/Button'

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
  shadow?: boolean
  emptyState: React.ReactNode
  loading: boolean
}

export function MessageList(p: MessageListProps) {
  const parentRef = useRef<HTMLDivElement>(null)
  const rowVirtualizer = useVirtualizer({ count: p.items.length + (p.hasMore ? 1 : 0), getScrollElement: () => parentRef.current, estimateSize: () => 56, overscan: 12 })
  useEffect(() => {
    if (p.items.length) rowVirtualizer.scrollToIndex(p.cursor, { align: 'auto' })
  }, [p.cursor]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const last = rowVirtualizer.getVirtualItems().at(-1)
    if (last && p.hasMore && !p.loadingMore && last.index >= p.items.length - 1) p.onLoadMore()
  }, [rowVirtualizer.getVirtualItems(), p.hasMore, p.loadingMore]) // eslint-disable-line react-hooks/exhaustive-deps
  if (!p.loading && !p.items.length) return <div className="min-h-0 flex-1 overflow-auto">{p.emptyState}</div>
  return (
    <div ref={parentRef} className="min-h-0 flex-1 overflow-auto" role="listbox" aria-multiselectable aria-label="Messages">
      <div style={{ height: rowVirtualizer.getTotalSize(), position: 'relative' }}>
        {rowVirtualizer.getVirtualItems().map((v) => {
          const m = p.items[v.index]
          if (!m) {
            return (
              <div key="more" style={{ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${v.start}px)`, height: v.size }} className="flex items-center justify-center text-xs text-clay">
                {p.loadingMore ? 'Loading more…' : <Button size="sm" variant="ghost" onClick={p.onLoadMore}>Load more</Button>}
              </div>
            )
          }
          const active = v.index === p.cursor
          const checked = p.selected.has(m.id)
          return (
            <div
              key={m.id}
              role="option"
              aria-selected={active}
              data-row-index={v.index}
              tabIndex={-1}
              style={{ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${v.start}px)`, height: v.size }}
              className={cx('flex cursor-default items-center gap-2 border-b border-line/60 px-3', active ? 'row-selected' : checked ? 'bg-ochre/5' : 'hover:bg-sand/60')}
              onClick={() => p.onCursor(v.index)}
              onDoubleClick={() => p.onOpen(m)}
            >
              <input type="checkbox" checked={checked} onChange={() => p.onToggle(m.id)} onClick={(e) => e.stopPropagation()} aria-label={`Select ${m.subject}`} className="shrink-0" />
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-2">
                  <span className={cx('w-40 shrink-0 truncate text-sm', m.disposition?.kind === 'find' ? 'font-semibold' : 'font-medium')}>
                    {m.isSent ? <Send className="mr-1 inline size-3 text-clay" aria-label="Sent" /> : null}
                    {m.isSent ? 'To: ' : ''}
                    {m.from.name ?? m.from.email}
                  </span>
                  <span className="min-w-0 flex-1 truncate text-sm">{m.subject}</span>
                  {m.hasAttachments ? <Paperclip className="size-3 shrink-0 text-clay" aria-label="Has attachments" /> : null}
                  <span className="shrink-0 text-xs text-clay">{fmtTime(m.date)}</span>
                </div>
                <div className="mt-0.5 flex items-center gap-2">
                  <span className="min-w-0 flex-1 truncate text-xs text-ink-2">{m.snippet}</span>
                  <span className="flex shrink-0 items-center gap-1">
                    <span className="text-[11px] text-clay">{p.digName(m.digId)}</span>
                    <StrataChips strata={m.strata} max={2} />
                    {p.showDisposition !== false ? <DispositionChip d={m.disposition} shadow={p.shadow} /> : null}
                  </span>
                </div>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}

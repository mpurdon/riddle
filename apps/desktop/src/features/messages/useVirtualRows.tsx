/**
 * The virtualiser scaffolding every message list shares: rows laid out by
 * estimate, the cursor row kept in view, the next page asked for when the
 * last row comes into view, and the listbox that renders it all with a
 * Load more row at the end. Lifted out of MessageList when SenderList had
 * grown a copy of it, options and effects alike.
 */
import { useEffect, useRef, type CSSProperties, type ReactNode, type RefObject } from 'react'
import { useVirtualizer, type VirtualItem, type Virtualizer } from '@tanstack/react-virtual'
import { Button } from '@/components/ui/Button'

/** The height a row is laid out at before anything opens under it. */
export const ROW_HEIGHT = 56

export interface VirtualRowsOptions {
  /** Rows in the list, without the Load more row. */
  count: number
  cursor: number
  hasMore: boolean
  loadingMore: boolean
  onLoadMore: () => void
  /** Extra height under a row, when something is open there. */
  extraHeight?: (index: number) => number
  /** Changes whenever `extraHeight` would answer differently, so the rows are laid out again. */
  layoutKey?: string
}

export interface VirtualRows {
  parentRef: RefObject<HTMLDivElement | null>
  rows: Virtualizer<HTMLDivElement, Element>
  count: number
  loadingMore: boolean
  onLoadMore: () => void
}

export function useVirtualRows(o: VirtualRowsOptions): VirtualRows {
  const parentRef = useRef<HTMLDivElement>(null)
  const rows = useVirtualizer({
    count: o.count + (o.hasMore ? 1 : 0),
    getScrollElement: () => parentRef.current,
    estimateSize: (i) => ROW_HEIGHT + (o.extraHeight?.(i) ?? 0),
    overscan: 12,
  })
  useEffect(() => {
    rows.measure()
  }, [o.layoutKey]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    if (o.count) rows.scrollToIndex(o.cursor, { align: 'auto' })
  }, [o.cursor]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const last = rows.getVirtualItems().at(-1)
    if (last && o.hasMore && !o.loadingMore && last.index >= o.count - 1) o.onLoadMore()
  }, [rows.getVirtualItems(), o.hasMore, o.loadingMore]) // eslint-disable-line react-hooks/exhaustive-deps
  return { parentRef, rows, count: o.count, loadingMore: o.loadingMore, onLoadMore: o.onLoadMore }
}

export const rowStyle = (start: number, size: number): CSSProperties => ({ position: 'absolute', top: 0, left: 0, width: '100%', transform: `translateY(${start}px)`, height: size })

/** The scroll box: the rows in view where the virtualiser puts them, Load more after the last, or the empty state once the list has loaded empty. */
export function VirtualListbox({ list, label, loading, emptyState, children }: { list: VirtualRows; label: string; loading: boolean; emptyState: ReactNode; children: (it: VirtualItem) => ReactNode }) {
  if (!loading && !list.count) return <div className="min-h-0 flex-1 overflow-auto">{emptyState}</div>
  return (
    <div ref={list.parentRef} className="min-h-0 flex-1 overflow-auto" role="listbox" aria-multiselectable aria-label={label}>
      <div style={{ height: list.rows.getTotalSize(), position: 'relative' }}>
        {list.rows.getVirtualItems().map((it) => (it.index < list.count ? children(it) : <LoadMoreRow key="more" style={rowStyle(it.start, it.size)} loadingMore={list.loadingMore} onLoadMore={list.onLoadMore} />))}
      </div>
    </div>
  )
}

export function LoadMoreRow({ style, loadingMore, onLoadMore }: { style: CSSProperties; loadingMore: boolean; onLoadMore: () => void }) {
  return (
    <div style={style} className="flex items-center justify-center text-xs text-clay">
      {loadingMore ? (
        'Loading more…'
      ) : (
        <Button size="sm" variant="ghost" onClick={onLoadMore}>
          Load more
        </Button>
      )}
    </div>
  )
}

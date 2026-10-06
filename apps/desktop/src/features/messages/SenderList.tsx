/**
 * Spoil by sender: one row per address with the count, the display name
 * and the newest subject; Enter or a click on the chevron opens the
 * sender's messages under it, indented, as ordinary message rows. Two
 * actions sit on the sender row, because they are what a reader does with
 * a sender in Spoil: "Not Spoil for this sender" (restore everything here
 * from them and add a rule) and "Never hide". Rows are a flat list from
 * `senderRows`, so j/k walk senders when collapsed and messages when open;
 * selection is by row id, and a selected sender stands for its messages.
 */
import { NeverHideButton } from '@/components/NeverHideButton'
import type { ReactNode } from 'react'
import { ChevronDown, ChevronRight, Undo2 } from 'lucide-react'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { cx, fmtTime, plural } from '@/lib/format'
import { keyHint } from '@/lib/shortcuts'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { MessageRow } from './MessageList'
import type { SenderGroup, SenderRow } from './senderGroups'
import { rowStyle, useVirtualRows, VirtualListbox } from './useVirtualRows'

export interface SenderListProps {
  rows: SenderRow[]
  cursor: number
  /** Row ids: a sender's id or a message id. */
  selected: Set<string>
  expanded: Set<string>
  onCursor: (i: number) => void
  onToggle: (id: string) => void
  onToggleExpand: (group: SenderGroup) => void
  onOpen: (m: MessageSummary) => void
  digName: (id: string) => string
  /** The verb of the page: "Not Spoil" or "Restore". */
  verb: string
  onCorrectSender: (group: SenderGroup) => void
  onNeverHide: (group: SenderGroup) => void
  busy: boolean
  hasMore: boolean
  loadingMore: boolean
  onLoadMore: () => void
  loading: boolean
  emptyState: ReactNode
}

export function SenderList(p: SenderListProps) {
  const list = useVirtualRows({ count: p.rows.length, cursor: p.cursor, hasMore: p.hasMore, loadingMore: p.loadingMore, onLoadMore: p.onLoadMore })
  const isChecked = (row: SenderRow) => (row.kind === 'sender' ? p.selected.has(row.id) || row.group.items.every((m) => p.selected.has(m.id)) : p.selected.has(row.id) || p.selected.has(`sender:${row.group.key}`))
  return (
    <VirtualListbox list={list} label="Senders" loading={p.loading} emptyState={p.emptyState}>
      {(it) => {
        const row = p.rows[it.index]
        const active = it.index === p.cursor
        if (row.kind === 'message') {
          return (
            <MessageRow
              key={row.id}
              m={row.m}
              index={it.index}
              active={active}
              checked={isChecked(row)}
              nested
              showDisposition={false}
              onCursor={() => p.onCursor(it.index)}
              onToggle={() => p.onToggle(row.id)}
              onOpen={() => p.onOpen(row.m)}
              digName={p.digName}
              style={rowStyle(it.start, it.size)}
            />
          )
        }
        const g = row.group
        const open = p.expanded.has(g.key)
        const checked = isChecked(row)
        const count = plural(g.items.length, 'message')
        return (
          <div
            key={row.id}
            role="option"
            aria-selected={active}
            aria-expanded={open}
            aria-label={`${g.name ?? g.email}, ${count}`}
            data-row-index={it.index}
            tabIndex={-1}
            style={rowStyle(it.start, it.size)}
            className={cx('flex cursor-default items-center gap-2 border-b border-line/60 px-3', active ? 'row-selected' : checked ? 'bg-ochre/5' : 'hover:bg-sand/60')}
            onClick={() => p.onCursor(it.index)}
            onDoubleClick={() => p.onToggleExpand(g)}
          >
            <input type="checkbox" checked={checked} onChange={() => p.onToggle(row.id)} onClick={(e) => e.stopPropagation()} aria-label={`Select every message from ${g.email}`} title={`Select or deselect every message from this sender (${keyHint('select')})`} className="shrink-0" />
            <button
              type="button"
              className="shrink-0 rounded p-0.5 text-clay hover:bg-sand hover:text-ink"
              onClick={(e) => {
                e.stopPropagation()
                p.onToggleExpand(g)
              }}
              aria-label={open ? `Collapse ${g.email}` : `Expand ${g.email}`}
              title={`${open ? 'Collapse' : 'Expand'} the messages from this sender (${keyHint('open')})`}
            >
              {open ? <ChevronDown className="size-3.5" aria-hidden /> : <ChevronRight className="size-3.5" aria-hidden />}
            </button>
            <div className="min-w-0 flex-1">
              <div className="flex items-center gap-2">
                <span className="w-40 shrink-0 truncate text-sm font-medium selectable" title={g.email}>
                  {g.name ?? g.email}
                </span>
                <Chip tone="neutral" title="Loaded messages from this sender; more may be on later pages">
                  {count}
                </Chip>
                <span className="min-w-0 flex-1 truncate text-sm text-ink-2 selectable">{g.newest.subject}</span>
                <span className="shrink-0 text-xs text-clay">{fmtTime(g.newest.date)}</span>
              </div>
              <div className="mt-0.5 flex items-center gap-2">
                <span className="min-w-0 flex-1 truncate text-xs text-clay selectable">{g.email}</span>
                <span className="flex shrink-0 items-center gap-1" onClick={(e) => e.stopPropagation()} onDoubleClick={(e) => e.stopPropagation()}>
                  <Button size="sm" variant="ghost" icon={<Undo2 />} disabledReason={p.busy ? 'Working' : null} onClick={() => p.onCorrectSender(g)} title={`Restore the ${count} here from ${g.email} and add a rule: never ${p.verb === 'Restore' ? 'Contamination' : 'Spoil'} from this sender`}>
                    {`${p.verb} for this sender`}
                  </Button>
                  <NeverHideButton email={g.email} variant="ghost" disabledReason={p.busy ? 'Working' : null} onAdd={() => p.onNeverHide(g)} title={`Add ${g.email} to Never Hidden and restore what is here from them (${keyHint('never_hidden')})`} />
                </span>
              </div>
            </div>
          </div>
        )
      }}
    </VirtualListbox>
  )
}

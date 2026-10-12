/**
 * The Inbox's mail column (option D of the mail-navigation study, picked
 * 11 Oct 2026). The Inbox filtered by label through a flat dropdown of every
 * Gmail label, with Riddle's Strata in a second dropdown: labels you could
 * not see, and a `Receipts` label beside a `Receipts` Stratum as two things.
 *
 * One column, short enough to scan (Hick; Kole Jain's progressive
 * disclosure): the mailboxes, your pins, Topics (a Stratum and your label of
 * the same name, one row), and the rest of your labels folded under one
 * line, with empty labels folded again. Every place shows a count, unread
 * first (Spool's scent of information). It collapses to icons with ⌘\ when
 * the list wants the width (Showalter), and `g l` reaches any place by name.
 * The fold and collapse states are this machine's preference; pins are
 * yours and go through the `pin_place` Action.
 */
import { useState } from 'react'
import { ChevronDown, ChevronRight, Inbox, Layers, Mails, PanelLeftClose, PanelLeftOpen, Pin, PinOff, Send, Star, Tag } from 'lucide-react'
import type { MailPlace } from '@/bindings/MailPlace'
import type { MailPlaces } from '@/bindings/MailPlaces'
import { cx, fmtInt } from '@/lib/format'
import { keyHint } from '@/lib/shortcuts'
import { useAction } from '@/lib/useAction'
import { Tip } from '@/components/ui/Tooltip'
import { labelCounts, labelTree, leafName } from './places'

const TOPICS_SHOWN = 8

/** A small persisted preference: this machine's fold states. */
function usePref<T>(key: string, initial: T): [T, (v: T) => void] {
  const [v, setV] = useState<T>(() => {
    try {
      const raw = localStorage.getItem(`riddle.mailColumn.${key}`)
      return raw === null ? initial : (JSON.parse(raw) as T)
    } catch {
      return initial
    }
  })
  const set = (next: T) => {
    setV(next)
    try {
      localStorage.setItem(`riddle.mailColumn.${key}`, JSON.stringify(next))
    } catch {
      // A preference that cannot be saved is still applied for this session.
    }
  }
  return [v, set]
}

export function useMailColumnSlim(): [boolean, (v: boolean) => void] {
  return usePref('slim', false)
}

function icon(row: MailPlace) {
  if (row.place === 'all') return <Mails />
  if (row.place === 'mailbox:INBOX') return <Inbox />
  if (row.place === 'mailbox:STARRED') return <Star />
  if (row.place === 'mailbox:SENT') return <Send />
  return row.kind === 'topic' ? <Layers /> : <Tag />
}

function Count({ row }: { row: MailPlace }) {
  if (row.unread) return <span className="ml-auto pl-2 text-2xs font-semibold tabular-nums text-ink">{fmtInt(row.unread)}</span>
  if (!row.messages) return null
  return <span className="ml-auto pl-2 text-2xs tabular-nums text-clay">{fmtInt(row.messages)}</span>
}

interface RowProps {
  row: MailPlace
  active: boolean
  onPlace: (place: string) => void
  depth?: number
  label?: string
  twisty?: { open: boolean; toggle: () => void } | null
  pinnable?: boolean
}

function PlaceRow({ row, active, onPlace, depth = 0, label, twisty, pinnable = row.kind !== 'mailbox' }: RowProps) {
  const { run } = useAction()
  const name = label ?? row.name
  const pin = () => void run(row.pinned ? 'unpin_place' : 'pin_place', { place: row.place })
  return (
    <div className={cx('group relative flex h-7 items-center rounded-md text-sm', active ? 'bg-sand-2 font-medium text-ink' : 'text-ink-2 hover:bg-sand-2 hover:text-ink')} style={{ paddingLeft: 4 + depth * 14 }}>
      {twisty !== undefined ? (
        <button type="button" className="grid size-4 shrink-0 place-items-center text-clay hover:text-ink [&>svg]:size-3" onClick={twisty?.toggle} aria-label={twisty ? `${twisty.open ? 'Fold' : 'Unfold'} ${name}` : undefined} tabIndex={twisty ? 0 : -1} disabled={!twisty}>
          {twisty ? twisty.open ? <ChevronDown /> : <ChevronRight /> : null}
        </button>
      ) : null}
      <button type="button" onClick={() => onPlace(row.place)} aria-current={active ? 'page' : undefined} title={row.name !== name ? row.name : undefined} className="flex h-full min-w-0 flex-1 items-center gap-2 px-1 text-left">
        <span className="shrink-0 text-clay [&>svg]:size-3.5">{icon(row)}</span>
        <span className="min-w-0 truncate">{name}</span>
        {row.hasLabel ? (
          <Tip text="Your Gmail label and Riddle's Stratum of the same name, together">
            <span className="shrink-0 rounded border border-line px-1 text-[10px] font-normal leading-4 text-clay">+ label</span>
          </Tip>
        ) : null}
        <Count row={row} />
      </button>
      {pinnable ? (
        <button type="button" onClick={pin} aria-label={`${row.pinned ? 'Unpin' : 'Pin'} ${name}`} className="absolute right-1 hidden size-5 place-items-center rounded bg-sand-2 text-clay hover:text-ink group-focus-within:grid group-hover:grid [&>svg]:size-3">
          {row.pinned ? <PinOff /> : <Pin />}
        </button>
      ) : null}
    </div>
  )
}

function Heading({ children, hint }: { children: React.ReactNode; hint?: string }) {
  return (
    <div className="mt-3 flex items-baseline justify-between px-2 pb-0.5 text-2xs font-semibold uppercase tracking-wider text-clay first:mt-0">
      <span>{children}</span>
      {hint ? <span className="font-normal normal-case tracking-normal">{hint}</span> : null}
    </div>
  )
}

const Fold = ({ onClick, children }: { onClick: () => void; children: React.ReactNode }) => (
  <button type="button" onClick={onClick} className="flex h-6 w-full items-center gap-1 rounded-md px-2 text-left text-xs text-clay hover:bg-sand-2 hover:text-ink">
    {children}
  </button>
)

export interface MailColumnProps {
  places: MailPlaces | undefined
  /** The place on screen; `all` when nothing is chosen. */
  place: string
  onPlace: (place: string) => void
  slim: boolean
  onSlim: (slim: boolean) => void
}

export function MailColumn({ places, place, onPlace, slim, onSlim }: MailColumnProps) {
  const [labelsOpen, setLabelsOpen] = usePref('labelsOpen', false)
  const [showEmpty, setShowEmpty] = usePref('showEmpty', false)
  const [closedList, setClosed] = usePref<string[]>('closed', [])
  const [allTopics, setAllTopics] = useState(false)
  const closed = new Set(closedList)
  const toggle = (name: string) => setClosed(closed.has(name) ? closedList.filter((n) => n !== name) : [...closedList, name])

  if (slim) {
    const rows = places ? [...places.mailboxes, ...places.pinned] : []
    return (
      <nav aria-label="Mail places" className="flex w-11 shrink-0 flex-col items-center gap-0.5 border-r border-line bg-sand/30 py-2">
        <Tip text={`Expand the mail column (${keyHint('toggle_mail_column')})`} side="right">
          <button type="button" onClick={() => onSlim(false)} aria-label="Expand the mail column" className="mb-1 grid size-7 place-items-center rounded-md text-clay hover:bg-sand-2 hover:text-ink [&>svg]:size-4">
            <PanelLeftOpen />
          </button>
        </Tip>
        {rows.map((r) => (
          <Tip key={r.place} text={`${r.name}${r.unread ? ` · ${fmtInt(r.unread)} unread` : ''}`} side="right">
            <button type="button" onClick={() => onPlace(r.place)} aria-label={r.name} aria-current={place === r.place ? 'page' : undefined} className={cx('relative grid size-7 place-items-center rounded-md [&>svg]:size-4', place === r.place ? 'bg-sand-2 text-ink' : 'text-clay hover:bg-sand-2 hover:text-ink')}>
              {icon(r)}
              {r.unread ? <span className="absolute right-0.5 top-0.5 size-1.5 rounded-full bg-ochre" aria-hidden /> : null}
            </button>
          </Tip>
        ))}
      </nav>
    )
  }

  const topics = places?.topics ?? []
  const shownTopics = allTopics ? topics : topics.slice(0, TOPICS_SHOWN)
  const labels = places?.labels ?? []
  const counts = labelCounts(labels)
  const tree = labelTree(labels, showEmpty, closed)
  return (
    <nav aria-label="Mail places" className="flex w-56 shrink-0 flex-col border-r border-line bg-sand/30">
      <div className="min-h-0 flex-1 overflow-auto px-2 py-2">
        <div className="flex items-center justify-between px-2 pb-0.5 text-2xs font-semibold uppercase tracking-wider text-clay">
          <span>Mail</span>
          <Tip text={`Collapse to icons (${keyHint('toggle_mail_column')})`}>
            <button type="button" onClick={() => onSlim(true)} aria-label="Collapse the mail column" className="grid size-5 place-items-center rounded text-clay hover:bg-sand-2 hover:text-ink [&>svg]:size-3.5">
              <PanelLeftClose />
            </button>
          </Tip>
        </div>
        {places?.mailboxes.map((r) => <PlaceRow key={r.place} row={r} active={place === r.place} onPlace={onPlace} />)}

        {places?.pinned.length ? (
          <>
            <Heading>Pinned</Heading>
            {places.pinned.map((r) => <PlaceRow key={r.place} row={r} active={place === r.place} onPlace={onPlace} label={leafName(r)} pinnable />)}
          </>
        ) : null}

        {topics.length ? (
          <>
            <Heading hint="Strata and your labels">Topics</Heading>
            {shownTopics.map((r) => <PlaceRow key={r.place} row={r} active={place === r.place} onPlace={onPlace} />)}
            {topics.length > TOPICS_SHOWN ? <Fold onClick={() => setAllTopics(!allTopics)}>{allTopics ? 'Fewer Topics' : `${topics.length - TOPICS_SHOWN} more Topics`}</Fold> : null}
          </>
        ) : null}

        {labels.length ? (
          <>
            <Heading>Labels</Heading>
            {labelsOpen ? (
              <>
                {tree.map((t) => (
                  <PlaceRow key={t.row.place} row={t.row} active={place === t.row.place} onPlace={onPlace} depth={t.depth} label={t.leaf} twisty={t.hasChildren ? { open: t.open, toggle: () => toggle(t.row.name) } : null} />
                ))}
                {counts.empty ? <Fold onClick={() => setShowEmpty(!showEmpty)}>{showEmpty ? 'Hide empty labels' : `Show ${counts.empty} empty label${counts.empty === 1 ? '' : 's'}`}</Fold> : null}
                <Fold onClick={() => setLabelsOpen(false)}>Fold labels</Fold>
              </>
            ) : (
              <Fold onClick={() => setLabelsOpen(true)}>
                <ChevronRight className="size-3" aria-hidden />
                {counts.withMail ? `${counts.withMail} label${counts.withMail === 1 ? '' : 's'} with mail` : `${counts.empty} empty label${counts.empty === 1 ? '' : 's'}`}
              </Fold>
            )}
          </>
        ) : null}
      </div>
      <div className="border-t border-line px-3 py-1.5 text-2xs text-clay">
        <kbd>g</kbd> <kbd>l</kbd> goes to any label or Topic
      </div>
    </nav>
  )
}

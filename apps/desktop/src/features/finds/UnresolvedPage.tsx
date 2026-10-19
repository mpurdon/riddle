/**
 * The Unresolved tray (README "Unresolved"): mail Riddle could not decide,
 * never hidden, with Decide controls that record a correction.
 *
 * From the UX study: `f` opens the type picker instead of assuming Respond;
 * `x` selects and the Decide buttons name their target ("Filed 12
 * selected"); three or more from one address form a cluster with a
 * "Decide all as…" header, because a burst from a notifier was the common
 * case and it took one keypress per message; and every decision says what
 * it taught and how many from that sender are still waiting.
 *
 * Every row shows why it is held, and the header groups the tray by that
 * reason ("3 held: an alert no rule mentions · 1: …"). The reason was
 * visible for the cursor row alone, so a tray of forty read as forty
 * unknowns. A row carries no reason of its own, so each row's explanation
 * is read once, under the key the pane uses for the cursor row.
 */
import { NeverHideButton } from '@/components/NeverHideButton'
import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { ListChecks } from 'lucide-react'
import type { Disposition } from '@/bindings/Disposition'
import type { FindType } from '@/bindings/FindType'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { explain, qk } from '@/lib/api'
import { cx, fmtAddress, fmtTime, num } from '@/lib/format'
import { useLevelTip } from '@/lib/level'
import { useDigContext } from '@/lib/queries'
import { dispositionLabel, findTypeLabel, FIND_TYPES, FOLLOW_UP } from '@/lib/selectors'
import { keyHint } from '@/lib/shortcuts'
import type { MessageFilter } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { useListNav } from '@/lib/useListNav'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { EmptyState } from '@/components/ui/EmptyState'
import { Menu, type MenuItem } from '@/components/ui/Menu'
import { Note } from '@/components/ui/Note'
import { ActionHelpButton } from '@/components/ActionHelp'
import { ErrorCard } from '@/components/ErrorCard'
import { ExplanationCard, FollowUpMenu, PageHeader, StrataChips } from '@/components/Bits'
import { FindTypeOverlay, FollowUpOverlay } from '@/components/OverlayList'
import { senderOf } from '@/components/ThreadRow'
import { openFromList } from '@/features/messages/queue'
import { usePagedMessages } from '@/features/messages/usePagedMessages'
import { senderKey } from '@/features/rules/yourCorrections'
import { decideOutcome, groupBySender, reasonGroups, reasonLine, remainingFromSender, targetLabel, teachingLine } from './model'
import { SelectionBar } from './SelectionBar'
import { useBulkRun } from './useBulkRun'

const UNRESOLVED: MessageFilter = { disposition: 'unresolved' }

export default function UnresolvedPage() {
  const { digId, digName } = useDigContext()
  const nav = useNavigate()
  const { run } = useAction()
  const q = usePagedMessages({ digId, filter: UNRESOLVED, limit: 200 })
  const groups = useMemo(() => groupBySender(q.items), [q.items])
  const ordered = useMemo(() => groups.flatMap((g) => g.items), [groups])
  const indexOf = useMemo(() => new Map(ordered.map((m, i) => [m.id, i])), [ordered])
  /** The message page gets the tray as its queue, so n/p step through it and it can say "3 of 41". */
  const open = (m: MessageSummary) => openFromList(nav, ordered, m)
  const list = useListNav(ordered, { onOpen: open })
  const current = list.current
  const { runBulk, bulkError } = useBulkRun(list)
  const ex = useQuery({ queryKey: current ? qk.explain(current.digId, current.id) : ['explain', 'none'], queryFn: () => explain(current!.digId, current!.id), enabled: !!current })
  /** Each row's reason rides on the row (`heldReason`); null when the decision gave none. The tray once asked `explain` for every one of its 200 rows. */
  const reasonOf = (m: MessageSummary) => m.heldReason?.trim() || null
  const heldLine = reasonLine(reasonGroups(ordered, reasonOf))
  const [neverHiddenBusy, setNeverHiddenBusy] = useState(false)
  /** The messages a keyboard `f` or a cluster's "Find…" is choosing a type for. */
  const [typePicker, setTypePicker] = useState<MessageSummary[] | null>(null)
  /** The messages a keyboard `⇧F` is choosing a follow-up date for. */
  const [datePicker, setDatePicker] = useState<MessageSummary[] | null>(null)
  const [taught, setTaught] = useState<string | null>(null)

  const count = useMemo(() => ordered.filter((m) => list.selected.has(m.id)).length, [ordered, list.selected])
  const { targets } = list

  const args = (m: MessageSummary, d: Disposition, dueAt: string | null = null) => ({ digId: m.digId, messageId: m.id, disposition: d, findType: d.kind === 'find' ? d.find_type : undefined, ...(dueAt ? { dueAt } : {}) })
  // Every decision here is a correction; at Essentials the fifth earns the one tip that the rest exists.
  const levelTip = useLevelTip()
  /** `dueAt` rides on a Follow Up decision: the Find opens due then. */
  const decide = async (ms: MessageSummary[], d: Disposition, dueAt: string | null = null) => {
    if (!ms.length) return
    levelTip('correction')
    const label = dispositionLabel(d)
    const senders = new Set(ms.map((m) => senderKey(m.from.email)))
    const line = teachingLine(label, remainingFromSender(ordered, ms), senders.size === 1 ? ms[0].from.email : `these ${senders.size} senders`)
    const ran = await runBulk(ms, {
      action: 'set_disposition',
      args: (m) => args(m, d, dueAt),
      digId: (m) => m.digId,
      outcome: (done, failed) => decideOutcome(label, done, failed),
      lead: line,
      failure: (failed, total) => `${failed} of ${total} could not be corrected; they are still Unresolved.`,
    })
    // A failure has been shown already; nothing recorded, nothing taught.
    if (ran?.done) setTaught(line)
  }
  const neverHide = async (m: MessageSummary) => {
    setNeverHiddenBusy(true)
    try {
      await run('add_never_hidden', { entry: m.from.email, digId: m.digId, messageId: m.id })
    } finally {
      setNeverHiddenBusy(false)
    }
  }
  useShortcuts('review', {
    decide_find: () => {
      const t = targets()
      if (t.length) setTypePicker(t)
    },
    decide_follow_up: () => {
      const t = targets()
      if (t.length) setDatePicker(t)
    },
    decide_filed: () => decide(targets(), { kind: 'filed' }),
    decide_spoil: () => decide(targets(), { kind: 'spoil' }),
    decide_contamination: () => decide(targets(), { kind: 'contamination' }),
    never_hidden: () => current && neverHide(current),
  })

  const clusterMenu = (items: MessageSummary[]): (MenuItem | 'separator')[] => [
    { label: 'Filed', onSelect: () => decide(items, { kind: 'filed' }) },
    { label: 'Spoil', onSelect: () => decide(items, { kind: 'spoil' }) },
    { label: 'Contamination', danger: true, onSelect: () => decide(items, { kind: 'contamination' }) },
    'separator',
    { label: 'Find…', onSelect: () => setTypePicker(items) },
  ]

  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="flex min-h-0 flex-1">
      <section className="flex w-[460px] shrink-0 flex-col border-r border-line">
        <PageHeader title="Unresolved" subtitle={q.total !== null ? `${num(q.total)} Riddle could not decide. Deciding here is the fastest way to teach it.` : 'Loading…'}>
          {heldLine ? (
            <span className="text-xs text-ink-2 selectable" aria-label="Why they are held">
              {heldLine}
            </span>
          ) : null}
        </PageHeader>
        <SelectionBar count={count} error={bulkError} onClear={list.clear} />
        <div className="min-h-0 flex-1 overflow-auto" role="listbox" aria-multiselectable aria-label="Unresolved messages">
          {!q.isPending && !ordered.length ? (
            <EmptyState icon={<ListChecks />} title="Nothing is Unresolved." body="Attention Zero includes this tray: when it is empty and there are no open Finds, there is nothing to deal with. When a judge or LLM is unavailable, mail lands here rather than being hidden." />
          ) : null}
          {groups.map((g) => (
            <div key={g.cluster ? `cluster:${g.email}` : g.items[0].id}>
              {g.cluster ? (
                <div className="sticky top-0 z-10 flex items-center justify-between gap-2 border-b border-line/60 bg-sand px-4 py-1 text-2xs font-semibold text-clay">
                  <span className="truncate selectable" title={fmtAddress(g)}>
                    {g.items.length} from {g.email}
                  </span>
                  <Menu label={`Decide all ${g.items.length} from ${g.email} as`} items={clusterMenu(g.items)} trigger={<Button size="sm" variant="ghost" title={`Record the same decision for all ${g.items.length} from this sender`}>Decide all as…</Button>} />
                </div>
              ) : null}
              {g.items.map((m) => {
                const idx = indexOf.get(m.id) ?? 0
                const active = current?.id === m.id
                const checked = list.selected.has(m.id)
                const reason = reasonOf(m)
                return (
                  <div key={m.id} role="option" aria-selected={active} data-row-index={idx} tabIndex={-1} onClick={() => list.setCursor(idx)} className={cx('flex cursor-default items-start gap-2 border-b border-line/60 px-3 py-2', active ? 'row-selected' : checked ? 'bg-ochre/5' : 'hover:bg-sand/60')}>
                    <input type="checkbox" checked={checked} onChange={() => list.toggle(m.id)} onClick={(e) => e.stopPropagation()} aria-label={`Select ${m.subject}`} title={`Select or deselect (${keyHint('select')})`} className="mt-1 shrink-0" />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="w-32 truncate text-sm font-medium selectable">{senderOf(m)}</span>
                        <span className="min-w-0 flex-1 truncate text-sm selectable">{m.subject}</span>
                        <span className="text-xs text-clay">{fmtTime(m.date)}</span>
                      </div>
                      <div className="mt-0.5 flex items-center gap-2 text-xs text-ink-2">
                        <span className="truncate selectable">{m.snippet}</span>
                      </div>
                      {reason ? (
                        <div className="mt-0.5 truncate text-2xs text-ochre selectable" title={reason}>
                          {reason}
                        </div>
                      ) : null}
                      <div className="mt-1 text-2xs text-clay">
                        {digName(m.digId)} <StrataChips strata={m.strata} />
                      </div>
                    </div>
                  </div>
                )
              })}
            </div>
          ))}
          {q.hasNextPage ? (
            <div className="flex justify-center py-2">
              <Button size="sm" variant="ghost" busy={q.isFetchingNextPage} onClick={() => void q.fetchNextPage()} title="Fetch the next page of Unresolved messages">
                Load more
              </Button>
            </div>
          ) : null}
        </div>
      </section>
      <section className="relative flex min-w-0 flex-1 flex-col">
        {current ? (
          <>
            <PageHeader
              title={<span className="selectable">{current.subject}</span>}
              subtitle={<span className="selectable">{`${fmtAddress(current.from)} · ${digName(current.digId)} · ${fmtTime(current.date)}`}</span>}
              actions={
                <>
                  <Menu label="Decide: it is a Find" items={FIND_TYPES.map((t: FindType) => ({ label: `Find: ${findTypeLabel(t)}`, onSelect: () => decide(targets(), { kind: 'find', find_type: t }) }))} trigger={<Button size="sm" variant="primary" title={`Decide: a Find, and pick its type (${keyHint('decide_find')})`}>{targetLabel('Find', count)}…</Button>} />
                  <FollowUpMenu onPick={(dueAt) => decide(targets(), FOLLOW_UP, dueAt)} trigger={<Button size="sm" title={`Decide: follow up by a date, a Find until a reply arrives (${keyHint('decide_follow_up')})`}>{targetLabel('Follow up', count)}…</Button>} />
                  <Button size="sm" onClick={() => decide(targets(), { kind: 'filed' })} title={`Decide: Filed (${keyHint('decide_filed')})`}>
                    {targetLabel('Filed', count)}
                  </Button>
                  <Button size="sm" onClick={() => decide(targets(), { kind: 'spoil' })} title={`Decide: Spoil (${keyHint('decide_spoil')})`}>
                    {targetLabel('Spoil', count)}
                  </Button>
                  <Button size="sm" variant="danger" onClick={() => decide(targets(), { kind: 'contamination' })} title={`Decide: Contamination (${keyHint('decide_contamination')})`}>
                    {targetLabel('Contamination', count)}
                  </Button>
                  <NeverHideButton email={current.from.email} variant="ghost" disabledReason={neverHiddenBusy ? 'Working' : null} onAdd={() => neverHide(current)} title={`Never hide this sender (${keyHint('never_hidden')})`} />
                  <ActionHelpButton group="decide" digId={current.digId} />
                </>
              }
            />
            <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
              {taught ? (
                <Note tone="ok" className="mb-3 selectable">
                  {taught}
                </Note>
              ) : null}
              <p className="text-sm text-ink-2 selectable">Decide what this is. Your answer is recorded as a correction; when the same kind keeps coming up, the Director can show you the pattern and propose a Mesh change.</p>
              {ex.data ? (
                <div className="mt-4">
                  <ExplanationCard ex={ex.data} digId={current.digId} subject={current.subject} from={senderOf(current)} digName={digName(current.digId)} />
                </div>
              ) : null}
              {ex.error ? (
                <div className="mt-4">
                  <ErrorCard error={ex.error} compact />
                </div>
              ) : null}
              <div className="mt-4">
                <Button size="sm" variant="ghost" onClick={() => open(current)} title={`Open the message on its own page (${keyHint('open')})`}>
                  Open the message
                </Button>
              </div>
            </div>
          </>
        ) : (
          <div className="flex min-h-0 flex-1 flex-col">
            {taught ? (
              <Note tone="ok" className="m-5 mb-0 selectable">
                {taught}
              </Note>
            ) : null}
            <EmptyState
              title="Select a message"
              body={`${keyHint('decide_find')} decides it is a Find and asks the type; ${keyHint('decide_follow_up')} a follow-up, and asks the date; ${keyHint('decide_filed')} Filed, ${keyHint('decide_spoil')} Spoil, ${keyHint('decide_contamination')} Contamination; ${keyHint('never_hidden')} never hides the sender. ${keyHint('select')} selects a row and ${keyHint('select_all')} every row, so one decision covers them all.`}
            />
          </div>
        )}
        {typePicker ? (
          <FindTypeOverlay
            title={typePicker.length === 1 ? 'Decide: a Find of type' : `Decide ${typePicker.length} as Finds of type`}
            onPick={(t) => {
              const items = typePicker
              setTypePicker(null)
              void decide(items, { kind: 'find', find_type: t })
            }}
            onClose={() => setTypePicker(null)}
          />
        ) : null}
        {datePicker ? (
          <FollowUpOverlay
            onPick={(dueAt) => {
              const items = datePicker
              setDatePicker(null)
              void decide(items, FOLLOW_UP, dueAt)
            }}
            onClose={() => setDatePicker(null)}
          />
        ) : null}
      </section>
    </div>
  )
}

/**
 * Spoil and Contamination review (README "Spoil", "Contamination"): bulk
 * select, Not Spoil / Restore through a scoped sheet under the toolbar (this
 * message, every loaded message from the sender, or the sender's domain),
 * Never Hidden quick-add that records the correction on the row it came
 * from, and for Contamination a move to Gmail's Trash behind the
 * type-the-count tier. A click previews the row beside the list with
 * Riddle's verdict under the summary, so "was it right?" can be answered
 * without opening anything. The Inbox keys (e, #, !, l, s, w) work here
 * too; n stays Not Spoil.
 *
 * Spoil can also be read by sender (Flow 5): one row per address with the
 * count, opened inline. The second study watched a reader page through
 * fourteen GitHub notifications to find one misfiled invoice, and restore
 * the same newsletter every week because "Not Spoil" taught nothing that
 * lasted. So the sender and domain scopes, and the sender row's own
 * button, restore what is here and add a rule to the Your corrections
 * Mesh in the same breath, and one toast says both. Contamination stays
 * flat: it is rarely many from one sender, and reading it by sender would
 * invite trusting one.
 */
import { NeverHideButton } from '@/components/NeverHideButton'
import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { AlertTriangle, Trash2, Undo2 } from 'lucide-react'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { cx, num, plural } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { keyHint, type ShortcutScope } from '@/lib/shortcuts'
import { toast } from '@/lib/stores'
import { useAction, useLastUndoable } from '@/lib/useAction'
import { useListNav } from '@/lib/useListNav'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { EmptyState } from '@/components/ui/EmptyState'
import { Note } from '@/components/ui/Note'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader, StrataPicker } from '@/components/Bits'
import { ActionHelpButton } from '@/components/ActionHelp'
import { SnoozeOverlay } from '@/components/OverlayList'
import { targetLabel } from '@/features/finds/model'
import { domainOf, guardLine, neverLabel, senderKey, type CorrectionKind, type CorrectionScope as RuleScope } from '@/features/rules/yourCorrections'
import { CorrectionSheet, type CorrectionScope } from './CorrectionSheet'
import { MessageList } from './MessageList'
import { PreviewAside, scrollToPreviewWhy } from './MessagePreview'
import { SenderList } from './SenderList'
import { openFromList } from './queue'
import { runPerDig } from './runPerDig'
import { readSpoilView, selectedMessages, senderGroups, senderRows, writeSpoilView, type SenderGroup, type SenderRow, type SpoilView } from './senderGroups'
import { useCorrectionRule } from './useCorrectionRule'
import { usePagedMessages } from './usePagedMessages'

const SCOPES: ShortcutScope[] = ['messages', 'review']

/** A row the cursor can stand on: a message in the flat view, a sender or a message by sender. */
type NavRow = MessageSummary | SenderRow

/** The one sender the targets share, so Never hide can show their standing; null for several. */
const oneSender = (ms: MessageSummary[]): string | null => {
  const senders = new Set(ms.map((m) => senderKey(m.from.email)))
  return senders.size === 1 ? ms[0].from.email : null
}

export function ReviewPage({ kind }: { kind: 'spoil' | 'contamination' }) {
  const nav = useNavigate()
  const { digId, digName } = useDigContext()
  const actions = useAction()
  const { run } = actions
  const { addRule } = useCorrectionRule()
  const isSpoil = kind === 'spoil'
  const never: CorrectionKind = isSpoil ? 'spoil' : 'contamination'
  const [view, setView] = useState<SpoilView>(() => (isSpoil ? readSpoilView() : 'messages'))
  const bySender = isSpoil && view === 'sender'
  // Mail a newer one of its series superseded (a store's earlier sales) sinks out of Spoil unless asked for.
  const [showSuperseded, setShowSuperseded] = useState(false)
  const q = usePagedMessages({ digId, filter: { disposition: kind, superseded: isSpoil && !showSuperseded ? false : undefined } })
  const items = q.items
  const groups = useMemo(() => senderGroups(items), [items])
  const [expanded, setExpanded] = useState<Set<string>>(new Set())
  const rows = useMemo(() => senderRows(groups, expanded), [groups, expanded])
  /** The order n/p step through on the message page: by sender when that is how the list reads. */
  const ordered = useMemo(() => (bySender ? groups.flatMap((g) => g.items) : items), [bySender, groups, items])
  const open = (m: MessageSummary, suffix = '') => openFromList(nav, ordered, m, suffix)
  const navRows: NavRow[] = bySender ? rows : items
  const list = useListNav<NavRow>(navRows, {
    onOpen: (r) => {
      if (!('kind' in r)) open(r)
      else if (r.kind === 'message') open(r.m)
      else toggleExpand(r.group)
    },
  })
  const row: SenderRow | null = bySender ? (rows[list.cursor] ?? null) : null
  /** The message the preview and the message-scoped verbs act on: on a sender row, its newest. */
  const cur: MessageSummary | null = bySender ? (row ? (row.kind === 'sender' ? row.group.newest : row.m) : null) : (items[list.cursor] ?? null)
  const curGroup: SenderGroup | null = bySender ? (row?.group ?? null) : cur ? (groups.find((g) => g.key === senderKey(cur.from.email)) ?? null) : null
  const [busy, setBusy] = useState(false)
  const [sheetOpen, setSheetOpen] = useState(false)
  const [snoozeOpen, setSnoozeOpen] = useState(false)
  const [strataOpen, setStrataOpen] = useState(false)
  const toggleExpand = (g: SenderGroup) => {
    const closing = expanded.has(g.key)
    setExpanded((s) => {
      const n = new Set(s)
      if (n.has(g.key)) n.delete(g.key)
      else n.add(g.key)
      return n
    })
    // Closing a sender from one of its messages lands on the sender row rather than whatever slides up.
    if (closing && row?.kind === 'message' && row.group.key === g.key) list.setCursor(rows.findIndex((r) => r.kind === 'sender' && r.group.key === g.key))
  }
  const switchView = (v: SpoilView) => {
    setView(v)
    writeSpoilView(v)
    list.clear()
    list.setCursor(0)
    setSheetOpen(false)
  }
  const targets = (): MessageSummary[] => {
    if (list.selected.size) return bySender ? selectedMessages(groups, list.selected) : items.filter((m) => list.selected.has(m.id))
    if (bySender) return row ? (row.kind === 'sender' ? row.group.items : [row.m]) : []
    return cur ? [cur] : []
  }
  const restore = async (ms: MessageSummary[]) => {
    if (!ms.length) return
    setBusy(true)
    try {
      await runPerDig(actions, ms, 'restore_messages')
      list.clear()
      setSheetOpen(false)
    } finally {
      setBusy(false)
    }
  }
  /**
   * Restore every loaded message from a sender or a domain and add the
   * matching rule, then say both in one toast whose Undo reverses both.
   * The rule is added after the restore: if it fails, the mail is still
   * back and the error toast says why the rule is not.
   */
  const correctWithRule = async (ms: MessageSummary[], scope: RuleScope, value: string) => {
    if (!ms.length) return
    setBusy(true)
    try {
      const undoIds = await runPerDig(actions, ms, 'restore_messages', {}, { successToast: false })
      let ruleId: string | null = null
      let ruleFailed = false
      try {
        ruleId = await addRule(guardLine(scope, value, never))
      } catch {
        ruleFailed = true
      }
      if (ruleId) undoIds.unshift(ruleId)
      if (undoIds.length) useLastUndoable.getState().set(undoIds[undoIds.length - 1])
      list.clear()
      setSheetOpen(false)
      const body = ruleFailed ? 'The rule could not be added; the messages are restored.' : `Rule ${ruleId ? 'added' : 'already in place'}: never ${neverLabel(never)} from ${value}. Edit it in Settings → Mail rules.`
      toast(`Restored ${ms.length} from ${value}`, {
        kind: 'success',
        body,
        action: undoIds.length
          ? {
              label: 'Undo',
              onClick: async () => {
                for (const id of undoIds) await run('undo_activity', { activityId: id }, { successToast: false })
                useLastUndoable.getState().set(null)
                toast('Undone', { kind: 'success' })
              },
            }
          : undefined,
      })
    } finally {
      setBusy(false)
    }
  }
  const inDomain = (domain: string) => items.filter((m) => domainOf(m.from.email) === domain)
  const confirmCorrection = (scope: CorrectionScope) => {
    if (scope === 'selection') return restore(targets())
    if (!cur || !curGroup) return
    if (scope === 'message') return restore([cur])
    if (scope === 'sender') return correctWithRule(curGroup.items, 'sender', curGroup.key)
    const domain = domainOf(curGroup.email)
    return correctWithRule(inDomain(domain), 'domain', domain)
  }
  /**
   * Never Hidden per distinct sender; each call names the first message from
   * that sender so the backend records the correction on it. Then restore,
   * since nothing from a Never Hidden sender belongs here.
   */
  const neverHide = async (ms: MessageSummary[]) => {
    if (!ms.length) return
    const firstBySender = new Map<string, MessageSummary>()
    for (const m of ms) {
      const k = senderKey(m.from.email)
      if (!firstBySender.has(k)) firstBySender.set(k, m)
    }
    setBusy(true)
    try {
      for (const [entry, m] of firstBySender) await run('add_never_hidden', { entry, digId: m.digId, messageId: m.id })
      await runPerDig(actions, ms, 'restore_messages')
      list.clear()
      setSheetOpen(false)
    } finally {
      setBusy(false)
    }
  }
  const trash = async () => {
    const ms = targets()
    if (!ms.length) return
    await runPerDig(actions, ms, 'permanent_delete')
    list.clear()
  }
  const bulk = async (action: string, extra: Record<string, unknown> = {}) => {
    await runPerDig(actions, targets(), action, extra)
    list.clear()
  }
  const showWhy = () => scrollToPreviewWhy(() => cur && open(cur, '#why'))
  useShortcuts(SCOPES, {
    not_spoil: () => targets().length && setSheetOpen(true),
    never_hidden: () => void neverHide(targets()),
    archive: () => void bulk('archive_messages'),
    // The key that names this page's own disposition stays unbound here.
    spoil: isSpoil ? undefined : () => void bulk('move_to_spoil'),
    contamination: isSpoil ? () => void bulk('mark_contamination') : undefined,
    label: () => targets().length && setStrataOpen(true),
    snooze_message: () => targets().length && setSnoozeOpen(true),
    why_message: showWhy,
  })
  const verb = isSpoil ? 'Not Spoil' : 'Restore'
  const count = list.selected.size
  const name = (v: string) => targetLabel(v, count, true)
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  const emptyState = (
    <EmptyState
      icon={isSpoil ? <Trash2 /> : <AlertTriangle />}
      title={isSpoil ? 'Spoil is empty.' : 'No Contamination.'}
      body={isSpoil ? 'Nothing has been moved to Spoil for this account yet. During the Shadow Period, Riddle only proposes; see the Summary for what it would have done.' : 'No mail failed the evidence checks: Gmail spam verdict, authentication, lookalike domains, first-time senders asking for money or credentials.'}
    />
  )
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={isSpoil ? 'Spoil' : 'Contamination'}
        subtitle={
          q.total !== null
            ? `${num(q.total)} messages${bySender ? ` from ${plural(groups.length, 'sender')} loaded` : ''} · ${digName(digId)} · ${isSpoil ? 'legitimate, low value; not deleted, still searchable' : 'untrusted; decided by evidence, never read by the Director'}`
            : 'Loading…'
        }
        actions={
          <>
            <Button size="sm" icon={<Undo2 />} busy={busy} onClick={() => setSheetOpen(true)} disabledReason={items.length ? null : 'Nothing to restore'} title={`${verb}: choose this message, the whole sender or its domain (${keyHint('not_spoil')})`}>
              {name(verb)}
            </Button>
            <NeverHideButton email={oneSender(targets())} onAdd={() => void neverHide(targets())} disabledReason={busy ? 'Working' : items.length ? null : 'Nothing selected'} title={`Never hide this sender, and restore what is here from them (${keyHint('never_hidden')})`}>
              Never hide sender
            </NeverHideButton>
            {!isSpoil ? (
              <Button size="sm" variant="danger" icon={<Trash2 />} onClick={() => void trash()} disabledReason={items.length ? null : 'Nothing to move'} title="Riddle forgets them; Gmail empties its Trash after 30 days. You confirm by typing the count.">
                {count ? `Move ${count} selected to Gmail's Trash…` : "Move to Gmail's Trash (cursor)…"}
              </Button>
            ) : null}
            <ActionHelpButton group="review" digId={digId} />
          </>
        }
      >
        {isSpoil ? (
          <>
            <ViewToggle value={view} onChange={switchView} />
            <label className="flex items-center gap-1.5 text-xs text-ink-2" title="Older mail a newer one from the same sender replaced, under a Mesh route with + Replaces">
              <input type="checkbox" checked={showSuperseded} onChange={(e) => setShowSuperseded(e.target.checked)} />
              Show superseded
            </label>
            <span className="text-xs text-ink-2">Spoil is not deleted and stays searchable; look through it now and then for anything misfiled. "Not Spoil" records a correction, and for a sender or a domain it adds a rule you can edit in Settings → Mail rules.</span>
          </>
        ) : (
          <span className="text-xs text-ink-2">Unimportant and untrustworthy are different. Restoring records a correction; Never hide makes the sender exempt from Spoil and Contamination for good.</span>
        )}
      </PageHeader>
      {sheetOpen && cur && curGroup ? (
        <CorrectionSheet
          target={{
            verb,
            selectedCount: count,
            senderName: curGroup.name ?? curGroup.email,
            senderEmail: curGroup.email,
            senderCount: curGroup.items.length,
            domain: domainOf(curGroup.email),
            domainCount: inDomain(domainOf(curGroup.email)).length,
            never,
            withMessage: !bySender || row?.kind === 'message',
          }}
          busy={busy}
          onConfirm={(scope) => void confirmCorrection(scope)}
          onNeverHide={() => void neverHide(count ? targets() : curGroup.items)}
          onClose={() => setSheetOpen(false)}
        />
      ) : null}
      {!isSpoil && items.length ? (
        <Note tone="warn" className="mx-4 mt-3">
          <span className="flex items-center gap-2">
            <AlertTriangle className="size-4 shrink-0" aria-hidden /> Mail that argues for its own importance is treated as evidence against it. Check the sender and authentication before restoring.
          </span>
        </Note>
      ) : null}
      <div className="flex min-h-0 flex-1">
        <section className="flex min-w-0 flex-1 flex-col">
          {bySender ? (
            <SenderList
              rows={rows}
              cursor={list.cursor}
              selected={list.selected}
              expanded={expanded}
              onCursor={list.setCursor}
              onToggle={list.toggle}
              onToggleExpand={toggleExpand}
              onOpen={(m) => open(m)}
              digName={digName}
              verb={verb}
              onCorrectSender={(g) => void correctWithRule(g.items, 'sender', g.key)}
              onNeverHide={(g) => void neverHide(g.items)}
              busy={busy}
              hasMore={!!q.hasNextPage}
              loadingMore={q.isFetchingNextPage}
              onLoadMore={() => void q.fetchNextPage()}
              loading={q.isLoading}
              emptyState={emptyState}
            />
          ) : (
            <MessageList
              items={items}
              cursor={list.cursor}
              selected={list.selected}
              onCursor={list.setCursor}
              onToggle={list.toggle}
              onOpen={(m) => open(m)}
              digName={digName}
              hasMore={!!q.hasNextPage}
              loadingMore={q.isFetchingNextPage}
              onLoadMore={() => void q.fetchNextPage()}
              loading={q.isLoading}
              showDisposition={false}
              emptyState={emptyState}
            />
          )}
        </section>
        {cur ? <PreviewAside row={cur} digName={digName(cur.digId)} verdictFirst onOpen={() => open(cur)} /> : null}
      </div>
      {snoozeOpen ? (
        <SnoozeOverlay
          onPick={(until) => {
            setSnoozeOpen(false)
            void bulk('snooze_messages', { until })
          }}
          onClose={() => setSnoozeOpen(false)}
        />
      ) : null}
      <StrataPicker open={strataOpen} onClose={() => setStrataOpen(false)} onPick={(names) => void bulk('label_messages', { strata: names })} />
    </div>
  )
}

/** "Messages · By sender": a segmented toggle, remembered per machine. */
function ViewToggle({ value, onChange }: { value: SpoilView; onChange: (v: SpoilView) => void }) {
  const option = (v: SpoilView, label: string, title: string) => (
    <button type="button" role="radio" aria-checked={value === v} title={title} onClick={() => onChange(v)} className={cx('h-5 rounded px-2 text-xs', value === v ? 'bg-paper text-ink shadow-sm' : 'text-ink-2 hover:text-ink')}>
      {label}
    </button>
  )
  return (
    <div role="radiogroup" aria-label="View" className="inline-flex items-center gap-0.5 rounded-md border border-line bg-sand p-0.5">
      {option('messages', 'Messages', 'One row per message, newest first')}
      {option('sender', 'By sender', 'One row per sender with the count; Enter opens their messages')}
    </div>
  )
}

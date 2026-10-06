/**
 * The chronological inbox (README: it still exists; it is not the centre).
 * Filters by account, disposition, Stratum, label, sender and date; keyboard
 * triage j/k/x/e/#/s/l; bulk actions through the confirmation tiers. A
 * click previews the row beside the list and Enter opens it, the model
 * Finds already used: the audit found rows here opened only on a double
 * click, with no way to read a message without leaving the list. Opening a
 * row carries the list in router state so the message page can step with
 * n/p.
 *
 * With `?shadow=1` (the Summary's "Review what Riddle would have done")
 * the same list is a review list (Flow 7): each row says what Riddle would
 * have done and takes Agree or Disagree…, the header counts what is
 * reviewed, and the real-action toolbar and keys are gone. The audit found
 * Archive and Spoil acting for real on a list that promised nothing was
 * applied. The rows are the proposals themselves (`shadowOnly`), across
 * whatever accounts are in scope, and each carries the reader's review:
 * the list used to scope itself to the accounts in their Shadow Period and
 * fetch the marks per account, and a row without a proposal could slip in.
 */
import { useMemo, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { Eye, Inbox, Search } from 'lucide-react'
import type { Disposition } from '@/bindings/Disposition'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { num } from '@/lib/format'
import { toRiddleError } from '@/lib/ipc'
import { useDigContext, useStrata } from '@/lib/queries'
import { errorAffordance } from '@/lib/selectors'
import { keyHint, type ShortcutScope } from '@/lib/shortcuts'
import type { DispositionFilter, MessageFilter } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { EmptyState } from '@/components/ui/EmptyState'
import { Input } from '@/components/ui/Field'
import { Select } from '@/components/ui/Select'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'
import { MessageList } from './MessageList'
import { PreviewAside } from './MessagePreview'
import { CHOOSER_HEIGHT, CHOOSER_TYPES_HEIGHT, DisagreeChooser, ROW_ERROR_HEIGHT, RowError, ShadowControls } from './ShadowReview'
import { FILTERABLE_LABELS, labelName } from './labels'
import { MessageOverlays, MessageToolbar, TriageError, useMessageTriage } from './useMessageTriage'
import { usePagedMessages } from './usePagedMessages'

const DISPOSITIONS: { value: DispositionFilter | 'all' | 'unsifted'; label: string }[] = [
  { value: 'all', label: 'Any disposition' },
  { value: 'find', label: 'Find' },
  { value: 'filed', label: 'Filed' },
  { value: 'spoil', label: 'Spoil' },
  { value: 'contamination', label: 'Contamination' },
  { value: 'unresolved', label: 'Unresolved' },
  { value: 'unsifted', label: 'Not yet sifted' },
]

const LABEL_OPTIONS = [{ value: 'any', label: 'Any label' }, ...FILTERABLE_LABELS.map((id) => ({ value: id, label: labelName(id) }))]

const INBOX_SCOPES: ShortcutScope[] = ['messages']
const SHADOW_SCOPES: ShortcutScope[] = ['messages', 'review']

/** The Disagree chooser under a row, and whether its Find types are open. */
interface Chooser {
  id: string
  types: boolean
}

export default function InboxPage() {
  const nav = useNavigate()
  const [params, setParams] = useSearchParams()
  const { digId, digName } = useDigContext()
  const strata = useStrata()
  const { run } = useAction()
  const query = params.get('q') ?? ''
  const disposition = (params.get('disposition') as DispositionFilter | 'all' | 'unsifted' | null) ?? 'all'
  const stratum = params.get('stratum') ?? ''
  const label = params.get('label') ?? ''
  const from = params.get('from') ?? ''
  const since = params.get('since') ?? ''
  const until = params.get('until') ?? ''
  const shadow = params.get('shadow') === '1'
  const set = (k: string, v: string) => {
    const next = new URLSearchParams(params)
    if (v) next.set(k, v)
    else next.delete(k)
    setParams(next, { replace: true })
  }
  const filter: MessageFilter = useMemo(
    () => ({
      disposition: disposition === 'all' || disposition === 'unsifted' ? undefined : disposition,
      unclassifiedOnly: disposition === 'unsifted' || undefined,
      stratum: stratum || undefined,
      label: label || undefined,
      from: from || undefined,
      since: since ? new Date(since).toISOString() : undefined,
      until: until ? new Date(`${until}T23:59:59`).toISOString() : undefined,
      // The review list: only the proposals, shown as the proposal, whatever accounts are in scope.
      shadowOnly: shadow || undefined,
    }),
    [disposition, stratum, label, from, since, until, shadow],
  )
  const q = usePagedMessages({ digId, filter, query: query || null })
  const items = q.items
  const triage = useMessageTriage(items)
  const { list, open, showWhy } = triage

  // ---- The Shadow review: the chooser under a row, errors under a row. The mark is on the row itself.
  const [chooser, setChooser] = useState<Chooser | null>(null)
  const [rowErrors, setRowErrors] = useState<Record<string, string>>({})
  const [reviewing, setReviewing] = useState<string | null>(null)
  const clearRowError = (id: string) =>
    setRowErrors((e) => {
      const n = { ...e }
      delete n[id]
      return n
    })
  /** Agree, or disagree with a decision; the outcome shows on the row, and so does a failure. */
  const review = async (m: MessageSummary, agree: boolean, decision?: Disposition) => {
    setReviewing(m.id)
    clearRowError(m.id)
    try {
      await run('review_shadow', { digId: m.digId, messageId: m.id, agree, ...(decision ? { disposition: decision } : {}) }, { digId: m.digId, toastErrors: false })
      setChooser(null)
    } catch (e) {
      const a = errorAffordance(toRiddleError(e))
      setRowErrors((prev) => ({ ...prev, [m.id]: a.body || a.title }))
    } finally {
      setReviewing(null)
    }
  }
  const chooserRow = chooser ? items.find((m) => m.id === chooser.id) : undefined
  const pick = (d: Disposition) => chooserRow && void review(chooserRow, false, d)
  // The height is asked for every row on every layout; the element only for the row that shows it.
  const rowBelowHeight = (m: MessageSummary): number => (chooser?.id === m.id ? (chooser.types ? CHOOSER_TYPES_HEIGHT : CHOOSER_HEIGHT) : rowErrors[m.id] ? ROW_ERROR_HEIGHT : 0)
  const rowBelow = (m: MessageSummary) => {
    if (chooser?.id === m.id) return <DisagreeChooser m={m} busy={reviewing === m.id} typesOpen={chooser.types} onTypes={() => setChooser({ id: m.id, types: true })} onPick={pick} onClose={() => setChooser(null)} />
    const err = rowErrors[m.id]
    return err ? <RowError text={err} onDismiss={() => clearRowError(m.id)} /> : null
  }
  const layoutKey = `${chooser?.id ?? ''}:${chooser?.types ? 1 : 0}:${Object.keys(rowErrors).join(',')}`
  const reviewable = (m: MessageSummary | null): m is MessageSummary => !!m && !m.shadowReview

  useShortcuts(
    shadow ? SHADOW_SCOPES : INBOX_SCOPES,
    shadow
      ? {
          why_message: showWhy,
          shadow_agree: () => reviewable(list.current) && void review(list.current, true),
          shadow_disagree: () => reviewable(list.current) && setChooser({ id: list.current.id, types: false }),
          // The decide keys pick for the open chooser only; without one they stay quiet here.
          decide_filed: chooser ? () => pick({ kind: 'filed' }) : undefined,
          decide_spoil: chooser ? () => pick({ kind: 'spoil' }) : undefined,
          decide_contamination: chooser ? () => pick({ kind: 'contamination' }) : undefined,
          decide_find: chooser ? () => setChooser({ id: chooser.id, types: true }) : undefined,
        }
      : triage.shortcuts,
  )
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  // Reviewed among the loaded rows, of the exact total; the count converges as pages load.
  const reviewed = items.filter((m) => m.shadowReview).length
  const total = q.total ?? items.length
  const filtered = !!(query || stratum || label || from || since || until || disposition !== 'all')
  return (
    <div className="relative flex min-h-0 flex-1 flex-col">
      <PageHeader
        title={query ? `Search: ${query}` : shadow ? 'Shadow review' : 'Inbox'}
        subtitle={
          q.total !== null
            ? shadow
              ? `${num(reviewed)} of ${num(total)} reviewed · ${digName(digId)} · what Riddle would have done; nothing applied`
              : `${num(q.total)} messages · ${digName(digId)}`
            : 'Loading…'
        }
        actions={
          shadow ? (
            <span className="text-xs text-ink-2">
              {keyHint('shadow_agree')} agrees, {keyHint('shadow_disagree')} disagrees on the cursor row
            </span>
          ) : triage.targets().length ? (
            <MessageToolbar triage={triage} digId={digId} />
          ) : null
        }
      >
        <div className="relative">
          <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-clay" aria-hidden />
          <Input defaultValue={query} key={query} placeholder="Search (from:, is:, stratum:, label:)" aria-label="Search messages" className="h-6 w-64 pl-7 text-xs" onKeyDown={(e) => e.key === 'Enter' && set('q', e.currentTarget.value)} />
        </div>
        <Select label="Disposition" size="sm" value={disposition} onChange={(v) => set('disposition', v === 'all' ? '' : v)} options={DISPOSITIONS} />
        <Select label="Stratum" size="sm" value={stratum || 'any'} onChange={(v) => set('stratum', v === 'any' ? '' : v)} options={[{ value: 'any', label: 'Any Stratum' }, ...(strata.data ?? []).map((s) => ({ value: s.name, label: s.name }))]} />
        <Select label="Label" size="sm" value={label || 'any'} onChange={(v) => set('label', v === 'any' ? '' : v)} options={label && !LABEL_OPTIONS.some((o) => o.value === label) ? [...LABEL_OPTIONS, { value: label, label: labelName(label) }] : LABEL_OPTIONS} />
        <Input defaultValue={from} key={`from-${from}`} placeholder="Sender" aria-label="Filter by sender" className="h-6 w-36 text-xs" onKeyDown={(e) => e.key === 'Enter' && set('from', e.currentTarget.value)} />
        <Input type="date" value={since} aria-label="Since" className="h-6 w-36 text-xs" onChange={(e) => set('since', e.target.value)} />
        <Input type="date" value={until} aria-label="Until" className="h-6 w-36 text-xs" onChange={(e) => set('until', e.target.value)} />
        {filtered ? (
          <Button size="sm" variant="ghost" onClick={() => setParams(new URLSearchParams(shadow ? { shadow: '1' } : {}), { replace: true })}>
            Clear filters
          </Button>
        ) : null}
      </PageHeader>
      <TriageError triage={triage} />
      <div className="flex min-h-0 flex-1">
        <section className="flex min-w-0 flex-1 flex-col">
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
            rowTrailing={shadow ? (m) => <ShadowControls m={m} busy={reviewing === m.id} onAgree={() => void review(m, true)} onDisagree={() => setChooser({ id: m.id, types: false })} /> : undefined}
            rowBelow={shadow ? rowBelow : undefined}
            rowBelowHeight={shadow ? rowBelowHeight : undefined}
            layoutKey={shadow ? layoutKey : undefined}
            emptyState={
              shadow ? (
                <EmptyState
                  icon={<Eye />}
                  title="Nothing to review."
                  body={filtered ? 'No proposal matches the filters.' : 'No account is in its Shadow Period, or nothing has been sifted yet. The Summary says when a Mesh has earned the right to act.'}
                  actions={<Button size="sm" onClick={() => nav('/')}>Open the Summary</Button>}
                />
              ) : (
                <EmptyState
                  icon={<Inbox />}
                  title={query ? 'No messages match.' : 'No messages here.'}
                  body={query ? 'Try a sender, a subject word, or from:, is:, stratum: and label: operators. Or ask the Director a question.' : digId ? 'This account has no messages matching the filters. If it was just connected, the Task Centre shows the initial sync.' : 'Connect an account in Settings to start the excavation.'}
                  actions={<Button size="sm" onClick={() => nav('/settings#digs')}>Open Settings</Button>}
                />
              )
            }
          />
        </section>
        {list.current ? <PreviewAside row={list.current} digName={digName(list.current.digId)} verdictFirst={shadow} onOpen={() => open(list.current!)} onFollowUp={shadow ? undefined : (dueAt) => void triage.followUp([list.current!], dueAt)} /> : null}
      </div>
      <MessageOverlays triage={triage} />
    </div>
  )
}

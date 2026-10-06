/**
 * The preview pane beside a message list: what the row is, then what
 * Riddle made of it. Until this, Inbox, Spoil and Contamination rows opened
 * only on Enter or a double click, so looking through Spoil meant a round
 * trip per message while Finds already previewed in place. On Spoil and
 * Contamination the verdict sits directly under the summary (`verdictFirst`),
 * because the question there is "was Riddle right?" and the reason is what
 * the reader checks against the mail.
 *
 * A held j/k lands on many rows in a second. The pane fetches only the
 * row the cursor rests on, and keeps the last message it read up, dimmed,
 * until the next arrives, rather than a fetch and a blank per row.
 *
 * The chip says "Would be …" when the verdict it shows is a Shadow
 * proposal: the explanation's own flag once it is this row's, the row's
 * until then. The page used to say it for the whole list.
 */
import { useEffect, useState, type ComponentProps } from 'react'
import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { ArrowUpRight, Flag, ShieldAlert } from 'lucide-react'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { explain, getMessage, qk } from '@/lib/api'
import { cx, fmtDateTime } from '@/lib/format'
import { keyHint } from '@/lib/shortcuts'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Note } from '@/components/ui/Note'
import { ErrorCard } from '@/components/ErrorCard'
import { MessageBody } from '@/components/MessageBody'
import { AttachmentChips } from '@/components/AttachmentChips'
import { AuthChip, DispositionChip, ExplanationCard, FollowUpMenu, StrataChips } from '@/components/Bits'
import { senderOf } from '@/components/ThreadRow'
import { labelName } from './labels'
import { Resizable } from '@/components/Resizable'

/** The element `w` scrolls to when the pane is visible. */
export const PREVIEW_WHY_ID = 'preview-why'

/** `w` on a list: scroll the pane's Why? into view when the pane is shown; below 1100px the page shows it instead, so `fallback` opens it there. */
export function scrollToPreviewWhy(fallback: () => void): void {
  const el = document.getElementById(PREVIEW_WHY_ID)
  if (el && el.offsetParent !== null) el.scrollIntoView({ block: 'start' })
  else fallback()
}

/** The pane beside a list, from 1100px wide; narrower, a row opens on its own page. */
export function PreviewAside(props: ComponentProps<typeof MessagePreview>) {
  return (
    <Resizable as="aside" id="preview" side="right" initial={560} min={360} max={960} label="Resize the preview" className="hidden border-l border-line min-[1100px]:flex min-[1100px]:flex-col" aria-label="Preview">
      <MessagePreview {...props} />
    </Resizable>
  )
}

/** How long the cursor rests on a row before the pane fetches it. */
const SETTLE_MS = 150

/** `value` once it has held for `ms`; the first value at once. */
function useSettled<T>(value: T, ms: number): T {
  const [settled, setSettled] = useState(value)
  useEffect(() => {
    if (value === settled) return
    const t = setTimeout(() => setSettled(value), ms)
    return () => clearTimeout(t)
  }, [value, settled, ms])
  return settled
}

export function MessagePreview({
  row,
  digName,
  onOpen,
  onFollowUp,
  verdictFirst,
}: {
  row: MessageSummary
  digName: string
  onOpen: () => void
  /** "Follow up by…" beside Open; absent on a list that applies nothing (the Shadow review). */
  onFollowUp?: (dueAt: string | null) => void
  verdictFirst?: boolean
}) {
  const live = useSettled(row.id, SETTLE_MS) === row.id
  const msg = useQuery({ queryKey: qk.message(row.digId, row.id), queryFn: () => getMessage(row.digId, row.id), enabled: live, placeholderData: keepPreviousData })
  const ex = useQuery({ queryKey: qk.explain(row.digId, row.id), queryFn: () => explain(row.digId, row.id), enabled: live, retry: false, placeholderData: keepPreviousData })
  const m = msg.data
  const sender = senderOf(row)
  // The header speaks for this row only: a placeholder is the last row's.
  const own = ex.isPlaceholderData ? undefined : ex.data
  const verdict = own?.disposition ?? row.disposition
  const proposed = own ? own.shadow : row.shadow
  const isContamination = verdict?.kind === 'contamination'
  const why = (
    <div id={PREVIEW_WHY_ID} className={cx(ex.isPlaceholderData && 'opacity-50')}>
      {ex.data ? (
        <ExplanationCard ex={ex.data} digId={row.digId} compact subject={row.subject} from={sender} digName={digName} />
      ) : ex.error ? (
        <Note tone="neutral" title="Not sifted yet">
          This message has no decision to explain yet. It is in the queue; the Task Centre shows progress.
        </Note>
      ) : null}
    </div>
  )
  const body = (
    <div className={cx(msg.isPlaceholderData && 'opacity-50')}>
      {msg.error ? <ErrorCard error={msg.error} compact onRetry={() => msg.refetch()} /> : null}
      {m ? <MessageBody key={m.id} html={m.bodyHtml} text={m.bodyText} maxHeight={520} /> : msg.isPending ? <div className="text-xs text-clay">Loading…</div> : null}
      {m ? <AttachmentChips attachments={m.attachments} className="mt-2" /> : null}
    </div>
  )
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="shrink-0 border-b border-line px-4 py-3">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0 selectable">
            <div className="truncate text-sm font-semibold">{row.subject || '(no subject)'}</div>
            <div className="mt-0.5 flex flex-wrap items-center gap-x-2 text-xs text-ink-2">
              <span className="font-medium text-ink">{sender}</span>
              {row.from.name ? <span className="text-clay">{row.from.email}</span> : null}
              <span className="text-clay">· {fmtDateTime(row.date)}</span>
              <span className="text-clay">· {digName}</span>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1">
            {onFollowUp ? <FollowUpMenu onPick={onFollowUp} trigger={<Button size="sm" variant="ghost" icon={<Flag />} title={`Follow up by a date: a Find until a reply arrives (${keyHint('follow_up')})`}>Follow up</Button>} /> : null}
            <Button size="sm" variant="ghost" icon={<ArrowUpRight />} onClick={onOpen} title={`Open the full message and its thread (${keyHint('open')})`}>
              Open
            </Button>
          </div>
        </div>
        <div className="mt-1.5 flex flex-wrap items-center gap-1.5">
          {ex.isPending ? (
            <Chip tone="neutral" title="Loading the decision">…</Chip>
          ) : (
            <DispositionChip d={verdict} shadow={proposed} />
          )}
          <StrataChips strata={row.strata} />
          {m && !msg.isPlaceholderData ? <AuthChip auth={m.headers.authentication} /> : null}
          {row.labels.length ? <span className="text-2xs text-clay selectable">{row.labels.map(labelName).join(', ')}</span> : null}
        </div>
      </div>
      <div className="min-h-0 flex-1 overflow-auto px-4 py-3">
        {isContamination ? (
          <Note tone="bad" className="mb-3">
            <span className="flex items-center gap-2">
              <ShieldAlert className="size-4 shrink-0" aria-hidden /> Contamination: links are neutralised and remote images blocked.
            </span>
          </Note>
        ) : null}
        {verdictFirst ? (
          <>
            {why}
            <div className="mt-4">{body}</div>
          </>
        ) : (
          <>
            {body}
            <div className="mt-4">{why}</div>
          </>
        )}
      </div>
    </div>
  )
}

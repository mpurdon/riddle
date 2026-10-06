/**
 * The triage a message list shares: open a row on its own page with the
 * list behind it, act on the checked rows or the cursor row (Archive,
 * Spoil, Stratum, Snooze, Follow up, Contamination), and show Why? in the
 * pane or on the page. The Inbox and the Priority inbox had each grown a
 * copy of the handlers, the toolbar and the overlays, and the copies had
 * drifted: the Inbox gained Follow up and Priority did not. The hook holds
 * the state and the handlers; `MessageToolbar`, `MessageOverlays` and
 * `TriageError` render them. The page binds the keys itself from
 * `shortcuts`, since the Inbox binds a different set while it is the
 * Shadow review.
 */
import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Archive, Clock, Flag, Layers, Trash2 } from 'lucide-react'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { fmtBy, plural } from '@/lib/format'
import { followUpArgs } from '@/lib/selectors'
import { keyHint } from '@/lib/shortcuts'
import { useAction } from '@/lib/useAction'
import { useListNav } from '@/lib/useListNav'
import type { Handlers } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { Note } from '@/components/ui/Note'
import { ActionHelpButton } from '@/components/ActionHelp'
import { FollowUpMenu, SnoozeMenu, StrataPicker } from '@/components/Bits'
import { FollowUpOverlay, SnoozeOverlay } from '@/components/OverlayList'
import { outcomeOf, targetLabel } from '@/features/finds/model'
import { useBulkRun } from '@/features/finds/useBulkRun'
import { scrollToPreviewWhy } from './MessagePreview'
import { openFromList } from './queue'
import { runPerDig } from './runPerDig'

export type MessageTriage = ReturnType<typeof useMessageTriage>

export function useMessageTriage(items: MessageSummary[]) {
  const nav = useNavigate()
  const actions = useAction()
  /** Open the full page, carrying the list so n/p work there. */
  const open = (m: MessageSummary, suffix = '') => openFromList(nav, items, m, suffix)
  const list = useListNav(items, { onOpen: (m) => open(m) })
  const { targets } = list
  /** One message verb over the targets, split by account; the selection clears once it has run. */
  const bulk = async (action: string, extra: Record<string, unknown> = {}) => {
    const ms = targets()
    if (!ms.length) return
    await runPerDig(actions, ms, action, extra)
    list.clear()
  }
  const [strataOpen, setStrataOpen] = useState(false)
  const [snoozeOpen, setSnoozeOpen] = useState(false)
  const [followUpOpen, setFollowUpOpen] = useState(false)
  const { runBulk, bulkError } = useBulkRun(list)
  /** Follow up by a date: one message runs as itself, with its toast and Undo; several run side by side with one toast. */
  const followUp = (ms: MessageSummary[], dueAt: string | null) =>
    runBulk(ms, {
      action: 'set_disposition',
      args: (m) => followUpArgs(m.digId, m.id, dueAt),
      digId: (m) => m.digId,
      outcome: (done, failed) => outcomeOf(`${failed ? `Marked ${done} of ${done + failed}` : `Marked ${plural(done, 'message')}`} for follow-up${dueAt ? ` by ${fmtBy(dueAt)}` : ''}`, failed, 'under the header'),
      failure: (failed, total) => `${failed} of ${total} could not be marked for follow-up.`,
    })
  const showWhy = () => scrollToPreviewWhy(() => list.current && open(list.current, '#why'))
  /** "Archive (cursor)" or "Archive 3 selected": the button says what it acts on. */
  const name = (verb: string) => targetLabel(verb, list.selected.size, true)
  const shortcuts: Handlers = {
    archive: () => void bulk('archive_messages'),
    spoil: () => void bulk('move_to_spoil'),
    contamination: () => void bulk('mark_contamination'),
    label: () => targets().length && setStrataOpen(true),
    snooze_message: () => targets().length && setSnoozeOpen(true),
    follow_up: () => targets().length && setFollowUpOpen(true),
    reply: () => list.current && open(list.current, '?reply=1'),
    forward: () => list.current && open(list.current, '?forward=1'),
    why_message: showWhy,
  }
  return {
    list,
    open,
    targets,
    bulk,
    followUp,
    showWhy,
    name,
    bulkError,
    shortcuts,
    overlays: { strataOpen, setStrataOpen, snoozeOpen, setSnoozeOpen, followUpOpen, setFollowUpOpen },
  }
}

/** The header's verbs, each naming its target. The page shows it only while there is a cursor row or a selection. */
export function MessageToolbar({ triage: t, digId }: { triage: MessageTriage; digId: string | null }) {
  return (
    <>
      <Button size="sm" icon={<Archive />} onClick={() => void t.bulk('archive_messages')} title={`Archive: record as Filed (${keyHint('archive')})`}>
        {t.name('Archive')}
      </Button>
      <Button size="sm" icon={<Trash2 />} onClick={() => void t.bulk('move_to_spoil')} title={`Move to Spoil (${keyHint('spoil')})`}>
        {t.name('Spoil')}
      </Button>
      <Button size="sm" icon={<Layers />} onClick={() => t.overlays.setStrataOpen(true)} title={`Add a Stratum (${keyHint('label')})`}>
        {t.name('Stratum')}
      </Button>
      <SnoozeMenu
        onPick={(until) => void t.bulk('snooze_messages', { until })}
        trigger={
          <Button size="sm" icon={<Clock />} title={`Snooze (${keyHint('snooze_message')})`}>
            {t.name('Snooze')}
          </Button>
        }
      />
      <FollowUpMenu
        label={t.name('Follow up by')}
        onPick={(dueAt) => void t.followUp(t.targets(), dueAt)}
        trigger={
          <Button size="sm" icon={<Flag />} title={`Follow up by a date: a Find until a reply arrives (${keyHint('follow_up')})`}>
            {t.name('Follow up')}
          </Button>
        }
      />
      <Button size="sm" variant="danger" onClick={() => void t.bulk('mark_contamination')} title={`Mark as Contamination (${keyHint('contamination')})`}>
        {t.name('Contamination')}
      </Button>
      <ActionHelpButton group="message" digId={digId} />
    </>
  )
}

/** The failure line under the header after a Follow up ran over several rows; the toast says to look here. */
export function TriageError({ triage: t }: { triage: MessageTriage }) {
  return t.bulkError ? (
    <Note tone="bad" className="mx-5 mt-2">
      {t.bulkError}
    </Note>
  ) : null
}

/** The keyboard pickers (s, ⇧F) and the Stratum dialog (l), over the page. */
export function MessageOverlays({ triage: t }: { triage: MessageTriage }) {
  const o = t.overlays
  return (
    <>
      {o.snoozeOpen ? (
        <SnoozeOverlay
          onPick={(until) => {
            o.setSnoozeOpen(false)
            void t.bulk('snooze_messages', { until })
          }}
          onClose={() => o.setSnoozeOpen(false)}
        />
      ) : null}
      {o.followUpOpen ? (
        <FollowUpOverlay
          onPick={(dueAt) => {
            o.setFollowUpOpen(false)
            void t.followUp(t.targets(), dueAt)
          }}
          onClose={() => o.setFollowUpOpen(false)}
        />
      ) : null}
      <StrataPicker open={o.strataOpen} onClose={() => o.setStrataOpen(false)} onPick={(names) => void t.bulk('label_messages', { strata: names })} />
    </>
  )
}

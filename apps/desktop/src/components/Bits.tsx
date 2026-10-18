/**
 * Small shared pieces: disposition chips, page headers, the snooze menu, a
 * Stratum picker, a "Why?" explanation card, and the dev overlay chips.
 */
import { useState, type ReactNode } from 'react'
import { useQuery } from '@tanstack/react-query'
import { useNavigate } from 'react-router-dom'
import { Check, Clock, Copy, Flag, HelpCircle, Layers } from 'lucide-react'
import type { Importance } from '@/bindings/Importance'
import type { Disposition } from '@/bindings/Disposition'
import type { ClauseCheck } from '@/bindings/ClauseCheck'
import type { Explanation } from '@/bindings/Explanation'
import type { RouteOutcome } from '@/bindings/RouteOutcome'
import type { FindType } from '@/bindings/FindType'
import { listStrata, qk } from '@/lib/api'
import { cx, fmtScore, plural } from '@/lib/format'
import { directorAbout, dispositionLabel, dispositionTone, findTypeLabel, FIND_TYPES, importanceLabel } from '@/lib/selectors'
import { toast, useDirector, useUi } from '@/lib/stores'
import { useSettings } from '@/lib/queries'
import { YOUR_CORRECTIONS_MESH } from '@/features/rules/yourCorrections'
import { Button } from './ui/Button'
import { Chip } from './ui/Chip'
import { Dialog } from './ui/Dialog'
import { Menu, type MenuItem } from './ui/Menu'
import { Input } from './ui/Field'
import { Tip } from './ui/Tooltip'

export function DispositionChip({ d, shadow }: { d: Disposition | null | undefined; shadow?: boolean }) {
  return (
    <Chip tone={dispositionTone(d)} title={shadow ? 'Shadow Period: proposed, not applied' : undefined}>
      {shadow ? 'Would be ' : ''}
      {dispositionLabel(d)}
    </Chip>
  )
}

/**
 * High and Critical as a chip; Normal and Low say nothing, so a list only
 * marks what stands out. `yours` says the level is the user's own.
 */
export function ImportanceChip({ level, yours }: { level: Importance; yours?: boolean }) {
  if (level !== 'high' && level !== 'critical') return null
  return (
    <Chip tone={level === 'critical' ? 'bad' : 'warn'} title={yours ? 'Importance you set for this message' : 'Importance from the rule that decided it'}>
      {importanceLabel(level)}
    </Chip>
  )
}

/** Copy text to the clipboard: `copied` is true for a moment after; a refusal says so. */
export function useCopy(text: string): [boolean, () => Promise<void>] {
  const [copied, setCopied] = useState(false)
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(true)
      setTimeout(() => setCopied(false), 1500)
    } catch {
      toast('Copy failed', { body: 'Clipboard access was refused.', kind: 'warn' })
    }
  }
  return [copied, copy]
}

/** A one-time sign-in code, large enough to read, with one click to copy. */
export function CopyCode({ code }: { code: string }) {
  const [copied, copy] = useCopy(code)
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation()
        void copy()
      }}
      aria-label={`Copy code ${code}`}
      title="Copy the sign-in code"
      className="inline-flex items-center gap-1.5 rounded border border-line bg-sand px-2 py-0.5 text-ink hover:bg-sand-2"
    >
      <span className="mono text-sm font-semibold tracking-wider">{code}</span>
      {copied ? <Check className="size-3.5 text-moss" aria-hidden /> : <Copy className="size-3.5 text-clay" aria-hidden />}
      <span className="sr-only" aria-live="polite">{copied ? 'Copied' : ''}</span>
    </button>
  )
}

/**
 * A link to one message, to paste in a note or another chat. The message
 * and Find pages showed the subject but never the id, so another session
 * reading the store had to search by subject for the mail being discussed.
 * The link is `riddle://message/<digId>/<messageId>`, the pair every read
 * takes (`get_message`, `explain`) and the Inbox route carries, with the
 * sender and subject so it reads on its own, like the Director's
 * `conversationLink`. The chip shows the provider's part of the id, enough
 * to tell two messages apart.
 */
export function messageLink(digId: string, messageId: string, about: string): string {
  return `riddle://message/${digId}/${messageId} (${about})`
}

export function CopyMessageLink({ digId, messageId, about }: { digId: string; messageId: string; about: string }) {
  const [copied, copy] = useCopy(messageLink(digId, messageId, about))
  const shown = messageId.slice(messageId.lastIndexOf(':') + 1)
  return (
    <button
      type="button"
      onClick={(e) => {
        e.stopPropagation()
        void copy()
      }}
      aria-label={copied ? 'Copied' : 'Copy message link'}
      title="Copy a link to this message (its id), to paste in a note or another chat"
      className="inline-flex items-center gap-1 rounded border border-line px-1.5 py-px text-2xs text-clay hover:bg-sand hover:text-ink"
    >
      <span className="mono">{shown}</span>
      {copied ? <Check className="size-3 text-moss" aria-hidden /> : <Copy className="size-3" aria-hidden />}
    </button>
  )
}

/**
 * One click copies the subject, beside the link: the link names the message
 * to a tool, the subject names it to a person, and a note wants one or the
 * other.
 */
export function CopySubject({ subject }: { subject: string }) {
  const [copied, copy] = useCopy(subject)
  return (
    <Button size="sm" variant="ghost" icon={copied ? <Check /> : <Copy />} onClick={() => void copy()} title="Copy the subject, to paste in a note or another chat">
      {copied ? 'Copied' : 'Copy subject'}
    </Button>
  )
}

/** A small separator dot between inline facts. */
export function Sep() {
  return (
    <span aria-hidden className="text-clay">
      ·
    </span>
  )
}

/** "auth failed" when SPF, DKIM or DMARC failed, with the three verdicts as the tooltip. One rule for the page and the preview. */
export function AuthChip({ auth }: { auth: { spf: string; dkim: string; dmarc: string } }) {
  const bad = auth.dmarc === 'fail' || auth.spf === 'fail' || auth.dkim === 'fail'
  if (!bad) return null
  return (
    <Chip tone="bad" title={`SPF ${auth.spf}, DKIM ${auth.dkim}, DMARC ${auth.dmarc}`}>
      auth failed
    </Chip>
  )
}

export function StrataChips({ strata, max = 3 }: { strata: string[]; max?: number }) {
  if (!strata.length) return null
  const shown = strata.slice(0, max)
  const rest = strata.length - shown.length
  return (
    <span className="inline-flex items-center gap-1">
      {shown.map((s) => (
        <Chip key={s} tone="neutral" className="font-normal">
          {s}
        </Chip>
      ))}
      {rest > 0 ? <span className="text-2xs text-clay">+{rest}</span> : null}
    </span>
  )
}

export function PageHeader({ title, subtitle, actions, children }: { title: ReactNode; subtitle?: ReactNode; actions?: ReactNode; children?: ReactNode }) {
  return (
    <div className="shrink-0 border-b border-line px-5 py-3">
      <div className="flex items-center justify-between gap-4">
        <div className="min-w-0">
          <h1 className="truncate text-base font-semibold">{title}</h1>
          {subtitle ? <div className="text-xs text-ink-2">{subtitle}</div> : null}
        </div>
        {actions ? <div className="flex shrink-0 items-center gap-2">{actions}</div> : null}
      </div>
      {children ? <div className="mt-2 flex flex-wrap items-center gap-2">{children}</div> : null}
    </div>
  )
}

export function snoozeOptions(now = new Date()): { label: string; until: string }[] {
  const at = (h: number, dayOffset: number) => {
    const d = new Date(now)
    d.setDate(d.getDate() + dayOffset)
    d.setHours(h, 0, 0, 0)
    return d.toISOString()
  }
  const opts = [] as { label: string; until: string }[]
  if (now.getHours() < 16) opts.push({ label: 'Later today (18:00)', until: at(18, 0) })
  opts.push({ label: 'Tomorrow morning (8:00)', until: at(8, 1) })
  const dow = now.getDay()
  const toMonday = ((8 - dow) % 7) || 7
  opts.push({ label: 'Next Monday (8:00)', until: at(8, toMonday) })
  opts.push({ label: 'In a week', until: at(8, 7) })
  return opts
}

export function SnoozeMenu({ onPick, trigger, label = 'Snooze' }: { onPick: (until: string) => void; trigger?: ReactNode; label?: string }) {
  const items: MenuItem[] = snoozeOptions().map((o) => ({ label: o.label, onSelect: () => onPick(o.until) }))
  return <Menu label={label} items={items} trigger={trigger ?? <Button size="sm" icon={<Clock />}>{label}</Button>} />
}

/**
 * "Follow up by…": the snooze dates, then no date. Each records the
 * message as a Follow Up Find due then (or moves the date of the Find it
 * already has), so the same dates mean the same thing in both menus.
 */
export function followUpOptions(now = new Date()): { label: string; dueAt: string | null }[] {
  return [...snoozeOptions(now).map((o) => ({ label: o.label, dueAt: o.until })), { label: 'No date', dueAt: null }]
}

/** `label` is the trigger's accessible name (the Menu puts it on the trigger), so a toolbar can say what it acts on. */
export function FollowUpMenu({ onPick, trigger, label = 'Follow up by' }: { onPick: (dueAt: string | null) => void; trigger?: ReactNode; label?: string }) {
  const items: MenuItem[] = followUpOptions().map((o) => ({ label: o.label, onSelect: () => onPick(o.dueAt) }))
  return <Menu label={label} items={items} trigger={trigger ?? <Button size="sm" icon={<Flag />}>Follow up…</Button>} />
}

export function RetypeMenu({ current, onPick, trigger }: { current: FindType; onPick: (t: FindType) => void; trigger?: ReactNode }) {
  const items: MenuItem[] = FIND_TYPES.map((t) => ({ label: findTypeLabel(t), onSelect: () => onPick(t), disabledReason: t === current ? 'current type' : null }))
  return <Menu label="Change Find type" items={items} trigger={trigger ?? <Button size="sm">Retype</Button>} />
}

export function StrataPicker({ open, onClose, onPick, title = 'Add a Stratum' }: { open: boolean; onClose: () => void; onPick: (names: string[]) => void; title?: string }) {
  const strata = useQuery({ queryKey: qk.strata, queryFn: listStrata, enabled: open })
  const [q, setQ] = useState('')
  const [picked, setPicked] = useState<string[]>([])
  const list = (strata.data ?? []).filter((s) => s.name.toLowerCase().includes(q.toLowerCase()))
  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()} title={title} description="A message can belong to several Strata.">
      <Input autoFocus placeholder="Filter Strata" value={q} onChange={(e) => setQ(e.target.value)} aria-label="Filter Strata" />
      <ul className="mt-2 max-h-64 overflow-auto rounded border border-line">
        {list.map((s) => {
          const on = picked.includes(s.name)
          return (
            <li key={s.id}>
              <label className="flex cursor-default items-center gap-2 px-2 py-1 hover:bg-sand">
                <input type="checkbox" checked={on} onChange={() => setPicked(on ? picked.filter((x) => x !== s.name) : [...picked, s.name])} aria-label={s.name} />
                <span className="flex-1">{s.name}</span>
                <span className="text-xs text-clay">{s.scopeGroup ?? 'All Digs'}</span>
              </label>
            </li>
          )
        })}
        {!list.length ? <li className="px-2 py-3 text-center text-xs text-clay">No Stratum matches. Create one under Strata.</li> : null}
      </ul>
      <div className="mt-4 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button
          variant="primary"
          icon={<Layers />}
          disabledReason={picked.length ? null : 'Pick at least one Stratum'}
          onClick={() => {
            onPick(picked)
            setPicked([])
            onClose()
          }}
        >
          Add {picked.length || ''}
        </Button>
      </div>
    </Dialog>
  )
}

const CLAUSE_TONE: Record<ClauseCheck['result'], string> = {
  pass: 'border-moss/40 bg-moss/10 text-moss',
  fail: 'border-line bg-paper text-ink-2',
  unknown: 'border-ochre/40 bg-ochre/10 text-ochre',
}
const CLAUSE_MARK: Record<ClauseCheck['result'], string> = { pass: '✓', fail: '✗', unknown: '?' }
const OUTCOME: Record<RouteOutcome, { mark: string; label: string; tone: string }> = {
  matched: { mark: '✓', label: 'matched: this decided it', tone: 'text-moss' },
  no_match: { mark: '✗', label: 'no match', tone: 'text-clay' },
  unknown: { mark: '?', label: 'a Signal was not scored, so the chain stopped here', tone: 'text-ochre' },
  not_reached: { mark: '·', label: 'not checked', tone: 'text-clay/60' },
}

/** One condition: what the rule asks for, what the message had, and the verdict. */
function Clause({ c }: { c: ClauseCheck }) {
  return (
    <span className={cx('inline-flex items-center gap-1.5 rounded border px-1.5 py-0.5 text-2xs mono', CLAUSE_TONE[c.result])} title={c.result === 'unknown' ? 'Not scored for this message' : undefined}>
      <span aria-hidden>{CLAUSE_MARK[c.result]}</span>
      <span>{c.text}</span>
      <span className="text-ink-2">is {c.actual ?? 'not scored'}</span>
    </span>
  )
}

/**
 * The deciding Mesh's routes in order, as the evaluator walked them: each
 * route's conditions with the message's actual values, which one decided,
 * and which were never checked. Replaces a single matched line and a wall of
 * Signal scores that left the reader to work out which numbers mattered.
 */
function RouteWalk({ ex }: { ex: Explanation }) {
  const [showAll, setShowAll] = useState(false)
  const lastRelevant = ex.routes.reduce((acc, r, i) => (r.outcome === 'not_reached' ? acc : i), 0)
  const visible = showAll ? ex.routes : ex.routes.slice(0, lastRelevant + 1)
  const hidden = ex.routes.length - visible.length
  return (
    <div className="mt-3">
      <div className="text-xs font-medium text-ink-2">
        How {ex.meshName ? `Mesh ${ex.meshName} v${ex.meshVersion}` : 'the Mesh'} decided, route by route
      </div>
      <ol className="mt-1.5 flex flex-col gap-1.5">
        {visible.map((r, i) => {
          const o = OUTCOME[r.outcome]
          const blockStart = r.when && (i === 0 || visible[i - 1].when?.text !== r.when.text)
          return (
            <li key={i} className={cx(r.when ? 'ml-4' : '', r.outcome === 'not_reached' ? 'opacity-50' : '')}>
              {blockStart && r.when ? (
                <div className="-ml-4 mb-1 flex flex-wrap items-center gap-1.5 text-2xs">
                  <span className="font-semibold uppercase tracking-wide text-clay">When</span>
                  <Clause c={r.when} />
                </div>
              ) : null}
              <div className={cx('rounded border px-2 py-1.5', r.outcome === 'matched' ? 'border-moss/50 bg-moss/5' : 'border-line bg-paper')}>
                <div className="flex flex-wrap items-center gap-1.5">
                  <span aria-hidden className={cx('w-3 text-center text-xs font-bold', o.tone)}>{o.mark}</span>
                  {r.clauses.length ? r.clauses.map((c, k) => <Clause key={k} c={c} />) : <span className="text-2xs text-ink-2 mono">{r.when ? 'ELSE (anything else in this block)' : 'ELSE (anything else)'}</span>}
                  <span className="text-2xs text-clay">→</span>
                  <span className="text-2xs font-medium mono">{r.output}</span>
                </div>
                <div className={cx('mt-0.5 pl-4 text-2xs', o.tone)}>{o.label}</div>
              </div>
            </li>
          )
        })}
      </ol>
      {hidden > 0 ? (
        <button type="button" className="mt-1 text-2xs text-ochre hover:underline" onClick={() => setShowAll(true)}>
          Show {plural(hidden, 'later route')} (not checked)
        </button>
      ) : null}
    </div>
  )
}

/**
 * The other Meshes that ran, one line each as the backend wrote them
 * ("Bills v3: payment_related .62, needed .80"): what looked first and
 * passed, and what looked after a borderline match and did not take it.
 */
function EarlierMeshes({ lines }: { lines: string[] }) {
  if (!lines.length) return null
  return (
    <div className="mt-2 text-xs" data-testid="earlier-meshes">
      <div className="font-medium text-ink-2">Other Meshes that ran</div>
      <ul className="mt-1 flex flex-col gap-0.5 text-2xs text-ink-2 mono">
        {lines.map((l, i) => (
          <li key={i}>{l}</li>
        ))}
      </ul>
    </div>
  )
}

/** Every Signal score, collapsed: the ones the routes used are shown above. */
function OtherSignals({ ex }: { ex: Explanation }) {
  const used = new Set(ex.routes.flatMap((r) => r.clauses.flatMap((c) => (c.signal ? [c.signal] : []))))
  const others = ex.signals.filter((s) => !used.has(`${s.name}@v${s.version}`))
  if (!others.length) return null
  return (
    <details className="mt-2 text-xs" open={!ex.routes.length}>
      <summary className="cursor-pointer select-none text-ink-2">
        {ex.routes.length ? 'Other Signal scores' : 'Signal scores'} ({others.length})
      </summary>
      <div className="mt-1.5 flex flex-wrap gap-1">
        {others.map((s) => (
          <Tip key={`${s.name}@${s.version}`} text={`${s.name} v${s.version}: ${s.selected} ${fmtScore(s.score)}${s.cached ? ' (cached)' : ' (fresh evaluation)'}`}>
            <span>
              <Chip tone="neutral" mono>
                {s.name}@v{s.version} {fmtScore(s.score)}
              </Chip>
            </span>
          </Tip>
        ))}
      </div>
    </details>
  )
}

/**
 * The "Why?" card (README "Mesh Feedback & Repair"). Its job is to show
 * exactly what happened: who decided, the decision in words, and under a
 * disclosure the route walk with each clause's actual value and the rule
 * as written. The sentence (`reason`, written by `riddle_mesh::humanize`)
 * is shown for every mechanism: it carries what nothing else on the page
 * does, such as the rule that looked first and nearly took the message
 * ("Bills v3 looked first and nearly took it: …, but the margin …"). An
 * earlier card dropped it for Mesh and LLM decisions and put the rule's
 * grammar in its place, so the one surface meant to explain a decision
 * showed syntax where a sentence was wanted. The inspector link appears
 * only in Developer Mode, since its destination says "Developer Mode is
 * off" to everyone else.
 */
export function ExplanationCard({
  ex,
  digId,
  onFiled,
  onUndo,
  compact,
  subject,
  from,
  digName,
}: {
  ex: Explanation
  digId: string
  /** "Mark as Filed": the message was not worth a Find; file it. */
  onFiled?: () => void
  onUndo?: () => void
  compact?: boolean
  /** How the Director conversation names the message; never an id. */
  subject?: string
  from?: string
  digName?: string
}) {
  const nav = useNavigate()
  const setPrefill = useDirector((s) => s.setPrefill)
  const developerMode = useSettings().data?.developerMode ?? false
  const mech = ex.mechanism
  const head =
    mech === 'mesh' && ex.meshName
      ? `Mesh ${ex.meshName} v${ex.meshVersion} decided ${dispositionLabel(ex.disposition)}.`
      : mech === 'gate'
        ? `A fixed rule decided: ${ex.gate}.`
        : mech === 'llm'
          ? `Signals were uncertain; the LLM decided ${dispositionLabel(ex.disposition)}.`
          : mech === 'user'
            ? `You decided: ${dispositionLabel(ex.disposition)}.`
            : mech === 'temporal'
              ? 'Made by time, not by mail: Temporal Sift.'
              : `Decided on Signals: ${dispositionLabel(ex.disposition)}.`
  // Words first: the decision as a person would say it, whoever made it. The grammar waits under the disclosure.
  const reason = ex.reason?.trim() || null
  const earlier = ex.earlierMeshes ?? []
  const hasDetail = ex.routes.length > 0 || ex.signals.length > 0 || !!ex.llmRationale || !!ex.matched || earlier.length > 0
  return (
    <div className={cx('rounded-md border border-line bg-sand selectable', compact ? 'p-2 text-xs' : 'p-3 text-sm')}>
      <div className="flex items-start gap-2">
        <HelpCircle className="mt-0.5 size-4 shrink-0 text-ochre" />
        <div className="min-w-0 flex-1">
          <div className="font-medium">Why? {ex.shadow ? <Chip tone="neutral">Shadow: proposed, not applied</Chip> : null}</div>
          <div className="mt-0.5">{head}</div>
          {reason ? <div className="mt-0.5 text-ink-2 selectable">{reason}</div> : null}
          {ex.meshName === YOUR_CORRECTIONS_MESH ? (
            <div className="mt-1.5">
              <Chip tone="ok" title="One of your own rules decided it: a sender or domain you corrected. Edit or remove it in Settings → Mail rules → Your corrections.">
                Your rule
              </Chip>
            </div>
          ) : ex.meshName ? (
            <div className="mt-1.5">
              <Chip tone="neutral" title="The rule that decided it. Click View Mesh to read or change it.">
                Rule: {ex.meshName} v{ex.meshVersion}
              </Chip>
            </div>
          ) : null}
          {hasDetail ? (
            <details className="mt-2 group">
              <summary className="cursor-pointer text-xs text-ink-2 hover:text-ink">How it was decided</summary>
              {ex.routes.length ? <RouteWalk ex={ex} /> : null}
              <EarlierMeshes lines={earlier} />
              <OtherSignals ex={ex} />
              {ex.llmRationale ? <blockquote className="mt-2 border-l-2 border-ochre/40 pl-2 text-ink-2">{ex.llmRationale}</blockquote> : null}
              {ex.matched ? (
                <details className="mt-2 text-xs">
                  <summary className="cursor-pointer select-none text-ink-2 hover:text-ink">Rule as written</summary>
                  <pre className="mt-1 whitespace-pre-wrap rounded border border-line bg-paper px-2 py-1 text-xs mono selectable">{ex.matched}</pre>
                </details>
              ) : null}
            </details>
          ) : null}
          <div className="mt-2 flex flex-wrap gap-1.5">
            {developerMode ? (
              <Button size="sm" variant="ghost" onClick={() => nav(`/developer/messages/${digId}/${ex.traceId}`)} title="Open the Message Inspector: the full trace behind this decision">
                Inspect decision
              </Button>
            ) : null}
            {ex.signals[0] && developerMode ? (
              <Button size="sm" variant="ghost" onClick={() => nav(`/signals/${ex.signals[0].name}`)}>
                View Signal
              </Button>
            ) : null}
            {ex.meshName ? (
              <Button size="sm" variant="ghost" onClick={() => nav(`/meshes?name=${encodeURIComponent(ex.meshName!)}`)}>
                View Mesh
              </Button>
            ) : null}
            {onFiled ? (
              <Button size="sm" variant="ghost" onClick={onFiled} title="Riddle was wrong about this one: record it as Filed. A correction it learns from.">
                Mark as Filed
              </Button>
            ) : null}
            {onUndo ? (
              <Button size="sm" variant="ghost" onClick={onUndo}>
                Undo
              </Button>
            ) : null}
            <Button
              size="sm"
              variant="ghost"
              title="Open the Director with this message filled in: why it was decided this way, and whether a Mesh should change. Nothing is sent until you press Enter there."
              onClick={() => {
                setPrefill(directorAbout({ label: `message Riddle marked ${dispositionLabel(ex.disposition)}`, title: subject, from, digName }), [from, subject].filter(Boolean).join(' · ') || null)
                nav('/director')
              }}
            >
              Talk to the Director about this
            </Button>
          </div>
        </div>
      </div>
    </div>
  )
}

/** Compact diagnostic chips in the normal UI (§83). Click opens the inspector. */
export function DevOverlay({ digId, traceId, mechanism, confidence, meshVersion, totalMs, escalated }: { digId: string; traceId: string | null; mechanism?: string | null; confidence?: number | null; meshVersion?: number | null; totalMs?: number | null; escalated?: boolean }) {
  const on = useUi((s) => s.devOverlay)
  const nav = useNavigate()
  if (!on || !traceId) return null
  return (
    <button type="button" className="inline-flex items-center gap-1" onClick={(e) => { e.stopPropagation(); nav(`/developer/messages/${digId}/${traceId}`) }} aria-label="Open the Message Inspector">
      {confidence !== null && confidence !== undefined ? <Chip tone="neutral" mono>{mechanism === 'llm' ? 'LLM' : 'JEV'} {fmtScore(confidence)}</Chip> : null}
      {meshVersion ? <Chip tone="neutral" mono>MESH v{meshVersion}</Chip> : null}
      <Chip tone="neutral" mono>{escalated ? 'LLM' : 'NO LLM'}</Chip>
      {totalMs ? <Chip tone="neutral" mono>{totalMs}ms</Chip> : null}
    </button>
  )
}

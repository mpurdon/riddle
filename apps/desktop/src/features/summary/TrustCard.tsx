/**
 * The trust card (Flow 7, option B): what Riddle decided in the last seven
 * days, how much of it the user reversed or undid, what it would have
 * archived had it been allowed to, and each rule's record. The study found
 * that people asked to let Riddle act had nothing to decide on: the Summary
 * said "41 to Spoil" and nothing about whether those calls were right. The
 * numbers are counts from the stores, never a survey, and each one says on
 * hover what it counts.
 *
 * Under it, the seducible moment: one account whose last forty Spoil calls
 * all stood is offered Act. It is confirmed on the card, not in a modal,
 * and "Not now" is remembered until the streak grows (`trust.ts`).
 */
import { useState } from 'react'
import { Link } from 'react-router-dom'
import type { RulePrecision } from '@/bindings/RulePrecision'
import type { TrustOffer } from '@/bindings/TrustOffer'
import { qk } from '@/lib/api'
import { fmtInt, num, plural } from '@/lib/format'
import { mailboxName } from '@/lib/gmailEffect'
import { toRiddleError } from '@/lib/ipc'
import { useDigContext, useSettings, useTrustStats } from '@/lib/queries'
import { useAction } from '@/lib/useAction'
import { Sep } from '@/components/Bits'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Tip } from '@/components/ui/Tooltip'
import { autonomyOf, belowAct } from '@/features/settings/accounts/model'
import { offerDismissed, readDismissed, withDismissed, writeDismissed, type DismissedOffers } from './trust'

const WINDOW_DAYS = 7

export function TrustCard() {
  const q = useTrustStats(WINDOW_DAYS)
  const { digId, digName } = useDigContext()
  const s = q.data
  if (!s || num(s.decisions) < 1) return null
  const where = digId ? `on ${digName(digId)}` : `across ${plural(s.digCount, 'account')}`
  return (
    <section className="mt-8 rounded-lg border border-line p-4" aria-label="Riddle's record">
      <div className="text-xs font-semibold uppercase tracking-wide text-clay">Riddle's record</div>
      <p className="mt-1 text-sm selectable">
        Last {WINDOW_DAYS} days {where}: <N n={s.decisions} what="Decisions the Sift made in the window. Your own moves and Shadow proposals are not counted." /> decisions,{' '}
        <N n={s.corrected} what="Decisions you changed in the window and have not undone since." /> you corrected, <N n={s.undone} what="Entries in Activity from the window that you undid." /> undone.
        {num(s.wouldArchive) > 0 ? (
          <>
            {' '}
            Riddle would have archived <N n={s.wouldArchive} what="Spoil decisions held inside Riddle because the account is below Act, on accounts whose Changes in Gmail setting archives." /> to Spoil.
          </>
        ) : null}
      </p>
      {s.rules.length ? (
        <>
          <ul className="mt-3 space-y-1 text-xs" aria-label="Rules">
            {s.rules.map((r) => (
              <RuleRow key={`${r.meshId}@${r.version}`} r={r} />
            ))}
          </ul>
          <p className="mt-2 text-xs text-clay">A rule you keep reversing steps back to Shadow, where it proposes and you decide.</p>
        </>
      ) : null}
      <OfferLine offer={s.offer} />
    </section>
  )
}

function RuleRow({ r }: { r: RulePrecision }) {
  const decided = num(r.decisions) > 0
  return (
    <li className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-ink-2 selectable">
      <Link to={`/meshes?name=${encodeURIComponent(r.name)}`} className="font-medium text-ink hover:underline">
        {r.name} v{r.version}
      </Link>
      {decided ? (
        <>
          <Sep />
          decided <N n={r.decisions} what={`Messages this rule decided in the last ${WINDOW_DAYS} days.`} />
          <Sep />
          you reversed <N n={r.reversed} what="Of those, the ones you changed afterwards." />
        </>
      ) : null}
      {r.status === 'shadow' ? (
        <>
          <Sep />
          proposed <N n={r.shadowProposed} what="Proposals it has made in its Shadow Period." />
          <Sep />
          you agreed <N n={r.shadowAgreed} what="Proposals you did not contradict." />
        </>
      ) : null}
      {r.reversedOften ? (
        <Chip tone="warn" title="You reversed a quarter or more of its last 20 or more decisions: the rule that steps a Mesh back to Shadow">
          reversed often
        </Chip>
      ) : null}
    </li>
  )
}

/** A count with what it counts on hover and on focus. */
function N({ n, what }: { n: bigint | number; what: string }) {
  return (
    <Tip text={what}>
      <span tabIndex={0} className="cursor-help font-medium text-ink underline decoration-dotted decoration-clay underline-offset-2">
        {fmtInt(n)}
      </span>
    </Tip>
  )
}

/**
 * The offer, only for an account whose Sift writes are still held for its
 * autonomy (the backend offers only then; the account's own `mailbox`
 * answer is checked again here so a stale card never offers what is
 * already true, and the settings stand in when the account is not loaded)
 * and only until "Not now" or the account changes.
 */
function OfferLine({ offer }: { offer: TrustOffer | null }) {
  const settings = useSettings().data
  const { digs } = useDigContext()
  const { run } = useAction()
  const [dismissed, setDismissed] = useState<DismissedOffers>(() => readDismissed())
  const [confirming, setConfirming] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  if (!offer || !settings) return null
  const dig = digs.find((d) => d.id === offer.digId)
  const heldForAutonomy = dig?.mailbox ? dig.mailbox.held === 'autonomy' : belowAct(autonomyOf(settings, offer.digId).level)
  if (!heldForAutonomy || !belowAct(offer.autonomy) || offer.projection === 'none') return null
  if (offerDismissed(dismissed, offer)) return null
  const name = offer.digName
  const mailbox = mailboxName(dig?.provider ?? 'gmail')
  const archives = offer.projection === 'full'
  const accept = async () => {
    setBusy(true)
    setError(null)
    try {
      const perDig = { ...settings.autonomy.perDig, [offer.digId]: 'act' as const }
      // The account's `mailbox` answer changes with its autonomy, so the accounts refresh too.
      await run('save_settings', { settings: { ...settings, autonomy: { ...settings.autonomy, perDig } } }, { successToast: `${name} is at Act`, invalidate: [qk.trustStats, qk.digs], toastErrors: false })
    } catch (e) {
      setError(toRiddleError(e).message)
    } finally {
      setBusy(false)
    }
  }
  const notNow = () => {
    const next = withDismissed(dismissed, offer)
    setDismissed(next)
    writeDismissed(next)
  }
  return (
    <div className="mt-3 border-t border-line pt-3 text-sm" role="group" aria-label={`Let Riddle act on ${name}`}>
      {!confirming ? (
        <>
          <p className="text-ink-2">
            Riddle got the last {fmtInt(offer.spoilCalls)} Spoil calls right on {name}. Let it {archives ? 'archive' : 'label'} Spoil there?
          </p>
          <div className="mt-2 flex gap-2">
            <Button size="sm" variant="primary" onClick={() => setConfirming(true)}>
              Let Riddle act on {name}
            </Button>
            <Button size="sm" variant="ghost" onClick={notNow}>
              Not now
            </Button>
          </div>
        </>
      ) : (
        <>
          <p className="text-ink-2">
            This sets {name} to Act: Riddle {archives ? 'labels and archives' : 'labels'} Spoil in {mailbox} as it classifies, as its Changes in Gmail setting allows. Every change is in Activity and can be undone.
          </p>
          <div className="mt-2 flex gap-2">
            <Button size="sm" variant="primary" busy={busy} onClick={accept}>
              Set {name} to Act
            </Button>
            <Button size="sm" variant="ghost" onClick={() => setConfirming(false)}>
              Cancel
            </Button>
          </div>
          {error ? (
            <p role="alert" className="mt-2 text-xs text-rust">
              {error}
            </p>
          ) : null}
        </>
      )}
    </div>
  )
}

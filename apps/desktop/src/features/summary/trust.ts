/**
 * The memory behind "Not now" on the trust card's seducible-moment offer.
 *
 * "Not now" has to mean not now. The first cut asked again on every visit
 * to the Summary, which turned an offer into a nag and made people set the
 * account to Act just to be rid of it. The count of clean Spoil calls at
 * the moment of dismissal is kept per account, and the offer comes back
 * only once the streak has grown past it: after the next clean run, never
 * sooner. localStorage is a convenience, not a record (`lib/local`); when
 * it is missing or refuses, the offer simply shows. "Reversed often" is the
 * engine's own verdict on a row (`RulePrecision.reversedOften`); the Meshes
 * page applies the same rule to its sum across versions
 * (`meshes/precision.ts`).
 */
import type { TrustOffer } from '@/bindings/TrustOffer'
import { num } from '@/lib/format'
import { readLocalJson, writeLocalJson } from '@/lib/local'

export const OFFER_KEY = 'riddle.trustOffer.v1'

/** The Spoil streak at which each account's offer was declined, by account id. */
export type DismissedOffers = Record<string, number>

type OfferRef = Pick<TrustOffer, 'digId' | 'spoilCalls'>

/** Only the finite numbers survive a read; anything else in the value is dropped. */
export function readDismissed(): DismissedOffers {
  const parsed = readLocalJson<unknown>(OFFER_KEY, null)
  if (!parsed || typeof parsed !== 'object') return {}
  const out: DismissedOffers = {}
  for (const [k, v] of Object.entries(parsed as Record<string, unknown>)) if (typeof v === 'number' && Number.isFinite(v)) out[k] = v
  return out
}

export function writeDismissed(next: DismissedOffers): void {
  writeLocalJson(OFFER_KEY, next)
}

/** True while the streak has not grown past the count at which the user said "Not now". */
export function offerDismissed(dismissed: DismissedOffers, offer: OfferRef): boolean {
  const at = dismissed[offer.digId]
  return at !== undefined && num(offer.spoilCalls) <= at
}

export function withDismissed(dismissed: DismissedOffers, offer: OfferRef): DismissedOffers {
  return { ...dismissed, [offer.digId]: num(offer.spoilCalls) }
}

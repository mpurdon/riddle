import { afterEach, describe, expect, it } from 'vitest'
import { u64 } from '@/lib/format'
import { OFFER_KEY, offerDismissed, readDismissed, withDismissed, writeDismissed } from './trust'

describe('the offer’s Not now memory', () => {
  const offer = { digId: 'dig_personal', spoilCalls: u64(40) }
  afterEach(() => localStorage.removeItem(OFFER_KEY))

  it('hides the offer until the streak grows past the count at which it was declined', () => {
    const d = withDismissed({}, offer)
    expect(offerDismissed(d, offer)).toBe(true)
    expect(offerDismissed(d, { ...offer, spoilCalls: u64(41) })).toBe(false)
    expect(offerDismissed(d, { digId: 'dig_work', spoilCalls: u64(40) })).toBe(false)
  })

  it('survives a round trip through storage and shrugs off a bad or missing value', () => {
    writeDismissed(withDismissed({}, offer))
    expect(readDismissed()).toEqual({ dig_personal: 40 })
    localStorage.setItem(OFFER_KEY, '{not json')
    expect(readDismissed()).toEqual({})
    localStorage.setItem(OFFER_KEY, JSON.stringify({ dig_personal: 'forty', dig_work: 12 }))
    expect(readDismissed()).toEqual({ dig_work: 12 })
    localStorage.removeItem(OFFER_KEY)
    expect(readDismissed()).toEqual({})
  })
})

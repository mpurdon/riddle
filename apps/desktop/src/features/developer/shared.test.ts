/**
 * YES/NO Signals render in a fixed order and colour so outcomes can be read
 * at a glance down a list; other choice sets stay sorted by score.
 */
import { describe, expect, it } from 'vitest'
import { distributionRows } from './shared'

describe('distributionRows', () => {
  it('puts YES on top in green and NO below in red, whichever scored higher', () => {
    const rows = distributionRows({ NO: 0.9, YES: 0.1 })
    expect(rows.map((r) => [r.label, r.bar])).toEqual([
      ['YES', 'bg-moss'],
      ['NO', 'bg-rust'],
    ])
  })

  it('sorts other choice sets by score and colours by the threshold', () => {
    const rows = distributionRows({ low: 0.1, high: 0.85, mid: 0.05 })
    expect(rows.map((r) => [r.label, r.bar])).toEqual([
      ['high', 'bg-moss'],
      ['low', 'bg-clay'],
      ['mid', 'bg-clay'],
    ])
  })
})

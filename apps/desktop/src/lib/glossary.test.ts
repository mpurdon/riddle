import { describe, expect, it } from 'vitest'
import { GLOSSARY } from './glossary'

describe('the glossary', () => {
  it('defines every noun the rail and the pages use', () => {
    const terms = GLOSSARY.map((g) => g.term)
    for (const t of ['Find', 'Unresolved', 'Filed', 'Spoil', 'Contamination', 'Stratum', 'Mesh', 'Signal', 'Director', 'Proposal', 'Expedition', 'Never Hidden', 'Autonomy', 'Changes in Gmail', 'Activity', 'Shadow Period', 'Account']) {
      expect(terms, `${t} is defined`).toContain(t)
    }
  })

  it('names the eight Find types in the Find entry and the four autonomy rungs in order', () => {
    const find = GLOSSARY.find((g) => g.term === 'Find')!.meaning
    for (const t of ['Respond', 'Decide', 'Review', 'Do', 'Pay', 'Attend', 'Follow Up', 'Know']) expect(find).toContain(t)
    expect(GLOSSARY.find((g) => g.term === 'Autonomy')!.meaning).toMatch(/Observe.*<.*Suggest.*<.*Act.*<.*Trusted/)
  })

  it('marks Signal as a developer term and says each account is its own Dig', () => {
    expect(GLOSSARY.find((g) => g.term === 'Signal')!.developer).toBe(true)
    expect(GLOSSARY.find((g) => g.term === 'Account')!.meaning).toMatch(/Each account is its own Dig/)
  })
})

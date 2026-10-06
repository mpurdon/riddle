import { describe, expect, it } from 'vitest'
import { autonomySentence, capChipTip, capValidTail } from './autonomy'

describe('a Mesh capped by an Autonomy line', () => {
  it('the list chip says the cap and that the mailbox is never touched', () => {
    expect(capChipTip('observe')).toBe("Capped at Observe: this Mesh's decisions never reach the mailbox, whatever the account allows")
  })

  it('the Valid line ends with the cap, saying decisions stay in Riddle below Act', () => {
    expect(capValidTail('suggest')).toBe(' · capped at Suggest (decisions stay in Riddle)')
    expect(capValidTail('act')).toBe(' · capped at Act')
    expect(capValidTail(undefined)).toBe('')
  })

  it('the Structure tab says what the cap does and how to lift it, or that the Mesh follows the account', () => {
    expect(autonomySentence({ autonomy: undefined })).toBe('Autonomy: follows the account.')
    expect(autonomySentence({ autonomy: 'suggest' })).toBe('Autonomy: capped at Suggest. Its decisions stay in Riddle whatever the account allows. Remove the Autonomy line to follow the account.')
    expect(autonomySentence({ autonomy: 'act' })).toBe('Autonomy: capped at Act. Its decisions reach the mailbox when the account allows, never as Trusted. Remove the Autonomy line to follow the account.')
  })
})

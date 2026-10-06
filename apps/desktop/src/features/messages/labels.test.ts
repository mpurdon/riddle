import { describe, expect, it } from 'vitest'
import { FILTERABLE_LABELS, labelName } from './labels'

describe('Gmail label names', () => {
  it('names the system labels the way Gmail does', () => {
    expect(labelName('INBOX')).toBe('Inbox')
    expect(labelName('SENT')).toBe('Sent')
    expect(labelName('IMPORTANT')).toBe('Important')
    expect(labelName('SNOOZED')).toBe('Snoozed')
    expect(labelName('STARRED')).toBe('Starred')
    expect(labelName('UNREAD')).toBe('Unread')
  })

  it('turns a category label into a title-case word without the prefix', () => {
    expect(labelName('CATEGORY_UPDATES')).toBe('Updates')
    expect(labelName('CATEGORY_PROMOTIONS')).toBe('Promotions')
    expect(labelName('CATEGORY_SOCIAL')).toBe('Social')
  })

  it('leaves a user label exactly as the user named it', () => {
    expect(labelName('Riddle/Spoil')).toBe('Riddle/Spoil')
    expect(labelName('Label_42')).toBe('Label_42')
    expect(labelName('clients')).toBe('clients')
  })

  it('does not mistake the bare prefix or an unknown id for a category', () => {
    expect(labelName('CATEGORY_')).toBe('CATEGORY_')
    expect(labelName('')).toBe('')
  })

  it('offers only labels it can name in the Inbox filter', () => {
    for (const id of FILTERABLE_LABELS) expect(labelName(id)).not.toBe(id)
  })
})

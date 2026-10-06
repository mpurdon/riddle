import { describe, expect, it } from 'vitest'
import type { Dig } from '@/bindings/Dig'
import { gmailEffectLine } from './gmailEffect'

const dig = (projection: Dig['projection'], provider: Dig['provider'] = 'gmail'): Dig => ({
  id: 'dig_1',
  name: 'Personal',
  email: 'me@example.com',
  provider,
  group: null,
  privacy: 'local_only',
  projection,
  status: { state: 'synced' },
  shadowStartedAt: null,
  disabled: false,
  createdAt: '2026-09-01T00:00:00Z',
})

describe('the toast says what a move did to the mailbox', () => {
  it('says Riddle only when Gmail changes are off, whatever the move', () => {
    expect(gmailEffectLine('move_to_spoil', {}, { dig: dig('none') })).toBe('Riddle only')
    expect(gmailEffectLine('archive_messages', {}, { dig: dig('none') })).toBe('Riddle only')
  })
  it('distinguishes labels from archiving', () => {
    expect(gmailEffectLine('move_to_spoil', {}, { dig: dig('labels_only') })).toBe('labelled in Gmail')
    expect(gmailEffectLine('move_to_spoil', {}, { dig: dig('full') })).toBe('labelled and archived in Gmail')
    expect(gmailEffectLine('archive_messages', {}, { dig: dig('labels_only') })).toBe('Riddle only')
  })
  it('follows the disposition of a correction', () => {
    expect(gmailEffectLine('set_disposition', { disposition: { kind: 'filed' } }, { dig: dig('full') })).toBe('archived in Gmail')
    expect(gmailEffectLine('set_disposition', { disposition: { kind: 'find', find_type: 'pay' } }, { dig: dig('full') })).toBe('back in the Gmail inbox')
  })
  it('says the dev build is holding the change until it is sent', () => {
    expect(gmailEffectLine('move_to_spoil', {}, { dig: dig('full'), dev: true, armed: false })).toBe('held: waits for you to send it (dev build)')
    expect(gmailEffectLine('move_to_spoil', {}, { dig: dig('full'), dev: true, armed: true })).toBe('labelled and archived in Gmail')
  })
  it('names the test mailbox and stays quiet for moves that never reach a mailbox', () => {
    expect(gmailEffectLine('archive_messages', {}, { dig: dig('full', 'fixture') })).toBe('archived in the test mailbox')
    expect(gmailEffectLine('snooze_messages', {}, { dig: dig('full') })).toBe('Riddle only')
    expect(gmailEffectLine('resolve_find', {}, { dig: dig('full') })).toBeNull()
  })
})

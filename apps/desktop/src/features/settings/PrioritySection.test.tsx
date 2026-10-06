/**
 * The Priority inbox is configured under Settings too, with the same
 * editor the page uses, so the two never disagree.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { mockState } from '@/lib/mocks'
import { mountApp } from '@/test/mountApp'

afterEach(cleanup)

describe('Settings, Priority inbox', () => {
  it('toggling a label in Settings saves the same criteria the page reads', async () => {
    mountApp('#/settings')
    const heading = await screen.findByText('Priority inbox')
    const section = heading.closest('section') ?? heading.parentElement!
    const important = await within(section).findByRole('button', { name: 'Important' })
    expect(important.getAttribute('aria-pressed')).toBe('false')
    fireEvent.click(important)
    await waitFor(() => expect(mockState().settings.priorityInbox.labels).toEqual(['STARRED', 'IMPORTANT']))
    await waitFor(() => expect(within(section).getByRole('button', { name: 'Important' }).getAttribute('aria-pressed')).toBe('true'))
  })
})

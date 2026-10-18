/**
 * The strip above the page at Essentials: the tabs that fold sibling pages
 * into one rail entry, and the Advanced chip on a page the rail does not
 * list. What this guards: Unresolved and Contamination stay one click
 * away, an advanced route opened directly is a page with a note, not a
 * dead end, and nothing of this shows at Everything.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { mockState } from '@/lib/mocks'
import { atLevel } from '@/test/level'
import { mountApp } from '@/test/mountApp'

afterEach(() => cleanup())

describe('the strip above the page at Essentials', () => {
  it('folds Finds and Unresolved into Needs you tabs, with the current one selected', async () => {
    mountApp('#/finds', atLevel('essentials'))
    const tabs = await screen.findByRole('tablist', { name: 'Needs you' })
    expect(within(tabs).getByRole('tab', { name: /^Finds/ }).getAttribute('aria-selected')).toBe('true')
    const unresolved = within(tabs).getByRole('tab', { name: /^Unresolved/ })
    expect(unresolved.getAttribute('aria-selected')).toBe('false')
    fireEvent.click(unresolved)
    await waitFor(() => expect(window.location.hash).toBe('#/unresolved'))
    await waitFor(() => expect(within(screen.getByRole('tablist', { name: 'Needs you' })).getByRole('tab', { name: /^Unresolved/ }).getAttribute('aria-selected')).toBe('true'))
    // The page itself renders: Unresolved is a disposition, never hidden.
    expect(await screen.findByRole('listbox', { name: 'Unresolved messages' })).toBeTruthy()
  })

  it('folds Spoil and Contamination into Set aside tabs', async () => {
    mountApp('#/spoil', atLevel('essentials'))
    const tabs = await screen.findByRole('tablist', { name: 'Set aside' })
    expect(within(tabs).getByRole('tab', { name: 'Spoil' }).getAttribute('aria-selected')).toBe('true')
    expect(within(tabs).getByRole('tab', { name: 'Contamination' }).getAttribute('aria-selected')).toBe('false')
  })

  it('folds the Inbox and Priority into Mail tabs', async () => {
    mountApp('#/inbox', atLevel('essentials'))
    const tabs = await screen.findByRole('tablist', { name: 'Mail' })
    expect(within(tabs).getByRole('tab', { name: 'Inbox' }).getAttribute('aria-selected')).toBe('true')
    expect(within(tabs).getByRole('tab', { name: 'Priority' })).toBeTruthy()
  })

  it('marks an advanced page opened directly as Advanced, and the note offers everything', async () => {
    mountApp('#/meshes', atLevel('essentials'))
    const note = await screen.findByRole('note', { name: 'Advanced page' })
    expect(within(note).getByText('Advanced')).toBeTruthy()
    fireEvent.click(within(note).getByRole('button', { name: 'Show everything' }))
    await waitFor(() => expect(mockState().settings.uiLevel).toBe('everything'))
    await waitFor(() => expect(screen.queryByRole('note', { name: 'Advanced page' })).toBeNull())
  })

  it('renders nothing at Everything', async () => {
    mountApp('#/finds', atLevel('everything'))
    await screen.findByRole('listbox', { name: 'Finds' })
    expect(screen.queryByRole('tablist', { name: 'Needs you' })).toBeNull()
    expect(screen.queryByRole('note', { name: 'Advanced page' })).toBeNull()
  })
})

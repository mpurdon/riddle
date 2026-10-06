import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, screen, waitFor } from '@testing-library/react'
import { mockState } from '@/lib/mocks'
import { mountApp } from '@/test/mountApp'

afterEach(() => cleanup())

describe('the shell renders against the mocks', () => {
  it('shows the rail, the Dig switcher, the Task Centre and the honest summary', async () => {
    mountApp('#/')
    expect(await screen.findByText('Catching up.')).toBeTruthy()
    expect(screen.getByText(/messages still to sift/)).toBeTruthy()
    for (const label of ['Summary', 'Finds', 'Unresolved', 'Inbox', 'Spoil', 'Contamination', 'Strata', 'Meshes', 'Signals', 'Activity', 'Director', 'Expeditions', 'Settings']) {
      expect(screen.getAllByText(label).length).toBeGreaterThan(0)
    }
    await waitFor(() => expect(screen.getByText('Developer')).toBeTruthy())
    expect(screen.getByLabelText(/^Task Centre:/)).toBeTruthy()
    expect(screen.getByLabelText('Ask Riddle')).toBeTruthy()
    await waitFor(() => expect(screen.getByText('DEV')).toBeTruthy())
  })

  it('the Task Centre shows each account moving through sync, Sift and projection, and syncs on demand', async () => {
    mountApp('#/')
    fireEvent.click(await screen.findByLabelText(/^Task Centre:/))
    const syncAll = await screen.findByRole('button', { name: 'Sync all' })
    expect(screen.getAllByRole('button', { name: /^Sync .+/ }).length).toBeGreaterThan(0)
    fireEvent.click(syncAll)
    // The mock's refresh runs a short job per account: the strip moves through Sync and Sift, then the
    // job lands under "Recently finished" and every account reads up to date.
    await waitFor(() => expect(mockState().jobs.some((j) => j.kind === 'incremental_sync' && j.status.state === 'running')).toBe(true))
    await waitFor(() => expect(mockState().jobs.some((j) => j.kind === 'incremental_sync' && j.status.state === 'completed')).toBe(true), { timeout: 4000 })
    await waitFor(() => expect(screen.getByText(/Recently finished/)).toBeTruthy())
    expect(screen.getAllByText('Up to date').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Sift').length).toBeGreaterThan(0)
  })

  // The window has no system title bar, so if this attribute goes, the window
  // becomes immovable — which is how the first build shipped. `-webkit-app-region`
  // looks equivalent and is silently ignored by WKWebView on macOS.
  it('marks the top bar as the window drag region', async () => {
    mountApp('#/')
    await screen.findByLabelText('Ask Riddle')
    const bar = document.querySelector('header[data-tauri-drag-region]')
    expect(bar, 'the top bar carries data-tauri-drag-region').toBeTruthy()
    expect(bar?.getAttribute('data-tauri-drag-region')).toBe('deep')
    // Traffic lights sit at the top left under the overlay title bar.
    expect(bar?.className).toMatch(/pl-\[\d+px\]/)
  })

  it('lists accounts in Settings and opens the Add account wizard on its first step', async () => {
    mountApp('#/settings#digs')
    const list = await screen.findByRole('complementary', { name: 'Accounts' })
    await waitFor(() => expect(list.textContent).toContain('Consulting'))
    expect(list.textContent).toContain('Google sign-in')
    expect(await screen.findByText('What Riddle does with this account')).toBeTruthy()
    screen.getAllByText('Add account')[0].click()
    expect(await screen.findByRole('dialog', { name: 'Add an account' })).toBeTruthy()
    expect(screen.getByRole('radio', { name: /Gmail/ }).getAttribute('aria-checked')).toBe('true')
  })
})

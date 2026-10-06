/**
 * A disabled account is not offered in the switcher; one line counts the
 * disabled accounts and leads to Settings, and a view that was showing the
 * account falls back to All accounts.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { useUi } from '@/lib/stores'
import { DigSwitcher } from './DigSwitcher'

afterEach(() => {
  cleanup()
  useUi.getState().setDigId(null)
})

describe('the account switcher', () => {
  it('leaves out a disabled account and falls back to All accounts when it was chosen', async () => {
    resetMockState()
    registerMocks()
    const [off, ...rest] = mockState().digs
    off.disabled = true
    useUi.getState().setDigId(off.id)
    render(
      <MemoryRouter>
        <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
          <DigSwitcher />
        </QueryClientProvider>
      </MemoryRouter>,
    )
    await waitFor(() => expect(useUi.getState().digId).toBeNull())
    const trigger = await screen.findByRole('button', { name: /Account: All accounts/ })
    fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false, pointerType: 'mouse' })
    expect(await screen.findByText('1 disabled · manage in Settings')).toBeTruthy()
    expect(screen.queryByText(new RegExp(`^${off.name} ·`))).toBeNull()
    expect(screen.getByText(new RegExp(`^${rest[0].name} ·`))).toBeTruthy()
  })
})

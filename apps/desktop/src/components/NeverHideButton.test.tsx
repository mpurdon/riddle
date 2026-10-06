/**
 * Never hide shows the sender's standing: once they are on Never Hidden the
 * button says so and offers to allow hiding again, removing the entry that
 * covers them, which may be their domain rather than their address.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { neverHiddenEntry } from '@/lib/neverHidden'
import { NeverHideButton } from './NeverHideButton'

afterEach(() => cleanup())

function renderButton(email: string, onAdd = () => {}) {
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <NeverHideButton email={email} onAdd={onAdd} />
    </QueryClientProvider>,
  )
}

describe('neverHiddenEntry', () => {
  it('finds the address or the domain that covers a sender, ignoring case', () => {
    const list = ['lena@example.com', '@Store.example']
    expect(neverHiddenEntry(list, 'Lena@Example.com')).toBe('lena@example.com')
    expect(neverHiddenEntry(list, 'sales@store.example')).toBe('@Store.example')
    expect(neverHiddenEntry(list, 'someone@else.example')).toBeNull()
  })
})

describe('the Never hide button', () => {
  it('offers Never hide to a sender who can be hidden', async () => {
    resetMockState()
    registerMocks()
    mockState().profile.neverHidden = []
    let added = false
    renderButton('sales@store.example', () => (added = true))
    fireEvent.click(await screen.findByRole('button', { name: /Never hide/ }))
    expect(added).toBe(true)
  })

  it('shows a protected sender as never hidden and removes the entry that covers them', async () => {
    resetMockState()
    registerMocks()
    mockState().profile.neverHidden = ['@store.example']
    renderButton('sales@store.example')
    const button = await screen.findByRole('button', { name: /Never hidden · Allow hiding/ })
    expect(button.getAttribute('title')).toContain('everything from @store.example')
    fireEvent.click(button)
    await waitFor(() => expect(mockState().profile.neverHidden).toEqual([]))
  })
})

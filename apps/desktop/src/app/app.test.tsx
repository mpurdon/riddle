import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { registerMocks, resetMockState } from '@/lib/mocks'
import { App } from './app'

beforeAll(() => {
  // jsdom lacks what Radix and the virtualiser probe for.
  class RO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', RO)
  Element.prototype.scrollIntoView = vi.fn()
  Element.prototype.scrollTo = vi.fn() as never
  window.matchMedia = ((q: string) => ({ matches: false, media: q, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })) as never
  localStorage.setItem('riddle.tour.v1', 'done')
})
afterEach(() => cleanup())

describe('the shell renders against the mocks', () => {
  it('shows the rail, the Dig switcher, the Task Centre and the honest summary', async () => {
    resetMockState()
    registerMocks()
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    window.location.hash = '#/'
    render(
      <QueryClientProvider client={qc}>
        <App />
      </QueryClientProvider>,
    )
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
})

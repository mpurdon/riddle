/**
 * The account page's per-rule note: a Mesh capped below Act keeps its
 * decisions in Riddle whatever the account allows, and the page says so
 * under the "Right now" sentence, naming the rules.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { App } from '@/app/app'

beforeAll(() => {
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

function mount(seed?: () => void) {
  resetMockState()
  seed?.()
  registerMocks()
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  window.location.hash = '#/settings#digs'
  render(
    <QueryClientProvider client={qc}>
      <App />
    </QueryClientProvider>,
  )
}

describe('the account page', () => {
  it('names the rules that keep their decisions in Riddle whatever the account allows', async () => {
    mount(() => {
      for (const m of mockState().meshes) if (m.id === 'mesh_receipts') m.autonomy = 'suggest'
    })
    await screen.findByText('What Riddle does with this account')
    const note = await screen.findByText(/keeps its decisions in Riddle whatever this account allows/)
    expect(note.textContent).toBe('1 rule keeps its decisions in Riddle whatever this account allows: Receipts.')
    expect(screen.getByRole('link', { name: 'Receipts' }).getAttribute('href')).toBe('#/meshes?name=Receipts')
  })

  it('says nothing when no rule in scope is capped', async () => {
    mount()
    await screen.findByText('What Riddle does with this account')
    await waitFor(() => expect(mockState().meshes.length).toBeGreaterThan(0))
    expect(screen.queryByText(/decisions in Riddle whatever this account allows/)).toBeNull()
  })
})

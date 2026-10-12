/**
 * The Labels page lists an account's labels by kind with counts, nests a
 * user's "a/b" labels, and opens the Inbox filtered to the one clicked.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter, Route, Routes, useLocation } from 'react-router-dom'
import { registerMocks, resetMockState } from '@/lib/mocks'
import { TooltipProvider } from '@/components/ui/Tooltip'
import LabelsPage, { nesting } from './LabelsPage'

beforeAll(() => {
  class RO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', RO)
})
afterEach(() => cleanup())

function Where() {
  const loc = useLocation()
  return <div data-testid="where">{loc.pathname + loc.search}</div>
}

describe('the Labels page', () => {
  it('reads a nested label as its last part', () => {
    expect(nesting('Receipts/2026')).toEqual({ depth: 1, leaf: '2026' })
    expect(nesting('Travel')).toEqual({ depth: 0, leaf: 'Travel' })
  })

  it('lists Gmail labels with counts and opens the Inbox on one', async () => {
    resetMockState()
    registerMocks()
    render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <TooltipProvider>
          <MemoryRouter initialEntries={['/labels']}>
            <Routes>
              <Route path="/labels" element={<LabelsPage />} />
              <Route path="/inbox" element={<Where />} />
            </Routes>
          </MemoryRouter>
        </TooltipProvider>
      </QueryClientProvider>,
    )
    const gmail = (await screen.findAllByRole('table', { name: /: Gmail$/ }))[0]
    fireEvent.click(within(gmail).getByRole('button', { name: 'Inbox' }))
    expect((await screen.findByTestId('where')).textContent).toBe('/inbox?place=mailbox%3AINBOX')
  })
})

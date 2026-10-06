/**
 * Grading a decision in the rule check records the grade and moves the
 * rule's tally from "not checked" to "1 right, 0 wrong".
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { TooltipProvider } from '@/components/ui/Tooltip'
import RuleCheckPage from './RuleCheckPage'

beforeAll(() => {
  class RO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', RO)
})
afterEach(() => cleanup())

describe('the rule check', () => {
  it('records a Right and shows it in the tally', async () => {
    resetMockState()
    registerMocks()
    render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <TooltipProvider>
          <MemoryRouter>
            <RuleCheckPage />
          </MemoryRouter>
        </TooltipProvider>
      </QueryClientProvider>,
    )
    const table = await screen.findByRole('table', { name: 'Rules' })
    fireEvent.click((await within(table).findAllByRole('button', { name: 'Check 10' }))[0])
    const list = await screen.findByRole('list', { name: 'Decisions to check' })
    const first = within(list).getAllByRole('listitem')[0]
    fireEvent.click(within(first).getByRole('button', { name: 'Right' }))
    await waitFor(() => expect(Object.values(mockState().grades)).toEqual(['right']))
    expect(await within(table).findByText('1 right, 0 wrong')).toBeTruthy()
  })
})

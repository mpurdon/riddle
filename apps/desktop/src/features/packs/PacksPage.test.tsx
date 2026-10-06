/**
 * Installing a pack from its card: the answer to its question becomes the
 * named list its rules read, and the card shows the pack as installed.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { TooltipProvider } from '@/components/ui/Tooltip'
import PacksPage, { parseEntries } from './PacksPage'

beforeAll(() => {
  class RO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', RO)
})
afterEach(() => cleanup())

describe('the Packs page', () => {
  it('reads one entry per line or comma', () => {
    expect(parseEntries(' uber.com,\n@lyft.com\n\n')).toEqual(['uber.com', '@lyft.com'])
  })

  it('installs a pack with the answer to its question as its list', async () => {
    resetMockState()
    registerMocks()
    render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <TooltipProvider>
          <MemoryRouter>
            <PacksPage />
          </MemoryRouter>
        </TooltipProvider>
      </QueryClientProvider>,
    )
    const card = await screen.findByRole('article', { name: 'Shopping and promotions' })
    fireEvent.click(within(card).getByRole('button', { name: 'Set up' }))
    fireEvent.change(within(card).getByLabelText('Entries for list:keep_promotions'), { target: { value: 'Uber.com' } })
    fireEvent.click(within(card).getByRole('button', { name: 'Install' }))
    await waitFor(() => expect(mockState().installedPacks.map((p) => p.id)).toEqual(['shopping']))
    expect(mockState().namedLists.find((l) => l.name === 'keep_promotions')?.entries).toEqual(['uber.com'])
    expect(await within(card).findByText('Installed v1')).toBeTruthy()
  })
})

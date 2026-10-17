/**
 * The Summary against the mocks: while an account catches up it shows the
 * sync job's progress and what has come of it, hoisted from the Task
 * Centre; before the first account its privacy line says what a new
 * account actually starts in.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { DIG_PERSONAL, DIG_WORK } from '@/lib/mocks/fixtures'
import { useUi } from '@/lib/stores'
import { TooltipProvider } from '@/components/ui/Tooltip'
import SummaryPage from './SummaryPage'

afterEach(() => {
  cleanup()
  useUi.getState().setDigId(null)
})

function mount(seed?: () => void) {
  resetMockState()
  seed?.()
  registerMocks()
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={qc}>
      <TooltipProvider>
        <MemoryRouter initialEntries={['/']}>
          <Routes>
            <Route path="/" element={<SummaryPage />} />
          </Routes>
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

describe('the Summary while catching up', () => {
  it('shows the catch-up job, its progress and ETA, and one line of what has come of it', async () => {
    mount()
    const panel = await screen.findByRole('region', { name: 'Catching up' })
    expect(panel.textContent).toContain('Catch-up: Fetching bodies, newest first')
    expect(panel.textContent).toContain('3,741 of 4,981 messages (75%) · ~4 min left')
    const bar = screen.getByRole('progressbar', { name: 'Catch-up: Work' })
    expect(bar.getAttribute('aria-valuenow')).toBe('75')
    // Fetching from the account's status, sorted from the Summary's sift counts, Finds from its open count.
    expect(panel.textContent).toContain('Fetching 1,240 · sorted 3,741 · 10 need you so far')
    expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Catching up.')
  })

  it('follows the account switcher: a synced account shows no catch-up panel', async () => {
    useUi.getState().setDigId(DIG_PERSONAL)
    mount()
    await screen.findByRole('heading', { level: 1 })
    await waitFor(() => expect(screen.getByRole('heading', { level: 1 }).textContent).not.toBe('Catching up.'))
    expect(screen.queryByRole('region', { name: 'Catching up' })).toBeNull()
  })

  it('shows the account its own first sync', async () => {
    useUi.getState().setDigId(DIG_WORK)
    mount()
    const panel = await screen.findByRole('region', { name: 'Catching up' })
    expect(panel.textContent).toContain('Fetching 1,240')
  })
})

describe('the Summary before the first account', () => {
  it('says what privacy mode a new account starts in and where the default is changed', async () => {
    mount(() => {
      mockState().digs = []
    })
    await screen.findByText('Connect an account to start.')
    const line = await screen.findByText(/Either way, a new account starts in/)
    expect(line.textContent).toBe("Either way, a new account starts in Cloud judgment: Jev reads message features to classify. No LLM. Change the default under Settings → Accounts → Defaults, or the account's own setting afterwards.")
    expect(screen.getByRole('link', { name: 'Settings → Accounts → Defaults' }).getAttribute('href')).toBe('/settings#new-digs')
    expect(screen.queryByText(/nothing leaves this machine/)).toBeNull()
  })
})

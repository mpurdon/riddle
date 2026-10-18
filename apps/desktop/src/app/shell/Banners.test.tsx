/**
 * An account that is not syncing says so in a banner that stays until it
 * syncs again, with the one thing to do: sign in again, or retry.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { TooltipProvider } from '@/components/ui/Tooltip'
import { Banners } from './Banners'

afterEach(() => cleanup())

function show() {
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <TooltipProvider>
        <MemoryRouter>
          <Banners />
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

describe('the sync banner', () => {
  it('asks to sign in again when the sign-in has expired', async () => {
    resetMockState()
    const d = mockState().digs[0]
    d.status = { state: 'sync_failing', failure: 'sign_in', since: new Date().toISOString(), last_synced_at: '2026-09-27T17:24:01Z' }
    registerMocks()
    show()
    expect((await screen.findByRole('alert')).textContent).toContain(`${d.name} is not syncing: the sign-in has expired`)
    expect(screen.getByRole('button', { name: 'Sign in again' })).toBeTruthy()
  })

  it('points to the setup when the OAuth client is missing', async () => {
    resetMockState()
    mockState().digs[0].status = { state: 'sync_failing', failure: 'not_set_up', since: new Date().toISOString(), last_synced_at: null }
    registerMocks()
    show()
    expect((await screen.findByRole('alert')).textContent).toContain('the OAuth client id is missing')
    expect(screen.getByRole('button', { name: 'Set up sign-in' })).toBeTruthy()
  })

  it('offers a retry when Gmail cannot be reached', async () => {
    resetMockState()
    mockState().digs[0].status = { state: 'sync_failing', failure: 'unreachable', since: new Date().toISOString(), last_synced_at: null }
    registerMocks()
    show()
    expect(await screen.findByRole('button', { name: 'Retry now' })).toBeTruthy()
  })
})

describe('the degraded banner', () => {
  it('says why the account is degraded and when the judge last answered', async () => {
    resetMockState()
    const s = mockState()
    const reason = 'judge paused after repeated failures, retrying at 14:35; nothing hidden'
    s.digs[0].status = { state: 'degraded', reason }
    s.judgeHealth = {
      breaker: { state: 'open', until: new Date(Date.now() + 300_000).toISOString() },
      lastSuccessAt: new Date(Date.now() - 600_000).toISOString(),
      lastFailureAt: new Date(Date.now() - 60_000).toISOString(),
      lastFailureKind: 'judge_unavailable',
    }
    registerMocks()
    show()
    const banner = (await screen.findAllByRole('status')).find((el) => el.textContent?.includes('is degraded'))
    expect(banner?.textContent).toContain(`${s.digs[0].name} is degraded: ${reason}.`)
    expect(banner?.textContent).toContain('Last judge call')
    expect(banner?.textContent).toContain('(failed: judge_unavailable)')
    expect(screen.getByRole('button', { name: 'Open Developer Mode' })).toBeTruthy()
  })
})

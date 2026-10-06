/**
 * The trust card and its offer against the mocks. The fixture has no
 * account with a forty-long clean Spoil streak below Act, so the offer path
 * is proved with a seeded `trust_stats` and a seeded autonomy level.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import type { TrustOffer } from '@/bindings/TrustOffer'
import type { TrustStats } from '@/bindings/TrustStats'
import { u64 } from '@/lib/format'
import { mock } from '@/lib/ipc'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { DIG_PERSONAL } from '@/lib/mocks/fixtures'
import { trustStats } from '@/lib/mocks/queries'
import { useUi } from '@/lib/stores'
import { TooltipProvider } from '@/components/ui/Tooltip'
import { OFFER_KEY } from './trust'
import { TrustCard } from './TrustCard'
import SummaryPage from './SummaryPage'

beforeAll(() => {
  class RO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', RO)
})
afterEach(() => {
  cleanup()
  localStorage.removeItem(OFFER_KEY)
  useUi.getState().setDigId(null)
})

const personalOffer: TrustOffer = { digId: DIG_PERSONAL, digName: 'Personal', spoilCalls: u64(40), projection: 'full', autonomy: 'suggest' }

/** Mount with the mocks, optionally reshaping what `trust_stats` answers. */
function mount(opts: { seed?: () => void; stats?: (base: TrustStats) => TrustStats; page?: 'card' | 'summary' } = {}) {
  resetMockState()
  opts.seed?.()
  registerMocks()
  if (opts.stats) mock('trust_stats', () => opts.stats!(trustStats(mockState(), useUi.getState().digId, 7)))
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={qc}>
      <TooltipProvider>
        <MemoryRouter initialEntries={['/']}>
          <Routes>
            <Route path="/" element={opts.page === 'summary' ? <SummaryPage /> : <TrustCard />} />
            <Route path="/meshes" element={<div>Meshes page</div>} />
          </Routes>
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

const card = () => screen.findByRole('region', { name: "Riddle's record" })

describe('the trust card', () => {
  it('says what Riddle decided across all accounts and what the user corrected, from the same counts the backend would give', async () => {
    mount()
    const expected = trustStats(mockState(), null, 7)
    expect(Number(expected.decisions)).toBeGreaterThan(0)
    const region = await card()
    expect(region.textContent).toContain(`Last 7 days across 3 accounts: ${Number(expected.decisions)} decisions, ${Number(expected.corrected)} you corrected, ${Number(expected.undone)} undone.`)
    if (Number(expected.wouldArchive) > 0) expect(region.textContent).toContain(`Riddle would have archived ${Number(expected.wouldArchive)} to Spoil.`)
    else expect(region.textContent).not.toContain('would have archived')
  })

  it('names the selected account instead of counting accounts', async () => {
    useUi.getState().setDigId(DIG_PERSONAL)
    mount()
    expect((await card()).textContent).toContain('Last 7 days on Personal:')
  })

  it('lists each rule with what it decided and what was reversed, linking to the Mesh by name, and flags the one the engine says is reversed often', async () => {
    mount({ stats: (b) => ({ ...b, rules: [{ meshId: 'mesh_github', name: 'GitHub Notifications', version: 7, status: 'active', decisions: u64(118), reversed: u64(2), shadowProposed: 219, shadowAgreed: 212, reversedOften: false }, { meshId: 'mesh_aws', name: 'AWS Notifications', version: 3, status: 'shadow', decisions: u64(20), reversed: u64(5), shadowProposed: 44, shadowAgreed: 40, reversedOften: true }] }) })
    const list = within(await card()).getByRole('list', { name: 'Rules' })
    const rows = within(list).getAllByRole('listitem')
    expect(rows[0].textContent).toContain('GitHub Notifications v7')
    expect(rows[0].textContent).toMatch(/decided\s*118/)
    expect(rows[0].textContent).toMatch(/you reversed\s*2/)
    expect(within(rows[0]).queryByText('reversed often')).toBeNull()
    expect(within(rows[0]).getByRole('link').getAttribute('href')).toBe('/meshes?name=GitHub%20Notifications')
    expect(rows[1].textContent).toMatch(/proposed\s*44/)
    expect(rows[1].textContent).toMatch(/you agreed\s*40/)
    expect(within(rows[1]).getByText('reversed often')).toBeTruthy()
    expect(list.parentElement?.textContent).toContain('A rule you keep reversing steps back to Shadow')
  })

  it('does not exist until there is at least one decision', async () => {
    mount({ stats: (b) => ({ ...b, decisions: u64(0), rules: [] }) })
    await new Promise((r) => setTimeout(r, 50))
    expect(screen.queryByRole('region', { name: "Riddle's record" })).toBeNull()
  })

  it('sits below the brief and above what Riddle noticed on the Summary', async () => {
    mount({ page: 'summary' })
    const region = await card()
    const noticed = await screen.findByText('Riddle noticed')
    const brief = screen.getByText(/messages arrived across/)
    expect(brief.compareDocumentPosition(region) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(region.compareDocumentPosition(noticed) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })
})

describe('the offer to let Riddle act', () => {
  const belowAct = () => {
    mockState().settings.autonomy.perDig[DIG_PERSONAL] = 'suggest'
  }

  it('confirms on the card and sets the account to Act', async () => {
    mount({ seed: belowAct, stats: (b) => ({ ...b, offer: personalOffer }) })
    const group = await screen.findByRole('group', { name: 'Let Riddle act on Personal' })
    expect(group.textContent).toContain('Riddle got the last 40 Spoil calls right on Personal. Let it archive Spoil there?')
    fireEvent.click(within(group).getByRole('button', { name: 'Let Riddle act on Personal' }))
    expect(group.textContent).toContain('This sets Personal to Act: Riddle labels and archives Spoil in the test mailbox as it classifies, as its Changes in Gmail setting allows. Every change is in Activity and can be undone.')
    expect(screen.queryByRole('dialog')).toBeNull()
    fireEvent.click(within(group).getByRole('button', { name: 'Set Personal to Act' }))
    await waitFor(() => expect(mockState().settings.autonomy.perDig[DIG_PERSONAL]).toBe('act'))
    await waitFor(() => expect(screen.queryByRole('group', { name: 'Let Riddle act on Personal' })).toBeNull())
  })

  it('Cancel goes back to the offer without changing anything', async () => {
    mount({ seed: belowAct, stats: (b) => ({ ...b, offer: personalOffer }) })
    const group = await screen.findByRole('group', { name: 'Let Riddle act on Personal' })
    fireEvent.click(within(group).getByRole('button', { name: 'Let Riddle act on Personal' }))
    fireEvent.click(within(group).getByRole('button', { name: 'Cancel' }))
    expect(within(group).getByRole('button', { name: 'Let Riddle act on Personal' })).toBeTruthy()
    expect(mockState().settings.autonomy.perDig[DIG_PERSONAL]).toBe('suggest')
  })

  it('Not now hides it, remembers, and lets it back only once the streak has grown', async () => {
    mount({ seed: belowAct, stats: (b) => ({ ...b, offer: personalOffer }) })
    const group = await screen.findByRole('group', { name: 'Let Riddle act on Personal' })
    fireEvent.click(within(group).getByRole('button', { name: 'Not now' }))
    expect(screen.queryByRole('group', { name: 'Let Riddle act on Personal' })).toBeNull()
    expect(JSON.parse(localStorage.getItem(OFFER_KEY) ?? '{}')).toEqual({ [DIG_PERSONAL]: 40 })
    cleanup()
    mount({ seed: belowAct, stats: (b) => ({ ...b, offer: personalOffer }) })
    await card()
    expect(screen.queryByRole('group', { name: 'Let Riddle act on Personal' })).toBeNull()
    cleanup()
    mount({ seed: belowAct, stats: (b) => ({ ...b, offer: { ...personalOffer, spoilCalls: u64(41) } }) })
    expect(await screen.findByRole('group', { name: 'Let Riddle act on Personal' })).toBeTruthy()
  })

  it('never shows for an account already at Act, whatever the stats say', async () => {
    mount({ stats: (b) => ({ ...b, offer: { ...personalOffer, autonomy: 'act' } }) })
    await card()
    expect(screen.queryByRole('group', { name: 'Let Riddle act on Personal' })).toBeNull()
    cleanup()
    // The settings are the truth even when the offer claims otherwise.
    mount({ stats: (b) => ({ ...b, offer: personalOffer }) })
    await card()
    expect(mockState().settings.autonomy.perDig[DIG_PERSONAL]).toBe('act')
    expect(screen.queryByRole('group', { name: 'Let Riddle act on Personal' })).toBeNull()
  })
})

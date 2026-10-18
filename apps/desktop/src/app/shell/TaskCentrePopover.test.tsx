/**
 * The Task Centre says why an account is degraded and what the judge's
 * breaker is doing: the breaker opened and closed for a fortnight while
 * every account read "Sifting".
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { TooltipProvider } from '@/components/ui/Tooltip'
import { useUi } from '@/lib/stores'
import { TaskCentrePopover } from './TaskCentrePopover'

afterEach(() => cleanup())

function show() {
  useUi.setState({ taskCentreOpen: true })
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <TooltipProvider>
        <MemoryRouter>
          <TaskCentrePopover />
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

describe('the Task Centre under a paused judge', () => {
  it('shows the degraded reason on the account and the judge line with its last call', async () => {
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
    const row = await screen.findByLabelText(`${s.digs[0].name}: Degraded: ${reason}`)
    expect(row.textContent).toContain(`Degraded: ${reason}`)
    const services = screen.getByLabelText('Services')
    expect(services.textContent).toContain('Judge paused after repeated failures, retrying at')
    expect(services.textContent).toContain('last judge call')
    expect(services.textContent).toContain('(failed: judge_unavailable)')
  })

  it('says the judge is answering and when it last did while the breaker is closed', async () => {
    resetMockState()
    registerMocks()
    show()
    const services = await screen.findByLabelText('Services')
    expect(services.textContent).toContain('Judge answering · last judge call')
    expect(services.textContent).toContain('LLM answering · last llm call')
  })
})

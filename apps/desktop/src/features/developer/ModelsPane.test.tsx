/**
 * The Models view: checking access marks which models are usable, a run
 * on decided mail scores each model against the person's decision, and a
 * replayed Director question can be picked.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { modelLabel } from '@/lib/mocks/comparison'
import { TooltipProvider } from '@/components/ui/Tooltip'
import { ModelsPane } from './ModelsPane'

beforeAll(() => {
  class RO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', RO)
})
afterEach(() => cleanup())

function show() {
  render(
    <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
      <TooltipProvider>
        <MemoryRouter>
          <ModelsPane />
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

describe('model labels', () => {
  it('read Bedrock ids as names', () => {
    expect(modelLabel('us.anthropic.claude-haiku-4-5-20251001-v1:0')).toBe('Claude Haiku 4.5')
    expect(modelLabel('us.openai.gpt-5.6-luna')).toBe('GPT-5.6 Luna')
  })
})

describe('the Models view', () => {
  it('checks access, scores a run on decided mail, and takes a Director pick', async () => {
    resetMockState()
    const s = mockState()
    // A message the person decided themselves: the right answer.
    const m = s.messages[0]
    const c = s.classifications.get(m.id)!
    c.disposition = { kind: 'filed' }
    c.handledByUserAt = new Date().toISOString()
    registerMocks()
    show()

    const roster = await screen.findByRole('table', { name: 'Model roster' })
    fireEvent.click(screen.getByRole('button', { name: 'Check access now' }))
    await waitFor(() => expect(within(roster).getAllByText('Usable').length).toBeGreaterThan(0))
    expect(within(roster).getAllByText('Not enabled').length).toBeGreaterThan(0)

    fireEvent.click(screen.getByRole('button', { name: 'Run on decided mail' }))
    const scores = await screen.findByRole('table', { name: 'escalation scores' })
    // Each model is scored against the mail the person decided.
    expect(within(scores).getAllByText(/^of \d+$/).length).toBeGreaterThan(0)
    expect(await screen.findByRole('table', { name: 'Escalation side by side' })).toBeTruthy()

    fireEvent.click(screen.getByRole('button', { name: 'Replay my questions' }))
    const list = await screen.findByRole('list', { name: 'Director side by side' })
    fireEvent.click(within(list).getAllByRole('button', { name: 'Best answer' })[0])
    await waitFor(() => expect(Object.keys(mockState().comparisonPicks).length).toBe(1))
  })
})

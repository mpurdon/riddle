/**
 * The Message Inspector says which pass decided, what every Mesh made of
 * the message, and previews a re-decision without writing anything. The
 * inspector used to show the deciding Mesh alone, and the only way to see
 * what today's rules would do was to send the message back to Sift.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import type { DecisionTrace } from '@/bindings/DecisionTrace'
import { registerMocks, resetMockState } from '@/lib/mocks'
import { TooltipProvider } from '@/components/ui/Tooltip'
import { TraceView } from './Inspectors'

afterEach(() => cleanup())

function mount(pick: (traces: DecisionTrace[]) => DecisionTrace | undefined) {
  const state = resetMockState()
  registerMocks()
  const t = pick([...state.traces.values()])
  if (!t) throw new Error('no such trace in the mock state')
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={qc}>
      <TooltipProvider>
        <MemoryRouter>
          <TraceView t={t} />
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  )
  return t
}

describe('the Message Inspector', () => {
  it('names the pass and lists every Mesh that ran', () => {
    const t = mount((all) => all.find((x) => x.meshName && x.meshOutcomes.length > 1))
    expect(screen.getByText(t.passId)).toBeTruthy()
    const outcomes = screen.getByTestId('mesh-outcomes')
    const items = Array.from(outcomes.querySelectorAll('li')).map((li) => li.textContent)
    expect(items.some((l) => l === `${t.meshName} v${t.meshVersion}: decided`)).toBe(true)
    expect(items.every((l) => !l?.endsWith('not reached'))).toBe(true)
  })

  it('previews a re-decision and shows the diff without changing the trace', async () => {
    const t = mount((all) => all.find((x) => x.meshName))
    fireEvent.click(screen.getByText('Preview a re-decision'))
    const result = await screen.findByTestId('preview-result')
    expect(result.textContent).toMatch(/the same: |still |would be /)
    expect(result.textContent).toContain('The stored decision stands.')
    expect(result.textContent).toContain(`Preview pass preview-${t.messageId}`)
    // The trace shown is still the stored one.
    expect(screen.getByText(t.passId)).toBeTruthy()
  })
})

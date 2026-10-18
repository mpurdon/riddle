/**
 * The Why? card says the decision in words for every mechanism, including
 * the rule that looked first and nearly took the message, and keeps the
 * rule's grammar folded under "How it was decided". A Mesh decision used
 * to show the grammar where the sentence belonged.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, render, screen } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import type { Explanation } from '@/bindings/Explanation'
import { registerMocks, resetMockState } from '@/lib/mocks'
import { TooltipProvider } from './ui/Tooltip'
import { ExplanationCard } from './Bits'

afterEach(() => cleanup())

const NEAR_MISS = 'Bills v3 looked first and nearly took it: it appears the sender is a bank, so it is filed under Finance, but the margin 0.08 is below the 0.15 required.'

const meshDecision: Explanation = {
  messageId: 'm_1',
  disposition: { kind: 'spoil' },
  mechanism: 'mesh',
  meshName: 'Frequent promotions',
  meshVersion: 2,
  matched: 'IF marketing@v1 > .70\n    → Spoil',
  signals: [],
  gate: null,
  escalated: false,
  llmRationale: null,
  traceId: 'tr_1',
  shadow: false,
  reason: `It appears marketing is likely, so it is set aside as Spoil. ${NEAR_MISS}`,
  routes: [{ when: null, clauses: [{ text: 'marketing@v1 > .70', actual: '0.91', result: 'pass', signal: 'marketing@v1' }], output: 'Spoil', outcome: 'matched' }],
  importance: 'normal',
  importanceSetByYou: false,
  earlierMeshes: [],
}

function mount(ex: Explanation) {
  resetMockState()
  registerMocks()
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={qc}>
      <TooltipProvider>
        <MemoryRouter>
          <ExplanationCard ex={ex} digId="dig_work" />
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

describe('the Why? card', () => {
  it('shows a Mesh decision in words, near miss included, and folds the rule as written under the disclosure', () => {
    mount(meshDecision)
    expect(screen.getByText('Mesh Frequent promotions v2 decided Spoil.')).toBeTruthy()
    const reason = screen.getByText((t) => t.includes(NEAR_MISS))
    expect(reason.closest('details')).toBeNull()
    // The grammar is still there for anyone who wants it, closed by default and after the words.
    const grammar = screen.getByText((_, el) => el?.tagName === 'PRE' && (el.textContent ?? '').includes('IF marketing@v1 > .70'))
    const fold = grammar.closest('details') as HTMLDetailsElement
    expect(fold.open).toBe(false)
    expect(fold.querySelector('summary')?.textContent).toBe('Rule as written')
    const outer = fold.parentElement!.closest('details') as HTMLDetailsElement
    expect(outer.querySelector('summary')?.textContent).toBe('How it was decided')
    expect(outer.open).toBe(false)
    expect(reason.compareDocumentPosition(grammar) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })

  it('lists the other Meshes that ran, one line each, under the disclosure', () => {
    const lines = ['Bills v3: payment_related .62, needed .80', 'System guards v1: no route matched']
    mount({ ...meshDecision, earlierMeshes: lines })
    const list = screen.getByTestId('earlier-meshes')
    expect(list.textContent).toContain('Other Meshes that ran')
    const items = Array.from(list.querySelectorAll('li')).map((li) => li.textContent)
    expect(items).toEqual(lines)
    // Folded with the rest of the grammar, after the words.
    const fold = list.closest('details') as HTMLDetailsElement
    expect(fold.querySelector('summary')?.textContent).toBe('How it was decided')
    expect(fold.open).toBe(false)
  })

  it('still shows the words for an LLM decision', () => {
    mount({ ...meshDecision, mechanism: 'llm', meshName: null, meshVersion: null, matched: null, routes: [], escalated: true, llmRationale: 'A first-time sender with nothing to act on.', reason: 'Too little context: a first-time sender about an unnamed matter.' })
    expect(screen.getByText('Too little context: a first-time sender about an unnamed matter.').closest('details')).toBeNull()
  })
})

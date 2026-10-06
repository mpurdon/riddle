/**
 * Mount the whole app at a hash over fresh mock state, the way every page
 * test drives a real page against the mocks; `seed` edits the state before
 * anything reads it. Four test files had each written this wrapper, and
 * the jsdom shims beside it, which live once in `vitest.setup.ts`.
 */
import { render } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { registerMocks, resetMockState } from '@/lib/mocks'
import type { MockState } from '@/lib/mocks/state'
import { App } from '@/app/app'

export function mountApp(hash: string, seed?: (state: MockState) => void): MockState {
  const state = resetMockState()
  seed?.(state)
  registerMocks()
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  window.location.hash = hash
  render(
    <QueryClientProvider client={qc}>
      <App />
    </QueryClientProvider>,
  )
  return state
}

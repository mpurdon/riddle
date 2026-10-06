/**
 * Shared test setup. The page tests drive real pages against the mocks
 * under jsdom, and the full suite runs them in parallel: a `waitFor` that
 * passes alone can miss the default one-second budget under load, which
 * looked like four flaky page tests. The budget is raised once here rather
 * than per assertion.
 */
import { configure } from '@testing-library/react'
import { vi } from 'vitest'

configure({ asyncUtilTimeout: 5_000 })

// jsdom lacks what Radix, the virtualiser and the tour probe for. One set of
// shims here, instead of a slightly different copy at the top of every page
// test.
class RO {
  observe() {}
  unobserve() {}
  disconnect() {}
}
vi.stubGlobal('ResizeObserver', RO)
Element.prototype.scrollIntoView = vi.fn()
Element.prototype.scrollTo = vi.fn() as never
Element.prototype.hasPointerCapture = vi.fn(() => false) as never
Element.prototype.setPointerCapture = vi.fn() as never
Element.prototype.releasePointerCapture = vi.fn() as never
Object.defineProperty(HTMLElement.prototype, 'offsetHeight', { configurable: true, get: () => 600 })
Object.defineProperty(HTMLElement.prototype, 'offsetWidth', { configurable: true, get: () => 1200 })
window.matchMedia = ((q: string) => ({ matches: false, media: q, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })) as never
try {
  localStorage.setItem('riddle.tour.v1', 'done')
} catch {
  /* ignore */
}

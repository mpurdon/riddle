/**
 * Seeds and readers the level tests share: put the mock settings at a level
 * (with or without a model), and read the rail's entries by name.
 */
import { expect } from 'vitest'
import { screen, waitFor, within } from '@testing-library/react'
import type { UiLevel } from '@/lib/level'
import type { MockState } from '@/lib/mocks/state'

export const atLevel =
  (level: UiLevel, llm = true) =>
  (state: MockState) => {
    state.settings.uiLevel = level
    state.settings.developerMode = level === 'developer'
    if (!llm) state.settings.llm.provider = 'null'
  }

/** The rail's entries by accessible name, without the count a badge appends. */
export async function railEntries(): Promise<string[]> {
  const nav = await screen.findByRole('navigation', { name: 'Sections' })
  await waitFor(() => expect(within(nav).getAllByRole('link').length).toBeGreaterThan(0))
  return within(nav)
    .getAllByRole('link')
    .map((a) => (a.getAttribute('aria-label') ?? '').split(',')[0])
}

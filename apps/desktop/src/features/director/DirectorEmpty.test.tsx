/**
 * The Director without a model, against the mocks. What this guards: at
 * Essentials the rail leaves the Director out when no LLM is set, the page
 * is an instruction rather than a dead end at every level, and the canned
 * questions at Essentials do not lead with rules or long jobs.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { atLevel, railEntries } from '@/test/level'
import { mountApp } from '@/test/mountApp'

afterEach(() => cleanup())

const EMPTY = 'The Director has no model to think with.'

describe('the Director without a model', () => {
  it('leaves the rail at Essentials and shows how to choose one', async () => {
    mountApp('#/director', atLevel('essentials', false))
    expect(await screen.findByText(EMPTY)).toBeTruthy()
    expect(await railEntries()).not.toContain('Director')
    fireEvent.click(screen.getByRole('button', { name: 'Choose a model' }))
    await waitFor(() => expect(window.location.hash).toBe('#/settings#providers'))
  })

  it('stays in the rail at Everything, with the same empty state', async () => {
    mountApp('#/director', atLevel('everything', false))
    expect(await screen.findByText(EMPTY)).toBeTruthy()
    expect(await railEntries()).toContain('Director')
  })

  it('with a model, Essentials offers the plain questions and not the rule or long-job ones', async () => {
    mountApp('#/director', atLevel('essentials'))
    const empty = await screen.findByRole('status')
    await waitFor(() => expect(within(empty).getByText('What actually needs my attention this morning?')).toBeTruthy())
    expect(within(empty).queryByText(/Handle those automatically/)).toBeNull()
    expect(within(empty).queryByText(/every subscription/)).toBeNull()
    expect(screen.queryByText(EMPTY)).toBeNull()
  })
})

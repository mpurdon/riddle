/**
 * The Priority inbox against the mocks: the rows are the ones the chosen
 * criteria pick (a starred message, the sources of Finds at 70+) and not
 * the rest, and a chip in "What surfaces here" saves through
 * `save_settings` rather than filtering the page alone.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { mockState } from '@/lib/mocks'
import { mountApp } from '@/test/mountApp'
import { describeCriteria } from './PriorityPage'

afterEach(() => cleanup())

describe('Priority', () => {
  it('lists the starred message and the sources of Finds at 70+, and not the rest', async () => {
    mountApp('#/priority')
    const list = await screen.findByRole('listbox', { name: 'Messages' })
    const state = mockState()
    const starred = state.messages.find((m) => m.labels.includes('STARRED'))!
    expect(starred, 'a starred fixture message').toBeTruthy()
    await waitFor(() => expect(within(list).getByText(starred.subject)).toBeTruthy())
    // PR #412 is a Find at 82; both messages in its thread are sources.
    expect(within(list).getAllByText(/PR #412/).length).toBeGreaterThan(0)
    // A routine GitHub notification is none of starred, a chosen Stratum or a Find.
    const routine = state.messages.find((m) => m.id === 'm_work_gh_1')!
    expect(within(list).queryByText(routine.subject)).toBeNull()
    // The header says what was chosen, so the rows are never a mystery.
    expect(screen.getByText('Starred · High and above · Finds 70+')).toBeTruthy()
  })

  it('adding and removing a Stratum saves the choice through save_settings', async () => {
    mountApp('#/priority')
    await screen.findByRole('listbox', { name: 'Messages' })
    fireEvent.click(screen.getByRole('button', { name: 'What surfaces here' }))
    const panel = await screen.findByRole('region', { name: 'What surfaces here' })
    // Strata can number in the hundreds: the row lists only the chosen ones and adds through the filtering picker.
    expect(within(panel).queryByRole('button', { name: 'Finance' })).toBeNull()
    fireEvent.click(within(panel).getByRole('button', { name: 'Add Strata…' }))
    const dialog = await screen.findByRole('dialog', { name: 'Surface these Strata' })
    fireEvent.change(within(dialog).getByLabelText('Filter Strata'), { target: { value: 'fin' } })
    fireEvent.click(await within(dialog).findByLabelText('Finance'))
    fireEvent.click(within(dialog).getByRole('button', { name: /^Add 1/ }))
    await waitFor(() => expect(mockState().settings.priorityInbox.strata).toEqual(['Finance']))
    const remove = await within(panel).findByRole('button', { name: 'Remove Finance' })
    // The rest of the settings travelled with it, untouched.
    expect(mockState().settings.priorityInbox.labels).toEqual(['STARRED'])
    expect(mockState().settings.notifications.mode).toBe('brief')
    fireEvent.click(remove)
    await waitFor(() => expect(mockState().settings.priorityInbox.strata).toEqual([]))
  })

  it('describes the criteria in one line', () => {
    expect(describeCriteria({ strata: [], labels: ['STARRED'], minFindPriority: 70, findTypes: [], minImportance: null })).toBe('Starred · Finds 70+')
    expect(describeCriteria({ strata: ['Finance', 'Travel'], labels: ['STARRED', 'IMPORTANT'], minFindPriority: null, findTypes: ['pay'], minImportance: null })).toBe('Starred, Important · Finance, Travel · Any open Find (Pay)')
    expect(describeCriteria({ strata: [], labels: [], minFindPriority: 85, findTypes: [], minImportance: null })).toBe('Finds 85+')
    expect(describeCriteria({ strata: [], labels: [], minFindPriority: 85, findTypes: [], minImportance: 'high' })).toBe('High and above · Finds 85+')
    // The backend writes no threshold as 0, since TOML has no null.
    expect(describeCriteria({ strata: [], labels: [], minFindPriority: 0, findTypes: [], minImportance: null })).toBe('Any open Find')
  })
})

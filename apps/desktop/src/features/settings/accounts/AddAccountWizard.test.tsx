/**
 * The Add account wizard: the Behaviour step asks for a name and says the
 * rest in one sentence, with the option cards folded under Advanced
 * options; the Done step's first offer is to watch the account sort.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { mockState } from '@/lib/mocks'
import { useUi } from '@/lib/stores'
import { mountApp } from '@/test/mountApp'
import { behaviourSentence } from './AddAccountWizard'

afterEach(() => {
  cleanup()
  useUi.getState().setDigId(null)
})

describe('the behaviour sentence', () => {
  it('says what the account will do, not what the settings are called', () => {
    expect(behaviourSentence({ autonomy: 'observe', projection: 'none', privacy: 'cloud_judgment' }, 'gmail')).toBe('Riddle will observe only, change nothing in Gmail, and use cloud judgment.')
    expect(behaviourSentence({ autonomy: 'act', projection: 'full', privacy: 'local_only' }, 'gmail')).toBe('Riddle will act on its decisions, add labels and archive mail in Gmail, and keep everything on this Mac.')
    expect(behaviourSentence({ autonomy: 'suggest', projection: 'labels_only', privacy: 'cloud_reasoning' }, 'gmail')).toBe('Riddle will suggest moves for you to apply, change nothing in Gmail until it is raised to Act, and use cloud reasoning.')
    expect(behaviourSentence({ autonomy: 'act', projection: 'labels_only', privacy: 'cloud_judgment' }, 'fixture')).toBe('Riddle will act on its decisions, add labels in the test mailbox, and use cloud judgment.')
  })
})

describe('the Add account wizard', () => {
  it('starts from the defaults, states them in one sentence, folds the cards, and offers to watch the new account sort', async () => {
    mountApp('#/settings#add-account')
    const dialog = await screen.findByRole('dialog', { name: 'Add an account' })
    fireEvent.click(within(dialog).getByRole('radio', { name: /Test mailbox/ }))
    fireEvent.click(within(dialog).getByRole('button', { name: 'Continue' }))
    // The fixture's defaults: Cloud judgment, Labels. At Observe the labels wait.
    expect(await within(dialog).findByText('Riddle will observe only, change nothing in the test mailbox until it is raised to Act, and use cloud judgment.')).toBeTruthy()
    const advanced = within(dialog).getByText('Advanced options').closest('details') as HTMLDetailsElement
    expect(advanced.open).toBe(false)
    expect(advanced.contains(within(dialog).getByRole('radiogroup', { name: 'Autonomy' }))).toBe(true)
    expect(advanced.contains(within(dialog).getByRole('radiogroup', { name: 'Privacy' }))).toBe(true)
    expect(within(dialog).getByRole('radio', { name: /Adds Riddle labels\. Nothing leaves the inbox/ }).getAttribute('aria-checked')).toBe('true')

    fireEvent.change(within(dialog).getByPlaceholderText('Side project'), { target: { value: 'Scratch' } })
    fireEvent.change(within(dialog).getByPlaceholderText('me@side.example'), { target: { value: 'me@scratch.example' } })
    fireEvent.click(within(dialog).getByRole('button', { name: 'Create test mailbox' }))
    await within(dialog).findByText('Scratch is connected')
    const created = mockState().digs.find((d) => d.name === 'Scratch')
    expect(created?.projection).toBe('labels_only')
    expect(created?.privacy).toBe('cloud_judgment')

    // The primary action goes to the Summary for this account; the settings link stays as a secondary one.
    expect(within(dialog).getByRole('button', { name: 'Open the account settings' })).toBeTruthy()
    fireEvent.click(within(dialog).getByRole('button', { name: 'Watch it sort' }))
    await waitFor(() => expect(window.location.hash).toBe('#/'))
    expect(useUi.getState().digId).toBe(created!.id)
    await waitFor(() => expect(screen.getByRole('heading', { level: 1 }).textContent).toBe('Catching up.'))
    expect((await screen.findByRole('region', { name: 'Catching up' })).textContent).toContain('Initial sync: Fetching headers')
  })
})

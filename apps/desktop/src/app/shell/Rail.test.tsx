/**
 * The rail by level, against the mocks. What this guards: Essentials is
 * five entries (six with a model), Everything is the full set without the
 * developer pages, Developer adds Signals and Developer, and the footer
 * switch saves the level through the same Action as Settings, with the
 * Developer flag following the level and never the other way round.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, screen, waitFor } from '@testing-library/react'
import { mockState } from '@/lib/mocks'
import { atLevel, railEntries } from '@/test/level'
import { mountApp } from '@/test/mountApp'

afterEach(() => cleanup())

describe('the rail by level', () => {
  it('shows five entries at Essentials without a model, and offers everything in the footer', async () => {
    mountApp('#/', atLevel('essentials', false))
    expect(await railEntries()).toEqual(['Today', 'Needs you', 'Mail', 'Set aside', 'Settings'])
    expect(screen.getByRole('button', { name: 'Show everything' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Show less' })).toBeNull()
  })

  it('adds the Director at Essentials once a model is set', async () => {
    mountApp('#/', atLevel('essentials'))
    expect(await railEntries()).toEqual(['Today', 'Needs you', 'Mail', 'Set aside', 'Director', 'Settings'])
  })

  it('shows the full set at Everything, without Signals and Developer, and offers less', async () => {
    mountApp('#/', atLevel('everything'))
    const entries = await railEntries()
    for (const label of ['Summary', 'Finds', 'Unresolved', 'Priority', 'Inbox', 'Spoil', 'Contamination', 'Meshes', 'Packs', 'Rule check', 'Strata', 'Labels', 'Director', 'Expeditions', 'Activity', 'Settings']) {
      expect(entries, label).toContain(label)
    }
    expect(entries).not.toContain('Signals')
    expect(entries).not.toContain('Developer')
    expect(screen.getByRole('button', { name: 'Show less' })).toBeTruthy()
  })

  it('shows Signals and Developer only at Developer', async () => {
    mountApp('#/', atLevel('developer'))
    const entries = await railEntries()
    expect(entries).toContain('Signals')
    expect(entries).toContain('Developer')
    expect(screen.queryByRole('button', { name: /^Show (everything|less)$/ })).toBeNull()
  })

  it('the footer switch saves the level through save_settings, and the flag follows the level', async () => {
    mountApp('#/', atLevel('essentials'))
    fireEvent.click(await screen.findByRole('button', { name: 'Show everything' }))
    await waitFor(() => expect(mockState().settings.uiLevel).toBe('everything'))
    expect(mockState().settings.developerMode).toBe(false)
    await waitFor(async () => expect(await railEntries()).toContain('Meshes'))
    fireEvent.click(await screen.findByRole('button', { name: 'Show less' }))
    await waitFor(() => expect(mockState().settings.uiLevel).toBe('essentials'))
    await waitFor(async () => expect(await railEntries()).toEqual(['Today', 'Needs you', 'Mail', 'Set aside', 'Director', 'Settings']))
  })
})

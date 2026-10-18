/**
 * Settings → General → How much Riddle shows, against the mocks. What this
 * guards: the three cards switch the level through save_settings, the
 * Developer flag follows the Developer card alone, Essentials trims the
 * tabs to what is left in them, and What changed reaches Activity from
 * Settings when the rail does not list it.
 */
import { afterEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { mockState } from '@/lib/mocks'
import { atLevel } from '@/test/level'
import { mountApp } from '@/test/mountApp'

afterEach(() => cleanup())

const cards = () => screen.getByRole('radiogroup', { name: 'How much Riddle shows' })

describe('the level cards', () => {
  it('switch the level through save_settings; the Developer flag follows the Developer card alone', async () => {
    mountApp('#/settings', atLevel('essentials'))
    await screen.findByRole('radiogroup', { name: 'How much Riddle shows' })
    expect(within(cards()).getByRole('radio', { name: /^Essentials/ }).getAttribute('aria-checked')).toBe('true')
    fireEvent.click(within(cards()).getByRole('radio', { name: /^Developer/ }))
    await waitFor(() => expect(mockState().settings.uiLevel).toBe('developer'))
    expect(mockState().settings.developerMode).toBe(true)
    await waitFor(() => expect(within(cards()).getByRole('radio', { name: /^Developer/ }).getAttribute('aria-checked')).toBe('true'))
    fireEvent.click(within(cards()).getByRole('radio', { name: /^Everything/ }))
    await waitFor(() => expect(mockState().settings.uiLevel).toBe('everything'))
    expect(mockState().settings.developerMode).toBe(false)
  })

  it('trim the tabs at Essentials to Privacy and Never Hidden, and What changed opens Activity', async () => {
    mountApp('#/settings', atLevel('essentials'))
    await screen.findByRole('radiogroup', { name: 'How much Riddle shows' })
    expect(screen.getByRole('tab', { name: 'Privacy' })).toBeTruthy()
    expect(screen.getByRole('tab', { name: 'Never Hidden' })).toBeTruthy()
    expect(screen.queryByRole('tab', { name: 'Providers' })).toBeNull()
    expect(screen.queryByRole('tab', { name: 'Mail rules' })).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Open What changed' }))
    await waitFor(() => expect(window.location.hash).toBe('#/activity'))
  })

  it('keep the full tabs at Everything, and the What changed card is not needed', async () => {
    mountApp('#/settings', atLevel('everything'))
    await screen.findByRole('radiogroup', { name: 'How much Riddle shows' })
    expect(screen.getByRole('tab', { name: 'Providers' })).toBeTruthy()
    expect(screen.getByRole('tab', { name: 'Mail rules' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Open What changed' })).toBeNull()
  })
})

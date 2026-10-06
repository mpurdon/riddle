/**
 * The notification settings: what each mode shows, and that a bad brief
 * time is refused here with the reason beside it rather than saved for the
 * backend to reject.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { registerMocks, resetMockState } from '@/lib/mocks'
import { buildSettings } from '@/lib/mocks/fixtures'
import type { Settings } from '@/lib/types'
import { TooltipProvider } from '@/components/ui/Tooltip'
import { NotificationsSection, timeProblem } from './Notifications'

beforeAll(() => {
  class RO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', RO)
})
afterEach(() => cleanup())

function mount(patch: Partial<Settings['notifications']> = {}, mode?: 'brief' | 'stream') {
  resetMockState()
  registerMocks()
  const settings = buildSettings()
  settings.notifications = { ...settings.notifications, ...patch, ...(mode ? { mode } : {}) }
  const save = vi.fn()
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={qc}>
      <TooltipProvider>
        <NotificationsSection settings={settings} save={save} />
      </TooltipProvider>
    </QueryClientProvider>,
  )
  return { save, settings }
}

describe('a brief time', () => {
  it('must read as HH:MM on a 24-hour clock and not already be listed', () => {
    expect(timeProblem('16:30', [])).toBeNull()
    expect(timeProblem('9:00', [])).toBe('Use HH:MM on a 24-hour clock, for example 16:30')
    expect(timeProblem('24:00', [])).toBe('Use HH:MM on a 24-hour clock, for example 16:30')
    expect(timeProblem('09:60', [])).toBe('Use HH:MM on a 24-hour clock, for example 16:30')
    expect(timeProblem('09:00', ['09:00'])).toBe('Already in the list')
  })
})

describe('Brief mode', () => {
  it('shows the times, the allow-list and the Never Hidden switch, with the defaults on', () => {
    const { settings } = mount()
    expect(screen.getByRole('radio', { name: /Brief/ }).getAttribute('aria-checked')).toBe('true')
    expect(screen.getByLabelText('Brief time 09:00')).toBeTruthy()
    expect(screen.getByLabelText('Brief time 16:00')).toBeTruthy()
    for (const t of ['Pay', 'Respond']) expect((screen.getByLabelText(t) as HTMLInputElement).checked).toBe(true)
    for (const t of ['Decide', 'Review', 'Do', 'Attend', 'Follow Up', 'Know']) expect((screen.getByLabelText(t) as HTMLInputElement).checked).toBe(false)
    expect((screen.getByLabelText('Due within hours') as HTMLInputElement).value).toBe(String(settings.notifications.dueWithinHours))
    expect(screen.getByRole('switch', { name: 'Never Hidden senders interrupt at once' }).getAttribute('aria-checked')).toBe('true')
    expect(screen.queryByLabelText('Notify priority threshold')).toBeNull()
    expect(screen.getByText('The dock badge counts open Finds plus Unresolved. A notification says the type and the reason, never the subject.')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Test Notification' })).toBeTruthy()
  })

  it('refuses a bad time beside the field and saves a good one in order', () => {
    const { save } = mount()
    fireEvent.click(screen.getByRole('button', { name: 'Add a time' }))
    const field = screen.getByLabelText('New brief time') as HTMLInputElement
    fireEvent.change(field, { target: { value: '9:00' } })
    fireEvent.keyDown(field, { key: 'Enter' })
    expect(screen.getByRole('alert').textContent).toBe('Use HH:MM on a 24-hour clock, for example 16:30')
    expect(field.getAttribute('aria-invalid')).toBe('true')
    expect(save).not.toHaveBeenCalled()
    fireEvent.change(field, { target: { value: '12:30' } })
    fireEvent.keyDown(field, { key: 'Enter' })
    expect(save).toHaveBeenCalledTimes(1)
    expect(save.mock.calls[0][0].notifications.briefTimes).toEqual(['09:00', '12:30', '16:00'])
  })

  it('keeps at least one time and at most four, saying why on the disabled control', () => {
    mount({ briefTimes: ['09:00'] })
    expect(screen.getByLabelText('Keep at least one time')).toBeTruthy()
    cleanup()
    mount({ briefTimes: ['07:00', '09:00', '12:00', '16:00'] })
    expect(screen.getByLabelText('4 times is the most')).toBeTruthy()
  })

  it('removes a time and toggles a type without touching the rest', () => {
    const { save } = mount()
    fireEvent.click(screen.getByRole('button', { name: 'Remove 09:00' }))
    expect(save.mock.calls[0][0].notifications.briefTimes).toEqual(['16:00'])
    fireEvent.click(screen.getByLabelText('Decide'))
    expect(save.mock.calls[1][0].notifications.immediateTypes).toEqual(['respond', 'decide', 'pay'])
    fireEvent.click(screen.getByLabelText('Pay'))
    expect(save.mock.calls[2][0].notifications.immediateTypes).toEqual(['respond'])
  })

  it('switching the mode saves it', () => {
    const { save } = mount()
    fireEvent.click(screen.getByRole('radio', { name: /Stream/ }))
    expect(save.mock.calls[0][0].notifications.mode).toBe('stream')
  })
})

describe('Stream mode', () => {
  it('shows the threshold, says it is Stream only, and explains it with the open Finds', async () => {
    mount({}, 'stream')
    expect(screen.getByRole('radio', { name: /Stream/ }).getAttribute('aria-checked')).toBe('true')
    const threshold = screen.getByLabelText('Notify priority threshold') as HTMLInputElement
    expect(threshold.value).toBe('70')
    expect(within(threshold.closest('label')!).getByText('Stream mode only')).toBeTruthy()
    expect(await screen.findByText(/would have notified you/)).toBeTruthy()
    expect(screen.queryByLabelText('Brief time 09:00')).toBeNull()
  })
})

/**
 * The palette against real bindings: a page mounts `useShortcuts` the way
 * Finds does, so the rows come from the table and the store, not from a
 * fixture. What this guards: a command that is not bound must be absent
 * (not greyed), Enter must run the preselected row, and the search row must
 * navigate rather than send anything.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { MemoryRouter, useLocation } from 'react-router-dom'
import { useShortcuts } from '@/lib/useShortcuts'
import { useDirector } from '@/lib/stores'
import { CommandPalette, matchesQuery, sectionsFor } from './CommandPalette'

beforeAll(() => {
  Element.prototype.scrollIntoView = vi.fn()
})
afterEach(() => {
  cleanup()
  useDirector.getState().setPrefill(null)
})

function Where() {
  const loc = useLocation()
  return <div data-testid="where">{loc.pathname + loc.search}</div>
}

/** A page that binds like Finds does, plus the Shell's globals. */
function Page({ resolve, goFinds }: { resolve: () => void; goFinds: () => void }) {
  useShortcuts('global', { go_finds: goFinds, help: () => {}, palette: () => {}, search: () => {} })
  useShortcuts('finds', { resolve })
  return null
}

function renderPalette(opts: { resolve?: () => void; goFinds?: () => void; onOpenChange?: (o: boolean) => void } = {}) {
  const resolve = opts.resolve ?? vi.fn()
  const goFinds = opts.goFinds ?? vi.fn()
  const onOpenChange = opts.onOpenChange ?? vi.fn()
  render(
    <MemoryRouter initialEntries={['/finds']}>
      <Page resolve={resolve} goFinds={goFinds} />
      <CommandPalette open onOpenChange={onOpenChange} />
      <Where />
    </MemoryRouter>,
  )
  return { resolve, goFinds, onOpenChange, input: screen.getByRole('combobox', { name: 'Command' }) }
}

describe('the command palette', () => {
  it('opening lists a bound page command with its key, and Enter runs it and closes the palette', async () => {
    const { resolve, onOpenChange, input } = renderPalette()
    const row = screen.getByRole('option', { name: /Resolve: mark the Find as dealt with/ })
    expect(within(row).getByText('e').tagName).toBe('KBD')
    expect(row.getAttribute('aria-selected'), 'the first match is preselected').toBe('true')
    // Unbound commands are absent, not greyed: nothing on this page archives.
    expect(screen.queryByText(/Archive: record it as Filed/)).toBeNull()
    // The palette, Esc and / have no row; the search row stands in for /.
    expect(screen.queryByText(/Command palette: every command/)).toBeNull()
    expect(screen.queryByText(/Ask Riddle: search/)).toBeNull()
    fireEvent.keyDown(input, { key: 'Enter' })
    expect(onOpenChange).toHaveBeenCalledWith(false)
    await waitFor(() => expect(resolve).toHaveBeenCalledOnce())
  })

  it('typing filters on every word of the label, and the synthetic rows follow the text', () => {
    const { input } = renderPalette()
    fireEvent.change(input, { target: { value: 'go finds' } })
    expect(screen.getByRole('option', { name: /Go to Finds/ })).toBeTruthy()
    expect(screen.queryByRole('option', { name: /Resolve/ })).toBeNull()
    expect(screen.getByRole('option', { name: 'Search mail for “go finds”' })).toBeTruthy()
    expect(screen.getByRole('option', { name: 'Ask the Director: “go finds”' })).toBeTruthy()
  })

  it('the search row navigates to the Inbox with the text as the query', async () => {
    const { input } = renderPalette()
    fireEvent.change(input, { target: { value: 'from:bob' } })
    // No command matches, so the search row is first and preselected.
    expect(screen.getByRole('option', { name: 'Search mail for “from:bob”' }).getAttribute('aria-selected')).toBe('true')
    fireEvent.keyDown(input, { key: 'Enter' })
    await waitFor(() => expect(screen.getByTestId('where').textContent).toBe('/inbox?q=from%3Abob'))
  })

  it('the ask row prefills the Director and goes there without sending', async () => {
    const { input } = renderPalette()
    fireEvent.change(input, { target: { value: 'what is due' } })
    fireEvent.keyDown(input, { key: 'ArrowDown' })
    expect(screen.getByRole('option', { name: 'Ask the Director: “what is due”' }).getAttribute('aria-selected')).toBe('true')
    fireEvent.keyDown(input, { key: 'Enter' })
    await waitFor(() => expect(screen.getByTestId('where').textContent).toBe('/director'))
    expect(useDirector.getState().prefill).toBe('what is due')
  })

  it('arrows move the selection and a go-to row runs its handler', async () => {
    const { goFinds, input } = renderPalette()
    fireEvent.change(input, { target: { value: 'find' } })
    // "Go to Finds" matches, as does "Resolve: mark the Find…"; the page row is first.
    expect(screen.getByRole('option', { name: /Resolve/ }).getAttribute('aria-selected')).toBe('true')
    fireEvent.keyDown(input, { key: 'ArrowDown' })
    expect(screen.getByRole('option', { name: /Go to Finds/ }).getAttribute('aria-selected')).toBe('true')
    fireEvent.keyDown(input, { key: 'Enter' })
    await waitFor(() => expect(goFinds).toHaveBeenCalledOnce())
  })

  it('shows the hint that teaches the key', () => {
    renderPalette()
    expect(screen.getByText('Every command shows its key, so you can skip the palette next time.')).toBeTruthy()
  })
})

describe('the palette filter and sections', () => {
  it('matches every typed word anywhere in the label, ignoring case', () => {
    expect(matchesQuery('Go to Finds', 'FIND')).toBe(true)
    expect(matchesQuery('Go to Finds', 'go find')).toBe(true)
    expect(matchesQuery('Go to Finds', 'go spoil')).toBe(false)
    expect(matchesQuery('Go to Finds', '')).toBe(true)
  })

  it('drops empty sections and adds the synthetic rows only once there is text', () => {
    const run = () => {}
    const cmds = [
      { id: 'go_finds', scope: 'global' as const, label: 'Go to Finds', keys: 'g f', run },
      { id: 'palette', scope: 'global' as const, label: 'Command palette', keys: 'mod+k', run },
      { id: 'undo', scope: 'global' as const, label: 'Undo the last undoable action', keys: 'u', run },
    ]
    const synthetic = { search: () => {}, ask: () => {} }
    expect(sectionsFor(cmds, '', synthetic).map((s) => s.title)).toEqual(['Go to', 'Also'])
    expect(sectionsFor(cmds, '  ', synthetic).map((s) => s.title)).toEqual(['Go to', 'Also'])
    expect(sectionsFor(cmds, 'zzz', synthetic).map((s) => s.title)).toEqual(['Search and ask'])
    expect(sectionsFor(cmds, '', synthetic).flatMap((s) => s.rows.map((r) => r.key))).not.toContain('global:palette')
  })
})

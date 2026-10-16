/**
 * The Finds and Unresolved pages against the mocks. These exist for the
 * two study findings a pure function cannot prove: that resolving the Find
 * in the URL leaves j/k and the pane working, and that `f` in Unresolved
 * asks for the type instead of assuming Respond.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { mockState } from '@/lib/mocks'
import type { MockState } from '@/lib/mocks/state'
import { mountApp } from '@/test/mountApp'

afterEach(() => cleanup())

const selectedRow = (list: HTMLElement) => within(list).getByRole('option', { selected: true })

describe('Finds', () => {
  it('shows the reason the backend wrote under each title, under Needs you / Later / Done tabs', async () => {
    mountApp('#/finds')
    expect(await screen.findByText('Jamie requested your review directly; a follow-up says it is blocking the merge.')).toBeTruthy()
    for (const name of ['Needs you', 'Later', 'Done']) expect(screen.getByRole('tab', { name })).toBeTruthy()
    expect(screen.queryByText(/thread_state/)).toBeNull()
  })

  it('shows the mail first and opens Why? as a drawer on w', async () => {
    mountApp('#/finds/find_review_412')
    const list = await screen.findByRole('listbox', { name: 'Finds' })
    await waitFor(() => expect(selectedRow(list).textContent).toContain('PR #412'))
    expect(screen.queryByRole('complementary', { name: 'Why this Find was raised' })).toBeNull()
    // The formatted mail is read without a click; the raw sources stay folded.
    const article = await screen.findByRole('article', { name: 'Message' })
    const sources = await screen.findByText(/Sources ·/)
    expect(article.compareDocumentPosition(sources) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect((sources.closest('details') as HTMLDetailsElement).open).toBe(false)
    fireEvent.keyDown(window, { key: 'w' })
    const drawer = await screen.findByRole('complementary', { name: 'Why this Find was raised' })
    await waitFor(() => expect(drawer.textContent).toContain('How it was decided'))
    fireEvent.click(within(drawer).getByRole('button', { name: 'Close' }))
    await waitFor(() => expect(screen.queryByRole('complementary', { name: 'Why this Find was raised' })).toBeNull())
    // Decide again sends the message back to Sift: the Find closes as Superseded and leaves Needs you.
    fireEvent.keyDown(window, { key: 'w' })
    const again = await screen.findByRole('complementary', { name: 'Why this Find was raised' })
    fireEvent.click(within(again).getByRole('button', { name: 'Decide again' }))
    await waitFor(() => expect(mockState().finds.find((f) => f.id === 'find_review_412')?.status).toBe('superseded'))
    await waitFor(() => expect(within(list).queryByText('PR #412')).toBeNull())
  })

  it('copies a link to the message that raised the Find', async () => {
    mountApp('#/finds/find_review_412')
    // The pane follows the cursor, which starts where the last test left it; wait for this Find's mail.
    const list = await screen.findByRole('listbox', { name: 'Finds' })
    await waitFor(() => expect(selectedRow(list).textContent).toContain('PR #412'))
    await waitFor(() => expect(screen.getByRole('article', { name: 'Message' }).textContent).toContain('Jamie Okafor'))
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    fireEvent.click(screen.getByRole('button', { name: 'Copy message link' }))
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('riddle://message/dig_work/m_work_pr412 (Jamie Okafor · [northwind/platform] Review requested: Retry budget for the projection outbox (PR #412))'))
    // The subject of the message that raised it, not the Find's title.
    fireEvent.click(screen.getByRole('button', { name: 'Copy subject' }))
    await waitFor(() => expect(writeText).toHaveBeenLastCalledWith('[northwind/platform] Review requested: Retry budget for the projection outbox (PR #412)'))
  })

  it('keeps j/k and the pane alive after the Find in the URL is resolved with e', async () => {
    mountApp('#/finds/find_review_412')
    const list = await screen.findByRole('listbox', { name: 'Finds' })
    await waitFor(() => expect(selectedRow(list).textContent).toContain('PR #412'))
    const rows = within(list).getAllByRole('option').length
    fireEvent.keyDown(window, { key: 'e' })
    await waitFor(() => expect(mockState().finds.find((f) => f.id === 'find_review_412')?.status).toBe('resolved'))
    await waitFor(() => expect(within(list).getAllByRole('option').length).toBe(rows - 1))
    // The pane follows the cursor, which now rests on another open Find; the URL follows too.
    const after = selectedRow(list).textContent ?? ''
    expect(after).not.toContain('PR #412')
    await waitFor(() => expect(window.location.hash).not.toBe('#/finds/find_review_412'))
    expect(window.location.hash).toMatch(/^#\/finds\/find_/)
    // j still moves, and the URL keeps up.
    fireEvent.keyDown(window, { key: 'j' })
    await waitFor(() => expect(selectedRow(list).textContent).not.toBe(after))
    const next = within(list).getAllByRole('option').findIndex((o) => o.getAttribute('aria-selected') === 'true')
    expect(next).toBeGreaterThan(0)
    await waitFor(() => expect(window.location.hash).toMatch(/^#\/finds\/find_/))
    await waitFor(() => expect(within(list).getAllByRole('option')[next].getAttribute('aria-selected')).toBe('true'))
  })

  it('shows who closed a Find in plain words on the Done tab', async () => {
    mountApp('#/finds')
    await screen.findByText('7 need you')
    // Radix tabs activate on pointer down, not click.
    const done = screen.getByRole('tab', { name: 'Done' })
    fireEvent.mouseDown(done, { button: 0 })
    await waitFor(() => expect(done.getAttribute('data-state')).toBe('active'))
    // The fixtures close two Finds: one resolved by a reply, one dismissed by hand.
    expect(await screen.findByText('2 done')).toBeTruthy()
    expect((await screen.findAllByText('Resolved by your reply')).length).toBeGreaterThan(0)
    expect(screen.getAllByText('Dismissed by you').length).toBeGreaterThan(0)
    expect(screen.queryByText(/thread_state/)).toBeNull()
  })
})

describe('Unresolved', () => {
  const seedCluster = (state: MockState) => {
    for (const id of ['m_work_gh_1', 'm_work_gh_2', 'm_work_gh_3']) state.classifications.get(id)!.disposition = { kind: 'unresolved' }
  }

  it('f opens the type picker rather than assuming Respond, and the decision says what it taught', async () => {
    mountApp('#/unresolved')
    const list = await screen.findByRole('listbox', { name: 'Unresolved messages' })
    await waitFor(() => expect(within(list).getAllByRole('option').length).toBe(5))
    const first = selectedRow(list).textContent ?? ''
    fireEvent.keyDown(window, { key: 'f' })
    const picker = await screen.findByRole('dialog', { name: 'Decide: a Find of type' })
    expect(within(picker).getByText('Respond')).toBeTruthy()
    expect(within(picker).getByText('Review')).toBeTruthy()
    fireEvent.keyDown(window, { key: '3' })
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Decide: a Find of type' })).toBeNull())
    // The selected row is whichever Unresolved message sorts first; find it by subject.
    const decided = mockState().messages.find((m) => first.includes(m.subject))
    expect(decided, `a fixture message for the selected row: ${first}`).toBeTruthy()
    await waitFor(() => expect(mockState().classifications.get(decided!.id)?.disposition).toEqual({ kind: 'find', find_type: 'review' }))
    expect(await screen.findByText(/Recorded as a correction: Find: Review\. (Nothing else from .+ is Unresolved|\d+ more from .+ (is|are) still Unresolved)\./)).toBeTruthy()
  })

  it('groups three or more from one sender under a header that can decide them all', async () => {
    mountApp('#/unresolved', seedCluster)
    expect(await screen.findByText('3 from notifications@github.example')).toBeTruthy()
    const list = await screen.findByRole('listbox', { name: 'Unresolved messages' })
    await waitFor(() => expect(within(list).getAllByRole('option').length).toBe(8))
    // The fixtures carry a second cluster (three monitoring alerts), so there may be more than one header.
    // The trigger's accessible name is the Menu label ("Decide all 3 from … as"), which a caller's aria-label now sets.
    expect(screen.getAllByRole('button', { name: /^Decide all \d+ from .+ as$/ }).length).toBeGreaterThanOrEqual(1)
  })

  it('n no longer files the message and ! marks Contamination', async () => {
    mountApp('#/unresolved')
    const list = await screen.findByRole('listbox', { name: 'Unresolved messages' })
    await waitFor(() => expect(within(list).getAllByRole('option').length).toBe(5))
    const before = new Map([...mockState().classifications].map(([k, v]) => [k, v.disposition?.kind]))
    fireEvent.keyDown(window, { key: 'n' })
    await new Promise((r) => setTimeout(r, 50))
    expect(new Map([...mockState().classifications].map(([k, v]) => [k, v.disposition?.kind]))).toEqual(before)
    fireEvent.keyDown(window, { key: '!' })
    await waitFor(() => expect([...mockState().classifications.values()].some((c) => c.disposition?.kind === 'contamination' && before.get('m_work_quickone') !== 'contamination')).toBe(true))
  })
})

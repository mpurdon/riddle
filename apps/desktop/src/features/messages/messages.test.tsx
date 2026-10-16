/**
 * The message surfaces against the mocks: one list model (click previews,
 * Enter opens, the page steps with n/p), the scoped correction sheet on
 * Spoil, Spoil by sender with its one-click rule, and the Shadow review
 * list. The virtualiser renders nothing in a box of zero height; the
 * shared setup (`vitest.setup.ts`) gives every element a size.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, screen, waitFor, within } from '@testing-library/react'
import { listMessages } from '@/lib/mocks/queries'
import type { MockState } from '@/lib/mocks/state'
import { dispositionLabel } from '@/lib/selectors'
import { YOUR_CORRECTIONS_MESH } from '@/features/rules/yourCorrections'
import { mountApp } from '@/test/mountApp'
import { SPOIL_VIEW_KEY } from './senderGroups'

afterEach(() => {
  cleanup()
  // The by-sender view is remembered per machine; a test that chose it must not choose for the next.
  localStorage.removeItem(SPOIL_VIEW_KEY)
  for (const k of Object.keys(localStorage)) if (k.startsWith('riddle.mailColumn.')) localStorage.removeItem(k)
})

const key = (k: string) => fireEvent.keyDown(window, { key: k })
const firstPage = { cursor: null, limit: 50 }

describe('the Inbox list model', () => {
  it('previews the cursor row beside the list, follows j, and Enter opens the page with the list behind it', async () => {
    const state = mountApp('#/inbox')
    const items = listMessages(state, { digId: null, filter: {}, page: firstPage }).items
    await screen.findAllByRole('option')
    const preview = screen.getByRole('complementary', { name: 'Preview' })
    await waitFor(() => expect(preview.textContent).toContain(items[0].subject))
    key('j')
    await waitFor(() => expect(preview.textContent).toContain(items[1].subject))
    key('Enter')
    await waitFor(() => expect(window.location.hash).toBe(`#/inbox/${items[1].digId}/${items[1].id}`))
    expect(await screen.findByText(`2 of ${items.length}`)).toBeTruthy()
    key('n')
    await waitFor(() => expect(window.location.hash).toBe(`#/inbox/${items[2].digId}/${items[2].id}`))
    expect(await screen.findByText(`3 of ${items.length}`)).toBeTruthy()
    key('p')
    await waitFor(() => expect(window.location.hash).toBe(`#/inbox/${items[1].digId}/${items[1].id}`))
  })

  it('names the target on the toolbar: the cursor row, or the checked count', async () => {
    mountApp('#/inbox')
    const rows = await screen.findAllByRole('option')
    expect(await screen.findByRole('button', { name: 'Archive (cursor)' })).toBeTruthy()
    fireEvent.click(within(rows[0]).getByRole('checkbox'))
    fireEvent.click(within(rows[1]).getByRole('checkbox'))
    expect(await screen.findByRole('button', { name: 'Archive 2 selected' })).toBeTruthy()
  })

})

describe('the mail column', () => {
  /** Two of the fixture's messages carry the person's labels: one `Receipts` (a Stratum's name too), one nested. */
  const withLabels = (state: MockState) => {
    const [a, b] = state.messages
    a.labels = [...a.labels, 'Receipts']
    b.labels = [...b.labels, 'Work/Clients', 'UNREAD']
  }
  const column = () => screen.getByRole('navigation', { name: 'Mail places' })

  it('lists the mailboxes by their Gmail names, and a label with a Stratum of its name as one Topic', async () => {
    mountApp('#/inbox', withLabels)
    const nav = await screen.findByRole('navigation', { name: 'Mail places' })
    await within(nav).findByRole('button', { name: /^Receipts/ })
    for (const name of ['All mail', 'Inbox', 'Starred', 'Sent']) expect(within(nav).getByRole('button', { name: new RegExp(`^${name}`) })).toBeTruthy()
    expect(nav.textContent).not.toMatch(/CATEGORY_|INBOX/)
    expect(within(nav).getAllByText('+ label').length).toBe(1)
    // Your labels are folded under one line until opened; the merged one is a Topic, not a label.
    fireEvent.click(within(nav).getByRole('button', { name: /1 label with mail/ }))
    expect(within(nav).getByRole('button', { name: /^Clients/ })).toBeTruthy()
  })

  it('a click on a Topic lists its mail, the label or the Stratum, and says where you are', async () => {
    const state = mountApp('#/inbox', withLabels)
    const a = state.messages[0]
    const nav = await screen.findByRole('navigation', { name: 'Mail places' })
    fireEvent.click(await within(nav).findByRole('button', { name: /^Receipts/ }))
    await waitFor(() => expect(window.location.hash).toBe('#/inbox?place=topic%3AReceipts'))
    const want = listMessages(state, { digId: null, filter: { place: 'topic:Receipts' }, page: firstPage }).items
    expect(want.some((m) => m.id === a.id)).toBe(true)
    await waitFor(() => expect(screen.getAllByRole('option').length).toBe(want.length))
    expect(screen.getByRole('heading', { name: 'Receipts' })).toBeTruthy()
    expect(within(nav).getByRole('button', { name: /^Receipts/ }).getAttribute('aria-current')).toBe('page')
  })

  it('pins a place to the top, and g l goes anywhere by name', async () => {
    const state = mountApp('#/inbox', withLabels)
    const nav = await screen.findByRole('navigation', { name: 'Mail places' })
    fireEvent.click(await within(nav).findByRole('button', { name: 'Pin Receipts' }))
    await waitFor(() => expect(state.pinnedPlaces).toEqual(['topic:Receipts']))
    await within(nav).findByText('Pinned')
    expect(within(nav).getByRole('button', { name: 'Unpin Receipts' })).toBeTruthy()
    key('g')
    key('l')
    const input = await screen.findByRole('combobox', { name: 'Label or Topic' })
    fireEvent.change(input, { target: { value: 'clie' } })
    fireEvent.keyDown(input, { key: 'Enter' })
    await waitFor(() => expect(window.location.hash).toBe('#/inbox?place=label%3AWork%2FClients'))
    // The chord's second key went to the chord: no Stratum picker opened behind it.
    expect(screen.queryAllByRole('dialog')).toEqual([])
  })

  it('collapses to icons with ⌘\\ and back', async () => {
    mountApp('#/inbox')
    await screen.findByRole('navigation', { name: 'Mail places' })
    fireEvent.keyDown(window, { key: '\\', metaKey: true, ctrlKey: true })
    expect(await within(column()).findByRole('button', { name: 'Expand the mail column' })).toBeTruthy()
    fireEvent.click(within(column()).getByRole('button', { name: 'Expand the mail column' }))
    expect(await within(column()).findByRole('button', { name: 'Collapse the mail column' })).toBeTruthy()
  })
})

describe('Follow up by a date', () => {
  it('the Follow up menu on a row records a Follow Up Find due on the date picked, and the toast names the date', async () => {
    const state = mountApp('#/inbox')
    const items = listMessages(state, { digId: null, filter: {}, page: firstPage }).items
    await screen.findAllByRole('option')
    // The menu names its trigger after what it acts on; Radix opens on pointer down, not click.
    const trigger = await screen.findByRole('button', { name: 'Follow up by (cursor)' })
    expect(trigger.textContent).toBe('Follow up (cursor)')
    fireEvent.pointerDown(trigger, { button: 0, pointerType: 'mouse' })
    fireEvent.click(trigger)
    fireEvent.click(await screen.findByRole('menuitem', { name: 'Tomorrow morning (8:00)' }))
    await waitFor(() => expect(state.classifications.get(items[0].id)?.disposition).toEqual({ kind: 'find', find_type: 'follow_up' }))
    const find = state.finds.find((f) => f.sources.some((s) => s.messageId === items[0].id))
    expect(find?.findType).toBe('follow_up')
    expect(find?.status).toBe('open')
    const due = new Date(find!.dueAt!)
    const tomorrow = new Date()
    tomorrow.setDate(tomorrow.getDate() + 1)
    expect(due.toDateString()).toBe(tomorrow.toDateString())
    expect(due.getHours()).toBe(8)
    expect(await screen.findByText(/^Marked for follow-up by tomorrow/)).toBeTruthy()
  })

  it('⇧F opens the date picker for the cursor row, and No date opens the Find without one', async () => {
    const state = mountApp('#/inbox')
    const items = listMessages(state, { digId: null, filter: {}, page: firstPage }).items
    await screen.findAllByRole('option')
    fireEvent.keyDown(window, { key: 'F', shiftKey: true })
    const picker = await screen.findByRole('dialog', { name: 'Follow up by' })
    fireEvent.click(within(picker).getByRole('button', { name: /^No date/ }))
    await waitFor(() => expect(state.classifications.get(items[0].id)?.disposition).toEqual({ kind: 'find', find_type: 'follow_up' }))
    const find = state.finds.find((f) => f.sources.some((s) => s.messageId === items[0].id))
    expect(find?.dueAt).toBeNull()
    expect(await screen.findByText(/^Marked for follow-up(?! by)/)).toBeTruthy()
  })
})

describe('the message page opened without a list', () => {
  it('has no n/p and no position', async () => {
    const state = mountApp('#/inbox')
    const items = listMessages(state, { digId: null, filter: {}, page: firstPage }).items
    window.location.hash = `#/inbox/${items[0].digId}/${items[0].id}`
    await screen.findByLabelText('Back')
    expect(screen.queryByText(/^\d+ of \d+$/)).toBeNull()
    expect(screen.queryByLabelText(/^Next message/)).toBeNull()
  })

  it('copies a link to the message that carries its ids and says what it is about', async () => {
    const state = mountApp('#/inbox')
    const items = listMessages(state, { digId: null, filter: {}, page: firstPage }).items
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    window.location.hash = `#/inbox/${items[0].digId}/${items[0].id}`
    await screen.findByLabelText('Back')
    fireEvent.click(screen.getByRole('button', { name: 'Copy message link' }))
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(`riddle://message/${items[0].digId}/${items[0].id} (${items[0].from.name ?? items[0].from.email} · ${items[0].subject})`))
    expect(await screen.findByRole('button', { name: 'Copied' })).toBeTruthy()
  })
})

describe('Spoil review', () => {
  it('Not Spoil opens a sheet scoped to this message or the sender, and Enter restores this message', async () => {
    const state = mountApp('#/spoil')
    const items = listMessages(state, { digId: null, filter: { disposition: 'spoil' }, page: firstPage }).items
    await screen.findAllByRole('option')
    fireEvent.click(await screen.findByRole('button', { name: 'Not Spoil (cursor)' }))
    const sheet = await screen.findByRole('group', { name: /Not Spoil/ })
    const fromSender = items.filter((m) => m.from.email.toLowerCase() === items[0].from.email.toLowerCase()).length
    expect(within(sheet).getByLabelText('this message')).toBeTruthy()
    expect(within(sheet).getByLabelText(`this sender (${fromSender})`)).toBeTruthy()
    fireEvent.keyDown(within(sheet).getByLabelText('this message'), { key: 'Enter' })
    expect(await screen.findByText(/^Restored/)).toBeTruthy()
    expect(state.classifications.get(items[0].id)?.disposition?.kind).not.toBe('spoil')
  })

  it('previews the verdict under the summary and keeps the Inbox keys', async () => {
    mountApp('#/spoil')
    await screen.findAllByRole('option')
    const preview = screen.getByRole('complementary', { name: 'Preview' })
    await waitFor(() => expect(preview.textContent).toContain('Why?'))
    expect(screen.queryByRole('button', { name: 'Delete permanently…' })).toBeNull()
  })
})

describe('Spoil by sender', () => {
  it('shows one row per sender with the count, and Not Spoil for this sender restores them all and adds a rule', async () => {
    const state = mountApp('#/spoil')
    await screen.findAllByRole('option')
    fireEvent.click(screen.getByRole('radio', { name: 'By sender' }))
    const gh = await screen.findByRole('option', { name: 'GitHub, 14 messages' })
    expect(localStorage.getItem(SPOIL_VIEW_KEY)).toBe('sender')
    const ghItems = listMessages(state, { digId: null, filter: { disposition: 'spoil' }, page: firstPage }).items.filter((m) => m.from.email === 'notifications@github.example')
    expect(ghItems).toHaveLength(14)
    // A sender with one message says so in the singular.
    expect(await screen.findByRole('option', { name: 'Morning Brew, 1 message' })).toBeTruthy()
    fireEvent.click(within(gh).getByRole('button', { name: 'Not Spoil for this sender' }))
    expect(await screen.findByText('Rule added: never Spoil from notifications@github.example. Edit it in Settings → Mail rules.')).toBeTruthy()
    for (const m of ghItems) expect(state.classifications.get(m.id)?.disposition?.kind).not.toBe('spoil')
    const mesh = state.meshes.filter((m) => m.name === YOUR_CORRECTIONS_MESH).sort((a, b) => b.version - a.version)[0]
    expect(mesh?.sourceText).toContain('NEVER Spoil IF sender = "notifications@github.example"')
    expect(mesh?.status).toBe('active')
  })

  it('Enter opens the sender inline and j walks into its messages', async () => {
    mountApp('#/spoil')
    await screen.findAllByRole('option')
    fireEvent.click(screen.getByRole('radio', { name: 'By sender' }))
    const gh = await screen.findByRole('option', { name: 'GitHub, 14 messages' })
    expect(gh.getAttribute('aria-expanded')).toBe('false')
    key('Enter')
    await waitFor(() => expect(screen.getByRole('option', { name: 'GitHub, 14 messages' }).getAttribute('aria-expanded')).toBe('true'))
    key('j')
    const preview = screen.getByRole('complementary', { name: 'Preview' })
    await waitFor(() => expect(preview.textContent).toContain('[northwind/platform]'))
  })

  it('the correction sheet offers the domain, and says which rule the sender and domain scopes add', async () => {
    mountApp('#/spoil')
    await screen.findAllByRole('option')
    fireEvent.click(await screen.findByRole('button', { name: 'Not Spoil (cursor)' }))
    const sheet = await screen.findByRole('group', { name: /Not Spoil/ })
    expect(within(sheet).queryByText(/Also adds a rule/)).toBeNull()
    fireEvent.click(within(sheet).getByLabelText(/^this sender \(\d+\)$/))
    expect(within(sheet).getByText('Also adds a rule: never Spoil from this sender.')).toBeTruthy()
    fireEvent.click(within(sheet).getByLabelText(/^this domain \(\d+\)$/))
    expect(within(sheet).getByText('Also adds a rule: never Spoil from this domain.')).toBeTruthy()
  })
})

describe('Shadow review', () => {
  /** The review list as the backend answers it: only the proposals, across every account, each carrying its review. */
  const shadowItems = (state: MockState) => listMessages(state, { digId: null, filter: { shadowOnly: true }, page: firstPage }).items
  const listbox = () => screen.getByRole('listbox', { name: 'Messages' })

  it('lists only the proposals, across accounts, without the real-action toolbar, and Agree marks the row', async () => {
    const state = mountApp('#/inbox?shadow=1')
    const items = shadowItems(state)
    expect(items.length).toBeGreaterThan(1)
    // A proposal is not the account's whole mailbox, and the plain list reads the same messages as unsifted.
    const plain = listMessages(state, { digId: null, filter: {}, page: firstPage }).items
    expect(plain.length).toBeGreaterThan(items.length)
    expect(plain.some((m) => m.shadow)).toBe(false)
    await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(items.length))
    expect(screen.queryByRole('button', { name: 'Archive (cursor)' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Spoil (cursor)' })).toBeNull()
    expect(screen.getAllByRole('button', { name: 'Agree' })).toHaveLength(items.length)
    // Every row says what Riddle would have done, and the count is exact from the first page.
    expect(within(listbox()).getAllByText(/^Would be /)).toHaveLength(items.length)
    expect(await screen.findByText(new RegExp(`^0 of ${items.length} reviewed`))).toBeTruthy()
    key('a')
    await waitFor(() => expect(state.shadowReviews.get(items[0].id)).toBe('agreed'))
    // The toast is titled "Agreed" too; the mark is the one on the row, which the refetched row carries.
    expect(await within(listbox()).findByText('Agreed')).toBeTruthy()
    expect(await screen.findByText(new RegExp(`^1 of ${items.length} reviewed`))).toBeTruthy()
    expect(screen.getAllByRole('button', { name: 'Agree' })).toHaveLength(items.length - 1)
  })

  it('Disagree opens a chooser under the row and records the decision as a correction', async () => {
    const state = mountApp('#/inbox?shadow=1')
    const items = shadowItems(state)
    const target = items[1]
    await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(items.length))
    key('j')
    key('d')
    const chooser = await screen.findByRole('group', { name: 'Disagree: what should it have been?' })
    expect(chooser.textContent).toContain(`Riddle would have: ${dispositionLabel(target.disposition)}`)
    const proposedFiled = target.disposition?.kind === 'filed'
    fireEvent.click(within(chooser).getByRole('button', { name: proposedFiled ? 'Spoil' : 'Filed' }))
    await waitFor(() => expect(state.shadowReviews.get(target.id)).toBe('disagreed'))
    expect(state.classifications.get(target.id)?.disposition?.kind).toBe(proposedFiled ? 'spoil' : 'filed')
    await waitFor(() => expect(screen.queryByRole('group', { name: 'Disagree: what should it have been?' })).toBeNull())
  })

  it('a row reviewed earlier carries its mark: no verbs, still the proposal on the chip, counted as reviewed', async () => {
    const state = mountApp('#/inbox?shadow=1', (s) => s.shadowReviews.set(shadowItems(s)[0].id, 'disagreed'))
    const items = shadowItems(state)
    expect(items[0].shadowReview).toBe('disagreed')
    await waitFor(() => expect(screen.getAllByRole('option')).toHaveLength(items.length))
    const rows = screen.getAllByRole('option')
    expect(within(rows[0]).getByText('Disagreed')).toBeTruthy()
    expect(within(rows[0]).queryByRole('button', { name: 'Agree' })).toBeNull()
    expect(within(rows[0]).getByText(`Would be ${dispositionLabel(items[0].disposition)}`)).toBeTruthy()
    expect(screen.getAllByRole('button', { name: 'Agree' })).toHaveLength(items.length - 1)
    expect(await screen.findByText(new RegExp(`^1 of ${items.length} reviewed`))).toBeTruthy()
  })
})

describe('Contamination review', () => {
  it('says the Trash button moves to Gmail\'s Trash', async () => {
    mountApp('#/contamination')
    await screen.findAllByRole('option')
    expect(await screen.findByRole('button', { name: "Move to Gmail's Trash (cursor)…" })).toBeTruthy()
    expect(screen.queryByRole('button', { name: /Delete permanently/ })).toBeNull()
  })
})

/**
 * The Director page against the mocks: the audit findings this guards are
 * proposals that lived only in a session store, a Dismiss that changed
 * nothing server-side, suggestions that sent on click, and an approval
 * dialog whose Enter reflex applied the Mesh.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { MESH_GITHUB_V7 } from '@/lib/mocks/fixtures'
import type { PendingAction } from '@/bindings/PendingAction'
import { u64 } from '@/lib/format'
import { App } from '@/app/app'
import { TwoStepDialog } from '@/components/confirm/TwoStepDialog'

beforeAll(() => {
  class RO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', RO)
  Element.prototype.scrollIntoView = vi.fn()
  Element.prototype.scrollTo = vi.fn() as never
  window.matchMedia = ((q: string) => ({ matches: false, media: q, onchange: null, addListener: vi.fn(), removeListener: vi.fn(), addEventListener: vi.fn(), removeEventListener: vi.fn(), dispatchEvent: vi.fn() })) as never
  localStorage.setItem('riddle.tour.v1', 'done')
})
afterEach(() => cleanup())

function seedProposal(): PendingAction {
  const text = MESH_GITHUB_V7.replace(' v7', ' v8')
  const p: PendingAction = {
    id: 'pend_test_1',
    action: 'update_mesh',
    args: { meshId: 'mesh_github', text, changeNote: 'Director: Spoil routine GitHub mail.' },
    realArguments: { meshId: 'mesh_github', name: 'GitHub Notifications', fromVersion: 7, toVersion: 8, text },
    summary: 'Update Mesh: GitHub Notifications v7 → v8',
    confirmation: 'approval',
    reason: 'The Director proposes; you apply.',
    principal: { kind: 'director', conversation_id: 'conv_gone', read_untrusted: true },
    createdAt: new Date().toISOString(),
  }
  mockState().pending.set(p.id, p)
  return p
}

function renderDirector() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  window.location.hash = '#/director'
  render(
    <QueryClientProvider client={qc}>
      <App />
    </QueryClientProvider>,
  )
}

describe('the Director page', () => {
  it('shows a proposal that only the backend knows about, with the change in one line', async () => {
    resetMockState()
    registerMocks()
    seedProposal()
    renderDirector()
    const panel = await screen.findByRole('region', { name: 'Proposals' })
    await waitFor(() => expect(within(panel).getByText('Update Mesh GitHub Notifications v7 → v8')).toBeTruthy())
    expect(within(panel).getByText('The Director proposes; you apply.')).toBeTruthy()
    expect(within(panel).getByText('read untrusted content')).toBeTruthy()
    // The Mesh text is behind a disclosure, closed by default.
    const details = within(panel).getByText(/^Mesh text/).closest('details')
    expect(details?.open).toBe(false)
    expect(within(panel).getByText(/Preview impact/)).toBeTruthy()
  })

  it('declines through dismiss_pending with the reason typed in place, and the proposal is gone for good', async () => {
    resetMockState()
    registerMocks()
    seedProposal()
    renderDirector()
    const panel = await screen.findByRole('region', { name: 'Proposals' })
    await waitFor(() => expect(within(panel).getByText('Decline')).toBeTruthy())
    fireEvent.click(within(panel).getByText('Decline'))
    const reason = within(panel).getByLabelText('Reason for declining') as HTMLInputElement
    fireEvent.change(reason, { target: { value: 'Too broad' } })
    fireEvent.keyDown(reason, { key: 'Enter' })
    await waitFor(() => expect(mockState().pending.size).toBe(0))
    await waitFor(() => expect(screen.queryByRole('region', { name: 'Proposals' })).toBeNull())
  })

  it('Apply opens the approval dialog on the verb, with focus on Reject, and Reject keeps the proposal', async () => {
    resetMockState()
    registerMocks()
    seedProposal()
    renderDirector()
    const panel = await screen.findByRole('region', { name: 'Proposals' })
    await waitFor(() => expect(within(panel).getByText('Apply')).toBeTruthy())
    fireEvent.click(within(panel).getByText('Apply'))
    const dialog = await screen.findByRole('dialog', { name: 'Update Mesh GitHub Notifications v7 → v8' })
    const reject = within(dialog).getByText('Reject')
    await waitFor(() => expect(document.activeElement).toBe(reject))
    expect(within(dialog).getByText('Apply the Mesh')).toBeTruthy()
    expect(within(dialog).queryByText('Approve')).toBeNull()
    // Rows, not a JSON blob: the Mesh name is a value, and the text is a <pre>.
    expect(within(dialog).getByText('GitHub Notifications')).toBeTruthy()
    expect(dialog.querySelector('pre')?.textContent).toContain('Mesh: GitHub Notifications v8')
    fireEvent.click(reject)
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
    expect(mockState().pending.size).toBe(1)
  })

  it('a suggestion prefills the composer and sends nothing', async () => {
    resetMockState()
    registerMocks()
    mockState().conversations.splice(0)
    renderDirector()
    const box = (await screen.findByLabelText('Message to the Director')) as HTMLTextAreaElement
    fireEvent.click(screen.getByText('What am I waiting for other people to respond to?'))
    expect(box.value).toBe('What am I waiting for other people to respond to?')
    expect(mockState().conversations.length).toBe(0)
    expect(await screen.findByText('Conversations are just talk; what the Director does lands in Finds, drafts, Meshes and Activity.')).toBeTruthy()
    expect(screen.getByText(/Scope: all accounts/)).toBeTruthy()
  })

  it('a failed turn counts down to the retry the provider named, then runs the message again by itself', async () => {
    resetMockState()
    registerMocks()
    const s = mockState()
    s.conversations.splice(0)
    const now = new Date()
    s.conversations.push({
      id: 'conv_tripped',
      title: 'Which invoices are unpaid?',
      turns: [
        { role: 'user', content: 'Which invoices are unpaid?', toolCalls: [], createdAt: now.toISOString(), inputTokens: u64(0), outputTokens: u64(0), costUsd: null },
        { role: 'director', content: 'I could not complete that: circuit open for sonnet; retry in 1s', toolCalls: [], createdAt: now.toISOString(), inputTokens: u64(0), outputTokens: u64(0), costUsd: null, failure: { kind: 'llm_unavailable', retryAfterSecs: 1 } },
      ],
      readUntrusted: false,
      digIds: s.digs.map((d) => d.id),
      modelId: null,
      promptVersion: null,
      createdAt: now.toISOString(),
      updatedAt: now.toISOString(),
    })
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    window.location.hash = '#/director/conv_tripped'
    render(
      <QueryClientProvider client={qc}>
        <App />
      </QueryClientProvider>,
    )
    expect(await screen.findByText(/Retrying in 1s/)).toBeTruthy()
    // The wait passes: the failed turn is gone and the Director answers the same question.
    await waitFor(() => expect(s.conversations[0].turns.some((t) => t.failure)).toBe(false), { timeout: 5000 })
    await waitFor(() => expect(screen.queryByText(/I could not complete that/)).toBeNull(), { timeout: 5000 })
    await waitFor(() => expect(s.conversations[0].turns.at(-1)?.role).toBe('director'), { timeout: 8000 })
    expect(s.conversations[0].turns.at(-1)?.failure).toBeUndefined()
  })

  it('a turn a limit cut short says why and continues on a click, sending "continue"', async () => {
    resetMockState()
    registerMocks()
    const s = mockState()
    s.conversations.splice(0)
    const now = new Date().toISOString()
    s.conversations.push({
      id: 'conv_capped',
      title: 'Daily reminders',
      turns: [
        { role: 'user', content: 'Make a rule for my daily reminders', toolCalls: [], createdAt: now, inputTokens: u64(0), outputTokens: u64(0), costUsd: null },
        { role: 'director', content: 'I simulated four drafts.', toolCalls: [], createdAt: now, inputTokens: u64(0), outputTokens: u64(0), costUsd: null, stoppedEarly: "the turn's tool limit is reached" },
      ],
      readUntrusted: false,
      digIds: s.digs.map((d) => d.id),
      modelId: null,
      promptVersion: null,
      createdAt: now,
      updatedAt: now,
    })
    window.location.hash = '#/director/conv_capped'
    render(
      <QueryClientProvider client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}>
        <App />
      </QueryClientProvider>,
    )
    expect(await screen.findByText(/Stopped early: the turn's tool limit is reached/)).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    await waitFor(() => expect(s.conversations[0].turns.some((t) => t.role === 'user' && t.content === 'continue')).toBe(true))
  })

  it('Enter sends the message and Shift+Enter does not', async () => {
    resetMockState()
    registerMocks()
    mockState().conversations.splice(0)
    renderDirector()
    const box = (await screen.findByLabelText('Message to the Director')) as HTMLTextAreaElement
    fireEvent.change(box, { target: { value: 'Which invoices are unpaid?' } })
    // Shift+Enter is the newline, so nothing leaves; jsdom applies no default action, so only the send is checked.
    fireEvent.keyDown(box, { key: 'Enter', shiftKey: true })
    expect(mockState().conversations.length).toBe(0)
    fireEvent.keyDown(box, { key: 'Enter' })
    await waitFor(() => expect(mockState().conversations.length).toBe(1))
    expect(box.value).toBe('')
  })
})

describe('the two-step dialog', () => {
  const pending: PendingAction = {
    id: 'pend_2',
    action: 'add_never_hidden',
    args: { entry: 'lena@example.com' },
    realArguments: { entry: 'lena@example.com' },
    summary: 'Add lena@example.com to Never Hidden',
    confirmation: 'two_step',
    reason: 'add_never_hidden requires confirmation',
    principal: { kind: 'user' },
    createdAt: new Date().toISOString(),
  }

  it('focuses Cancel and names the verb on the primary', async () => {
    const onConfirm = vi.fn()
    render(<TwoStepDialog pending={pending} onConfirm={onConfirm} onCancel={() => {}} />)
    const dialog = await screen.findByRole('dialog', { name: 'Add lena@example.com to Never Hidden' })
    const cancel = within(dialog).getByText('Cancel')
    await waitFor(() => expect(document.activeElement).toBe(cancel))
    expect(within(dialog).queryByText('Confirm')).toBeNull()
    fireEvent.click(within(dialog).getByText('Add to Never Hidden'))
    expect(onConfirm).toHaveBeenCalledOnce()
  })
})

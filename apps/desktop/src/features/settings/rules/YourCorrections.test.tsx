/**
 * The Your corrections list against the mocks: the Mesh's guard lines read
 * as rules, Remove rewrites the Mesh or deletes it when the last rule goes,
 * and with no Mesh the section says how to make one.
 */
import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { parseMesh } from '@/lib/mocks/queries'
import { guardLine, parseCorrections, withCorrection, YOUR_CORRECTIONS_MESH } from '@/features/rules/yourCorrections'
import { TooltipProvider } from '@/components/ui/Tooltip'
import { YourCorrections } from './YourCorrections'

beforeAll(() => {
  class RO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', RO)
})
afterEach(() => cleanup())

const MESH_ID = 'mesh_corrections'

/** Seed the corrections Mesh into the mock store the way the Spoil page's action would have. */
function seed(lines: string[]) {
  const text = lines.reduce<string | null>((acc, l) => withCorrection(acc, l), null)
  if (!text) return
  const state = mockState()
  const mesh = parseMesh(state, text)
  state.meshes.push({ ...mesh, id: MESH_ID, status: 'active', createdBy: 'user', changeNote: 'Not Spoil for this sender' })
}

function mount(lines: string[] = []) {
  resetMockState()
  seed(lines)
  registerMocks()
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  render(
    <QueryClientProvider client={qc}>
      <TooltipProvider>
        <MemoryRouter>
          <YourCorrections />
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

const latest = () => mockState().meshes.filter((m) => m.id === MESH_ID).sort((a, b) => b.version - a.version)[0]

describe('Your corrections', () => {
  it('lists each guard line as a rule in plain words and links to the Mesh by name', async () => {
    mount([guardLine('sender', 'alerts@uptime.example', 'spoil'), guardLine('domain', '@northwind.example', 'contamination')])
    const list = await screen.findByRole('list', { name: 'Your corrections' })
    const rows = within(list).getAllByRole('listitem')
    expect(rows.map((r) => r.textContent?.replace(/Remove$/, '').trim())).toEqual(['Never Spoil·alerts@uptime.example', 'Never Contamination·@northwind.example'])
    expect(screen.getByRole('link', { name: YOUR_CORRECTIONS_MESH }).getAttribute('href')).toBe('/meshes?name=Your%20corrections')
  })

  it('Remove rewrites the Mesh without that line and leaves the others', async () => {
    mount([guardLine('sender', 'alerts@uptime.example', 'spoil'), guardLine('domain', '@northwind.example', 'contamination')])
    const list = await screen.findByRole('list', { name: 'Your corrections' })
    fireEvent.click(within(within(list).getAllByRole('listitem')[0]).getByRole('button', { name: 'Remove' }))
    await waitFor(() => expect(latest().version).toBe(2))
    expect(parseCorrections(latest().sourceText).map((r) => r.value)).toEqual(['northwind.example'])
    expect(latest().changeNote).toBe('Removed a correction')
    await waitFor(() => expect(within(list).getAllByRole('listitem')).toHaveLength(1))
  })

  it('removing the last rule moves the Mesh to Recently Deleted and shows the empty state', async () => {
    mount([guardLine('sender', 'alerts@uptime.example', 'spoil')])
    const list = await screen.findByRole('list', { name: 'Your corrections' })
    fireEvent.click(within(list).getByRole('button', { name: 'Remove' }))
    await waitFor(() => expect(latest().deletedAt).toBeTruthy())
    expect(await screen.findByRole('status')).toBeTruthy()
    expect(screen.getByRole('status').textContent).toBe('No corrections yet. Not Spoil for this sender on the Spoil page adds one.')
  })

  it('with no Mesh yet, says where a correction will live instead of linking to nothing', async () => {
    mount()
    expect((await screen.findByRole('status')).textContent).toContain('No corrections yet.')
    expect(screen.queryByRole('link')).toBeNull()
    expect(screen.getByText(/appears on the Meshes page once you add one/)).toBeTruthy()
  })
})

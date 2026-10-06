import { afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter, Route, Routes } from 'react-router-dom'
import type { RulePrecision } from '@/bindings/RulePrecision'
import type { TrustStats } from '@/bindings/TrustStats'
import { u64 } from '@/lib/format'
import { mock } from '@/lib/ipc'
import { mockState, registerMocks, resetMockState } from '@/lib/mocks'
import { MESH_GITHUB_V7 } from '@/lib/mocks/fixtures'
import { parseMesh as mockParseMesh } from '@/lib/mocks/queries'
import type { MockState } from '@/lib/mocks/state'
import { keyHint } from '@/lib/shortcuts'
import { useDirector } from '@/lib/stores'
import { TooltipProvider } from '@/components/ui/Tooltip'
import type { MeshDraftState } from '@/features/director/ProposalsPanel'
import { guardLine, withCorrection, YOUR_CORRECTIONS_MESH } from '@/features/rules/yourCorrections'
import MeshesPage from './MeshesPage'
import { dayLabel } from './precision'

beforeAll(() => {
  // jsdom lacks what Radix probes for.
  class RO {
    observe() {}
    unobserve() {}
    disconnect() {}
  }
  vi.stubGlobal('ResizeObserver', RO)
  Element.prototype.scrollIntoView = vi.fn()
  Element.prototype.hasPointerCapture = vi.fn(() => false) as never
  Element.prototype.releasePointerCapture = vi.fn() as never
})
afterEach(() => {
  cleanup()
  useDirector.getState().setPrefill(null)
})

/** `prepare` runs on the fresh mock state before anything is fetched. */
function renderAt(path: string, state?: unknown, prepare?: (s: MockState) => void) {
  const s = resetMockState()
  registerMocks()
  prepare?.(s)
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  return render(
    <QueryClientProvider client={qc}>
      <TooltipProvider>
        <MemoryRouter initialEntries={[{ pathname: path, state }]}>
          <Routes>
            <Route path="/meshes" element={<MeshesPage />} />
            <Route path="/meshes/:meshId" element={<MeshesPage />} />
            <Route path="/director" element={<div>Director page</div>} />
          </Routes>
        </MemoryRouter>
      </TooltipProvider>
    </QueryClientProvider>,
  )
}

const editor = () => screen.findByLabelText('Mesh text') as Promise<HTMLTextAreaElement>

/** Trust stats with only the rule rows that matter to the test. */
const statsWith = (rules: RulePrecision[]): TrustStats => ({ since: new Date().toISOString(), digCount: 3, decisions: u64(0), corrected: u64(0), undone: u64(0), wouldArchive: u64(0), rules, offer: null })
const rule = (meshId: string, version: number, status: RulePrecision['status'], decisions: number, reversed: number, shadow: [number, number] = [0, 0]): RulePrecision => ({ meshId, name: meshId, version, status, decisions: u64(decisions), reversed: u64(reversed), shadowProposed: shadow[0], shadowAgreed: shadow[1], reversedOften: decisions >= 20 && reversed * 4 >= decisions })
const precisionLine = () => screen.findByText(/^Decided/, { selector: 'p' })

/** Every version of a fixture Mesh gets the cap, as the store would carry it. */
const capMesh = (s: MockState, id: string, autonomy: 'observe' | 'suggest' | 'act') => {
  for (const m of s.meshes) if (m.id === id) m.autonomy = autonomy
}

/** The rules Mesh as the Spoil page's "Not Spoil for this sender" would have created it. */
const addRulesMesh = (s: MockState) => {
  const text = withCorrection(null, guardLine('sender', 'alerts@uptime.example', 'spoil'))
  s.meshes.push({ ...mockParseMesh(s, text), id: 'mesh_corrections', status: 'active', priority: 0, createdBy: 'user' })
}

describe('the Mesh list and header', () => {
  it('status chips say what the status means, never the enum', async () => {
    renderAt('/meshes/mesh_github')
    await editor()
    const statuses = mockState().meshes.map((m) => m.status)
    for (const raw of ['shadow', 'active', 'draft', 'disabled', 'deleted']) expect(screen.queryByText(raw, { selector: 'span' })).toBeNull()
    if (statuses.includes('active')) expect(screen.getAllByTitle('Decides on its own').length).toBeGreaterThan(0)
    if (statuses.includes('shadow')) expect(screen.getAllByTitle('Proposes only; you decide').length).toBeGreaterThan(0)
  })

  it('an active Mesh offers Apply to past mail, previewed and confirmed before anything changes', async () => {
    renderAt('/meshes/mesh_github', undefined, (s) => {
      for (const m of s.meshes) if (m.id === 'mesh_github') m.status = 'active'
    })
    await editor()
    const trigger = await screen.findByRole('button', { name: 'Apply to past mail' })
    fireEvent.pointerDown(trigger, { button: 0, pointerType: 'mouse' })
    fireEvent.click(trigger)
    fireEvent.click(await screen.findByRole('menuitem', { name: 'The last 30 days' }))
    // The Action layer answers with a preview to confirm (the Shell's dialog, not rendered here); nothing has changed yet.
    await waitFor(() => expect([...mockState().pending.values()].some((p) => /^Re-decide \d+ messages from the last 30 days under Mesh GitHub Notifications/.test(p.summary))).toBe(true))
    expect(mockState().meshes.find((m) => m.id === 'mesh_github')?.status).toBe('active')
  })

  it('delete says where the Mesh goes', async () => {
    renderAt('/meshes/mesh_github')
    await editor()
    expect(screen.getByText('Move to Recently Deleted')).toBeTruthy()
    expect(screen.getByText(/Show Recently Deleted \(\d+\)/)).toBeTruthy()
  })

  it('a default Mesh is labelled as one rather than as created by "system"', async () => {
    renderAt('/meshes/mesh_github')
    await editor()
    const m = mockState().meshes.find((x) => x.id === 'mesh_github')!
    if (m.createdBy === 'system') expect(screen.getByText('a Riddle default')).toBeTruthy()
    expect(screen.queryByText(/created by system/)).toBeNull()
  })
})

describe('a Mesh capped by an Autonomy line', () => {
  it('wears a "Riddle only" chip in the list and the Structure tab says what the cap does', async () => {
    renderAt('/meshes/mesh_github', undefined, (s) => capMesh(s, 'mesh_github', 'suggest'))
    await editor()
    const chips = screen.getAllByText('Riddle only')
    expect(chips).toHaveLength(1)
    expect(chips[0].getAttribute('title')).toBe("Capped at Suggest: this Mesh's decisions never reach the mailbox, whatever the account allows")
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Structure' }), { button: 0 })
    expect(await screen.findByText('Autonomy: capped at Suggest. Its decisions stay in Riddle whatever the account allows. Remove the Autonomy line to follow the account.')).toBeTruthy()
  })

  it('an uncapped Mesh has no chip and its Structure tab says it follows the account', async () => {
    renderAt('/meshes/mesh_github')
    await editor()
    expect(screen.queryByText('Riddle only')).toBeNull()
    fireEvent.mouseDown(screen.getByRole('tab', { name: 'Structure' }), { button: 0 })
    expect(await screen.findByText('Autonomy: follows the account.')).toBeTruthy()
  })

  it('the Valid line ends with the cap, and Trusted is refused in the parser\'s words', async () => {
    renderAt('/meshes/mesh_github')
    const ta = await editor()
    await screen.findByText(/^Valid:/)
    const base = ta.value
    fireEvent.change(ta, { target: { value: base.replace('\n', '\nAutonomy: Suggest\n') } })
    // The cap is the parser's answer, so the line changes once the debounced parse lands, not on the keystroke.
    await waitFor(() => expect(screen.getByText(/^Valid:/).textContent).toMatch(/ · capped at Suggest \(decisions stay in Riddle\)\.$/))
    fireEvent.change(ta, { target: { value: base.replace('\n', '\nAutonomy: Trusted\n') } })
    // The Syntax box mentions Trusted too; the error is the list item under the editor.
    expect((await screen.findByText(/Trusted is an account setting/, { selector: 'li' })).textContent).toBe('line 2: Autonomy: a Mesh can be capped at Observe, Suggest or Act; Trusted is an account setting')
  })
})

describe('precision under the header', () => {
  it('says what the Mesh decided in the last 30 days and how much of it you reversed, with tooltips on the numbers', async () => {
    renderAt('/meshes/mesh_github', undefined, () => mock('trust_stats', () => statsWith([rule('mesh_github', 6, 'active', 100, 1), rule('mesh_github', 7, 'active', 18, 1)])))
    const line = await precisionLine()
    expect(line.textContent).toBe('Decided 118 in the last 30 days · you reversed 2')
    expect(screen.getByTitle(/Sift decisions any version of this Mesh made in the last 30 days/).textContent).toBe('118')
    expect(screen.getByTitle(/the ones you changed afterwards/).textContent).toBe('2')
    expect(screen.queryByText('reversed often')).toBeNull()
    expect(screen.queryByText(/Stepped back to Shadow/)).toBeNull()
  })

  it('flags a Mesh reversed a quarter of the time or more', async () => {
    renderAt('/meshes/mesh_github', undefined, () => mock('trust_stats', () => statsWith([rule('mesh_github', 7, 'active', 40, 10)])))
    await precisionLine()
    expect(screen.getByText('reversed often').getAttribute('title')).toMatch(/steps a Mesh back to Shadow/)
  })

  it('a Shadow Mesh shows its proposals, and one the backend stepped back says so with the date and the counts it acted on', async () => {
    const at = new Date(Date.now() - 3 * 86_400_000).toISOString()
    renderAt('/meshes/mesh_aws', undefined, (s) => {
      mock('trust_stats', () => statsWith([rule('mesh_aws', 3, 'shadow', 30, 9, [20, 18])]))
      for (const m of s.meshes) if (m.id === 'mesh_aws') m.demoted = { at, reversed: 9, decided: 30 }
    })
    const line = await precisionLine()
    expect(line.textContent).toBe('Decided 30 in the last 30 days · you reversed 9')
    expect(screen.getByText(/^· proposed/, { selector: 'p' }).textContent).toBe('· proposed 20 · you agreed 18 (90%)')
    expect(screen.getByText(`Stepped back to Shadow on ${dayLabel(at)}: you reversed 9 of 30.`)).toBeTruthy()
    // The header no longer repeats the agreement the line already carries.
    expect(screen.queryByText(/Shadow agreement/)).toBeNull()
  })

  it('a Shadow Mesh the backend did not step back is not described as stepped back, whatever was reversed', async () => {
    // The page used to guess a step back from the status and the Shadow start date; the fixture Mesh is in Shadow since three days ago.
    renderAt('/meshes/mesh_aws', undefined, () => mock('trust_stats', () => statsWith([rule('mesh_aws', 3, 'shadow', 30, 9, [20, 18])])))
    const line = await precisionLine()
    expect(line.textContent).toBe('Decided 30 in the last 30 days · you reversed 9')
    expect(mockState().meshes.find((m) => m.id === 'mesh_aws')!.demoted).toBeUndefined()
    expect(screen.queryByText(/Stepped back to Shadow/)).toBeNull()
  })
})

describe('the rules Mesh', () => {
  it('is listed first as your rules, and its editor says where the lines come from', async () => {
    renderAt('/meshes/mesh_corrections', undefined, addRulesMesh)
    const ta = await editor()
    expect(ta.value).toContain('NEVER Spoil IF sender = "alerts@uptime.example"')
    const rows = screen.getAllByRole('option')
    expect(rows[0].textContent).toContain(YOUR_CORRECTIONS_MESH)
    expect(rows[0].textContent).toContain('your rules')
    expect(screen.getAllByText('your rules').length).toBeGreaterThanOrEqual(2)
    expect(screen.getByText(/Built from your corrections on the Spoil and Contamination pages; each line is one sender or domain\. Edit here, or remove lines in/)).toBeTruthy()
    expect(screen.getByRole('link', { name: 'Settings → Mail rules' }).getAttribute('href')).toBe('/settings#rules')
  })

  it('any other Mesh has no such note', async () => {
    renderAt('/meshes/mesh_github', undefined, addRulesMesh)
    await editor()
    expect(screen.queryByText(/Built from your corrections/)).toBeNull()
    expect(screen.getAllByRole('option')[0].textContent).toContain(YOUR_CORRECTIONS_MESH)
  })
})

describe('a draft from a Director proposal', () => {
  it('opens the Mesh the proposal is for, with the draft text and a pending note', async () => {
    const draftText = MESH_GITHUB_V7.replace('v7', 'v8').replace('> .80', '> .85')
    const state: MeshDraftState = { draftText, fromPendingId: 'pa_1', meshId: 'mesh_github', meshName: 'GitHub Notifications' }
    renderAt('/meshes', state)
    const ta = await editor()
    await waitFor(() => expect(ta.value).toBe(draftText))
    expect(screen.getByText('From a Director proposal; the proposal stays pending until you apply or decline it.')).toBeTruthy()
    expect(screen.getByRole('heading', { level: 1, name: /GitHub Notifications v7/ })).toBeTruthy()
    expect(screen.getByText('Apply as v8')).toBeTruthy()
  })

  it('opens the New editor for a proposal that creates a Mesh', async () => {
    const draftText = 'Mesh: Brand New v1\n\nScope:\nAll Digs\n\nROUTES\n\nIF routine@v1 > .90\n    → Spoil\n'
    const state: MeshDraftState = { draftText, fromPendingId: 'pa_2', meshId: null, meshName: 'Brand New' }
    renderAt('/meshes', state)
    const ta = await editor()
    await waitFor(() => expect(ta.value).toBe(draftText))
    expect(screen.getByText('Create')).toBeTruthy()
    expect(screen.getByText(/From a Director proposal/)).toBeTruthy()
  })
})

describe('the editor', () => {
  it('the line-number gutter follows the textarea when it scrolls', async () => {
    renderAt('/meshes/mesh_github')
    const ta = await editor()
    const gutter = ta.previousElementSibling as HTMLElement
    expect(gutter.getAttribute('aria-hidden')).toBe('true')
    let gutterTop = 0
    Object.defineProperty(ta, 'scrollTop', { get: () => 400, configurable: true })
    Object.defineProperty(gutter, 'scrollTop', { get: () => gutterTop, set: (v: number) => { gutterTop = v }, configurable: true })
    fireEvent.scroll(ta)
    expect(gutterTop).toBe(400)
    expect(ta.getAttribute('wrap')).toBe('off')
  })

  it('leaving the editor says what the text would do over the last 30 days, and a second blur is free', async () => {
    renderAt('/meshes/mesh_github')
    const ta = await editor()
    await screen.findByText(/^Valid:/)
    fireEvent.blur(ta)
    const line = await screen.findByText(/Over the last 30 days/, {}, { timeout: 3000 })
    expect(line.textContent).toMatch(/messages? in scope/)
    // The mock's simulate_mesh takes 350ms; a cached blur shows nothing in between.
    fireEvent.blur(ta)
    expect(screen.queryByText(/Checking the last 30 days/)).toBeNull()
    expect(screen.getByText(/Over the last 30 days/)).toBeTruthy()
  })

  it('the Syntax box lists every fact without an ellipsis, and documents the header lines', async () => {
    renderAt('/meshes/mesh_github')
    await editor()
    const syntax = screen.getByText(/^Mesh: <name> vN/)
    expect(syntax.textContent).toContain('known_sender')
    expect(syntax.textContent).toContain('asks_for_payment_or_credentials')
    expect(syntax.textContent).not.toContain('automated_sender, …')
    expect(syntax.textContent).toContain('Autonomy: Observe | Suggest | Act')
    expect(syntax.textContent).toContain('Priority: 0')
  })

  it('⌘I opens the Insert menu and a chosen fact lands at the caret', async () => {
    renderAt('/meshes/mesh_github')
    const ta = await editor()
    const before = ta.value
    ta.focus()
    ta.setSelectionRange(before.length, before.length)
    // jsdom reports no platform, so the table's "mod" is Ctrl here.
    fireEvent.keyDown(window, { key: 'i', ctrlKey: true })
    const item = await screen.findByRole('menuitem', { name: /^has_attachments/ })
    expect(screen.getByRole('menuitem', { name: /^header X-GitHub-Reason/ })).toBeTruthy()
    expect(screen.getByRole('menuitem', { name: /^routine@v1/ })).toBeTruthy()
    expect(screen.getByText('Insert').closest('button')?.getAttribute('title')).toBe(`Insert a header line, fact, mail header or Signal (${keyHint('mesh_insert')})`)
    fireEvent.click(item)
    await waitFor(() => expect(ta.value).toBe(`${before}has_attachments`))
  })

  it('the Insert menu offers the Autonomy header line, which lands on a line of its own', async () => {
    renderAt('/meshes/mesh_github')
    const ta = await editor()
    const before = ta.value
    const endOfFirstLine = before.indexOf('\n')
    ta.focus()
    ta.setSelectionRange(endOfFirstLine, endOfFirstLine)
    fireEvent.keyDown(window, { key: 'i', ctrlKey: true })
    const item = await screen.findByRole('menuitem', { name: /^Autonomy: Suggest/ })
    expect(screen.getByRole('menuitem', { name: /^Scope:/ })).toBeTruthy()
    expect(screen.getByRole('menuitem', { name: /^Priority: 0/ })).toBeTruthy()
    fireEvent.click(item)
    await waitFor(() => expect(ta.value).toBe(before.replace('\n', '\nAutonomy: Suggest\n')))
    expect(ta.value.slice(ta.selectionStart, ta.selectionEnd)).toBe('Suggest')
  })

  it('Ask the Director to draft it prefills the composer and goes to the Director', async () => {
    renderAt('/meshes')
    fireEvent.click(screen.getByText('New'))
    await editor()
    const btn = screen.getByText('Ask the Director to draft it')
    expect(btn.getAttribute('aria-disabled')).toBe('true')
    fireEvent.change(screen.getByLabelText('Describe the rule'), { target: { value: 'file CI failures I am not tagged on.' } })
    fireEvent.click(screen.getByText('Ask the Director to draft it'))
    expect(await screen.findByText('Director page')).toBeTruthy()
    expect(useDirector.getState().prefill).toBe('Draft a Mesh that: file CI failures I am not tagged on. Show me the DSL and what it would do to recent mail before proposing it.')
  })
})

describe('the run order', () => {
  it('opens on the Meshes in the order they run and flags mail an earlier Mesh takes', async () => {
    const { container } = renderAt('/meshes', undefined, () => {
      mock('mesh_order', () => ({
      windowDays: 30,
      messages: u64(120),
      rows: [
        { position: 1, meshId: 'a', meshName: 'Security and account alerts', version: 1, status: 'active', scope: { kind: 'all_digs' }, priority: 0, fallback: false, decided: u64(40), takenFirstBy: [] },
        { position: 2, meshId: 'b', meshName: 'GitHub Notifications', version: 3, status: 'active', scope: { kind: 'all_digs' }, priority: 10, fallback: false, decided: u64(70), takenFirstBy: [{ meshId: 'a', meshName: 'Security and account alerts', priority: 0, count: u64(15) }] },
          { position: 3, meshId: 'c', meshName: 'Updates', version: 1, status: 'active', scope: { kind: 'all_digs' }, priority: 45, fallback: true, decided: u64(10), takenFirstBy: [{ meshId: 'b', meshName: 'GitHub Notifications', priority: 10, count: u64(500) }] },
      ],
      }))
    })
    const table = await screen.findByRole('table', { name: 'Run order' })
    await waitFor(() => expect(table.textContent).toContain('GitHub Notifications'))
    const rows = [...table.querySelectorAll('tbody tr')].map((r) => r.textContent ?? '')
    expect(rows[0]).toContain('Security and account alerts')
    expect(rows[1]).toContain('15 · Security and account alerts')
    expect(rows[2]).toContain('fallback')
    expect(container.textContent).toContain('1 Mesh has mail taken by an earlier one')
  })
})

/**
 * Client-side stores. Everything durable lives in the Rust core; these hold
 * only what the webview needs between renders: toasts, banners, the Dig
 * filter, the Director's streaming state, pending proposals, the composer
 * and the confirmation queue.
 */
import { create } from 'zustand'
import type { PendingAction } from '@/bindings/PendingAction'
import type { ToolCall } from '@/bindings/ToolCall'
import type { Draft } from '@/bindings/Draft'
import type { RiddleEvent } from '@/bindings/RiddleEvent'

// ---------------------------------------------------------------------------
// Toasts: fleeting acknowledgements, and the in-place confirmation tier.

export interface Toast {
  id: number
  title: string
  body?: string
  kind: 'info' | 'success' | 'warn' | 'error'
  /** A single affordance, e.g. Undo. */
  action?: { label: string; onClick: () => void | Promise<void> }
  ttlMs: number
  createdAt: number
}

interface ToastState {
  toasts: Toast[]
  push: (t: Omit<Toast, 'id' | 'createdAt' | 'ttlMs'> & { ttlMs?: number }) => number
  dismiss: (id: number) => void
}

let toastSeq = 0
export const useToasts = create<ToastState>((set) => ({
  toasts: [],
  push: (t) => {
    const id = ++toastSeq
    const toast: Toast = { ...t, id, createdAt: Date.now(), ttlMs: t.ttlMs ?? (t.action ? 8000 : 4000) }
    set((s) => ({ toasts: [...s.toasts.slice(-4), toast] }))
    return id
  },
  dismiss: (id) => set((s) => ({ toasts: s.toasts.filter((x) => x.id !== id) })),
}))

export function toast(title: string, opts: Partial<Omit<Toast, 'id' | 'createdAt' | 'title'>> = {}): number {
  return useToasts.getState().push({ title, kind: opts.kind ?? 'info', ...opts })
}

// ---------------------------------------------------------------------------
// Banners: persistent, until the condition clears.

interface BannerState {
  storeFault: string | null
  updateAvailable: string | null
  setStoreFault: (m: string | null) => void
  setUpdateAvailable: (v: string | null) => void
}

export const useBanners = create<BannerState>((set) => ({
  storeFault: null,
  updateAvailable: null,
  setStoreFault: (storeFault) => set({ storeFault }),
  setUpdateAvailable: (updateAvailable) => set({ updateAvailable }),
}))

// ---------------------------------------------------------------------------
// UI: the Dig filter, dev flag, help sheet.

interface UiState {
  digId: string | null
  setDigId: (id: string | null) => void
  helpOpen: boolean
  setHelpOpen: (v: boolean) => void
  taskCentreOpen: boolean
  setTaskCentreOpen: (v: boolean) => void
  /** Developer overlay chips on Finds and messages (§83). */
  devOverlay: boolean
  setDevOverlay: (v: boolean) => void
}

export const useUi = create<UiState>((set) => ({
  digId: null,
  setDigId: (digId) => set({ digId }),
  helpOpen: false,
  setHelpOpen: (helpOpen) => set({ helpOpen }),
  taskCentreOpen: false,
  setTaskCentreOpen: (taskCentreOpen) => set({ taskCentreOpen }),
  devOverlay: false,
  setDevOverlay: (devOverlay) => set({ devOverlay }),
}))

// ---------------------------------------------------------------------------
// Director: streaming deltas and tool calls per conversation, until done.

export interface LiveTurn {
  text: string
  toolCalls: ToolCall[]
  done: boolean
  error: string | null
  startedAt: number
}

interface DirectorState {
  live: Record<string, LiveTurn>
  prefill: string | null
  setPrefill: (p: string | null) => void
  begin: (conversationId: string) => void
  delta: (conversationId: string, text: string) => void
  toolCall: (conversationId: string, call: ToolCall) => void
  done: (conversationId: string, error: string | null) => void
  clear: (conversationId: string) => void
}

export const useDirector = create<DirectorState>((set) => ({
  live: {},
  prefill: null,
  setPrefill: (prefill) => set({ prefill }),
  begin: (id) => set((s) => ({ live: { ...s.live, [id]: { text: '', toolCalls: [], done: false, error: null, startedAt: Date.now() } } })),
  delta: (id, text) =>
    set((s) => {
      const cur = s.live[id] ?? { text: '', toolCalls: [], done: false, error: null, startedAt: Date.now() }
      return { live: { ...s.live, [id]: { ...cur, text: cur.text + text } } }
    }),
  toolCall: (id, call) =>
    set((s) => {
      const cur = s.live[id] ?? { text: '', toolCalls: [], done: false, error: null, startedAt: Date.now() }
      return { live: { ...s.live, [id]: { ...cur, toolCalls: [...cur.toolCalls, call] } } }
    }),
  done: (id, error) =>
    set((s) => {
      const cur = s.live[id]
      if (!cur) return s
      return { live: { ...s.live, [id]: { ...cur, done: true, error } } }
    }),
  clear: (id) =>
    set((s) => {
      const next = { ...s.live }
      delete next[id]
      return { live: next }
    }),
}))

// ---------------------------------------------------------------------------
// Proposals: ProposalCreated events, resolved to PendingActions on demand.

export interface Proposal {
  pendingId: string
  summary: string
  createdAt: number
}

interface ProposalState {
  proposals: Proposal[]
  add: (p: Proposal) => void
  remove: (pendingId: string) => void
}

export const useProposals = create<ProposalState>((set) => ({
  proposals: [],
  add: (p) => set((s) => (s.proposals.some((x) => x.pendingId === p.pendingId) ? s : { proposals: [...s.proposals, p] })),
  remove: (pendingId) => set((s) => ({ proposals: s.proposals.filter((x) => x.pendingId !== pendingId) })),
}))

// ---------------------------------------------------------------------------
// Composer: one draft open at a time, as a slide-over.

export interface ComposerOpen {
  digId: string
  draft: Draft
  mode: 'new' | 'reply' | 'forward'
}

interface ComposerState {
  open: ComposerOpen | null
  show: (c: ComposerOpen) => void
  close: () => void
}

export const useComposer = create<ComposerState>((set) => ({
  open: null,
  show: (open) => set({ open }),
  close: () => set({ open: null }),
}))

// ---------------------------------------------------------------------------
// Confirmation queue: a PendingAction waiting on a real control (§51).

export interface ConfirmRequest {
  id: number
  pending: PendingAction
  resolve: (confirmed: boolean) => void
}

interface ConfirmState {
  queue: ConfirmRequest[]
  ask: (pending: PendingAction) => Promise<boolean>
  settle: (id: number, confirmed: boolean) => void
}

let confirmSeq = 0
export const useConfirmQueue = create<ConfirmState>((set) => ({
  queue: [],
  ask: (pending) =>
    new Promise<boolean>((resolve) => {
      const id = ++confirmSeq
      set((s) => ({ queue: [...s.queue, { id, pending, resolve }] }))
    }),
  settle: (id, confirmed) =>
    set((s) => {
      const req = s.queue.find((r) => r.id === id)
      req?.resolve(confirmed)
      return { queue: s.queue.filter((r) => r.id !== id) }
    }),
}))

// ---------------------------------------------------------------------------
// Live pipeline: PipelineStage events, kept briefly for the Developer views.

export interface PipelineEntry {
  at: number
  messageId: string
  stage: string
  ms: number
}

interface PipelineState {
  entries: PipelineEntry[]
  push: (e: PipelineEntry) => void
  clear: () => void
}

export const usePipeline = create<PipelineState>((set) => ({
  entries: [],
  push: (e) => set((s) => ({ entries: [...s.entries.slice(-199), e] })),
  clear: () => set({ entries: [] }),
}))

// ---------------------------------------------------------------------------
// Recent events, for the Activity Timeline and the Developer overlay.

export interface SeenEvent {
  at: number
  event: RiddleEvent
}

interface EventLogState {
  events: SeenEvent[]
  push: (e: RiddleEvent) => void
}

export const useEventLog = create<EventLogState>((set) => ({
  events: [],
  push: (event) => set((s) => ({ events: [...s.events.slice(-299), { at: Date.now(), event }] })),
}))

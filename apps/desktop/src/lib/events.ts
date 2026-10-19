/**
 * The event bridge. One Tauri event, `riddle://event`, carries a
 * `RiddleEvent`; the client refetches on signals and never expects data
 * volume in an event (ARCHITECTURE.md §7). In browser mode a mock bus
 * (`emitMock`) drives the same handlers, so the UI behaves identically.
 *
 * The events a pass emits per message are coalesced: a Sift over a hundred
 * messages sends a hundred `message_classified`, and each once refetched
 * the lists, the Summary and the traces, so a catch-up had the Inbox
 * refetching more often than it rendered. Those keys now refetch once a
 * short pause after the last event, and at least once every couple of
 * seconds while the stream keeps coming. A Find arriving still refreshes
 * at once: it is what a notification points at.
 */
import { useEffect } from 'react'
import { useQueryClient, type QueryClient, type QueryKey } from '@tanstack/react-query'
import { bridge, isTauri, log } from './ipc'
import { qk } from './api'
import { useBanners, useDirector, useEventLog, usePipeline, toast } from './stores'
import type { RiddleEvent } from '@/bindings/RiddleEvent'
import { num } from './format'

export const EVENT_NAME = 'riddle://event'

/** How long after the last event of a storm a key refetches. */
export const COALESCE_MS = 400
/** A stream that never pauses still refetches this often. */
export const COALESCE_MAX_MS = 2_000

const waiting = new Map<string, { timer: ReturnType<typeof setTimeout>; since: number }>()

/** Refetch `key` once the events naming it pause, or once `COALESCE_MAX_MS` has passed since the first of them. */
function invalidateLater(qc: QueryClient, key: QueryKey): void {
  const id = JSON.stringify(key)
  const now = Date.now()
  const w = waiting.get(id)
  if (w) clearTimeout(w.timer)
  const since = w?.since ?? now
  const fire = () => {
    waiting.delete(id)
    void qc.invalidateQueries({ queryKey: key })
  }
  const delay = Math.min(COALESCE_MS, since + COALESCE_MAX_MS - now)
  if (delay <= 0) return fire()
  waiting.set(id, { timer: setTimeout(fire, delay), since })
}

/** Drop every refetch still waiting: the shell unmounting, and a test tearing down. */
export function clearPendingInvalidations(): void {
  for (const w of waiting.values()) clearTimeout(w.timer)
  waiting.clear()
}

type Listener = (e: RiddleEvent) => void
const listeners = new Set<Listener>()

/** Subscribe to every event, in either mode. Returns an unsubscribe. */
export function onEvent(fn: Listener): () => void {
  listeners.add(fn)
  return () => {
    listeners.delete(fn)
  }
}

/** Browser-mode bus: the mocks emit through here. */
export function emitMock(e: RiddleEvent): void {
  if (isTauri) return
  dispatch(e)
}

function dispatch(e: RiddleEvent): void {
  for (const l of Array.from(listeners)) {
    try {
      l(e)
    } catch (err) {
      log('error', 'ui', `event listener failed for ${e.type}: ${String((err as Error)?.message ?? err)}`)
    }
  }
}

/** Route an event to query invalidations and stores. Pure of React. */
export function handleEvent(qc: QueryClient, e: RiddleEvent): void {
  useEventLog.getState().push(e)
  const inv = (key: QueryKey) => void qc.invalidateQueries({ queryKey: key })
  const later = (key: QueryKey) => invalidateLater(qc, key)
  switch (e.type) {
    case 'job_changed':
      inv(qk.taskCentre)
      inv(qk.summary)
      break
    case 'job_progress':
      // Per step of a job, on top of the Task Centre's own poll.
      later(qk.taskCentre)
      break
    case 'dig_status_changed':
      inv(qk.digs)
      inv(qk.taskCentre)
      inv(qk.summary)
      break
    case 'find_created':
    case 'find_changed':
      inv(qk.finds)
      inv(qk.summary)
      if (e.type === 'find_changed') inv(qk.find(e.find_id))
      break
    case 'activity_appended':
      later(qk.activity)
      break
    case 'message_classified':
    case 'messages_changed':
      // One per message during a pass: the families wait for the storm to pause.
      later(qk.messages)
      later(['thread'])
      later(['message'])
      later(qk.summary)
      later(qk.recentTraces)
      if (e.type === 'message_classified') {
        // The one message open, if it is this one, updates at once; nothing is mounted for the rest.
        inv(qk.explain(e.dig_id, e.message_id))
        inv(qk.message(e.dig_id, e.message_id))
      }
      break
    case 'summary_changed':
      later(qk.summary)
      break
    case 'store_fault':
      useBanners.getState().setStoreFault(e.message)
      inv(qk.taskCentre)
      break
    case 'store_fault_cleared':
      useBanners.getState().setStoreFault(null)
      inv(qk.taskCentre)
      break
    case 'notify':
      toast(e.title, { body: e.body, kind: 'info', ttlMs: 8000 })
      break
    case 'gmail_sign_in':
      // The sign-in screens watch this for their own account (useGmailSignIn); the lists refresh for everyone.
      if (e.connected) {
        inv(qk.digs)
        inv(qk.summary)
        inv(qk.taskCentre)
      }
      break
    case 'update_available':
      useBanners.getState().setUpdateAvailable(e.version)
      inv(qk.updatePreview)
      break
    case 'pipeline_stage':
      usePipeline.getState().push({ at: Date.now(), messageId: e.message_id, passId: e.pass_id, stage: e.stage, ms: num(e.ms) })
      break
    case 'director_delta':
      useDirector.getState().delta(e.conversation_id, e.text)
      break
    case 'director_tool_call':
      useDirector.getState().toolCall(e.conversation_id, e.call)
      inv(qk.activity)
      break
    case 'director_done':
      useDirector.getState().done(e.conversation_id, e.error)
      inv(qk.conversation(e.conversation_id))
      inv(qk.conversations)
      inv(qk.usage)
      break
    case 'proposal_created':
    case 'proposal_resolved':
      // The event is a signal; the proposals themselves come from list_pending.
      inv(qk.pending)
      break
    case 'expedition_changed':
      inv(qk.expeditions)
      inv(qk.expedition(e.expedition_id))
      inv(qk.taskCentre)
      break
    case 'settings_changed':
      inv(qk.settings)
      inv(qk.profile)
      inv(qk.digs)
      inv(qk.digGroups)
      break
    default:
      break
  }
}

/** Mount once in the shell: subscribes to Tauri (or the mock bus) and dispatches. */
export function useEvents(): void {
  const qc = useQueryClient()
  useEffect(() => {
    const off = onEvent((e) => handleEvent(qc, e))
    let unlisten: (() => void) | null = null
    let cancelled = false
    if (isTauri) {
      void import('@tauri-apps/api/event')
        .then(({ listen }) => listen<RiddleEvent>(EVENT_NAME, (ev) => dispatch(ev.payload)))
        .then((fn) => {
          if (cancelled) fn()
          else unlisten = fn
        })
        .catch((err) => log('error', 'ui', `event listen failed: ${String((err as Error)?.message ?? err)}`))
    }
    let source: EventSource | null = null
    if (!isTauri && bridge) {
      // The dev bridge streams the same events over SSE.
      source = new EventSource(`http://127.0.0.1:${bridge.port}/events?token=${bridge.token}`)
      source.onmessage = (m) => {
        try {
          dispatch(JSON.parse(m.data) as RiddleEvent)
        } catch (err) {
          log('warn', 'ui', `bad bridge event: ${String((err as Error)?.message ?? err)}`)
        }
      }
    }
    return () => {
      cancelled = true
      off()
      unlisten?.()
      source?.close()
      clearPendingInvalidations()
    }
  }, [qc])
}

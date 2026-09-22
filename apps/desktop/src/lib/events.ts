/**
 * The event bridge. One Tauri event, `riddle://event`, carries a
 * `RiddleEvent`; the client refetches on signals and never expects data
 * volume in an event (ARCHITECTURE.md §7). In browser mode a mock bus
 * (`emitMock`) drives the same handlers, so the UI behaves identically.
 */
import { useEffect } from 'react'
import { useQueryClient, type QueryClient } from '@tanstack/react-query'
import { isTauri, log } from './ipc'
import { qk } from './api'
import { useBanners, useDirector, useEventLog, usePipeline, useProposals, toast } from './stores'
import type { RiddleEvent } from '@/bindings/RiddleEvent'
import { num } from './format'

export const EVENT_NAME = 'riddle://event'

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
  const inv = (key: readonly unknown[]) => void qc.invalidateQueries({ queryKey: key })
  switch (e.type) {
    case 'job_changed':
    case 'job_progress':
      inv(qk.taskCentre)
      if (e.type === 'job_changed') inv(qk.summary)
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
      inv(qk.activity)
      break
    case 'message_classified':
    case 'messages_changed':
      inv(qk.messages)
      inv(['thread'])
      inv(['message'])
      inv(qk.summary)
      inv(qk.recentTraces)
      if (e.type === 'message_classified') {
        inv(qk.explain(e.dig_id, e.message_id))
        inv(qk.message(e.dig_id, e.message_id))
      }
      break
    case 'summary_changed':
      inv(qk.summary)
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
    case 'update_available':
      useBanners.getState().setUpdateAvailable(e.version)
      inv(qk.updatePreview)
      break
    case 'pipeline_stage':
      usePipeline.getState().push({ at: Date.now(), messageId: e.message_id, stage: e.stage, ms: num(e.ms) })
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
      useProposals.getState().add({ pendingId: e.pending_id, summary: e.summary, createdAt: Date.now() })
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
    return () => {
      cancelled = true
      off()
      unlisten?.()
    }
  }, [qc])
}

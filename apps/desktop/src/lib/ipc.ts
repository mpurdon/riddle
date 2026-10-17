/**
 * The one instrumented invoke wrapper (ARCHITECTURE.md §7).
 *
 * Every backend call passes through `call`: it is timed, recorded in an
 * in-flight registry so a hang is visible, logged at debug on success and
 * loudly on failure, and never logs argument values (they carry message ids at
 * best and bodies at worst). Errors are the backend's typed
 * `{kind, message, detail}`; nothing here sniffs strings.
 *
 * Outside Tauri (a browser tab in development, or vitest) the wrapper routes
 * to a mock registry so the UI can be built and tested without the shell.
 */
import { invoke as rawInvoke } from '@tauri-apps/api/core'
import type { RiddleError } from '@/bindings/RiddleError'

export type LogLevel = 'error' | 'warn' | 'info' | 'debug' | 'trace'

export const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window

/**
 * The loopback dev bridge: the real Rust core reachable from a browser tab.
 * Configured by `?bridge=<port>:<token>` in the URL (remembered in
 * localStorage) or `VITE_RIDDLE_BRIDGE=<port>:<token>`. Never used in Tauri.
 */
export interface Bridge {
  port: number
  token: string
}

function readBridge(): Bridge | null {
  if (isTauri || typeof window === 'undefined') return null
  try {
    const fromUrl = new URLSearchParams(window.location.search).get('bridge')
    const raw = fromUrl ?? localStorage.getItem('riddleBridge') ?? (import.meta.env.VITE_RIDDLE_BRIDGE as string | undefined) ?? null
    if (!raw) return null
    const [port, token] = raw.split(':')
    if (!port || !token) return null
    if (fromUrl) localStorage.setItem('riddleBridge', raw)
    return { port: Number(port), token }
  } catch {
    return null
  }
}

export const bridge: Bridge | null = readBridge()

async function bridgeCall<T>(command: string, args: Record<string, unknown>): Promise<T> {
  const b = bridge!
  const res = await fetch(`http://127.0.0.1:${b.port}/invoke`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${b.token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify({ command, args }),
  })
  if (res.status === 401) throw { kind: 'provider_auth', message: 'bridge token rejected', detail: null } satisfies RiddleError
  const body = (await res.json()) as { ok: boolean; result?: T; error?: RiddleError }
  if (!body.ok) throw body.error ?? { kind: 'internal', message: 'bridge error', detail: null }
  return body.result as T
}

/** Commands that must not be logged: logging them would log the act of reading the log. */
const UNLOGGED = new Set(['ui_log', 'read_app_logs', 'log_categories', 'dev_info', 'task_centre', 'ui_snapshot'])

const SLOW_CALL_MS = 2_000
const TIMEOUT_MS = 60_000

let seq = 0
const inFlight = new Map<number, { command: string; started: number }>()

export function inFlightCalls(): { command: string; elapsedMs: number }[] {
  const now = performance.now()
  return [...inFlight.values()]
    .map(({ command, started }) => ({ command, elapsedMs: Math.round(now - started) }))
    .sort((a, b) => b.elapsedMs - a.elapsedMs)
}

/** Fire-and-forget: logging must never fail the thing it describes or recurse. */
export function log(level: LogLevel, category: string, message: string): void {
  if (!isTauri) {
    if (bridge) {
      void fetch(`http://127.0.0.1:${bridge.port}/invoke`, {
        method: 'POST',
        headers: { Authorization: `Bearer ${bridge.token}`, 'Content-Type': 'application/json' },
        body: JSON.stringify({ command: 'ui_log', args: { level, category, message } }),
      }).catch(() => {})
    }
    if (level === 'error' || level === 'warn') console[level](`[${category}] ${message}`)
    return
  }
  void rawInvoke('ui_log', { level, category, message }).catch(() => {})
}

export function isRiddleError(e: unknown): e is RiddleError {
  return typeof e === 'object' && e !== null && 'kind' in e && 'message' in e
}

export function toRiddleError(e: unknown): RiddleError {
  if (isRiddleError(e)) return e
  if (e instanceof Error) return { kind: 'internal', message: e.message, detail: null }
  return { kind: 'internal', message: String(e), detail: null }
}


type MockHandler = (args: Record<string, unknown>) => unknown | Promise<unknown>
const mocks = new Map<string, MockHandler>()

/** Register a mock for browser development and tests. */
export function mock(command: string, handler: MockHandler): void {
  mocks.set(command, handler)
}
export function clearMocks(): void {
  mocks.clear()
}

export async function call<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const id = ++seq
  const started = performance.now()
  inFlight.set(id, { command, started })
  const logged = !UNLOGGED.has(command)
  try {
    let result: T
    if (isTauri) {
      result = await Promise.race([
        rawInvoke<T>(command, args),
        new Promise<never>((_, reject) => setTimeout(() => reject({ kind: 'internal', message: `${command} timed out after ${TIMEOUT_MS / 1000}s`, detail: null }), TIMEOUT_MS)),
      ])
    } else if (bridge) {
      result = await bridgeCall<T>(command, args)
    } else {
      const handler = mocks.get(command)
      if (!handler) throw { kind: 'internal', message: `no mock for ${command}`, detail: null } satisfies RiddleError
      result = (await handler(args)) as T
    }
    const ms = Math.round(performance.now() - started)
    if (logged) log(ms > SLOW_CALL_MS ? 'warn' : 'debug', 'ui', `${command} ok in ${ms}ms`)
    return result
  } catch (e) {
    const err = toRiddleError(e)
    const ms = Math.round(performance.now() - started)
    if (logged && err.kind !== 'needs_confirmation') log('warn', 'ui', `${command} failed in ${ms}ms: ${err.kind}`)
    throw err
  } finally {
    inFlight.delete(id)
  }
}

/**
 * Open a link outside the app: the opener plugin in Tauri, a new tab in the
 * browser build. Failure is logged and swallowed; a link that did not open
 * never fails the work around it.
 */
export function openExternal(href: string): void {
  if (isTauri) {
    void import('@tauri-apps/plugin-opener')
      .then((m) => m.openUrl(href))
      .catch((e) => log('warn', 'ui', `openUrl failed: ${String((e as Error)?.message ?? e)}`))
  } else {
    window.open(href, '_blank', 'noopener,noreferrer')
  }
}

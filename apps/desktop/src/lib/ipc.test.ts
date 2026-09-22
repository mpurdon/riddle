import { describe, expect, it, beforeEach } from 'vitest'
import { call, clearMocks, inFlightCalls, isRiddleError, mock, toRiddleError } from './ipc'

describe('the invoke wrapper outside Tauri', () => {
  beforeEach(() => clearMocks())

  it('routes to a registered mock and clears the in-flight registry', async () => {
    mock('echo', (args) => args)
    const out = await call<{ a: number }>('echo', { a: 1 })
    expect(out.a).toBe(1)
    expect(inFlightCalls()).toHaveLength(0)
  })

  it('surfaces a typed error when no mock exists', async () => {
    await expect(call('missing')).rejects.toMatchObject({ kind: 'internal' })
  })

  it('normalises thrown values into RiddleError', () => {
    expect(isRiddleError({ kind: 'not_found', message: 'x' })).toBe(true)
    expect(toRiddleError(new Error('boom')).message).toBe('boom')
    expect(toRiddleError('plain').kind).toBe('internal')
  })
})

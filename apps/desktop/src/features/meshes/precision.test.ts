import { describe, expect, it } from 'vitest'
import type { RulePrecision } from '@/bindings/RulePrecision'
import { u64 } from '@/lib/format'
import { dayLabel, precisionFor, reversedOften, steppedBack } from './precision'

const row = (meshId: string, version: number, decisions: number, reversed: number, shadow: [number, number] = [0, 0]): RulePrecision => ({ meshId, name: meshId, version, status: 'active', decisions: u64(decisions), reversed: u64(reversed), shadowProposed: shadow[0], shadowAgreed: shadow[1], reversedOften: decisions >= 20 && reversed * 4 >= decisions })
const mesh = { id: 'mesh_a', shadowProposed: 44, shadowAgreed: 40 }

describe('a Mesh’s precision from the trust card rows', () => {
  it('sums decisions and reversals across the Mesh’s versions and ignores other Meshes', () => {
    const p = precisionFor([row('mesh_a', 6, 100, 1), row('mesh_a', 7, 18, 1), row('mesh_b', 1, 500, 200)], mesh)
    expect(p.decisions).toBe(118)
    expect(p.reversed).toBe(2)
    expect(p.reversedShare).toBeCloseTo(2 / 118)
    expect(p.reversedOften).toBe(false)
  })

  it('has no share without decisions, and no agreement without proposals', () => {
    const p = precisionFor([], { id: 'mesh_a', shadowProposed: 0, shadowAgreed: 0 })
    expect(p.decisions).toBe(0)
    expect(p.reversedShare).toBeNull()
    expect(p.reversedOften).toBe(false)
    expect(p.agreement).toBeNull()
  })

  it('takes the Shadow counters from the latest row, or from the Mesh when no row exists', () => {
    expect(precisionFor([row('mesh_a', 6, 1, 0, [10, 5]), row('mesh_a', 7, 1, 0, [20, 18])], mesh)).toMatchObject({ shadowProposed: 20, shadowAgreed: 18, agreement: 0.9 })
    expect(precisionFor([], mesh)).toMatchObject({ shadowProposed: 44, shadowAgreed: 40 })
    expect(precisionFor([], mesh).agreement).toBeCloseTo(40 / 44)
  })

  it('applies the engine’s demotion rule to the sum: 20 or more decisions, a quarter or more of them reversed', () => {
    expect(precisionFor([row('mesh_a', 1, 20, 5)], mesh).reversedOften).toBe(true)
    expect(precisionFor([row('mesh_a', 1, 20, 4)], mesh).reversedOften).toBe(false)
    // Every decision reversed, but too few of them for the engine to act.
    expect(precisionFor([row('mesh_a', 1, 19, 19)], mesh).reversedOften).toBe(false)
    // Neither version reaches the floor alone; the Mesh does.
    expect(precisionFor([row('mesh_a', 6, 10, 5), row('mesh_a', 7, 10, 0)], mesh).reversedOften).toBe(true)
  })

  it('the rule is the engine’s: decisions >= 20 and reversed * 4 >= decisions', () => {
    expect(reversedOften(20, 5)).toBe(true)
    expect(reversedOften(20, 4)).toBe(false)
    expect(reversedOften(19, 19)).toBe(false)
    expect(reversedOften(100, 25)).toBe(true)
    expect(reversedOften(100, 24)).toBe(false)
    expect(reversedOften(0, 0)).toBe(false)
  })
})

describe('a Mesh stepped back to Shadow', () => {
  const demoted = { at: '2026-09-23T15:04:00Z', reversed: 9, decided: 30 }

  it('is one in Shadow that carries the backend’s demotion, with its date and counts', () => {
    expect(steppedBack({ status: 'shadow', demoted })).toEqual(demoted)
  })

  it('is not one without a demotion, whatever its reversals, nor one no longer in Shadow', () => {
    expect(steppedBack({ status: 'shadow' })).toBeNull()
    expect(steppedBack({ status: 'shadow', demoted: undefined })).toBeNull()
    expect(steppedBack({ status: 'active', demoted })).toBeNull()
    expect(steppedBack({ status: 'disabled', demoted })).toBeNull()
  })

  it('names the day it happened', () => {
    expect(dayLabel('2026-09-23T15:04:00Z')).toBe(new Date('2026-09-23T15:04:00Z').toLocaleDateString([], { month: 'short', day: 'numeric' }))
    expect(dayLabel('not a date')).toBe('not a date')
  })
})

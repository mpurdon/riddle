import { describe, expect, it } from 'vitest'
import type { SimulationReport } from '@/bindings/SimulationReport'
import { u64 } from '@/lib/format'
import { liveMatchSentence } from './liveMatch'

const report = (o: Partial<Record<'historicalMessages' | 'changed' | 'unchanged' | 'newJudgeCallsRequired', number>> & { windowDays?: number } = {}): SimulationReport => ({
  meshId: null,
  meshName: 'T',
  fromVersion: null,
  toVersion: 1,
  historicalMessages: u64(o.historicalMessages ?? 0),
  unchanged: u64(o.unchanged ?? 0),
  changed: u64(o.changed ?? 0),
  transitions: [],
  newJudgeCallsRequired: u64(o.newJudgeCallsRequired ?? 0),
  joinsSeries: u64(0),
  recordedChanges: u64(0),
  takenFirstBy: [],
  takesFrom: [],
    rescore: u64(0),
  estimatedCostUsd: null,
  sampleChanged: [],
  windowDays: o.windowDays ?? 30,
})

describe('the live match line', () => {
  it('says plainly when the window holds nothing to check', () => {
    expect(liveMatchSentence(report())).toBe('Over the last 30 days there were no messages in this scope to check.')
  })

  it('says plainly when nothing would change', () => {
    expect(liveMatchSentence(report({ historicalMessages: 412, unchanged: 412 }))).toBe('Over the last 30 days, none of the 412 messages in scope would change.')
  })

  it('counts the changes against the window total', () => {
    expect(liveMatchSentence(report({ historicalMessages: 412, unchanged: 395, changed: 17 }))).toBe('Over the last 30 days, 17 of the 412 messages in scope would change.')
  })

  it('does not let uncached Signals pass as a verdict', () => {
    expect(liveMatchSentence(report({ historicalMessages: 40, unchanged: 40, newJudgeCallsRequired: 9 }))).toBe('Over the last 30 days, none of the 40 messages in scope would change. 9 messages need a fresh judge call first and count as unchanged.')
    expect(liveMatchSentence(report({ historicalMessages: 1, unchanged: 1, newJudgeCallsRequired: 1 }))).toContain('1 message needs a fresh judge call first and counts as unchanged.')
  })

  it('uses the window the report was run over', () => {
    expect(liveMatchSentence(report({ windowDays: 90 }))).toContain('last 90 days')
  })
})

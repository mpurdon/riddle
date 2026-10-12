import { describe, expect, it } from 'vitest'
import type { MailPlace } from '@/bindings/MailPlace'
import { labelCounts, labelTree, matchPlace, placeTrail } from './places'

const label = (name: string, messages = 0, unread = 0): MailPlace => ({ place: `label:${name}`, kind: 'label', name, hasLabel: false, messages, unread, pinned: false })
const LABELS = [label('Accounting', 35, 7), label('Accounting/Taxes'), label('Travel'), label('Travel/Iceland 2027'), label('Work'), label('Work/Northwind'), label('Work/Northwind/Github', 2463), label('[Imap]/Sent')]

describe('the label tree', () => {
  it('shows labels with mail, and a parent whose mail is all in a child', () => {
    const rows = labelTree(LABELS, false, new Set())
    expect(rows.map((r) => r.row.name)).toEqual(['Accounting', 'Work', 'Work/Northwind', 'Work/Northwind/Github'])
    expect(rows.map((r) => r.depth)).toEqual([0, 0, 1, 2])
    expect(rows[3].leaf).toBe('Github')
    expect(rows[0].hasChildren).toBe(true)
  })

  it('shows the empty ones when asked, legacy folders unindented', () => {
    const rows = labelTree(LABELS, true, new Set())
    expect(rows).toHaveLength(LABELS.length)
    expect(rows.at(-1)).toMatchObject({ leaf: '[Imap]/Sent', depth: 0 })
  })

  it('hides everything under a folded label', () => {
    const rows = labelTree(LABELS, false, new Set(['Work']))
    expect(rows.map((r) => r.row.name)).toEqual(['Accounting', 'Work'])
    expect(rows[1].open).toBe(false)
  })

  it('counts what the fold holds', () => {
    expect(labelCounts(LABELS)).toEqual({ withMail: 2, empty: 6 })
  })
})

describe('the picker and the header', () => {
  it('matches every typed word anywhere in the name', () => {
    expect(matchPlace(label('Work/Northwind/Github'), 'work git')).toBe(true)
    expect(matchPlace(label('Work/Northwind/Github'), 'travel')).toBe(false)
  })

  it('names where you are', () => {
    expect(placeTrail('label:Work/Northwind/Github', undefined)).toEqual({ kind: 'Label', parts: ['Work', 'Northwind', 'Github'] })
    expect(placeTrail('topic:Receipts', undefined)).toEqual({ kind: 'Topic', parts: ['Receipts'] })
    expect(placeTrail('all', undefined)).toEqual({ kind: null, parts: ['All mail'] })
  })
})

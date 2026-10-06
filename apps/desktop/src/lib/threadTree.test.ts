import { describe, expect, it } from 'vitest'
import type { Message } from '@/bindings/Message'
import { buildThreadTree } from './threadTree'

const msg = (id: string, date: string, inReplyTo: string | null = null, references: string[] = []): Message =>
  ({ id, date, headers: { messageId: `<${id}@x>`, inReplyTo: inReplyTo ? `<${inReplyTo}@x>` : null, references: references.map((r) => `<${r}@x>`) } }) as unknown as Message

describe('buildThreadTree', () => {
  it('puts each reply under what it answers, depth first by date', () => {
    const tree = buildThreadTree([msg('c', '3', 'a'), msg('a', '1'), msg('b', '2', 'a'), msg('d', '4', 'b')])
    expect(tree.map((n) => [n.message.id, n.depth])).toEqual([
      ['a', 0],
      ['b', 1],
      ['d', 2],
      ['c', 1],
    ])
    expect(tree.find((n) => n.message.id === 'd')?.parentId).toBe('b')
  })

  it('keeps the rail of a level running while a later sibling follows', () => {
    const tree = buildThreadTree([msg('a', '1'), msg('b', '2', 'a'), msg('d', '3', 'b'), msg('c', '4', 'a')])
    const d = tree.find((n) => n.message.id === 'd')!
    expect(d.rails).toEqual([false, true]) // root has no sibling; b has sibling c after it
    expect(tree.find((n) => n.message.id === 'b')!.hasNextSibling).toBe(true)
  })

  it('falls back to References when the direct parent is missing', () => {
    const tree = buildThreadTree([msg('a', '1'), msg('c', '3', 'missing', ['a', 'missing'])])
    expect(tree.find((n) => n.message.id === 'c')).toMatchObject({ parentId: 'a', depth: 1 })
  })

  it('treats messages without a known parent, and header cycles, as roots', () => {
    const tree = buildThreadTree([msg('a', '1', 'b'), msg('b', '2', 'a'), msg('z', '3', 'nowhere')])
    expect(tree.every((n) => n.message.id === 'z' ? n.depth === 0 : true)).toBe(true)
    expect(tree).toHaveLength(3)
  })
})

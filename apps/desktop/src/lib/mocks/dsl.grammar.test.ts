// The shared grammar corpus against the browser mock's parser. The Rust
// parser runs the same file (crates/riddle-mesh/tests/grammar.rs), so a rule
// added on one side and not the other fails here instead of drifting.
import { describe, expect, it } from 'vitest'
import { MeshSyntaxError, parseMeshText } from './dsl'
import corpusText from '../../../../../crates/riddle-mesh/tests/grammar.json?raw'

interface Case {
  name: string
  text: string
  mesh?: Record<string, unknown>
  outputs?: Record<string, unknown>[]
  error?: string
}

const corpus: { cases: Case[] } = JSON.parse(corpusText)

// Every field the case names; a field the mock leaves out reads as null,
// as an absent Option serializes from Rust.
const subset = (actual: object, expected: Record<string, unknown>) =>
  Object.fromEntries(Object.keys(expected).map((k) => [k, (actual as Record<string, unknown>)[k] ?? null]))

describe('the shared Mesh grammar corpus', () => {
  it.each(corpus.cases)('$name', (c) => {
    if (c.error) {
      expect(() => parseMeshText(c.text)).toThrow(MeshSyntaxError)
      expect(() => parseMeshText(c.text)).toThrow(c.error)
      return
    }
    const mesh = parseMeshText(c.text)
    if (c.mesh) expect(subset(mesh, c.mesh)).toEqual(c.mesh)
    if (c.outputs) {
      expect(mesh.routes).toHaveLength(c.outputs.length)
      c.outputs.forEach((want, i) => expect(subset(mesh.routes[i].output, want), `route ${i}`).toEqual(want))
    }
  })
})

import { describe, expect, it } from 'vitest'
import { issuesFrom } from './meshIssues'
import { MeshSyntaxError, parseMeshText } from './mocks/dsl'

describe('parse_mesh errors map to editor lines', () => {
  it('reads a single {line, message} detail', () => {
    expect(issuesFrom({ kind: 'invalid', message: 'bad', detail: { line: 8, column: 4, message: 'cannot read condition' } })).toEqual([{ line: 8, column: 4, message: 'cannot read condition' }])
  })
  it('reads an issues array and sorts by line', () => {
    const out = issuesFrom({ kind: 'invalid', message: 'bad', detail: { issues: [{ line: 12, message: 'b' }, { line: 3, message: 'a' }] } })
    expect(out.map((i) => i.line)).toEqual([3, 12])
  })
  it('reads a bare array and falls back to the message when no line is given', () => {
    expect(issuesFrom({ kind: 'invalid', message: 'x', detail: [{ line: '5', message: 'y' }] })).toEqual([{ line: 5, column: undefined, message: 'y' }])
    expect(issuesFrom({ kind: 'invalid', message: 'Mesh text is invalid: line 7: oops', detail: null })).toEqual([{ line: 7, message: 'Mesh text is invalid: line 7: oops' }])
    expect(issuesFrom({ kind: 'invalid', message: 'nothing', detail: null })[0].line).toBe(1)
  })
  it('agrees with the mock parser on the failing line', () => {
    const text = 'Mesh: X v1\n\nScope:\nAll Digs\n\nROUTES\n\nIF routine > .9\n    → Spoil\n'
    try {
      parseMeshText(text)
      throw new Error('expected a syntax error')
    } catch (e) {
      expect(e).toBeInstanceOf(MeshSyntaxError)
      const issues = issuesFrom((e as MeshSyntaxError).toRiddleError())
      expect(issues[0].line).toBe(8)
      expect(issues[0].message).toMatch(/pinned version/)
    }
  })
})

describe('the mock Mesh DSL', () => {
  it('parses the ARCHITECTURE §2 example into routes and guards', () => {
    const m = parseMeshText(`Mesh: GitHub Notifications v8

Scope:
Work Digs

ROUTES

IF review_requested@v3 > .80
    → Find: Review

ELSE IF ci_failure@v4 > .85
     AND personally_relevant@v3 > .70
    → Find: Do

ELSE IF routine@v1 > .90
     AND personally_relevant@v3 < .30
    → Spoil

ELSE
    → Escalate

GUARDS

NEVER Spoil IF sender IN never_hidden
`)
    expect(m.name).toBe('GitHub Notifications')
    expect(m.version).toBe(8)
    expect(m.scope).toEqual({ kind: 'dig_group', group: 'Work' })
    expect(m.routes).toHaveLength(4)
    expect(m.routes[0].output.disposition).toEqual({ kind: 'find', find_type: 'review' })
    expect(m.routes[1].condition.kind).toBe('and')
    expect(m.routes[3].condition).toEqual({ kind: 'always' })
    expect(m.routes[3].output.escalate).toBe(true)
    expect(m.guards[0]).toMatchObject({ never: 'spoil', condition: { kind: 'in_list', list: 'never_hidden' } })
  })
  it('reports every problem with its line', () => {
    try {
      parseMeshText('Mesh: Bad v1\nROUTES\nIF routine@v1 > 1.5\n    → Nowhere\nGUARDS\nNEVER Spoil IF sender IN nobody\n')
      throw new Error('expected a syntax error')
    } catch (e) {
      const issues = (e as MeshSyntaxError).issues
      expect(issues.map((i) => i.line)).toEqual([3, 4, 6])
    }
  })
})

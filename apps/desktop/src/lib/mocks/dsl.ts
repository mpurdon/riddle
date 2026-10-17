/**
 * A mock parser and evaluator for the Mesh DSL of ARCHITECTURE.md §2, so
 * the browser build can validate Mesh text line by line and simulate a
 * change against the fixture mailbox. The real parser lives in
 * `riddle-mesh`; this one exists only behind the mock registry.
 *
 * Grammar (one statement per line, blank lines ignored):
 *   Mesh: <name> v<version>
 *   Scope:                         followed by  All Digs | <Group> Digs | Dig: <name>
 *   Priority: <integer>               (lower runs first within a scope)
 *   Autonomy: Observe | Suggest | Act   (the most this Mesh may do in the mailbox)
 *   ROUTES
 *   IF <cond>                      ELSE IF <cond>     ELSE
 *       AND <cond> | OR <cond>     (continuation lines)
 *       → <output> [+ <output>]    (also "->")
 *   GUARDS
 *   NEVER <Disposition> IF <cond>
 * Conditions: signal@vN [:CHOICE] <op> <number> | sender IN <list> | <fact> <op> <value> | <fact> CONTAINS <text>
 *             | <bare bool fact> | header <Name> <op> <value>   (the older `fact <name> …` spelling still parses)
 * Outputs: Find: <Type> | Filed | Spoil | Contamination | Escalate | Stratum: <name> | Notify
 */
import type { Expr } from '@/bindings/Expr'
import type { Autonomy } from '@/bindings/Autonomy'
import type { CmpOp } from '@/bindings/CmpOp'
import { FACTS } from '@/features/meshes/meshSyntax'
import type { Guard } from '@/bindings/Guard'
import type { Route } from '@/bindings/Route'
import type { MeshOutput } from '@/bindings/MeshOutput'
import type { MeshScope } from '@/bindings/MeshScope'
import type { DispositionKind } from '@/bindings/DispositionKind'
import type { FindType } from '@/bindings/FindType'
import type { NamedList } from '@/bindings/NamedList'
import type { Disposition } from '@/bindings/Disposition'
import type { Expiry } from '@/bindings/Expiry'
import type { RiddleError } from '@/bindings/RiddleError'
import type { MeshParseIssue } from '../types'

export interface ParsedMesh {
  name: string
  version: number
  scope: MeshScope
  routes: Route[]
  guards: Guard[]
  /** `Autonomy:` header: the most this Mesh may do in the mailbox. */
  autonomy?: Autonomy
  /** `Priority:` header; lower runs first within a scope. */
  priority?: number
}

export class MeshSyntaxError extends Error {
  issues: MeshParseIssue[]
  constructor(issues: MeshParseIssue[]) {
    super(issues.map((i) => `line ${i.line}: ${i.message}`).join('; '))
    this.issues = issues
  }
  toRiddleError(): RiddleError {
    const first = this.issues[0]
    return {
      kind: 'invalid',
      message: `Mesh text is invalid: line ${first.line}: ${first.message}`,
      detail: { line: first.line, column: first.column ?? null, message: first.message, issues: this.issues },
    }
  }
}

/** Mirrors `HEADER_PREFIX` in riddle-core: a kept header reads as the fact `header:<name>`. */
const HEADER_PREFIX = 'header:'
const isFact = (name: string) => FACTS.some((f) => f.name === name.toLowerCase())
const OPS: Record<string, CmpOp> = { '>': 'gt', '>=': 'gte', '<': 'lt', '<=': 'lte', '=': 'eq', '==': 'eq', '!=': 'neq' }
const LISTS: NamedList[] = ['never_hidden', 'contacts', 'vips', 'organisation', 'family']
const FIND_TYPES: Record<string, FindType> = {
  respond: 'respond',
  decide: 'decide',
  review: 'review',
  do: 'do',
  pay: 'pay',
  attend: 'attend',
  'follow up': 'follow_up',
  follow_up: 'follow_up',
  followup: 'follow_up',
  know: 'know',
}
const DISPOSITIONS: Record<string, DispositionKind> = { find: 'find', filed: 'filed', spoil: 'spoil', contamination: 'contamination', unresolved: 'unresolved' }

interface DigLookup {
  digIdByName: (name: string) => string | null
}

/** `end of day`, `next morning`, `after its dates`, or a span (`18h`, `2 days`); a string is the error. Mirrors riddle-mesh `parse_expiry`. */
function parseExpiry(raw: string): Expiry | string {
  const t = raw.trim().toLowerCase().split(/\s+/).join(' ')
  if (t === 'end of day' || t === 'end of the day') return { kind: 'end_of_day' }
  if (t === 'next morning' || t === 'tomorrow morning') return { kind: 'next_morning' }
  if (['after its dates', 'after the dates', 'after booking', 'after the booking'].includes(t)) return { kind: 'after_dates' }
  const mins = /^(\d+)\s*(m|min|mins|minutes?)$/.exec(t)
  if (mins) {
    const n = Number(mins[1])
    if (n >= 1 && n < 60) return { kind: 'after_minutes', minutes: n }
    if (n >= 60 && n % 60 === 0) return { kind: 'after', hours: n / 60 }
    return 'Expires in minutes is between 1 and 59, e.g. Expires: 15m'
  }
  const m = /^(\d+)\s*(h|hr|hrs|hours?|d|days?)$/.exec(t)
  if (!m) return `Expires needs minutes, hours, days, end of day, next morning or after its dates, e.g. Expires: 18h or Expires: end of day, not "${raw.trim()}"`
  const hours = Number(m[1]) * (m[2].startsWith('d') ? 24 : 1)
  if (hours < 1 || hours > 90 * 24) return 'Expires is between 1 hour and 90 days'
  return { kind: 'after', hours }
}

export function parseMeshText(text: string, lookup: DigLookup = { digIdByName: () => null }): ParsedMesh {
  const issues: MeshParseIssue[] = []
  const lines = text.split('\n')
  let name = ''
  let version = 1
  let scope: MeshScope | null = null
  let autonomy: Autonomy | undefined
  let priority: number | undefined
  const routes: Route[] = []
  const guards: Guard[] = []
  let section: 'head' | 'scope' | 'routes' | 'guards' = 'head'
  let cur: { conds: { join: 'and' | 'or'; expr: Expr }[]; isElse: boolean; line: number; text: string[] } | null = null
  let sawElse = false
  // The open `WHEN <condition>` block: its routes AND the condition in, and
  // its ELSE is the condition alone (riddle-mesh `parse`).
  // Held in an object: it is set inside closures, which control flow
  // analysis cannot follow from the end of the function.
  const when: { block: { expr: Expr; text: string; hasRoute: boolean } | null } = { block: null }

  const fail = (line: number, message: string, column?: number) => issues.push({ line, message, column })

  const factExpr = (fact: string, op: string, rawValue: string): Expr => {
    const raw = rawValue.trim()
    const value = raw === 'true' || raw === 'false' ? { type: 'bool' as const, value: raw === 'true' } : /^-?\d+(\.\d+)?$/.test(raw) ? { type: 'number' as const, value: Number(raw) } : { type: 'text' as const, value: raw.replace(/^"|"$/g, '') }
    return { kind: 'fact', fact, op: op.toUpperCase() === 'CONTAINS' ? 'contains' : OPS[op], value }
  }
  const parseCond = (s: string, line: number): Expr | null => {
    const src = s.trim()
    let m = /^([a-z_]+)\s+IN\s+list:([a-z0-9_]+)$/i.exec(src)
    if (m) return { kind: 'in_named_list', subject: m[1].toLowerCase(), list: m[2].toLowerCase() }
    m = /^sender\s+IN\s+([a-z_]+)$/i.exec(src)
    if (m) {
      const list = m[1].toLowerCase() as NamedList
      if (!LISTS.includes(list)) {
        fail(line, `unknown list "${m[1]}"; one of ${LISTS.join(', ')}`)
        return null
      }
      return { kind: 'in_list', subject: 'sender', list }
    }
    // The real grammar: `header X-GitHub-Reason = ci_activity`, `subject CONTAINS "invoice"`,
    // `sender_domain = github.com`, a bare `has_attachments`, and the older `fact <name> <op> <value>`.
    m = /^header\s+("?[A-Za-z0-9-]+"?)\s+(CONTAINS|>=|<=|!=|==|>|<|=)\s*(.+)$/i.exec(src)
    if (m) return factExpr(`${HEADER_PREFIX}${m[1].replace(/"/g, '').toLowerCase()}`, m[2], m[3])
    m = /^(?:fact\s+)?([a-z_][a-z0-9_]*)\s+(CONTAINS)\s+(.+)$/i.exec(src) ?? /^(?:fact\s+)?([a-z_][a-z0-9_]*)\s*(>=|<=|!=|==|>|<|=)\s*(.+)$/i.exec(src)
    if (m && (/^fact\s/i.test(src) || isFact(m[1]))) return factExpr(m[1], m[2], m[3])
    if (/^[a-z_][a-z0-9_]*$/i.test(src) && isFact(src)) return { kind: 'fact', fact: src, op: 'eq', value: { type: 'bool', value: true } }
    m = /^([a-z_][a-z0-9_]*)@v(\d+)(?::([A-Za-z_]+))?\s*(>=|<=|!=|==|>|<|=)\s*(\.\d+|\d+(?:\.\d+)?)$/i.exec(src)
    if (m) {
      const threshold = Number(m[5])
      if (threshold < 0 || threshold > 1) {
        fail(line, `threshold ${m[5]} is outside 0..1; thresholds are calibrated scores`)
        return null
      }
      return { kind: 'signal', signal: m[1], version: Number(m[2]), choice: m[3] ?? null, op: OPS[m[4]], threshold }
    }
    if (/^[a-z_][a-z0-9_]*\s*(>=|<=|>|<|=)/i.test(src)) fail(line, `Signal "${src.split(/\s/)[0]}" needs a pinned version, e.g. ${src.split(/\s/)[0]}@v1`)
    else fail(line, `cannot read condition "${src}"`)
    return null
  }

  const parseOutput = (s: string, line: number): MeshOutput | null => {
    const out: MeshOutput = { disposition: null, strata: [], notify: false, escalate: false, replaces: false }
    const parts = s.split('+').map((p) => p.trim()).filter(Boolean)
    if (!parts.length) {
      fail(line, 'route needs an output after →')
      return null
    }
    for (const p of parts) {
      let m = /^Find:\s*(.+)$/i.exec(p)
      if (m) {
        const t = FIND_TYPES[m[1].trim().toLowerCase()]
        if (!t) {
          fail(line, `unknown Find type "${m[1].trim()}"; one of Respond, Decide, Review, Do, Pay, Attend, Follow Up, Know`)
          return null
        }
        out.disposition = { kind: 'find', find_type: t }
        continue
      }
      const expiresText = /^Expires$/i.test(p) ? '' : /^Expires:\s*(.*)$/i.exec(p)?.[1]
      if (expiresText !== undefined) {
        const expires = parseExpiry(expiresText)
        if (typeof expires === 'string') {
          fail(line, expires)
          return null
        }
        out.expires = expires
        continue
      }
      m = /^Importance:\s*(.+)$/i.exec(p)
      if (m) {
        const level = m[1].trim().toLowerCase()
        if (level !== 'low' && level !== 'normal' && level !== 'high' && level !== 'critical') {
          fail(line, `unknown Importance ${m[1].trim()}; use Low, Normal, High or Critical`)
          return null
        }
        out.importance = level
        continue
      }
      m = /^Stratum:\s*(.+)$/i.exec(p)
      if (m) {
        for (const name of m[1].split(',').map((x) => x.trim()).filter(Boolean)) if (!out.strata.includes(name)) out.strata.push(name)
        continue
      }
      const low = p.toLowerCase()
      if (low === 'escalate') out.escalate = true
      else if (low === 'notify') out.notify = true
      else if (low === 'replaces') out.replaces = true
      else if (/^replaces:\s*same subject$/.test(low)) {
        out.replaces = true
        out.replacesScope = 'subject'
      } else if (low.startsWith('replaces:')) {
        fail(line, `Replaces is bare (the newest from the sender) or Replaces: same subject, not ${p}`)
        return null
      } else if (low === 'filed') out.disposition = { kind: 'filed' }
      else if (low === 'spoil') out.disposition = { kind: 'spoil' }
      else if (low === 'contamination') out.disposition = { kind: 'contamination' }
      else if (low === 'find') {
        fail(line, 'Find needs a type, e.g. Find: Review')
        return null
      } else {
        fail(line, `unknown output "${p}"`)
        return null
      }
    }
    // Mirrors riddle-mesh `parse_output`: Replaces on a Find, Spoil or Filed; Expires on a Find.
    const kind = out.disposition?.kind
    if (out.replaces && kind !== 'find' && kind !== 'spoil' && kind !== 'filed') {
      fail(line, 'Replaces needs a Find, Spoil or Filed, e.g. Spoil + Replaces')
      return null
    }
    if (out.expires != null && kind !== 'find') {
      fail(line, 'Expires closes Finds, so it needs a Find, e.g. Find: Do + Expires')
      return null
    }
    return out
  }

  const closeRoute = (line: number) => {
    if (!cur) return
    if (!cur.conds.length && !cur.isElse) fail(cur.line, 'IF needs a condition')
    fail(line, `route starting at line ${cur.line} has no → output`)
    cur = null
  }

  const combine = (conds: { join: 'and' | 'or'; expr: Expr }[]): Expr => {
    if (!conds.length) return { kind: 'always' }
    if (conds.length === 1) return conds[0].expr
    // Left to right with AND binding tighter than OR.
    const orGroups: Expr[][] = [[conds[0].expr]]
    for (const c of conds.slice(1)) {
      if (c.join === 'or') orGroups.push([c.expr])
      else orGroups[orGroups.length - 1].push(c.expr)
    }
    const ands = orGroups.map((g) => (g.length === 1 ? g[0] : ({ kind: 'and', items: g } as Expr)))
    return ands.length === 1 ? ands[0] : { kind: 'or', items: ands }
  }

  lines.forEach((raw, idx) => {
    const line = idx + 1
    const s = raw.trim()
    if (!s || s.startsWith('#')) return
    let m: RegExpExecArray | null
    if ((m = /^Mesh:\s*(.+?)\s*(?:v(\d+))?$/i.exec(s)) && section === 'head' && !name) {
      name = m[1].trim()
      if (m[2]) version = Number(m[2])
      if (!name) fail(line, 'Mesh needs a name')
      return
    }
    if ((m = /^Priority:\s*(.*)$/i.exec(s)) && section === 'head') {
      if (/^-?\d+$/.test(m[1].trim())) priority = Number(m[1].trim())
      else fail(line, 'Priority: needs an integer')
      return
    }
    if ((m = /^Autonomy:\s*(.+)$/i.exec(s)) && section === 'head') {
      const v = m[1].trim().toLowerCase()
      if (v === 'trusted') fail(line, 'Autonomy: a Mesh can be capped at Observe, Suggest or Act; Trusted is an account setting')
      else if (v === 'observe' || v === 'suggest' || v === 'act') autonomy = v
      else fail(line, `Autonomy: "${m[1].trim()}" is not one of Observe, Suggest or Act`)
      return
    }
    if (/^Scope:\s*$/i.test(s)) {
      section = 'scope'
      return
    }
    if ((m = /^Scope:\s*(.+)$/i.exec(s))) {
      section = 'scope'
      scope = parseScope(m[1], line)
      return
    }
    if (/^ROUTES$/i.test(s)) {
      section = 'routes'
      return
    }
    if (/^GUARDS$/i.test(s)) {
      if (cur) closeRoute(line)
      section = 'guards'
      return
    }
    switch (section) {
      case 'head':
        fail(line, `expected "Mesh: <name> vN" first, got "${s}"`)
        return
      case 'scope':
        if (scope) fail(line, 'Scope given twice')
        scope = parseScope(s, line)
        return
      case 'routes':
        routeLine(s, line)
        return
      case 'guards': {
        if ((m = /^NEVER\s+([A-Za-z]+)\s+IF\s+(.+)$/i.exec(s))) {
          const never = DISPOSITIONS[m[1].toLowerCase()]
          if (!never) {
            fail(line, `unknown disposition "${m[1]}"`)
            return
          }
          const expr = parseCond(m[2], line)
          if (expr) guards.push({ never, condition: expr, text: s, words: '' })
          return
        }
        fail(line, `cannot read "${s}" in GUARDS; expected NEVER <Disposition> IF <condition>`)
        return
      }
    }
  })
  if (cur) closeRoute(lines.length)
  if (when.block?.hasRoute === false) fail(lines.length, 'a WHEN block needs at least one route')
  if (!name) name = 'Untitled Mesh'
  if (!scope) scope = { kind: 'all_digs' }
  if (!routes.length && !guards.length && !issues.length) fail(lines.length || 1, 'a Mesh needs at least one route or guard')
  if (issues.length) throw new MeshSyntaxError(issues.sort((a, b) => a.line - b.line))
  return { name, version, scope, routes, guards, autonomy, priority }

  function routeLine(s: string, line: number): void {
    let m: RegExpExecArray | null
    // `IF cond → output` on one line reads as the condition, then the arrow.
    if ((m = /^(?!→|->)(.+?)\s*(→|->)\s*(.*)$/.exec(s))) {
      routeLine(m[1], line)
      routeLine(`→ ${m[3]}`, line)
      return
    }
    if ((m = /^WHEN\s+(.+)$/i.exec(s))) {
      if (cur) closeRoute(line)
      if (when.block && !when.block.hasRoute) fail(line, 'a WHEN block needs at least one route before the next WHEN')
      const expr = parseCond(m[1], line)
      when.block = expr ? { expr, text: s, hasRoute: false } : null
      sawElse = false
      return
    }
    if (/^END$/i.test(s)) {
      if (cur) closeRoute(line)
      if (!when.block) fail(line, 'END without a WHEN')
      else if (!when.block.hasRoute) fail(line, 'a WHEN block needs at least one route')
      when.block = null
      sawElse = false
      return
    }
    if ((m = /^(?:ELSE\s+)?IF\s+(.+)$/i.exec(s))) {
      if (cur) closeRoute(line)
      if (sawElse) fail(line, 'no route may follow ELSE')
      const expr = parseCond(m[1], line)
      cur = { conds: expr ? [{ join: 'and', expr }] : [], isElse: false, line, text: [s] }
      return
    }
    if (/^ELSE$/i.test(s)) {
      if (cur) closeRoute(line)
      if (sawElse) fail(line, 'ELSE given twice')
      sawElse = true
      cur = { conds: [], isElse: true, line, text: [s] }
      return
    }
    if ((m = /^(AND|OR)\s+(.+)$/i.exec(s))) {
      if (!cur || cur.isElse) {
        fail(line, `${m[1].toUpperCase()} outside an IF`)
        return
      }
      const expr = parseCond(m[2], line)
      if (expr) cur.conds.push({ join: m[1].toLowerCase() as 'and' | 'or', expr })
      cur.text.push(s)
      return
    }
    if ((m = /^(?:→|->)\s*(.+)$/.exec(s))) {
      if (!cur) {
        fail(line, '→ without an IF or ELSE')
        return
      }
      const out = parseOutput(m[1], line)
      cur.text.push(s)
      if (out) routes.push(route(cur, out))
      cur = null // the route is consumed even when its output was bad
      return
    }
    fail(line, `cannot read "${s}" in ROUTES; expected IF, ELSE IF, ELSE, AND, OR or →`)
    return
  }

  function route(r: { conds: { join: 'and' | 'or'; expr: Expr }[]; isElse: boolean; text: string[] }, output: MeshOutput): Route {
    const local = r.isElse ? null : combine(r.conds)
    const block = when.block
    if (!block) return { condition: local ?? { kind: 'always' }, output, text: r.text.join('\n'), when: null, local: null, words: '' }
    block.hasRoute = true
    return {
      condition: local ? { kind: 'and', items: [block.expr, local] } : block.expr,
      output,
      text: `${block.text} ▸ ${r.text.join('\n')}`,
      when: block.expr,
      local,
      words: '',
    }
  }

  function parseScope(s: string, line: number): MeshScope {
    const t = s.trim()
    if (/^All Digs$/i.test(t)) return { kind: 'all_digs' }
    let m = /^(.+?)\s+Digs$/i.exec(t)
    if (m) return { kind: 'dig_group', group: m[1].trim() }
    m = /^Dig:\s*(.+)$/i.exec(t)
    if (m) {
      const id = lookup.digIdByName(m[1].trim())
      if (!id) fail(line, `no Dig named "${m[1].trim()}"`)
      return { kind: 'dig', dig_id: id ?? m[1].trim() }
    }
    fail(line, `cannot read scope "${t}"; use "All Digs", "<Group> Digs" or "Dig: <name>"`)
    return { kind: 'all_digs' }
  }
}

// ---------------------------------------------------------------------------
// Evaluation against one message's cached Signals and facts.

export interface EvalInput {
  /** P(selected) per "name@vN" and per "name@vN:CHOICE". */
  signals: Record<string, number>
  facts: Record<string, boolean | number | string>
  lists: Record<NamedList, boolean>
  /** Whether the sender is on each named list (`sender IN list:banks`). */
  named?: Record<string, boolean>
}

export interface EvalResult {
  disposition: Disposition | null
  strata: string[]
  notify: boolean
  escalate: boolean
  matched: string | null
  /** True when only the catch-all ELSE matched. */
  matchedAlways: boolean
  guardVetoes: string[]
  missingSignals: string[]
}

function cmp(op: CmpOp, a: number | string | boolean, b: number | string | boolean): boolean {
  switch (op) {
    case 'gt':
      return a > b
    case 'gte':
      return a >= b
    case 'lt':
      return a < b
    case 'lte':
      return a <= b
    case 'eq':
      return a === b
    case 'neq':
      return a !== b
    case 'contains':
      return String(a).toLowerCase().includes(String(b).toLowerCase())
  }
}

export function evalExpr(e: Expr, input: EvalInput, missing: string[]): boolean {
  switch (e.kind) {
    case 'always':
      return true
    case 'and':
      return e.items.every((i) => evalExpr(i, input, missing))
    case 'or':
      return e.items.some((i) => evalExpr(i, input, missing))
    case 'not':
      return !evalExpr(e.item, input, missing)
    case 'in_list':
      return input.lists[e.list] ?? false
    case 'in_named_list':
      return input.named?.[e.list] ?? false
    case 'fact': {
      const v = input.facts[e.fact]
      if (v === undefined) return false
      return cmp(e.op, v, e.value.value)
    }
    case 'signal': {
      const key = `${e.signal}@v${e.version}${e.choice ? `:${e.choice}` : ''}`
      const v = input.signals[key]
      if (v === undefined) {
        missing.push(key)
        return false
      }
      return cmp(e.op, v, e.threshold)
    }
  }
}

export function evalMesh(m: { routes: Route[]; guards: Guard[] }, input: EvalInput): EvalResult {
  const missing: string[] = []
  let out: MeshOutput | null = null
  let matched: string | null = null
  let matchedAlways = false
  for (const r of m.routes) {
    if (evalExpr(r.condition, input, missing)) {
      out = r.output
      matched = r.text
      matchedAlways = r.condition.kind === 'always'
      break
    }
  }
  const vetoes: string[] = []
  let disposition = out?.disposition ?? null
  for (const g of m.guards) {
    if (disposition && disposition.kind === g.never && evalExpr(g.condition, input, missing)) {
      vetoes.push(g.text)
      disposition = { kind: 'unresolved' }
    }
  }
  return {
    disposition,
    strata: out?.strata ?? [],
    notify: out?.notify ?? false,
    escalate: out?.escalate ?? false,
    matched,
    matchedAlways,
    guardVetoes: vetoes,
    missingSignals: [...new Set(missing)],
  }
}

export function describeScope(s: MeshScope, digName: (id: string) => string): string {
  switch (s.kind) {
    case 'all_digs':
      return 'All Digs'
    case 'dig_group':
      return `${s.group} Digs`
    case 'dig':
      return `Dig: ${digName(s.dig_id)}`
  }
}

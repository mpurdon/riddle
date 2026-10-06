/**
 * "Your corrections": the sender and domain rules the user makes with one
 * click ("Not Spoil for this sender", Flow 6), kept as guard lines in one
 * Mesh named below. A Mesh is the only place a rule lives in Riddle, so a
 * correction that should outlast one message becomes text the user can
 * read, edit and remove, in Settings → Mail rules → Your corrections and
 * on the Meshes page. Guards run after every route from every scope, so a
 * `NEVER Spoil IF sender = "x"` line vetoes Spoil whatever other Mesh says.
 *
 * The functions are pure so both the Spoil page (adding) and Settings
 * (listing and removing) agree on the text.
 */

export const YOUR_CORRECTIONS_MESH = 'Your corrections'

export type CorrectionScope = 'sender' | 'domain'
export type CorrectionKind = 'spoil' | 'contamination'

export interface CorrectionRule {
  scope: CorrectionScope
  /** The address, or the domain without `@`. */
  value: string
  /** The disposition the rule vetoes. */
  never: CorrectionKind
  /** The exact guard line, for removal. */
  line: string
}

const NEVER: Record<CorrectionKind, string> = { spoil: 'Spoil', contamination: 'Contamination' }

/** The disposition a rule vetoes, as it reads in the DSL and the UI. */
export function neverLabel(kind: CorrectionKind): string {
  return NEVER[kind]
}

/** How a sender is identified everywhere: the address, trimmed and lower-cased. */
export function senderKey(email: string): string {
  return email.trim().toLowerCase()
}

/** The guard line for a rule: `NEVER Spoil IF sender = "alerts@uptime.example"`. */
export function guardLine(scope: CorrectionScope, value: string, never: CorrectionKind): string {
  const v = value.trim().toLowerCase().replace(/^@/, '')
  const fact = scope === 'sender' ? 'sender' : 'sender_domain'
  return `NEVER ${NEVER[never]} IF ${fact} = "${v.replace(/"/g, '')}"`
}

const HEADER = `Mesh: ${YOUR_CORRECTIONS_MESH} v1\nScope: All Digs\n\nGUARDS\n`

/** The Mesh text with one more rule, creating the Mesh text when there is none. Idempotent. */
export function withCorrection(text: string | null | undefined, line: string): string {
  const base = text?.trim() ? text : HEADER
  if (parseCorrections(base).some((r) => r.line === line)) return base
  const body = base.replace(/\s+$/, '')
  if (!/GUARDS\s*$/m.test(body) && !/\nGUARDS\n/.test(`${body}\n`)) return `${body}\n\nGUARDS\n${line}\n`
  return `${body}\n${line}\n`
}

/** The Mesh text without one rule; null when no rule remains (delete the Mesh instead). */
export function withoutCorrection(text: string, line: string): string | null {
  const kept = text
    .split('\n')
    .filter((l) => l.trim() !== line.trim())
    .join('\n')
  return parseCorrections(kept).length ? kept.replace(/\n{3,}/g, '\n\n') : null
}

const RULE = /^\s*NEVER\s+(Spoil|Contamination)\s+IF\s+(sender|sender_domain)\s*=\s*"?([^"\s]+)"?\s*$/i

/** Every rule the text holds, in order. Lines that are not rules are ignored. */
export function parseCorrections(text: string | null | undefined): CorrectionRule[] {
  if (!text) return []
  const out: CorrectionRule[] = []
  for (const raw of text.split('\n')) {
    const m = RULE.exec(raw)
    if (!m) continue
    out.push({
      never: m[1].toLowerCase() as CorrectionKind,
      scope: m[2].toLowerCase() === 'sender' ? 'sender' : 'domain',
      value: m[3].toLowerCase(),
      line: raw.trim(),
    })
  }
  return out
}

/** The domain of an address, for the "this domain" scope. */
export function domainOf(email: string): string {
  const at = email.lastIndexOf('@')
  return at >= 0 ? email.slice(at + 1).toLowerCase() : email.toLowerCase()
}

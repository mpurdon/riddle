/**
 * The mail column's pure parts: the label tree with its folds, the rows the
 * `g l` picker offers, and the trail the list header shows for a place. The
 * places themselves come from the backend (`mail_places`), which has
 * already merged a Stratum and a label of the same name into one Topic.
 */
import type { MailPlace } from '@/bindings/MailPlace'
import type { MailPlaces } from '@/bindings/MailPlaces'

export interface TreeRow {
  row: MailPlace
  depth: number
  /** The last part of the name: `Github` for `Work/Northwind/Github`. */
  leaf: string
  hasChildren: boolean
  open: boolean
}

const legacy = (name: string) => name.startsWith('[')
const parts = (name: string) => (legacy(name) ? [name] : name.split('/'))

/**
 * The labels as a tree, a child after its parent. A label shows when it,
 * or a label under it, has mail; empty ones only with `showEmpty`. A label
 * under a closed parent is hidden. A child whose parent is not a label of
 * its own (Gmail allows `Work/CP` with no `Work`) hangs at the parent's
 * depth anyway, so the indent still says where it lives.
 */
export function labelTree(labels: MailPlace[], showEmpty: boolean, closed: ReadonlySet<string>): TreeRow[] {
  const names = labels.map((l) => l.name)
  const below = (name: string) => names.filter((n) => n.startsWith(`${name}/`))
  const hasMail = (l: MailPlace) => l.messages > 0 || labels.some((x) => x.messages > 0 && x.name.startsWith(`${l.name}/`))
  const out: TreeRow[] = []
  for (const l of labels) {
    if (!showEmpty && !hasMail(l)) continue
    const p = parts(l.name)
    const ancestors = p.slice(0, -1).map((_, i) => p.slice(0, i + 1).join('/'))
    if (ancestors.some((a) => closed.has(a))) continue
    const hasChildren = below(l.name).length > 0
    out.push({ row: l, depth: p.length - 1, leaf: p[p.length - 1], hasChildren, open: hasChildren && !closed.has(l.name) })
  }
  return out
}

/** How many labels the Labels fold hides: those with mail (folded) and the empty ones. */
export function labelCounts(labels: MailPlace[]): { withMail: number; empty: number } {
  const withMail = labels.filter((l) => l.messages > 0).length
  return { withMail, empty: labels.length - withMail }
}

/** Every place, for the picker: mailboxes, pins, Topics, labels with mail, then empty labels. */
export function allPlaces(p: MailPlaces): MailPlace[] {
  const labels = [...p.labels].sort((a, b) => Number(b.messages > 0) - Number(a.messages > 0))
  return [...p.mailboxes, ...p.pinned, ...p.topics, ...labels]
}

/** Every word typed appears in the name, case-insensitively. */
export function matchPlace(row: MailPlace, q: string): boolean {
  const words = q.toLowerCase().split(/\s+/).filter(Boolean)
  const hay = row.name.toLowerCase()
  return words.every((w) => hay.includes(w))
}

/** The place's trail for the list header: `Label › Work › Northwind › Github`. */
export function placeTrail(place: string | null, places: MailPlaces | undefined): { kind: string | null; parts: string[] } {
  if (!place || place === 'all') return { kind: null, parts: ['All mail'] }
  const found = places ? allPlaces(places).find((p) => p.place === place) : undefined
  const [kind, ...rest] = place.split(':')
  const name = found?.name ?? rest.join(':')
  if (kind === 'label') return { kind: 'Label', parts: parts(name) }
  if (kind === 'topic') return { kind: 'Topic', parts: [name] }
  return { kind: null, parts: [found?.name ?? name] }
}

/** The leaf of a place's name, for short mentions: `Github`. */
export const leafName = (row: MailPlace) => (row.kind === 'label' ? parts(row.name).at(-1)! : row.name)

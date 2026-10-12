/**
 * The mail column in the mock backend, mirroring `riddle_app::places`:
 * mailboxes, the pins in pin order, Topics (a Stratum with the person's
 * label of the same name, one row, counted once per message), then the
 * remaining labels in tree order. The fixture's label ids are their names.
 */
import type { MailPlace } from '@/bindings/MailPlace'
import type { MailPlaces } from '@/bindings/MailPlaces'
import type { PlaceKind } from '@/bindings/PlaceKind'
import type { MockState } from './state'

type Msg = MockState['messages'][number]

const MAILBOXES: [string, string][] = [
  ['INBOX', 'Inbox'],
  ['STARRED', 'Starred'],
  ['SENT', 'Sent'],
]
const same = (a: string, b: string) => a.toLowerCase() === b.toLowerCase()
const isOwn = (id: string) => !/^[A-Z_]+$/.test(id) && !id.startsWith('CATEGORY_') && !id.startsWith('Riddle/')

function strataOf(state: MockState, m: Msg): string[] {
  return state.classifications.get(m.id)?.strata ?? []
}

/** Whether a message is at a place (`label:…`, `topic:…`, `mailbox:…`, `all`). */
export function atPlace(state: MockState, m: Msg, place: string): boolean {
  if (place === 'all') return true
  const [kind, ...rest] = place.split(':')
  const name = rest.join(':')
  if (kind === 'mailbox' || kind === 'label') return m.labels.includes(name)
  if (kind === 'topic') return strataOf(state, m).some((s) => same(s, name)) || m.labels.some((l) => isOwn(l) && same(l, name))
  return false
}

export function mailPlaces(state: MockState, digId: string | null): MailPlaces {
  const mail = state.messages.filter((m) => !digId || m.digId === digId)
  const count = (pred: (m: Msg) => boolean): [number, number] => {
    const hit = mail.filter(pred)
    return [hit.length, hit.filter((m) => m.labels.includes('UNREAD')).length]
  }
  const row = (place: string, kind: PlaceKind, name: string, [messages, unread]: [number, number], hasLabel = false): MailPlace => ({ place, kind, name, hasLabel, messages, unread, pinned: false })

  const mailboxes = [row('all', 'mailbox', 'All mail', count(() => true)), ...MAILBOXES.map(([id, name]) => row(`mailbox:${id}`, 'mailbox', name, count((m) => m.labels.includes(id))))]
  const own = [...new Set(mail.flatMap((m) => m.labels).filter(isOwn))]
  const merged = new Set<string>()
  const topics = state.strata
    .map((s) => {
      const labels = own.filter((l) => same(l, s.name))
      labels.forEach((l) => merged.add(l))
      return row(`topic:${s.name}`, 'topic', s.name, count((m) => atPlace(state, m, `topic:${s.name}`)), labels.length > 0)
    })
    .filter((t) => t.messages > 0 || t.hasLabel || state.pinnedPlaces.includes(t.place))
    .sort((a, b) => b.messages - a.messages || a.name.localeCompare(b.name))
  const labels = own
    .filter((l) => !merged.has(l))
    .sort((a, b) => Number(a.startsWith('[')) - Number(b.startsWith('[')) || a.toLowerCase().localeCompare(b.toLowerCase()))
    .map((l) => row(`label:${l}`, 'label', l, count((m) => m.labels.includes(l))))

  const pinned: MailPlace[] = []
  for (const p of state.pinnedPlaces) {
    for (const section of [mailboxes, topics, labels]) {
      const i = section.findIndex((x) => x.place === p)
      if (i >= 0) {
        pinned.push({ ...section.splice(i, 1)[0], pinned: true })
        break
      }
    }
  }
  return { mailboxes, pinned, topics, labels }
}

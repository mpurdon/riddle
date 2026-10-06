/**
 * The Action layer's real arguments as key/value rows. A raw JSON blob is
 * what the confirmation dialogs used to show, and the UX study watched
 * people skip it; rows read at a glance, and JSON survives only for nested
 * values that have no better shape. Mesh text is a `<pre>` because its line
 * breaks are the policy.
 */
import { safeJson, fmtAddress } from '@/lib/format'
import { argLabel } from '@/lib/pendingVerb'

const isPrimitive = (v: unknown): v is string | number | boolean | bigint | null | undefined => v === null || v === undefined || ['string', 'number', 'boolean', 'bigint'].includes(typeof v)

function recipient(v: unknown): string | null {
  if (typeof v === 'string') return v
  if (v && typeof v === 'object' && 'email' in v) {
    const r = v as { email?: unknown; name?: unknown }
    return fmtAddress({ email: typeof r.email === 'string' ? r.email : '', name: typeof r.name === 'string' ? r.name : null })
  }
  return null
}

function Value({ v }: { v: unknown }) {
  if (v === null || v === undefined) return <span className="text-clay">—</span>
  if (typeof v === 'boolean') return <>{v ? 'yes' : 'no'}</>
  if (isPrimitive(v)) return <>{String(v)}</>
  if (Array.isArray(v)) {
    if (!v.length) return <span className="text-clay">none</span>
    const names = v.map(recipient)
    if (names.every((n) => n !== null)) return <>{names.join(', ')}</>
    if (v.every(isPrimitive)) return <>{v.map(String).join(', ')}</>
  }
  return <pre className="max-h-40 overflow-auto rounded border border-line bg-sand p-2 text-xs selectable whitespace-pre-wrap">{safeJson(v)}</pre>
}

export function RealArgumentRows({ real, omit = [], className }: { real: unknown; omit?: string[]; className?: string }) {
  if (!real || typeof real !== 'object' || Array.isArray(real)) {
    return <pre className="max-h-40 overflow-auto rounded border border-line bg-sand p-2 text-xs selectable">{safeJson(real)}</pre>
  }
  const entries = Object.entries(real as Record<string, unknown>).filter(([k, v]) => !omit.includes(k) && v !== undefined)
  if (!entries.length) return <div className="text-xs text-clay">No arguments.</div>
  const text = entries.find(([k, v]) => k === 'text' && typeof v === 'string')
  const rest = entries.filter((e) => e !== text)
  return (
    <div className={className}>
      {rest.length ? (
        <dl className="grid grid-cols-[minmax(6rem,auto)_1fr] gap-x-3 gap-y-1">
          {rest.map(([k, v]) => (
            <div key={k} className="contents">
              <dt className="text-clay">{argLabel(k)}</dt>
              <dd className="min-w-0 break-words selectable">
                <Value v={v} />
              </dd>
            </div>
          ))}
        </dl>
      ) : null}
      {text ? (
        <div className={rest.length ? 'mt-2' : ''}>
          <div className="mb-1 text-clay">Mesh text</div>
          <pre className="max-h-72 overflow-auto rounded border border-line bg-sand p-2 text-xs mono selectable whitespace-pre">{String(text[1])}</pre>
        </div>
      ) : null}
    </div>
  )
}

/**
 * Labels: each account's labels with how much mail carries them, and one
 * click to read that mail in the Inbox. Gmail's labels reached Riddle only
 * as ids (`Label_4452336792128208639` on 2,200 messages), so the app had no
 * way to show the labels an account has; the account's label list is now
 * fetched on every sync. Your own labels come first, nested by "/", then
 * Gmail's built-in labels, the category tabs, and the ones Riddle made.
 */
import { useNavigate } from 'react-router-dom'
import { Tag } from 'lucide-react'
import type { LabelKind } from '@/bindings/LabelKind'
import type { LabelSummary } from '@/bindings/LabelSummary'
import { cx, fmtInt, num } from '@/lib/format'
import { useDigContext, useLabels } from '@/lib/queries'
import { useUi } from '@/lib/stores'
import { EmptyState } from '@/components/ui/EmptyState'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'
import { labelName } from '@/features/messages/labels'

const SECTIONS: { kind: LabelKind; title: string }[] = [
  { kind: 'user', title: 'Your labels' },
  { kind: 'system', title: 'Gmail' },
  { kind: 'category', title: 'Categories' },
  { kind: 'riddle', title: "Riddle's labels" },
]

/** `Receipts/2026` reads as `2026`, indented under `Receipts`. */
export function nesting(name: string): { depth: number; leaf: string } {
  const parts = name.split('/')
  return { depth: parts.length - 1, leaf: parts[parts.length - 1] }
}

export default function LabelsPage() {
  const q = useLabels()
  const { digs, digName, digId } = useDigContext()
  const setDigId = useUi((s) => s.setDigId)
  const nav = useNavigate()
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  const labels = q.data ?? []
  const accounts = digId ? [digId] : digs.map((d) => d.id).filter((id) => labels.some((l) => l.digId === id))
  // Your own labels and Gmail's mailboxes are places in the mail column; the rest filter by id.
  const open = (l: LabelSummary) => {
    setDigId(l.digId)
    const place = l.kind === 'user' ? `label:${l.name}` : ['INBOX', 'STARRED', 'SENT'].includes(l.id) ? `mailbox:${l.id}` : null
    nav(place ? `/inbox?place=${encodeURIComponent(place)}` : `/inbox?label=${encodeURIComponent(l.id)}`)
  }
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Labels" subtitle="Each account's labels in Gmail, with how much of your mail carries them. Open one to read that mail." />
      <div className="min-h-0 flex-1 overflow-auto px-5 py-3">
        {q.data && !labels.length ? <EmptyState icon={<Tag />} title="No labels yet." body="An account's labels appear after its next sync." /> : null}
        {accounts.map((dig) => (
          <section key={dig} aria-label={digName(dig)} className="mb-5">
            {accounts.length > 1 || !digId ? <h2 className="mb-1 text-sm font-semibold">{digName(dig)}</h2> : null}
            {SECTIONS.map(({ kind, title }) => {
              const rows = labels.filter((l) => l.digId === dig && l.kind === kind)
              if (!rows.length) return null
              return (
                <table key={kind} className="mb-3 w-full text-sm" aria-label={`${digName(dig)}: ${title}`}>
                  <thead>
                    <tr className="border-b border-line text-left text-2xs uppercase tracking-wide text-clay">
                      <th className="py-1 font-medium">{title}</th>
                      <th className="w-24 py-1 text-right font-medium">Messages</th>
                      <th className="w-24 py-1 text-right font-medium">Unread</th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((l) => {
                      const { depth, leaf } = kind === 'user' || kind === 'riddle' ? nesting(l.name) : { depth: 0, leaf: labelName(l.name) }
                      return (
                        <tr key={l.id} className="border-b border-line/50">
                          <td className="py-1" style={{ paddingLeft: depth * 16 }}>
                            <button type="button" onClick={() => open(l)} className="text-left hover:underline" title={l.name}>
                              {leaf}
                            </button>
                          </td>
                          <td className="py-1 text-right tabular-nums">{fmtInt(l.messages)}</td>
                          <td className={cx('py-1 text-right tabular-nums', num(l.unread) ? 'font-medium' : 'text-clay')}>{fmtInt(l.unread)}</td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              )
            })}
          </section>
        ))}
      </div>
    </div>
  )
}

/**
 * The Unresolved tray (README "Unresolved"): mail Riddle could not decide,
 * never hidden, with Decide controls that record a correction.
 */
import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { ListChecks } from 'lucide-react'
import type { Disposition } from '@/bindings/Disposition'
import type { FindType } from '@/bindings/FindType'
import type { MessageSummary } from '@/bindings/MessageSummary'
import { explain, listMessages, page, qk } from '@/lib/api'
import { cx, fmtTime, num } from '@/lib/format'
import { useDigContext } from '@/lib/queries'
import { findTypeLabel, FIND_TYPES } from '@/lib/selectors'
import { useAction } from '@/lib/useAction'
import { useListNav } from '@/lib/useListNav'
import { useShortcuts } from '@/lib/useShortcuts'
import { Button } from '@/components/ui/Button'
import { EmptyState } from '@/components/ui/EmptyState'
import { Menu } from '@/components/ui/Menu'
import { ErrorCard } from '@/components/ErrorCard'
import { ExplanationCard, PageHeader, StrataChips } from '@/components/Bits'

export default function UnresolvedPage() {
  const { digId, digName } = useDigContext()
  const nav = useNavigate()
  const { run } = useAction()
  const q = useQuery({ queryKey: [...qk.messages, 'unresolved', digId], queryFn: () => listMessages({ digId, filter: { disposition: 'unresolved' }, page: page(null, 200) }) })
  const items = useMemo(() => q.data?.items ?? [], [q.data])
  const list = useListNav(items, { onOpen: (m) => nav(`/inbox/${m.digId}/${m.id}`), multi: false })
  const current = list.current
  const ex = useQuery({ queryKey: current ? qk.explain(current.digId, current.id) : ['explain', 'none'], queryFn: () => explain(current!.digId, current!.id), enabled: !!current })
  const [neverHiddenBusy, setNeverHiddenBusy] = useState(false)

  const decide = (m: MessageSummary, d: Disposition) => run('set_disposition', { digId: m.digId, messageId: m.id, disposition: d, findType: d.kind === 'find' ? d.find_type : undefined }, { digId: m.digId })
  useShortcuts('review', {
    decide_find: () => current && decide(current, { kind: 'find', find_type: 'respond' }),
    decide_filed: () => current && decide(current, { kind: 'filed' }),
    decide_spoil: () => current && decide(current, { kind: 'spoil' }),
    never_hidden: () => current && run('add_never_hidden', { entry: current.from.email }),
    not_spoil: () => current && decide(current, { kind: 'filed' }),
  })
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="flex min-h-0 flex-1">
      <section className="flex w-[460px] shrink-0 flex-col border-r border-line">
        <PageHeader title="Unresolved" subtitle={q.data ? `${num(q.data.total)} Riddle could not decide. Deciding here is the fastest way to teach it.` : 'Loading…'} />
        <div className="min-h-0 flex-1 overflow-auto" role="listbox" aria-label="Unresolved messages">
          {q.data && !items.length ? (
            <EmptyState icon={<ListChecks />} title="Nothing is Unresolved." body="Attention Zero includes this tray: when it is empty and there are no open Finds, there is nothing to deal with. When a judge or LLM is unavailable, mail lands here rather than being hidden." />
          ) : null}
          {items.map((m, idx) => (
            <div key={m.id} role="option" aria-selected={current?.id === m.id} data-row-index={idx} tabIndex={-1} onClick={() => list.setCursor(idx)} className={cx('cursor-default border-b border-line/60 px-4 py-2', current?.id === m.id ? 'row-selected' : 'hover:bg-sand/60')}>
              <div className="flex items-center gap-2">
                <span className="w-32 truncate text-sm font-medium">{m.from.name ?? m.from.email}</span>
                <span className="min-w-0 flex-1 truncate text-sm">{m.subject}</span>
                <span className="text-xs text-clay">{fmtTime(m.date)}</span>
              </div>
              <div className="mt-0.5 flex items-center gap-2 text-xs text-ink-2">
                <span className="truncate">{m.snippet}</span>
              </div>
              <div className="mt-1 text-[11px] text-clay">
                {digName(m.digId)} <StrataChips strata={m.strata} />
              </div>
            </div>
          ))}
        </div>
      </section>
      <section className="flex min-w-0 flex-1 flex-col">
        {current ? (
          <>
            <PageHeader
              title={current.subject}
              subtitle={`${current.from.name ?? ''} <${current.from.email}> · ${digName(current.digId)} · ${fmtTime(current.date)}`}
              actions={
                <>
                  <Menu label="Decide: it is a Find" items={FIND_TYPES.map((t: FindType) => ({ label: `Find: ${findTypeLabel(t)}`, onSelect: () => decide(current, { kind: 'find', find_type: t }) }))} trigger={<Button size="sm" variant="primary" title="Decide: Find (f)">Find…</Button>} />
                  <Button size="sm" onClick={() => decide(current, { kind: 'filed' })} title="Decide: Filed (i)">
                    Filed
                  </Button>
                  <Button size="sm" onClick={() => decide(current, { kind: 'spoil' })} title="Decide: Spoil (#)">
                    Spoil
                  </Button>
                  <Button size="sm" variant="danger" onClick={() => decide(current, { kind: 'contamination' })}>
                    Contamination
                  </Button>
                  <Button
                    size="sm"
                    variant="ghost"
                    busy={neverHiddenBusy}
                    onClick={async () => {
                      setNeverHiddenBusy(true)
                      try {
                        await run('add_never_hidden', { entry: current.from.email })
                      } finally {
                        setNeverHiddenBusy(false)
                      }
                    }}
                    title="Never hide this sender (h)"
                  >
                    Never hide
                  </Button>
                </>
              }
            />
            <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
              <p className="text-sm text-ink-2">Decide what this is. Your answer is recorded as a correction; repeated corrections of the same kind lead to a Mesh proposal.</p>
              {ex.data ? (
                <div className="mt-4">
                  <ExplanationCard ex={ex.data} digId={current.digId} />
                </div>
              ) : null}
              <div className="mt-4">
                <Button size="sm" variant="ghost" onClick={() => nav(`/inbox/${current.digId}/${current.id}`)}>
                  Open the message
                </Button>
              </div>
            </div>
          </>
        ) : (
          <EmptyState title="Select a message" body="f decides it is a Find, i Filed, # Spoil, h never hides the sender." />
        )}
      </section>
    </div>
  )
}

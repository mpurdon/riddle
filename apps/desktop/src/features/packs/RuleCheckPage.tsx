/**
 * Rule check: is each rule doing what it says? Every running Mesh with how
 * much it decided and how the decisions you checked were graded; "Check"
 * shows ten of its recent decisions, one per sender before any repeats, to
 * mark Right or Wrong. Wrong asks what it should have been and corrects the
 * message, which is what teaches; the grade is the tally. A new person
 * onboarding has no Director conversation to judge a pack by, so this is
 * where a pack earns trust, or shows where it needs a list answered.
 */
import { useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Link } from 'react-router-dom'
import { Check, X } from 'lucide-react'
import type { Disposition } from '@/bindings/Disposition'
import type { RuleCheckRow } from '@/bindings/RuleCheckRow'
import type { RuleSample } from '@/bindings/RuleSample'
import { qk, ruleCheck, ruleSamples } from '@/lib/api'
import { cx, fmtInt, num } from '@/lib/format'
import { dispositionLabel, findTypeLabel, FIND_TYPES } from '@/lib/selectors'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Menu, type MenuItem } from '@/components/ui/Menu'
import { PageHeader } from '@/components/Bits'
import { ErrorCard } from '@/components/ErrorCard'

export default function RuleCheckPage() {
  const q = useQuery({ queryKey: qk.ruleCheck, queryFn: ruleCheck })
  const [open, setOpen] = useState<string | null>(null)
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Rule check" subtitle="Check a few decisions each rule made. Wrong ones are corrected on the spot, and the tally shows which rules you can trust." />
      <div className="min-h-0 flex-1 overflow-auto px-5 py-3">
        <table className="w-full text-sm" aria-label="Rules">
          <thead>
            <tr className="border-b border-line text-left text-2xs uppercase tracking-wide text-clay">
              <th className="py-1 font-medium">Mesh</th>
              <th className="w-24 py-1 text-right font-medium">Decided</th>
              <th className="w-40 py-1 pl-4 font-medium">Checked</th>
              <th className="w-24 py-1" />
            </tr>
          </thead>
          <tbody>
            {(q.data ?? []).map((r) => (
              <RuleRow key={r.meshId} r={r} open={open === r.meshId} onToggle={() => setOpen(open === r.meshId ? null : r.meshId)} />
            ))}
          </tbody>
        </table>
      </div>
    </div>
  )
}

function RuleRow({ r, open, onToggle }: { r: RuleCheckRow; open: boolean; onToggle: () => void }) {
  const right = num(r.right)
  const wrong = num(r.wrong)
  const checked = right + wrong
  return (
    <>
      <tr className="border-b border-line/50">
        <td className="py-1.5">
          <Link to={`/meshes/${r.meshId}`} className="font-medium hover:underline">
            {r.meshName}
          </Link>
          {r.fallback ? <span className="ml-1.5"><Chip>fallback</Chip></span> : null}
        </td>
        <td className="py-1.5 text-right tabular-nums">{fmtInt(r.decided)}</td>
        <td className="py-1.5 pl-4 text-xs">
          {checked ? (
            <span className={cx(wrong / checked > 0.2 ? 'text-rust' : 'text-ink-2')}>
              {right} right, {wrong} wrong
            </span>
          ) : (
            <span className="text-clay">not checked</span>
          )}
        </td>
        <td className="py-1 text-right">
          <Button size="sm" variant={open ? 'ghost' : 'secondary'} disabledReason={num(r.decided) ? null : 'It has not decided anything yet'} onClick={onToggle}>
            {open ? 'Close' : 'Check 10'}
          </Button>
        </td>
      </tr>
      {open ? (
        <tr>
          <td colSpan={4} className="bg-sand/40 px-3 py-2">
            <Samples meshId={r.meshId} />
          </td>
        </tr>
      ) : null}
    </>
  )
}

function Samples({ meshId }: { meshId: string }) {
  const q = useQuery({ queryKey: qk.ruleSamples(meshId), queryFn: () => ruleSamples(meshId, 10) })
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} compact />
  if (!q.data) return <p className="text-xs text-clay">Loading…</p>
  if (!q.data.length) return <p className="text-xs text-clay">No recent decisions to check.</p>
  return (
    <ul aria-label="Decisions to check">
      {q.data.map((s) => (
        <SampleRow key={s.messageId} meshId={meshId} s={s} />
      ))}
    </ul>
  )
}

function SampleRow({ meshId, s }: { meshId: string; s: RuleSample }) {
  const { run } = useAction()
  const grade = (verdict: 'right' | 'wrong') => run('grade_decision', { meshId, messageId: s.messageId, verdict }, { successToast: false })
  const correct = async (d: Disposition) => {
    await run('set_disposition', { digId: s.digId, messageId: s.messageId, disposition: d, findType: d.kind === 'find' ? d.find_type : undefined }, { digId: s.digId })
    await grade('wrong')
  }
  const choices: (MenuItem | 'separator')[] = [
    ...FIND_TYPES.map((t) => ({ label: `Find: ${findTypeLabel(t)}`, onSelect: () => void correct({ kind: 'find', find_type: t }) })),
    'separator',
    { label: 'Filed', onSelect: () => void correct({ kind: 'filed' }) },
    { label: 'Spoil', onSelect: () => void correct({ kind: 'spoil' }) },
    { label: 'Contamination', danger: true, onSelect: () => void correct({ kind: 'contamination' }) },
  ]
  return (
    <li className="flex items-center gap-2 border-b border-line/40 py-1 text-xs last:border-b-0" aria-label={s.subject}>
      <span className="w-48 shrink-0 truncate text-ink-2" title={s.from.email}>
        {s.from.name || s.from.email}
      </span>
      <Link to={`/inbox/${s.digId}/${s.messageId}`} className="min-w-0 flex-1 truncate hover:underline" title={s.subject}>
        {s.subject || '(no subject)'}
      </Link>
      <Chip>{dispositionLabel(s.disposition)}</Chip>
      {s.strata.length ? <span className="text-clay">{s.strata.join(', ')}</span> : null}
      {s.grade ? (
        <Chip tone={s.grade === 'right' ? 'ok' : 'bad'}>{s.grade === 'right' ? 'Right' : 'Wrong'}</Chip>
      ) : (
        <>
          <Button size="sm" variant="ghost" icon={<Check />} onClick={() => void grade('right')}>
            Right
          </Button>
          <Menu label="What should it have been?" items={choices} trigger={<Button size="sm" variant="ghost" icon={<X />}>Wrong…</Button>} />
        </>
      )}
    </li>
  )
}

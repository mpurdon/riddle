/**
 * Strata (README "Strata"): list with counts; rename, merge and create with
 * a near-duplicate warning before a new Stratum is made.
 */
import { useMemo, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Layers, Merge, Pencil, Plus } from 'lucide-react'
import type { Stratum } from '@/bindings/Stratum'
import { fmtInt } from '@/lib/format'
import { useStrata, useDigGroups } from '@/lib/queries'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Dialog } from '@/components/ui/Dialog'
import { EmptyState } from '@/components/ui/EmptyState'
import { Field, Input, TextArea } from '@/components/ui/Field'
import { Note } from '@/components/ui/Note'
import { Select } from '@/components/ui/Select'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'

function similarity(a: string, b: string): number {
  const x = a.toLowerCase().replace(/[^a-z0-9]/g, '')
  const y = b.toLowerCase().replace(/[^a-z0-9]/g, '')
  if (!x || !y) return 0
  if (x === y) return 1
  if (x.includes(y) || y.includes(x)) return 0.85
  const bi = (s: string) => new Set(Array.from({ length: s.length - 1 }, (_, i) => s.slice(i, i + 2)))
  const bx = bi(x)
  const by = bi(y)
  let hit = 0
  for (const g of bx) if (by.has(g)) hit++
  return (2 * hit) / (bx.size + by.size)
}

export function nearDuplicates(name: string, strata: Stratum[]): Stratum[] {
  return strata.filter((s) => similarity(name, s.name) >= 0.6)
}

export default function StrataPage() {
  const q = useStrata()
  const groups = useDigGroups()
  const nav = useNavigate()
  const { run } = useAction()
  const [renaming, setRenaming] = useState<Stratum | null>(null)
  const [merging, setMerging] = useState<Stratum | null>(null)
  const [creating, setCreating] = useState(false)
  const strata = q.data ?? []
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Strata" subtitle="Overlapping categories. Riddle proposes them as patterns appear; you rename, merge and retire them." actions={<Button size="sm" icon={<Plus />} onClick={() => setCreating(true)}>New Stratum</Button>} />
      <div className="min-h-0 flex-1 overflow-auto">
        {q.data && !strata.length ? <EmptyState icon={<Layers />} title="No Strata yet." body="Strata emerge from how you work. Once the Sift has seen enough mail it proposes categories such as Purchases, Travel or Engineering. You can also create one now." actions={<Button size="sm" onClick={() => setCreating(true)}>Create a Stratum</Button>} /> : null}
        <table className="w-full text-sm">
          <thead className="sticky top-0 bg-sand text-left text-[11px] uppercase tracking-wide text-clay">
            <tr>
              <th className="px-5 py-1.5 font-semibold">Stratum</th>
              <th className="px-3 py-1.5 font-semibold">Scope</th>
              <th className="px-3 py-1.5 text-right font-semibold">Messages</th>
              <th className="px-3 py-1.5 font-semibold">Origin</th>
              <th className="px-3 py-1.5 font-semibold">v</th>
              <th className="px-3 py-1.5" />
            </tr>
          </thead>
          <tbody>
            {strata.map((s) => (
              <tr key={s.id} className="border-b border-line/60 hover:bg-sand/60">
                <td className="px-5 py-1.5">
                  <button type="button" className="font-medium hover:underline" onClick={() => nav(`/inbox?stratum=${encodeURIComponent(s.name)}`)}>
                    {s.name}
                  </button>
                  <div className="text-xs text-ink-2">{s.description}</div>
                </td>
                <td className="px-3 py-1.5 text-xs">{s.scopeGroup ?? 'All Digs'}</td>
                <td className="px-3 py-1.5 text-right tabular-nums">{fmtInt(s.messageCount)}</td>
                <td className="px-3 py-1.5">{s.proposedByRiddle ? <Chip>proposed by Riddle</Chip> : <Chip tone="neutral">yours</Chip>}</td>
                <td className="px-3 py-1.5 text-xs text-clay">{s.version}</td>
                <td className="px-3 py-1.5 text-right">
                  <Button size="sm" variant="ghost" icon={<Pencil />} onClick={() => setRenaming(s)}>
                    Rename
                  </Button>
                  <Button size="sm" variant="ghost" icon={<Merge />} onClick={() => setMerging(s)} disabledReason={strata.length > 1 ? null : 'Nothing to merge into'}>
                    Merge
                  </Button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      {renaming ? (
        <NameDialog
          title={`Rename ${renaming.name}`}
          initial={renaming.name}
          strata={strata.filter((s) => s.id !== renaming.id)}
          onClose={() => setRenaming(null)}
          onSubmit={async (name) => {
            await run('rename_stratum', { stratumId: renaming.id, name })
            setRenaming(null)
          }}
        />
      ) : null}
      {creating ? (
        <NameDialog
          title="New Stratum"
          initial=""
          strata={strata}
          withDescription
          groups={groups.data?.map((g) => g.name) ?? []}
          onClose={() => setCreating(false)}
          onSubmit={async (name, description, scopeGroup) => {
            await run('create_stratum', { name, description, scopeGroup })
            setCreating(false)
          }}
        />
      ) : null}
      {merging ? (
        <MergeDialog from={merging} strata={strata.filter((s) => s.id !== merging.id)} onClose={() => setMerging(null)} onSubmit={async (intoId) => { await run('merge_strata', { fromId: merging.id, intoId }); setMerging(null) }} />
      ) : null}
    </div>
  )
}

function NameDialog({ title, initial, strata, onClose, onSubmit, withDescription, groups }: { title: string; initial: string; strata: Stratum[]; onClose: () => void; onSubmit: (name: string, description: string, scopeGroup: string | null) => Promise<void>; withDescription?: boolean; groups?: string[] }) {
  const [name, setName] = useState(initial)
  const [description, setDescription] = useState('')
  const [scope, setScope] = useState('all')
  const dupes = useMemo(() => (name.trim() ? nearDuplicates(name, strata) : []), [name, strata])
  const exact = dupes.some((d) => d.name.toLowerCase() === name.trim().toLowerCase())
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title={title}>
      <Field label="Name">
        <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} />
      </Field>
      {withDescription ? (
        <>
          <Field label="Description" className="mt-3">
            <TextArea value={description} onChange={(e) => setDescription(e.target.value)} placeholder="What belongs here" />
          </Field>
          <Field label="Scope" className="mt-3" hint="A Stratum belongs to a Dig Group or to all Digs, so a work category does not leak into Personal.">
            <Select label="Scope" value={scope} onChange={setScope} options={[{ value: 'all', label: 'All Digs' }, ...(groups ?? []).map((g) => ({ value: g, label: `${g} Digs` }))]} />
          </Field>
        </>
      ) : null}
      {dupes.length ? (
        <Note tone={exact ? 'bad' : 'warn'} title={exact ? 'A Stratum with this name exists' : 'Similar Strata exist'} className="mt-3">
          {dupes.map((d) => d.name).join(', ')}. {exact ? 'Pick another name or merge into it.' : 'Consider merging instead of creating a near duplicate.'}
        </Note>
      ) : null}
      <div className="mt-4 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button variant="primary" disabledReason={!name.trim() ? 'Give it a name' : exact ? 'That name is taken' : null} onClick={() => void onSubmit(name.trim(), description, scope === 'all' ? null : scope)}>
          {initial ? 'Rename' : 'Create'}
        </Button>
      </div>
    </Dialog>
  )
}

function MergeDialog({ from, strata, onClose, onSubmit }: { from: Stratum; strata: Stratum[]; onClose: () => void; onSubmit: (intoId: string) => Promise<void> }) {
  const [into, setInto] = useState(strata[0]?.id ?? '')
  return (
    <Dialog open onOpenChange={(o) => !o && onClose()} title={`Merge ${from.name} into…`} description={`${fmtInt(from.messageCount)} messages will carry the target Stratum instead. Meshes that route to ${from.name} are re-pointed.`}>
      <Select label="Merge into" value={into} onChange={setInto} options={strata.map((s) => ({ value: s.id, label: `${s.name} (${fmtInt(s.messageCount)})` }))} />
      <div className="mt-4 flex justify-end gap-2">
        <Button variant="ghost" onClick={onClose}>
          Cancel
        </Button>
        <Button variant="primary" disabledReason={into ? null : 'Pick a target'} onClick={() => void onSubmit(into)}>
          Merge…
        </Button>
      </div>
    </Dialog>
  )
}

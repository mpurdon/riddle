/**
 * Rename and delete Dig Groups where they are listed. Meshes and Strata
 * refer to a group by name, so a group something is scoped to cannot be
 * renamed or deleted: the controls say which rules use it, from the list
 * the backend puts on the group (`DigGroup.usedBy`), and the Action refuses
 * the same way if asked directly. The page used to fetch every Mesh and
 * Stratum to work the list out itself, and disagreed with the Action when
 * a version it had not loaded named the group.
 */
import { useState } from 'react'
import { Pencil, Trash2 } from 'lucide-react'
import type { Dig } from '@/bindings/Dig'
import type { DigGroup } from '@/bindings/DigGroup'
import { plural } from '@/lib/format'
import { toRiddleError } from '@/lib/ipc'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Field'

export function Groups({ groups, digs }: { groups: DigGroup[]; digs: Dig[] }) {
  if (groups.length === 0) return <p className="text-xs text-clay">No groups yet. Pick “New group…” on an account to create one.</p>
  return (
    <ul className="flex flex-col gap-1.5 text-sm">
      {groups.map((g) => (
        <GroupRow key={g.name} group={g} digs={digs} others={groups.filter((x) => x.name !== g.name).map((x) => x.name)} />
      ))}
    </ul>
  )
}

type Mode = 'view' | 'rename' | 'delete'

function GroupRow({ group, digs, others }: { group: DigGroup; digs: Dig[]; others: string[] }) {
  const { run } = useAction()
  const [mode, setMode] = useState<Mode>('view')
  const [name, setName] = useState(group.name)
  const [error, setError] = useState<string | null>(null)
  const members = group.digs.map((id) => digs.find((d) => d.id === id)?.name ?? id)
  const users = group.usedBy
  const locked = users.length ? `${users.join(', ')} ${users.length === 1 ? 'is' : 'are'} scoped to this group. Change ${users.length === 1 ? 'its' : 'their'} scope first.` : null
  const trimmed = name.trim()
  const clash = others.some((o) => o.toLowerCase() === trimmed.toLowerCase())
  const act = async (f: () => Promise<unknown>) => {
    setError(null)
    try {
      await f()
      setMode('view')
    } catch (e) {
      setError(toRiddleError(e).message)
    }
  }

  return (
    <li className="rounded-md border border-line px-3 py-2">
      {mode === 'rename' ? (
        <div className="flex items-center gap-2">
          <Input
            autoFocus
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape') setMode('view')
              if (e.key === 'Enter' && trimmed && !clash && trimmed !== group.name) void act(() => run('rename_dig_group', { from: group.name, to: trimmed }))
            }}
            className="w-56"
            aria-label={`New name for ${group.name}`}
          />
          <Button size="sm" variant="primary" disabledReason={!trimmed ? 'Name the group' : trimmed === group.name ? 'No change' : clash ? 'A group with that name exists' : null} onClick={() => act(() => run('rename_dig_group', { from: group.name, to: trimmed }))}>
            Rename
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setMode('view')}>
            Cancel
          </Button>
        </div>
      ) : (
        <div className="flex items-center gap-2">
          <span className="font-medium">{group.name}</span>
          <span className="min-w-0 flex-1 truncate text-xs text-clay">{members.join(', ') || 'no accounts'}</span>
          {users.length ? <span className="shrink-0 text-2xs text-ink-2" title={users.join(', ')}>Used by {plural(users.length, 'rule')}</span> : null}
          <Button size="sm" variant="ghost" icon={<Pencil />} disabledReason={locked} onClick={() => { setName(group.name); setMode('rename') }}>
            Rename
          </Button>
          <Button size="sm" variant="ghost" icon={<Trash2 />} disabledReason={locked} onClick={() => setMode('delete')}>
            Delete
          </Button>
        </div>
      )}
      {mode === 'delete' ? (
        <div role="alert" className="mt-2 flex items-center gap-2 rounded-md border border-rust/30 bg-rust/5 px-3 py-2 text-xs">
          <span className="flex-1">
            Delete “{group.name}”? {members.length ? `Its ${members.length === 1 ? 'account stays' : `${members.length} accounts stay`}, in no group.` : 'It has no accounts.'} You can undo this from the toast or Activity.
          </span>
          <Button size="sm" variant="ghost" onClick={() => setMode('view')}>
            Cancel
          </Button>
          <Button size="sm" variant="danger" onClick={() => act(() => run('delete_dig_group', { name: group.name }))}>
            Delete group
          </Button>
        </div>
      ) : null}
      {error ? <p role="alert" className="mt-2 text-xs text-rust">{error}</p> : null}
    </li>
  )
}

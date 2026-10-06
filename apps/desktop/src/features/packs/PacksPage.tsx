/**
 * Rule packs: bundles of Meshes for a kind of mail (GitHub, money,
 * shopping…), installed with answers to the questions their lists ask.
 * Packs keep a person's values out of the rules: "keep promotions from
 * list:keep_promotions", with the list filled in here, so a pack can be
 * shared and a newer version can replace its Meshes without touching
 * anyone's answers. Installing places each Mesh in its band (safety,
 * personal, specific, general, fallback), so nobody picks Priority numbers.
 * New Meshes start in Shadow unless the person chooses otherwise.
 */
import { useMemo, useRef, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Download, Package, Upload } from 'lucide-react'
import type { PackInfo } from '@/bindings/PackInfo'
import { listPacks, qk } from '@/lib/api'
import { cx } from '@/lib/format'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Dialog } from '@/components/ui/Dialog'
import { PageHeader } from '@/components/Bits'
import { ErrorCard } from '@/components/ErrorCard'

/** One entry per line or comma; what save_list and install_pack take. */
export function parseEntries(text: string): string[] {
  return text
    .split(/[\n,]/)
    .map((e) => e.trim())
    .filter(Boolean)
}

export default function PacksPage() {
  const q = useQuery({ queryKey: qk.packs, queryFn: listPacks })
  const [importing, setImporting] = useState(false)
  const areas = useMemo(() => {
    const by = new Map<string, PackInfo[]>()
    for (const p of q.data ?? []) by.set(p.pack.area, [...(by.get(p.pack.area) ?? []), p])
    return [...by.entries()]
  }, [q.data])
  if (q.error) return <ErrorCard error={q.error} onRetry={() => q.refetch()} />
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader
        title="Rule packs"
        subtitle="Ready-made Meshes for kinds of mail. Install a pack, answer its questions, and it runs in Shadow until it has earned the right to act."
        actions={
          <Button size="sm" icon={<Upload />} onClick={() => setImporting(true)}>
            Import a pack
          </Button>
        }
      />
      <div className="min-h-0 flex-1 overflow-auto px-5 py-4">
        {areas.map(([area, packs]) => (
          <section key={area} className="mb-6" aria-label={area}>
            <h2 className="mb-2 text-2xs font-semibold uppercase tracking-wide text-clay">{area}</h2>
            <div className="grid gap-3 lg:grid-cols-2">
              {packs.map((p) => (
                <PackCard key={p.pack.id} info={p} />
              ))}
            </div>
          </section>
        ))}
        {q.data && !q.data.length ? <p className="text-sm text-clay">No packs.</p> : null}
      </div>
      <ImportDialog open={importing} onOpenChange={setImporting} />
    </div>
  )
}

function PackCard({ info }: { info: PackInfo }) {
  const { run } = useAction()
  const { pack, installed, conflicts } = info
  const [open, setOpen] = useState(false)
  const [active, setActive] = useState(false)
  const [answers, setAnswers] = useState<Record<string, string>>(() =>
    Object.fromEntries(pack.lists.map((l) => [l.name, (info.listEntries[l.name]?.length ? (info.listEntries[l.name] ?? []) : l.default).join('\n')])),
  )
  const [exported, setExported] = useState<string | null>(null)
  const update = installed && installed.version < pack.version
  const lists = () => Object.fromEntries(pack.lists.map((l) => [l.name, parseEntries(answers[l.name] ?? '')]))
  const install = () =>
    run('install_pack', { packId: pack.id, lists: lists(), status: active ? 'active' : 'shadow', replace: conflicts.length > 0 }, { successToast: installed ? `${pack.name} updated` : `${pack.name} installed` })
  const saveLists = async () => {
    for (const l of pack.lists) await run('save_list', { name: l.name, entries: parseEntries(answers[l.name] ?? ''), description: l.description }, { successToast: false })
  }
  const exportPack = async () => {
    const r = await run('export_pack', { packId: pack.id }, { successToast: false })
    setExported((r?.result as { text?: string } | undefined)?.text ?? null)
  }
  return (
    <article className={cx('rounded-md border px-4 py-3', installed ? 'border-moss/40 bg-moss/5' : 'border-line')} aria-label={pack.name}>
      <div className="flex items-start gap-2">
        <Package className="mt-0.5 size-4 shrink-0 text-clay" />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-1.5">
            <span className="font-medium">{pack.name}</span>
            <span className="text-xs text-clay">v{pack.version}</span>
            {installed ? <Chip tone="ok">Installed v{installed.version}</Chip> : null}
            {update ? <Chip tone="warn">Update available</Chip> : null}
            {!info.builtin ? <Chip>Imported</Chip> : null}
          </div>
          <p className="mt-1 text-sm text-ink-2">{pack.summary}</p>
          <p className="mt-1 text-xs text-clay">Meshes: {info.meshNames.join(', ')}</p>
          {conflicts.length ? <p className="mt-1 text-xs text-ochre">Takes over your {conflicts.join(', ')}: their rules are replaced by the pack's, with Undo.</p> : null}
        </div>
      </div>
      {open ? (
        <div className="mt-3 border-t border-line pt-3">
          {pack.lists.map((l) => (
            <label key={l.name} className="mb-3 block text-sm">
              <span className="font-medium">{l.question || l.description || l.name}</span>
              <span className="ml-1.5 text-xs text-clay">list:{l.name}</span>
              <textarea
                className="mt-1 block h-20 w-full rounded-md border border-line bg-paper px-2 py-1 font-mono text-xs"
                value={answers[l.name] ?? ''}
                onChange={(e) => setAnswers((a) => ({ ...a, [l.name]: e.target.value }))}
                placeholder="One address or domain per line, like uber.com"
                aria-label={`Entries for list:${l.name}`}
              />
            </label>
          ))}
          {!pack.lists.length ? <p className="mb-3 text-xs text-clay">This pack asks no questions.</p> : null}
          {!installed ? (
            <label className="mb-3 flex items-center gap-2 text-xs">
              <input type="checkbox" checked={active} onChange={(e) => setActive(e.target.checked)} />
              Start Active instead of Shadow (it acts at once rather than proposing first)
            </label>
          ) : null}
          <div className="flex flex-wrap gap-2">
            {installed && !update ? (
              <Button size="sm" onClick={() => void saveLists()}>
                Save answers
              </Button>
            ) : (
              <Button size="sm" variant="primary" onClick={() => void install()}>
                {update ? `Update to v${pack.version}` : 'Install'}
              </Button>
            )}
            <Button size="sm" variant="ghost" icon={<Download />} onClick={() => void exportPack()}>
              Pack file
            </Button>
            {installed ? (
              <Button size="sm" variant="ghost" onClick={() => void run('remove_pack', { packId: pack.id }, { successToast: `${pack.name} removed` })}>
                Remove
              </Button>
            ) : null}
          </div>
        </div>
      ) : (
        <div className="mt-2">
          <Button size="sm" variant={installed ? 'ghost' : 'primary'} onClick={() => setOpen(true)}>
            {installed ? (update ? 'Review the update' : 'Answers and options') : 'Set up'}
          </Button>
        </div>
      )}
      <Dialog open={exported !== null} onOpenChange={(o) => !o && setExported(null)} title={`${pack.name}: pack file`} description="Save this as a .riddlepack.toml file to share it. It holds the rules and the questions; your answers stay here." wide>
        <textarea readOnly className="h-72 w-full rounded-md border border-line bg-sand p-2 font-mono text-xs" value={exported ?? ''} aria-label="Pack file" />
        <div className="mt-2 flex justify-end">
          <Button size="sm" onClick={() => void navigator.clipboard?.writeText(exported ?? '')}>
            Copy
          </Button>
        </div>
      </Dialog>
    </article>
  )
}

function ImportDialog({ open, onOpenChange }: { open: boolean; onOpenChange: (o: boolean) => void }) {
  const { run } = useAction()
  const [text, setText] = useState('')
  const file = useRef<HTMLInputElement>(null)
  const submit = async () => {
    await run('import_pack', { text }, { successToast: 'Pack added to the catalogue' })
    setText('')
    onOpenChange(false)
  }
  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="Import a pack" description="Paste a pack file, or choose one. It joins the catalogue; installing it is a separate step." wide>
      <input
        ref={file}
        type="file"
        accept=".toml,.riddlepack,text/plain"
        className="mb-2 block text-xs"
        aria-label="Pack file to import"
        onChange={async (e) => {
          const f = e.target.files?.[0]
          if (f) setText(await f.text())
        }}
      />
      <textarea className="h-64 w-full rounded-md border border-line bg-paper p-2 font-mono text-xs" value={text} onChange={(e) => setText(e.target.value)} aria-label="Pack file text" placeholder='id = "my-pack"&#10;name = "My pack"&#10;…' />
      <div className="mt-2 flex justify-end gap-2">
        <Button size="sm" variant="ghost" onClick={() => onOpenChange(false)}>
          Cancel
        </Button>
        <Button size="sm" variant="primary" disabledReason={text.trim() ? null : 'Paste or choose a pack file'} onClick={() => void submit()}>
          Add to catalogue
        </Button>
      </div>
    </Dialog>
  )
}

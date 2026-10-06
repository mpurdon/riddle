/**
 * The operations pane (§67): Runtime, Storage with reveal, Logs live tail,
 * IPC in-flight, Diagnostics copy, Probes, and the dev build's mailbox writes.
 */
import { useEffect, useMemo, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Copy, FolderOpen, Pause, Play } from 'lucide-react'
import { exportDiagnostics, inFlightCalls, logCategories, probeJudge, probeLlm, probeProvider, qk, readAppLogs } from '@/lib/api'
import { cx, fmtBytes, fmtClockMs, fmtDuration, safeJson } from '@/lib/format'
import { isTauri, log } from '@/lib/ipc'
import { useDevInfo, useDigs, useSettings } from '@/lib/queries'
import { toast, useUi } from '@/lib/stores'
import type { ProbeResult } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Input } from '@/components/ui/Field'
import { Note } from '@/components/ui/Note'
import { Select } from '@/components/ui/Select'
import { Switch } from '@/components/ui/Switch'
import { HoldButton } from '@/components/confirm/HoldButton'
import { KV, Section } from './shared'
import { MailboxWritesControl } from './MailboxWrites'

function reveal(path: string) {
  if (isTauri) void import('@tauri-apps/plugin-opener').then((m) => m.revealItemInDir(path)).catch((e) => log('warn', 'ui', `reveal failed: ${String((e as Error)?.message ?? e)}`))
  else toast('Reveal in Finder', { body: `${path}\n(only available inside the desktop app)`, kind: 'info' })
}

export function OpsPane() {
  const info = useDevInfo()
  const settings = useSettings()
  const { run } = useAction()
  const devOverlay = useUi((s) => s.devOverlay)
  const setDevOverlay = useUi((s) => s.setDevOverlay)
  const d = info.data
  return (
    <div className="grid grid-cols-2 gap-6 p-5">
      <div>
        <Section title="Runtime">
          {d ? (
            <KV
              rows={[
                ['Version', `${d.build.version} (${d.build.gitSha}${d.build.dirty ? '-dirty' : ''})`],
                ['Built', d.build.builtAt],
                ['Profile', `${d.build.profile} · ${d.dev ? 'dev' : 'prod'}`],
                ['OS', `${d.os} ${d.arch}`],
                ['Uptime', fmtDuration(d.uptimeSeconds)],
                ['Log level', `${d.logLevel} (setting: ${settings.data?.logLevel ?? '…'})`],
                ['Store fault', d.storeFault ?? 'none'],
              ]}
            />
          ) : (
            <span className="text-xs text-clay">Loading…</span>
          )}
        </Section>
        <Section title="Storage">
          {d ? (
            <div className="flex flex-col gap-1 text-xs">
              {[['Data', d.dataDir], ['Config', d.configDir], ['Cache', d.cacheDir], ['Logs', d.logDir]].map(([k, v]) => (
                <div key={k} className="flex items-center gap-2">
                  <span className="w-12 text-clay">{k}</span>
                  <span className="min-w-0 flex-1 truncate mono selectable" title={v}>{v}</span>
                  <Button size="sm" variant="ghost" icon={<FolderOpen />} onClick={() => reveal(v)}>Reveal</Button>
                </div>
              ))}
              <table className="mt-2 w-full">
                <tbody>
                  {d.stores.map((s) => (
                    <tr key={s.label} className="border-t border-line/50">
                      <td className="py-0.5 pr-2">{s.label}</td>
                      <td className="py-0.5 pr-2 mono text-clay truncate max-w-[16rem]" title={s.path}>{s.path.split('/').pop()}</td>
                      <td className="py-0.5 text-right tabular-nums">{fmtBytes(s.sizeBytes)}</td>
                      <td className="py-0.5 text-right"><Button size="sm" variant="ghost" onClick={() => reveal(s.path)}>Reveal</Button></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : null}
        </Section>
        <Section title="Mailbox writes">
          <MailboxWritesControl />
        </Section>
        <Section title="Sift again">
          <p className="text-xs text-ink-2">Every message Riddle decided is sifted again with the current rules and cached Signal scores; your own decisions are kept, and the earlier decisions stay in the inspector as history. Signals with no cached score ask the judge.</p>
          <div className="mt-2">
            <HoldButton label="Sift everything again" coaching="Hold for 1.5 seconds. Runs across every account; with Gmail changes on and mailbox writes on Allow, labels are rewritten." onComplete={() => run('resift', {}, { confirmed: true, successToast: 'Sifting every account again' })} />
          </div>
        </Section>
        <Section title="Developer overlay">
          <label className="flex items-center gap-2 text-xs">
            <Switch checked={devOverlay} onChange={setDevOverlay} label="Show diagnostic chips on Finds and messages" /> Show [JEV .96] [MESH v7] [NO LLM] [209ms] chips on Finds and messages; clicking opens the inspector.
          </label>
        </Section>
        <Probes />
        <Diagnostics />
      </div>
      <div className="flex min-h-0 flex-col">
        <InFlight />
        <LogTail />
      </div>
    </div>
  )
}

function Probes() {
  const digs = useDigs()
  const [results, setResults] = useState<Record<string, ProbeResult | 'running'>>({})
  const probe = async (key: string, fn: () => Promise<ProbeResult>) => {
    setResults((r) => ({ ...r, [key]: 'running' }))
    try {
      setResults((r) => ({ ...r, [key]: 'running' }))
      const res = await fn()
      setResults((r) => ({ ...r, [key]: res }))
    } catch (e) {
      setResults((r) => ({ ...r, [key]: { ok: false, latencyMs: 0, message: String((e as Error).message ?? e) } }))
    }
  }
  const row = (key: string, label: string, fn: () => Promise<ProbeResult>) => {
    const r = results[key]
    return (
      <div key={key} className="flex items-center gap-2 text-xs">
        <Button size="sm" busy={r === 'running'} onClick={() => probe(key, fn)}>{label}</Button>
        {r && r !== 'running' ? <span className={r.ok ? 'text-moss' : 'text-rust'}>{r.ok ? 'OK' : 'FAIL'} {r.latencyMs}ms · {r.message}</span> : null}
      </div>
    )
  }
  return (
    <Section title="Probes">
      <p className="mb-2 text-xs text-ink-2">Run the judge, LLM and provider probes outside the pipeline.</p>
      <div className="flex flex-col gap-1.5">
        {row('judge', 'Probe judge', probeJudge)}
        {row('llm', 'Probe LLM', probeLlm)}
        {(digs.data ?? []).map((d) => row(`dig-${d.id}`, `Probe ${d.name}`, () => probeProvider(d.id)))}
      </div>
    </Section>
  )
}

function Diagnostics() {
  const [text, setText] = useState<string | null>(null)
  return (
    <Section title="Diagnostics">
      <div className="flex gap-2">
        <Button
          size="sm"
          icon={<Copy />}
          onClick={async () => {
            const t = await exportDiagnostics()
            setText(t)
            try {
              await navigator.clipboard.writeText(t)
              toast('Diagnostics copied', { body: 'Redacted: no subjects, bodies, addresses or tokens.', kind: 'success' })
            } catch {
              toast('Diagnostics ready', { body: 'Clipboard refused; the block is shown below.', kind: 'warn' })
            }
          }}
        >
          Copy diagnostics block
        </Button>
      </div>
      {text ? <pre className="mt-2 max-h-48 overflow-auto rounded border border-line bg-sand p-2 mono text-2xs selectable">{text}</pre> : null}
    </Section>
  )
}

function InFlight() {
  const q = useQuery({ queryKey: qk.inFlight, queryFn: inFlightCalls, refetchInterval: 1000 })
  return (
    <Section title="IPC in flight" right={<span className="text-xs text-clay">{q.data?.length ?? 0}</span>}>
      {q.data?.length ? (
        <ul className="text-xs">
          {q.data.map((c, i) => (
            <li key={i} className="flex justify-between mono">
              <span>{c.command}</span>
              <span className={cx(c.elapsedMs > 2000 && 'text-ochre')}>{c.elapsedMs}ms</span>
            </li>
          ))}
        </ul>
      ) : (
        <span className="text-xs text-clay">No commands in flight. A hang would show here with its elapsed time.</span>
      )}
    </Section>
  )
}

const LEVELS = ['error', 'warn', 'info', 'debug', 'trace']

function LogTail() {
  const cats = useQuery({ queryKey: qk.logCategories, queryFn: logCategories, staleTime: Infinity })
  const [category, setCategory] = useState('all')
  const [level, setLevel] = useState('debug')
  const [text, setText] = useState('')
  const [follow, setFollow] = useState(true)
  const [open, setOpen] = useState<number | null>(null)
  const q = useQuery({ queryKey: [...qk.logs, category, level, text], queryFn: () => readAppLogs({ limit: 400, category: category === 'all' ? null : category, level, text: text || null }), refetchInterval: follow ? 2000 : false })
  const lines = useMemo(() => q.data ?? [], [q.data])
  useEffect(() => {
    if (follow) document.getElementById('log-end')?.scrollIntoView({ block: 'end' })
  }, [lines, follow])
  return (
    <Section
      title="Logs"
      right={
        <div className="flex items-center gap-1">
          <Select label="Category" size="sm" value={category} onChange={setCategory} options={[{ value: 'all', label: 'all' }, ...(cats.data ?? []).map((c) => ({ value: c, label: c }))]} />
          <Select label="Level" size="sm" value={level} onChange={setLevel} options={LEVELS.map((l) => ({ value: l, label: `≥ ${l}` }))} />
          <Input value={text} onChange={(e) => setText(e.target.value)} placeholder="filter text" aria-label="Filter log text" className="h-6 w-32 text-xs" />
          <Button size="sm" variant="ghost" icon={follow ? <Pause /> : <Play />} onClick={() => setFollow((f) => !f)}>
            {follow ? 'Following' : 'Paused'}
          </Button>
        </div>
      }
    >
      <div className="max-h-[70vh] overflow-auto rounded border border-line bg-sand mono text-2xs leading-4">
        {lines.map((l, i) => (
          <div key={i}>
            <button type="button" className={cx('flex w-full gap-2 px-2 py-px text-left hover:bg-sand-2', l.level === 'error' && 'text-rust', l.level === 'warn' && 'text-ochre')} onClick={() => setOpen(open === i ? null : i)} aria-expanded={open === i}>
              <span className="text-clay">{fmtClockMs(l.at)}</span>
              <span className="w-10 uppercase">{l.level}</span>
              <span className="w-16 text-ink-2">{l.category}</span>
              <span className="min-w-0 flex-1 truncate selectable">{l.message}</span>
              {l.payload ? <span className="text-clay">{'{…}'}</span> : null}
            </button>
            {open === i && l.payload ? <pre className="border-y border-line bg-paper px-6 py-1 selectable">{safeJson(l.payload)}</pre> : null}
          </div>
        ))}
        {!lines.length ? <div className="p-3 text-clay">No log lines match. Logs carry identifiers only; never subjects, bodies, addresses or tokens.</div> : null}
        <div id="log-end" />
      </div>
      {!isTauri ? <Note className="mt-2">Browser mode: these are mock log lines. In the app they are the backend ring buffer, live.</Note> : null}
    </Section>
  )
}

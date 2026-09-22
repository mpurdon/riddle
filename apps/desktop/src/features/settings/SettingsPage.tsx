/**
 * Settings: Digs, Dig Groups, per-Dig privacy, projection and autonomy,
 * Never Hidden, User Profile, notifications, budgets, judge and LLM
 * providers with probes, the privacy statement, Update Preview, Developer
 * Mode and log level.
 */
import { useEffect, useState } from 'react'
import { useQuery } from '@tanstack/react-query'
import { Bell, Plug, Plus, ShieldCheck, Trash2 } from 'lucide-react'
import type { Autonomy } from '@/bindings/Autonomy'
import type { Dig } from '@/bindings/Dig'
import type { PrivacyMode } from '@/bindings/PrivacyMode'
import type { ProjectionMode } from '@/bindings/ProjectionMode'
import type { UserProfile } from '@/bindings/UserProfile'
import { privacyStatement, probeJudge, probeLlm, probeProvider, qk, updatePreview } from '@/lib/api'
import { fmtRelative, fmtUsd } from '@/lib/format'
import { isTauri } from '@/lib/ipc'
import { useDigGroups, useDigs, useProfile, useSettings } from '@/lib/queries'
import { digStatusChip } from '@/lib/selectors'
import type { ProbeResult, Settings } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { toast } from '@/lib/stores'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Dialog } from '@/components/ui/Dialog'
import { Field, Input } from '@/components/ui/Field'
import { Note } from '@/components/ui/Note'
import { Select } from '@/components/ui/Select'
import { Switch } from '@/components/ui/Switch'
import { HoldButton } from '@/components/confirm/HoldButton'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'
import { resetTour, Tour } from '@/components/Tour'
import { SimulationTable } from '@/features/meshes/SimulationTable'

const AUTONOMY: { value: Autonomy; label: string }[] = [
  { value: 'observe', label: 'Observe: never acts' },
  { value: 'suggest', label: 'Suggest: proposes, you apply' },
  { value: 'act', label: 'Act: after a Shadow Period' },
  { value: 'trusted', label: 'Trusted: acts without asking' },
]
const PRIVACY: { value: PrivacyMode; label: string }[] = [
  { value: 'local_only', label: 'Local only: nothing leaves the device' },
  { value: 'cloud_judgment', label: 'Cloud judgment: Jev Signals, no LLM' },
  { value: 'cloud_reasoning', label: 'Cloud reasoning: Jev and LLM escalation' },
]
const PROJECTION: { value: ProjectionMode; label: string }[] = [
  { value: 'none', label: 'None: Riddle only' },
  { value: 'labels_only', label: 'Labels only' },
  { value: 'full', label: 'Full: labels and archive state' },
]

function Section({ id, title, children, description }: { id: string; title: string; description?: string; children: React.ReactNode }) {
  return (
    <section id={id} className="scroll-mt-4 border-b border-line py-5">
      <h2 className="text-sm font-semibold">{title}</h2>
      {description ? <p className="mt-0.5 max-w-2xl text-xs text-ink-2">{description}</p> : null}
      <div className="mt-3">{children}</div>
    </section>
  )
}

export default function SettingsPage() {
  const settings = useSettings()
  const digs = useDigs()
  const groups = useDigGroups()
  const profile = useProfile()
  const { run } = useAction()
  const [tourForce, setTourForce] = useState(false)
  useEffect(() => {
    const id = window.location.hash.split('#')[2]
    if (id) document.getElementById(id)?.scrollIntoView()
  }, [settings.data])
  if (settings.error) return <ErrorCard error={settings.error} onRetry={() => settings.refetch()} />
  const s = settings.data
  if (!s) return <div className="p-6 text-sm text-clay">Loading settings…</div>
  const save = (patch: Partial<Settings>) => run('save_settings', { settings: { ...s, ...patch } }, { successToast: 'Settings saved' })
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Settings" subtitle="Everything here is saved through the Action layer and appears in Activity." />
      <div className="min-h-0 flex-1 overflow-auto px-6">
        <Section id="digs" title="Digs" description="A Dig is a connected account. Each has its own store, status and understanding.">
          <div className="flex flex-col gap-2">
            {(digs.data ?? []).map((d) => (
              <DigCard key={d.id} dig={d} settings={s} groups={groups.data?.map((g) => g.name) ?? []} onAutonomy={(a) => save({ autonomy: { ...s.autonomy, perDig: { ...s.autonomy.perDig, [d.id]: a } } })} />
            ))}
          </div>
          <div className="mt-3 flex flex-wrap gap-2">
            <AddFixtureDig />
            <ConnectGmail clientId={s.gmail.clientId} />
          </div>
        </Section>

        <Section id="groups" title="Dig Groups" description="A Dig belongs to at most one group, so there is never a question of which group's behaviour wins. Meshes and Strata can be scoped to a group.">
          <DigGroups digs={digs.data ?? []} groups={groups.data ?? []} />
        </Section>

        <Section id="autonomy" title="Autonomy" description="Per Dig and per Action, unlocked from Observe through Suggest to Act as the Shadow Period shows agreement. Trusted is opt-in per Dig and needs a held confirmation.">
          <div className="grid max-w-lg grid-cols-[10rem_1fr] items-center gap-2 text-sm">
            <span>Default</span>
            <Select label="Default autonomy" value={s.autonomy.default} onChange={(v) => save({ autonomy: { ...s.autonomy, default: v as Autonomy } })} options={AUTONOMY.filter((a) => a.value !== 'trusted')} />
            <span>Shadow Period</span>
            <Input type="number" value={s.shadowPeriodDays} onChange={(e) => save({ shadowPeriodDays: Number(e.target.value) || 7 })} className="w-24" aria-label="Shadow Period days" />
            <span>Send mail</span>
            <Select label="Autonomy for send_message" value={s.autonomy.perAction.send_message ?? 'suggest'} onChange={(v) => save({ autonomy: { ...s.autonomy, perAction: { ...s.autonomy.perAction, send_message: v as Autonomy } } })} options={AUTONOMY.filter((a) => a.value !== 'trusted')} />
          </div>
        </Section>

        <Section id="never-hidden" title="Never Hidden" description="Mail from these addresses and domains can never become Spoil or Contamination, whatever a Mesh or a Signal says. These rules run after every Mesh and win every time. You edit this list; Riddle cannot.">
          <NeverHidden entries={profile.data?.neverHidden ?? []} />
        </Section>

        <Section id="profile" title="User Profile" description="personally_relevant and directly_addressed are undefined without it.">
          {profile.data ? <ProfileForm p={profile.data} /> : null}
        </Section>

        <Section id="notifications" title="Notifications" description="Only Finds above the threshold interrupt. Everything else waits in the Task Centre and the summary.">
          <div className="flex flex-wrap items-center gap-3 text-sm">
            <label className="flex items-center gap-2">
              Priority threshold
              <Input type="number" min={0} max={100} value={s.notifyPriorityThreshold} onChange={(e) => save({ notifyPriorityThreshold: Number(e.target.value) })} className="w-20" aria-label="Notify priority threshold" />
            </label>
            <label className="flex items-center gap-2">
              Spoil digest
              <Select label="Spoil digest" value={s.spoilDigest ?? 'daily'} onChange={(v) => save({ spoilDigest: v as Settings['spoilDigest'] })} options={[{ value: 'daily', label: 'Daily' }, { value: 'weekly', label: 'Weekly' }, { value: 'off', label: 'Off' }]} />
            </label>
            <Button icon={<Bell />} onClick={() => toast('Test notification', { body: 'If you saw a system notification, the helper works. On macOS 26 the plugin API reports success and shows nothing; Riddle ships its own helper.', kind: 'info', ttlMs: 8000 })}>
              Test Notification
            </Button>
          </div>
        </Section>

        <Section id="budgets" title="Budgets" description="Daily caps per Dig for judge and LLM spend. Exhaustion pauses backfill and experiments first and shows in the Task Centre.">
          <div className="grid max-w-lg grid-cols-3 gap-3">
            {(['dailyJudgeUsd', 'dailyLlmUsd', 'escalationPerDigUsd'] as const).map((k) => (
              <Field key={k} label={k === 'dailyJudgeUsd' ? 'Judge / day' : k === 'dailyLlmUsd' ? 'LLM / day' : 'Escalation / Dig'}>
                <Input type="number" step="0.5" min={0} value={s.budgets[k]} onChange={(e) => save({ budgets: { ...s.budgets, [k]: Number(e.target.value) } })} />
              </Field>
            ))}
          </div>
        </Section>

        <Section id="providers" title="Judge and LLM" description="Credentials live in the keychain or SSM; the app only ever sees whether a key is present.">
          <div className="grid grid-cols-2 gap-3">
            <ProviderCard
              title="Judge"
              rows={[
                ['Provider', s.judge.provider],
                ['Model', s.judge.model],
                ['Key source', `${s.judge.keySource}${s.judge.ssmProfile ? ` (${s.judge.ssmProfile}, ${s.judge.ssmParameter})` : ''}`],
                ['API key', s.judge.apiKeyPresent ? 'present' : 'absent'],
              ]}
              keyPresent={s.judge.apiKeyPresent}
              probe={probeJudge}
            />
            <ProviderCard title="LLM" rows={[['Provider', s.llm.provider], ['Profile', s.llm.profile], ['Region', s.llm.region], ['Escalation model', s.llm.escalationModel], ['Director model', s.llm.directorModel]]} keyPresent probe={probeLlm} />
          </div>
        </Section>

        <Section id="privacy" title="Privacy" description="What leaves the device, to whom, and why, in one place.">
          <PrivacyText />
        </Section>

        <Section id="update" title="Update" description="Nothing changes silently on update. When a release changes a default Mesh or Signal, Riddle previews the effect on your mail and asks first.">
          <UpdateSection />
        </Section>

        <Section id="developer" title="Developer Mode" description="Shows more; grants no additional authority. Exports and logs stay redacted.">
          <div className="flex flex-wrap items-center gap-6 text-sm">
            <label className="flex items-center gap-2">
              <Switch checked={s.developerMode} onChange={(v) => save({ developerMode: v })} label="Developer Mode" /> Developer Mode
            </label>
            <label className="flex items-center gap-2">
              Log level
              <Select label="Log level" value={s.logLevel} onChange={(v) => save({ logLevel: v as Settings['logLevel'] })} options={['error', 'warn', 'info', 'debug', 'trace'].map((l) => ({ value: l, label: l }))} />
              <span className="text-xs text-clay">applies on next launch</span>
            </label>
            <label className="flex items-center gap-2">
              <Switch checked={s.launchAtLogin} onChange={(v) => save({ launchAtLogin: v })} label="Launch at login" /> Launch at login
            </label>
            <label className="flex items-center gap-2">
              <Switch checked={s.closeToHide} onChange={(v) => save({ closeToHide: v })} label="Close hides the window" /> Close hides the window
            </label>
            <Button size="sm" variant="ghost" onClick={() => { resetTour(); setTourForce(true) }}>
              Show the tour again
            </Button>
          </div>
        </Section>
        <div className="h-8" />
      </div>
      {tourForce ? <Tour force onDone={() => setTourForce(false)} /> : null}
    </div>
  )
}

/**
 * Connecting a mailbox happens in the browser, so the Action returns the
 * consent URL and finishes on its own; the notification when Google comes
 * back arrives as an event. docs/GMAIL_SETUP.md has the console steps that
 * produce the client id.
 */
function ConnectGmail({ clientId }: { clientId: string }) {
  const { run } = useAction()
  const [waiting, setWaiting] = useState(false)
  if (!clientId.trim()) {
    return (
      <Button
        icon={<Plug />}
        disabledReason="No Gmail OAuth client id yet. docs/GMAIL_SETUP.md walks through the Google Cloud project and consent screen; paste the id into Providers below. Until then every Dig is a Fixture Dig."
      >
        Connect Gmail
      </Button>
    )
  }
  return (
    <Button
      icon={<Plug />}
      disabled={waiting}
      onClick={async () => {
        setWaiting(true)
        try {
          const out = await run<{ authUrl: string }>('connect_gmail', {}, { successToast: 'Finish the sign-in in your browser' })
          const url = out?.result?.authUrl
          if (url) {
            if (isTauri) await import('@tauri-apps/plugin-opener').then((m) => m.openUrl(url))
            else window.open(url, '_blank', 'noopener')
          }
        } finally {
          setWaiting(false)
        }
      }}
    >
      {waiting ? 'Waiting for Google…' : 'Connect Gmail'}
    </Button>
  )
}

function DigCard({ dig, settings, groups, onAutonomy }: { dig: Dig; settings: Settings; groups: string[]; onAutonomy: (a: Autonomy) => void }) {
  const { run } = useAction()
  const chip = digStatusChip(dig.status)
  const autonomy = settings.autonomy.perDig[dig.id] ?? settings.autonomy.default
  const update = (patch: Partial<Dig>) => run('update_dig', { dig: { ...dig, ...patch } }, { successToast: `${dig.name} updated` })
  const [probe, setProbe] = useState<ProbeResult | null>(null)
  return (
    <div className="rounded-md border border-line p-3">
      <div className="flex items-center gap-2">
        <span className="font-medium">{dig.name}</span>
        <span className="text-xs text-clay">{dig.email}</span>
        <Chip tone={chip.tone} title={chip.title}>{chip.label}</Chip>
        <Chip>{dig.provider}</Chip>
        <span className="flex-1" />
        <Button size="sm" variant="ghost" onClick={async () => setProbe(await probeProvider(dig.id))}>
          Probe
        </Button>
        <Button size="sm" variant="danger" icon={<Trash2 />} onClick={() => run('remove_dig', { digId: dig.id })}>
          Remove…
        </Button>
      </div>
      {probe ? <div className={`mt-1 text-xs ${probe.ok ? 'text-moss' : 'text-rust'}`}>{probe.message} ({probe.latencyMs}ms)</div> : null}
      <div className="mt-2 grid grid-cols-4 gap-2 text-xs">
        <Field label="Group">
          <Select label={`${dig.name} group`} size="sm" value={dig.group ?? 'none'} onChange={(v) => update({ group: v === 'none' ? null : v })} options={[{ value: 'none', label: 'No group' }, ...groups.map((g) => ({ value: g, label: g }))]} />
        </Field>
        <Field label="Privacy">
          <Select label={`${dig.name} privacy`} size="sm" value={dig.privacy} onChange={(v) => update({ privacy: v as PrivacyMode })} options={PRIVACY} />
        </Field>
        <Field label="Projection">
          <Select label={`${dig.name} projection`} size="sm" value={dig.projection} onChange={(v) => update({ projection: v as ProjectionMode })} options={PROJECTION} />
        </Field>
        <Field label="Autonomy">
          <Select label={`${dig.name} autonomy`} size="sm" value={autonomy} onChange={(v) => onAutonomy(v as Autonomy)} options={AUTONOMY.filter((a) => a.value !== 'trusted')} />
        </Field>
      </div>
      {autonomy !== 'trusted' ? (
        <div className="mt-2 flex items-center gap-3">
          <HoldButton
            label={`Raise ${dig.name} to Trusted`}
            coaching="Trusted acts without asking, including on external side effects. Hold for 1.5 seconds to confirm."
            onComplete={() => run('save_settings', { settings: { ...settings, autonomy: { ...settings.autonomy, perDig: { ...settings.autonomy.perDig, [dig.id]: 'trusted' } } } }, { confirmed: true, successToast: `${dig.name} is Trusted` })}
          />
        </div>
      ) : (
        <div className="mt-2 flex items-center gap-2 text-xs">
          <Chip tone="warn">Trusted</Chip>
          <Button size="sm" variant="ghost" onClick={() => onAutonomy('act')}>
            Lower to Act
          </Button>
        </div>
      )}
      {dig.shadowStartedAt ? <div className="mt-1 text-[11px] text-clay">Shadow Period started {fmtRelative(dig.shadowStartedAt)}.</div> : null}
    </div>
  )
}

function AddFixtureDig() {
  const { run } = useAction()
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [count, setCount] = useState('120')
  return (
    <>
      <Button icon={<Plus />} onClick={() => setOpen(true)}>
        Add Fixture Dig
      </Button>
      <Dialog open={open} onOpenChange={setOpen} title="Add a Fixture Dig" description="A synthetic mailbox for development, tests and Demo Mode. It syncs like a real Dig, with no network.">
        <div className="flex flex-col gap-3">
          <Field label="Name"><Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Side project" /></Field>
          <Field label="Address"><Input value={email} onChange={(e) => setEmail(e.target.value)} placeholder="me@side.example" /></Field>
          <Field label="Messages to generate"><Input value={count} onChange={(e) => setCount(e.target.value)} inputMode="numeric" /></Field>
        </div>
        <div className="mt-4 flex justify-end gap-2">
          <Button variant="ghost" onClick={() => setOpen(false)}>Cancel</Button>
          <Button variant="primary" disabledReason={!name.trim() ? 'Name it' : !/@/.test(email) ? 'Give it an address' : null} onClick={async () => { const out = await run('add_fixture_dig', { name: name.trim(), email: email.trim(), count: Number(count) || 120 }); if (out) setOpen(false) }}>
            Add
          </Button>
        </div>
      </Dialog>
    </>
  )
}

function DigGroups({ digs, groups }: { digs: Dig[]; groups: { name: string; digs: string[] }[] }) {
  const { run } = useAction()
  const [name, setName] = useState('')
  return (
    <div className="flex flex-col gap-2 text-sm">
      {groups.map((g) => (
        <div key={g.name} className="flex flex-wrap items-center gap-2 rounded-md border border-line px-3 py-2">
          <span className="font-medium">{g.name}</span>
          <span className="text-xs text-clay">{g.digs.map((id) => digs.find((d) => d.id === id)?.name ?? id).join(', ') || 'no Digs'}</span>
          <span className="flex-1" />
          {digs.filter((d) => !g.digs.includes(d.id)).map((d) => (
            <Button key={d.id} size="sm" variant="ghost" onClick={() => run('save_dig_group', { group: { name: g.name, digs: [...g.digs, d.id] } })}>
              + {d.name}
            </Button>
          ))}
        </div>
      ))}
      <div className="flex items-center gap-2">
        <Input value={name} onChange={(e) => setName(e.target.value)} placeholder="New group" className="w-48" aria-label="New group name" />
        <Button size="sm" disabledReason={name.trim() ? null : 'Name the group'} onClick={async () => { await run('save_dig_group', { group: { name: name.trim(), digs: [] } }); setName('') }}>
          Create group
        </Button>
      </div>
    </div>
  )
}

function NeverHidden({ entries }: { entries: string[] }) {
  const { run } = useAction()
  const [entry, setEntry] = useState('')
  const valid = /^(@[^\s@]+\.[^\s@]+|[^\s@]+@[^\s@]+\.[^\s@]+)$/.test(entry.trim())
  return (
    <div>
      <ul className="flex flex-wrap gap-1.5">
        {entries.map((e) => (
          <li key={e} className="flex items-center gap-1 rounded-full border border-line bg-sand px-2 py-0.5 text-xs">
            <ShieldCheck className="size-3 text-moss" />
            <span className="mono">{e}</span>
            <button type="button" aria-label={`Remove ${e}`} className="ml-1 text-clay hover:text-rust" onClick={() => run('remove_never_hidden', { entry: e })}>
              ×
            </button>
          </li>
        ))}
      </ul>
      <div className="mt-2 flex items-center gap-2">
        <Input value={entry} onChange={(e) => setEntry(e.target.value)} placeholder="name@example.com or @example.com" className="w-72" aria-label="Never Hidden entry" onKeyDown={(e) => e.key === 'Enter' && valid && (run('add_never_hidden', { entry: entry.trim() }), setEntry(''))} />
        <Button size="sm" disabledReason={valid ? null : 'An address, or a domain starting with @'} onClick={async () => { await run('add_never_hidden', { entry: entry.trim() }); setEntry('') }}>
          Add
        </Button>
      </div>
      <p className="mt-2 text-xs text-clay">Also never hidden, by rule: people you have written to, your contacts, your organisation, VIPs, security and account alerts, financial and legal mail, calendar invitations, and mail Riddle is unsure about.</p>
    </div>
  )
}

function ProfileForm({ p }: { p: UserProfile }) {
  const { run } = useAction()
  const [form, setForm] = useState(p)
  useEffect(() => setForm(p), [p])
  const list = (k: keyof UserProfile) => (form[k] as string[]).join(', ')
  const setList = (k: keyof UserProfile, v: string) => setForm({ ...form, [k]: v.split(',').map((x) => x.trim()).filter(Boolean) })
  return (
    <div className="grid max-w-3xl grid-cols-2 gap-3 text-sm">
      <Field label="Name"><Input value={form.name ?? ''} onChange={(e) => setForm({ ...form, name: e.target.value })} /></Field>
      <Field label="Role"><Input value={form.role ?? ''} onChange={(e) => setForm({ ...form, role: e.target.value })} /></Field>
      <Field label="Addresses" hint="Your addresses across Digs."><Input value={list('addresses')} onChange={(e) => setList('addresses', e.target.value)} /></Field>
      <Field label="Aliases"><Input value={list('aliases')} onChange={(e) => setList('aliases', e.target.value)} /></Field>
      <Field label="Group addresses" hint="Lists you receive through; mail to them is not directly addressed."><Input value={list('groupAddresses')} onChange={(e) => setList('groupAddresses', e.target.value)} /></Field>
      <Field label="Organisation domains"><Input value={list('organisationDomains')} onChange={(e) => setList('organisationDomains', e.target.value)} /></Field>
      <Field label="Active projects"><Input value={list('activeProjects')} onChange={(e) => setList('activeProjects', e.target.value)} /></Field>
      <Field label="VIPs"><Input value={list('vips')} onChange={(e) => setList('vips', e.target.value)} /></Field>
      <Field label="Family"><Input value={list('family')} onChange={(e) => setList('family', e.target.value)} /></Field>
      <Field label="Timezone"><Input value={form.timezone ?? ''} onChange={(e) => setForm({ ...form, timezone: e.target.value })} /></Field>
      <div className="col-span-2">
        <Button variant="primary" size="sm" onClick={() => run('save_user_profile', { profile: form })} disabledReason={JSON.stringify(form) === JSON.stringify(p) ? 'No changes' : null}>
          Save profile
        </Button>
      </div>
    </div>
  )
}

function ProviderCard({ title, rows, keyPresent, probe }: { title: string; rows: [string, string][]; keyPresent: boolean; probe: () => Promise<ProbeResult> }) {
  const [result, setResult] = useState<ProbeResult | null>(null)
  const [busy, setBusy] = useState(false)
  return (
    <div className="rounded-md border border-line p-3 text-sm">
      <div className="flex items-center justify-between">
        <span className="font-medium">{title}</span>
        <Chip tone={keyPresent ? 'ok' : 'bad'}>{keyPresent ? 'API key: present' : 'API key: absent'}</Chip>
      </div>
      <dl className="mt-2 grid grid-cols-[8rem_1fr] gap-y-0.5 text-xs text-ink-2">
        {rows.map(([k, v]) => (
          <div key={k} className="contents">
            <dt className="text-clay">{k}</dt>
            <dd className="mono">{v}</dd>
          </div>
        ))}
      </dl>
      <div className="mt-2 flex items-center gap-2">
        <Button size="sm" busy={busy} onClick={async () => { setBusy(true); try { setResult(await probe()) } catch (e) { setResult({ ok: false, latencyMs: 0, message: String((e as Error).message ?? e) }) } finally { setBusy(false) } }} disabledReason={keyPresent ? null : 'No API key configured'}>
          Probe
        </Button>
        {result ? <span className={`text-xs ${result.ok ? 'text-moss' : 'text-rust'}`}>{result.ok ? 'OK' : 'Failed'} · {result.latencyMs}ms · {result.message}</span> : null}
      </div>
    </div>
  )
}

function PrivacyText() {
  const digs = useDigs()
  const [digId, setDigId] = useState<string>('all')
  const q = useQuery({ queryKey: qk.privacy(digId === 'all' ? null : digId), queryFn: () => privacyStatement(digId === 'all' ? null : digId) })
  return (
    <div>
      <Select label="Privacy statement for" size="sm" value={digId} onChange={setDigId} options={[{ value: 'all', label: 'Defaults' }, ...(digs.data ?? []).map((d) => ({ value: d.id, label: d.name }))]} />
      <pre className="mt-2 max-w-3xl whitespace-pre-wrap rounded border border-line bg-sand p-3 font-sans text-xs leading-5 selectable">{q.data ?? 'Loading…'}</pre>
    </div>
  )
}

function UpdateSection() {
  const q = useQuery({ queryKey: qk.updatePreview, queryFn: updatePreview })
  const { run } = useAction()
  if (q.isLoading) return <div className="text-xs text-clay">Checking…</div>
  if (!q.data) return <Note>You are on the latest version. Riddle checks silently shortly after launch and every few hours.</Note>
  const u = q.data
  return (
    <div className="max-w-xl rounded-md border border-ochre/40 bg-ochre/5 p-3 text-sm">
      <div className="font-medium">Riddle {u.toVersion} changes {u.changedSignals} system Signal{u.changedSignals === 1 ? '' : 's'} and {u.changedMeshes} default Mesh{u.changedMeshes === 1 ? '' : 'es'}.</div>
      <div className="mt-2">
        <SimulationTable r={u.report} compact />
      </div>
      <div className="mt-3 flex gap-2">
        <Button variant="primary" size="sm" onClick={() => run('apply_update_preview', { keep: false })}>
          Apply…
        </Button>
        <Button size="sm" onClick={() => run('apply_update_preview', { keep: true })}>
          Keep previous behaviour
        </Button>
      </div>
      <p className="mt-2 text-xs text-clay">Nothing is reclassified until you choose. Estimated cost of the change: {fmtUsd(u.report.estimatedCostUsd)}.</p>
    </div>
  )
}

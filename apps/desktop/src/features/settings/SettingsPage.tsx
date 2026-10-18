/**
 * Settings, in four tabs. General: how much Riddle shows (the level,
 * `lib/level`: Essentials, Everything or Developer, one switch that also
 * sets the Developer flag), how the app itself behaves (launch at login,
 * close to hide, notifications as Brief or Stream, updates), and at
 * Essentials a What changed card, since the rail does not list Activity
 * there. Accounts: a list-and-detail view of every account with
 * an Add account wizard, the Google client and the defaults (`accounts/`).
 * Providers: the judge and the LLM, their keys, budgets and the privacy
 * statement, which together say what leaves the device and what it costs.
 * Mail rules: Never Hidden, Your corrections (`rules/`) and the User
 * Profile. Deep links (`/settings#digs`) pick the tab that holds the
 * section.
 *
 * Every control here changes the running app when it is saved. An audit
 * found a Spoil digest, a Shadow Period and a "Providers" field that saved
 * nothing or changed nothing; controls without a working backend are left
 * out rather than shown. The priority threshold is explained with the open
 * Finds either side of it, because a bare "82" meant nothing to anyone.
 */
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { History, ShieldCheck } from 'lucide-react'
import type { UserProfile } from '@/bindings/UserProfile'
import { privacyStatement, probeJudge, probeLlm, qk, updatePreview } from '@/lib/api'
import { fmtUsd, plural } from '@/lib/format'
import { levelShows, type UiLevel } from '@/lib/level'
import { useAllDigs, useDigs, useProfile, useSettings } from '@/lib/queries'
import { judgeStateLine } from '@/lib/selectors'
import type { JudgeSettings, LlmSettings, Settings } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { toast } from '@/lib/stores'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Field, Input } from '@/components/ui/Field'
import { Note } from '@/components/ui/Note'
import { Select } from '@/components/ui/Select'
import { Switch } from '@/components/ui/Switch'
import { TabPanel, Tabs } from '@/components/ui/Tabs'
import { ErrorCard } from '@/components/ErrorCard'
import { PageHeader } from '@/components/Bits'
import { resetTour } from '@/components/Tour'
import { SimulationTable } from '@/features/meshes/SimulationTable'
import { resetLevelTip } from '@/lib/level'
import { AccountsTab } from './accounts/AccountsTab'
import type { Option } from './accounts/model'
import { OptionCards } from './accounts/pieces'
import { NotificationsSection } from './Notifications'
import { PriorityCriteriaEditor } from '@/features/priority/PriorityCriteria'
import { YourCorrections } from './rules/YourCorrections'
import { NumberSetting, Probe, SecretField, Section, useDraft } from './shared'

const SECTION_TAB: Record<string, string> = {
  digs: 'accounts',
  'add-account': 'accounts',
  'new-digs': 'accounts',
  groups: 'accounts',
  autonomy: 'accounts',
  providers: 'providers',
  budgets: 'providers',
  privacy: 'providers',
  'never-hidden': 'rules',
  corrections: 'rules',
  profile: 'rules',
  rules: 'rules',
}
const anchor = () => window.location.hash.split('#')[2] ?? ''
const tabFor = (id: string) => SECTION_TAB[id] ?? 'general'

/** The three levels as option cards; the level is the one switch and the Developer flag follows it. */
export const LEVEL_OPTIONS: Option<UiLevel>[] = [
  { value: 'essentials', label: 'Essentials', detail: 'Today, what needs you, your mail, what was set aside, and settings. The Director speaks in plain words.' },
  { value: 'everything', label: 'Everything', detail: 'Adds rules, packs, rule check, topics, labels, expeditions and Activity to the rail.' },
  { value: 'developer', label: 'Developer', detail: 'Adds Signals, inspectors, traces and the dev build’s arming switch. Shows more; grants nothing.' },
]


export default function SettingsPage() {
  const settings = useSettings()
  const digs = useAllDigs()
  const profile = useProfile()
  const { run } = useAction()
  const nav = useNavigate()
  const [tab, setTab] = useState(() => tabFor(anchor()))
  // Follow the deep link once the page has content and whenever the link
  // changes; not on every refetch, which would pull you off the tab you chose.
  const loaded = !!settings.data
  useEffect(() => {
    if (!loaded) return
    const sync = () => {
      const id = anchor()
      if (!id) return
      setTab(tabFor(id))
      requestAnimationFrame(() => document.getElementById(id)?.scrollIntoView())
    }
    sync()
    window.addEventListener('hashchange', sync)
    return () => window.removeEventListener('hashchange', sync)
  }, [loaded])
  if (settings.error) return <ErrorCard error={settings.error} onRetry={() => settings.refetch()} />
  const s = settings.data
  if (!s) return <div className="p-6 text-sm text-clay">Loading settings…</div>
  const save = (patch: Partial<Settings>) => run('save_settings', { settings: { ...s, ...patch } }, { successToast: 'Settings saved' })
  // Essentials trims the page to accounts, privacy, Never Hidden and the
  // level itself; the rest is one card away. The tabs are named for what is
  // left in them so "Providers" does not promise a judge nobody can see.
  const full = levelShows(s.uiLevel, 'everything')
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <PageHeader title="Settings" subtitle="Every change here is recorded in Activity and can be undone from there." />
      <Tabs value={tab} onChange={setTab} className="min-h-0 flex-1 px-5" tabs={[{ value: 'general', label: 'General' }, { value: 'accounts', label: `Accounts (${digs.data?.length ?? 0})` }, { value: 'providers', label: full ? 'Providers' : 'Privacy' }, { value: 'rules', label: full ? 'Mail rules' : 'Never Hidden' }]}>
        <TabPanel value="general" className="px-1">
          <Section id="level" title="How much Riddle shows" description="Every page stays reachable at every level: a link still opens it, Undo and Never Hidden work everywhere, and nothing changes in your mail. Developer shows more and grants no additional authority; exports and logs stay redacted.">
            <div className="max-w-3xl">
              <OptionCards label="How much Riddle shows" options={LEVEL_OPTIONS} value={s.uiLevel} onChange={(uiLevel) => save({ uiLevel, developerMode: uiLevel === 'developer' })} />
            </div>
            {full ? (
              <label className="mt-3 flex items-center gap-2 text-sm">
                Log level
                <Select label="Log level" value={s.logLevel} onChange={(v) => save({ logLevel: v as Settings['logLevel'] })} options={['error', 'warn', 'info', 'debug', 'trace'].map((l) => ({ value: l, label: l }))} />
              </label>
            ) : null}
          </Section>
          {full ? null : (
            <Section id="what-changed" title="What changed" description="Everything Riddle or you changed, by whom and why, with Undo beside anything that can be undone. Settings changes land there too.">
              <Button size="sm" icon={<History />} onClick={() => nav('/activity')}>
                Open What changed
              </Button>
            </Section>
          )}
          <Section id="app" title="App" description="How Riddle starts and what closing the window does.">
            <div className="flex flex-wrap items-center gap-6 text-sm">
              <label className="flex items-center gap-2">
                <Switch checked={s.launchAtLogin} onChange={(v) => save({ launchAtLogin: v })} label="Launch at login" /> Launch at login
              </label>
              <label className="flex items-center gap-2">
                <Switch checked={s.closeToHide} onChange={(v) => save({ closeToHide: v })} label="Close hides the window" /> Close hides the window
              </label>
              <Button
                size="sm"
                variant="ghost"
                onClick={() => {
                  resetTour()
                  resetLevelTip()
                  toast('Tips will show again', { body: 'Once each, the next time you open Finds, Unresolved and the Director.', kind: 'info' })
                }}
              >
                Show the tips again
              </Button>
            </div>
          </Section>

          <Section id="notifications" title="Notifications" description="How Riddle interrupts you. A brief at set times with a short allow-list that gets through at once, or a stream of every important Find as it is raised. Everything else waits in Finds and the Summary.">
            <NotificationsSection settings={s} save={save} />
          </Section>

          {full ? (
            <Section id="priority" title="Priority inbox" description="What the Priority view in the rail shows first: any message carrying one of these labels or Strata, or belonging to an open Find at or above the chosen priority. The same controls are on the page itself.">
              <PriorityCriteriaEditor chosen={s.priorityInbox} save={(patch) => save({ priorityInbox: { ...s.priorityInbox, ...patch } })} />
            </Section>
          ) : null}

          <Section id="update" title="Update" description="Nothing changes silently on update. When a release changes a default rule, Riddle previews the effect on your mail and asks first.">
            <UpdateSection />
          </Section>
          <div className="h-8" />
        </TabPanel>
        <TabPanel value="accounts" className="flex flex-col overflow-hidden px-1">
          <AccountsTab settings={s} focus={tab === 'accounts' ? anchor() : undefined} />
        </TabPanel>
        <TabPanel value="providers" className="px-1">
          {full ? (
            <>
              <Section id="providers" title="Judge and LLM" description="Keys live in the keychain, SSM or the environment; the app only ever sees whether one is present. A saved change takes effect at once; Probe tests what is running.">
                <div className="grid grid-cols-2 gap-3">
                  <JudgeCard judge={s.judge} onSave={(judge) => save({ judge })} />
                  <LlmCard llm={s.llm} onSave={(llm) => save({ llm })} />
                </div>
              </Section>

              <Section id="budgets" title="Budgets" description="The most Riddle may spend in a day, per account, in US dollars. Judge and LLM cap everything sent to that provider. Second opinions are the LLM's share of that: when the judge is unsure about a message, Riddle asks the LLM to look again. Once that cap is reached, unsure mail stays Unresolved until tomorrow; nothing is hidden and nothing is guessed.">
                <div className="grid max-w-lg grid-cols-3 gap-3">
                  {(['dailyJudgeUsd', 'dailyLlmUsd', 'escalationPerDigUsd'] as const).map((k) => (
                    <Field key={k} label={k === 'dailyJudgeUsd' ? 'Judge / day (USD)' : k === 'dailyLlmUsd' ? 'LLM / day (USD)' : 'Second opinions / day (USD)'}>
                      <NumberSetting value={s.budgets[k]} min={0} step={0.5} onCommit={(v) => save({ budgets: { ...s.budgets, [k]: v } })} label={k} />
                    </Field>
                  ))}
                </div>
              </Section>
            </>
          ) : null}

          <Section id="privacy" title="Privacy" description={full ? 'What leaves the device, to whom, and why, in one place.' : 'What leaves the device, to whom, and why, in one place. The judge, the model and their budgets are under Everything.'}>
            <PrivacyText />
          </Section>
          <div className="h-8" />
        </TabPanel>
        <TabPanel value="rules" className="px-1">
          <Section id="never-hidden" title="Never Hidden" description={full ? 'Mail from these addresses and domains can never become Spoil or Contamination, whatever a Mesh or a Signal says. These rules run after every Mesh and win every time. You edit this list. The Director can propose an addition; nothing is added without your approval.' : 'Mail from these addresses and domains can never be set aside, whatever any rule says. You edit this list. The Director can propose an addition; nothing is added without your approval.'}>
            <NeverHidden entries={profile.data?.neverHidden ?? []} />
          </Section>

          {full ? (
            <>
              <Section id="corrections" title="Your corrections" description="Sender and domain rules you made with one click on the Spoil page: never Spoil, or never Contamination, for that sender or that domain. Like Never Hidden they run after every Mesh and veto what they name.">
                <YourCorrections />
              </Section>

              <Section id="profile" title="User Profile" description="Who you are: your addresses, the people who matter, the projects you are on. Without it, Riddle cannot tell what is personally relevant or addressed to you.">
                {profile.data ? <ProfileForm p={profile.data} /> : null}
              </Section>
            </>
          ) : null}
          <div className="h-8" />
        </TabPanel>
      </Tabs>
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
      <Field label="Addresses" hint="Your addresses across accounts."><Input value={list('addresses')} onChange={(e) => setList('addresses', e.target.value)} /></Field>
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





const JUDGE_PROVIDERS = [
  { value: 'jev', label: 'Jev (TypeSafe)' },
  { value: 'null', label: 'None: every uncertain message is Unresolved' },
  { value: 'scripted', label: 'Scripted (development)' },
]
const KEY_SOURCES = [
  { value: 'keychain', label: 'Keychain' },
  { value: 'ssm', label: 'AWS SSM Parameter Store' },
  { value: 'env', label: 'Environment (TYPESAFE_API_KEY)' },
]

function JudgeCard({ judge, onSave }: { judge: JudgeSettings; onSave: (j: JudgeSettings) => void }) {
  const [d, set, dirty] = useDraft(judge)
  const jev = d.provider === 'jev'
  const missing = jev && !d.model.trim() ? 'Name the model' : jev && d.keySource === 'ssm' && !(d.ssmProfile ?? '').trim() ? 'Name the AWS profile' : jev && d.keySource === 'ssm' && !(d.ssmParameter ?? '').trim() ? 'Name the SSM parameter' : null
  return (
    <div className="flex flex-col gap-2 rounded-md border border-line p-3 text-sm">
      <div className="flex items-center justify-between">
        <span className="font-medium">Judge</span>
        {jev ? <Chip tone={judge.apiKeyPresent ? 'ok' : 'bad'}>{judge.apiKeyPresent ? 'API key: present' : 'API key: absent'}</Chip> : null}
      </div>
      <Field label="Provider">
        <Select label="Judge provider" size="sm" value={d.provider} onChange={(v) => set({ ...d, provider: v as JudgeSettings['provider'] })} options={JUDGE_PROVIDERS} />
      </Field>
      {jev ? (
        <>
          <Field label="Model" hint="An exact model id, pinned to a version. Probe checks it.">
            <Input className="mono" value={d.model} onChange={(e) => set({ ...d, model: e.target.value })} aria-label="Judge model" />
          </Field>
          <Field label="Key source">
            <Select label="Judge key source" size="sm" value={d.keySource} onChange={(v) => set({ ...d, keySource: v as JudgeSettings['keySource'] })} options={KEY_SOURCES} />
          </Field>
          {d.keySource === 'ssm' ? (
            <div className="grid grid-cols-2 gap-2">
              <Field label="AWS profile"><Input value={d.ssmProfile ?? ''} onChange={(e) => set({ ...d, ssmProfile: e.target.value })} aria-label="SSM profile" /></Field>
              <Field label="Parameter"><Input className="mono" value={d.ssmParameter ?? ''} onChange={(e) => set({ ...d, ssmParameter: e.target.value })} aria-label="SSM parameter" /></Field>
            </div>
          ) : null}
        </>
      ) : null}
      <div className="flex items-center gap-2">
        <Button size="sm" variant="primary" disabledReason={!dirty ? 'No changes' : missing} onClick={() => onSave(d)}>
          Save judge
        </Button>
        <Probe probe={probeJudge} disabledReason={dirty ? 'Save first; Probe tests the running judge' : d.provider === 'null' ? 'No judge configured' : null} />
      </div>
      {judgeStateLine(judge) ? (
        <p className="text-xs text-ochre" role="status">
          {judgeStateLine(judge)}.
        </p>
      ) : null}
      {jev && judge.keySource === 'keychain' && d.keySource === 'keychain' ? (
        <SecretField secret="jev_api_key" label={judge.apiKeyPresent ? 'Jev API key' : 'Paste a judge key (optional)'} hint="Written to the OS keychain, never to a file or a log; the judge picks it up at once." present={judge.apiKeyPresent} />
      ) : jev && d.keySource === 'keychain' ? (
        <p className="text-xs text-clay">Save the judge to paste a key into the keychain.</p>
      ) : null}
    </div>
  )
}

const LLM_PROVIDERS = [
  { value: 'bedrock', label: 'AWS Bedrock' },
  { value: 'null', label: 'None: no second opinions; the Director is unavailable' },
  { value: 'scripted', label: 'Scripted (development)' },
]

function LlmCard({ llm, onSave }: { llm: LlmSettings; onSave: (l: LlmSettings) => void }) {
  const [d, set, dirty] = useDraft(llm)
  const bedrock = d.provider === 'bedrock'
  const missing = bedrock && !d.region.trim() ? 'Name the region' : bedrock && (!d.escalationModel.trim() || !d.directorModel.trim()) ? 'Name both models' : null
  return (
    <div className="flex flex-col gap-2 rounded-md border border-line p-3 text-sm">
      <div className="flex items-center justify-between">
        <span className="font-medium">LLM</span>
        {bedrock ? <Chip>AWS credentials from the profile</Chip> : null}
      </div>
      <Field label="Provider">
        <Select label="LLM provider" size="sm" value={d.provider} onChange={(v) => set({ ...d, provider: v as LlmSettings['provider'] })} options={LLM_PROVIDERS} />
      </Field>
      {bedrock ? (
        <>
          <div className="grid grid-cols-2 gap-2">
            <Field label="AWS profile" hint="Empty uses the default chain."><Input value={d.profile} onChange={(e) => set({ ...d, profile: e.target.value })} aria-label="Bedrock profile" /></Field>
            <Field label="Region"><Input value={d.region} onChange={(e) => set({ ...d, region: e.target.value })} aria-label="Bedrock region" /></Field>
          </div>
          <Field label="Second-opinion model" hint="Looks again at mail the judge is unsure about. An exact model id; Probe checks it.">
            <Input className="mono" value={d.escalationModel} onChange={(e) => set({ ...d, escalationModel: e.target.value })} aria-label="Escalation model" />
          </Field>
          <Field label="Director model" hint="The Director and Expeditions. An exact model id; Probe checks it.">
            <Input className="mono" value={d.directorModel} onChange={(e) => set({ ...d, directorModel: e.target.value })} aria-label="Director model" />
          </Field>
        </>
      ) : null}
      <div className="flex items-center gap-2">
        <Button size="sm" variant="primary" disabledReason={!dirty ? 'No changes' : missing} onClick={() => onSave(d)}>
          Save LLM
        </Button>
        <Probe probe={probeLlm} disabledReason={dirty ? 'Save first; Probe tests the running model' : d.provider === 'null' ? 'No LLM configured' : null} />
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
      <div className="font-medium">Riddle {u.toVersion} changes {plural(u.changedSignals, 'system Signal')} and {plural(u.changedMeshes, 'default Mesh', 'default Meshes')}.</div>
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

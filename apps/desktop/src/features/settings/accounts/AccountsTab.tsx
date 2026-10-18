/**
 * Accounts as a list and a detail pane. The list stays short however many
 * accounts there are (two lines each, grouped, searchable past a handful),
 * and everything about one account lives in its pane. App-wide account
 * setup, the Google client and the defaults, sits under the list as its own
 * entries rather than interleaved with the accounts.
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import { FlaskConical, KeyRound, Plus, Search, SlidersHorizontal } from 'lucide-react'
import type { Autonomy } from '@/bindings/Autonomy'
import type { Dig } from '@/bindings/Dig'
import type { PrivacyMode } from '@/bindings/PrivacyMode'
import type { ProjectionMode } from '@/bindings/ProjectionMode'
import { cx, plural } from '@/lib/format'
import { useDevArmed, useDigGroups, useAllDigs } from '@/lib/queries'
import { digStatusChip, googleClientLine } from '@/lib/selectors'
import type { Settings } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Input } from '@/components/ui/Field'
import { Select } from '@/components/ui/Select'
import { SecretField } from '../shared'
import { AccountDetail } from './AccountDetail'
import { AddAccountWizard } from './AddAccountWizard'
import { GmailClientId, GoogleConsoleSteps, ImportGmailClient } from './GoogleClient'
import { Groups } from './Groups'
import { AUTONOMY_OPTIONS, PRIVACY_OPTIONS, PROJECTION_OPTIONS, autonomyLabel, autonomyOf, gmailEffect, groupAccounts } from './model'
import { Avatar, Block, OptionCards } from './pieces'

type Selection = { kind: 'dig'; id: string } | { kind: 'google' } | { kind: 'defaults' }

const SEARCH_FROM = 6

export function AccountsTab({ settings, focus }: { settings: Settings; focus?: string }) {
  const digs = useAllDigs().data ?? []
  const groups = useDigGroups().data ?? []
  const [query, setQuery] = useState('')
  const [wizard, setWizard] = useState(false)
  const [sel, setSel] = useState<Selection | null>(null)
  const { dev, armed } = useDevArmed()

  useEffect(() => {
    if (focus && ['groups', 'autonomy', 'new-digs'].includes(focus)) setSel({ kind: 'defaults' })
    if (focus === 'add-account') setWizard(true)
  }, [focus])
  // Landing on #digs from a "Sign in" affordance: open the account that
  // needs it, once the list has loaded, and only once.
  const preselected = useRef(false)
  useEffect(() => {
    if (focus !== 'digs' || preselected.current || !digs.length) return
    preselected.current = true
    const needs = digs.find((d) => d.status.state === 'not_connected')
    if (needs) setSel({ kind: 'dig', id: needs.id })
  }, [focus, digs])
  // Keep a valid selection: the first account, or the Google client when there are none.
  const current: Selection = useMemo(() => {
    if (sel?.kind === 'dig' && digs.some((d) => d.id === sel.id)) return sel
    if (sel && sel.kind !== 'dig') return sel
    return digs.length ? { kind: 'dig', id: [...digs].sort((a, b) => a.name.localeCompare(b.name))[0].id } : { kind: 'google' }
  }, [sel, digs])
  // A new selection starts at the top of its pane, not where the last one was scrolled to.
  const pane = useRef<HTMLElement>(null)
  const selectionKey = current.kind === 'dig' ? current.id : current.kind
  useEffect(() => {
    pane.current?.scrollTo?.({ top: 0 })
  }, [selectionKey])
  const grouped = groupAccounts(digs, query)
  const selectedDig = current.kind === 'dig' ? digs.find((d) => d.id === current.id) : undefined

  return (
    <div className="grid min-h-0 flex-1 grid-cols-[minmax(260px,320px)_1fr] gap-0 pt-3">
      <aside className="flex min-h-0 flex-col border-r border-line pr-3" aria-label="Accounts">
        <div className="flex items-center gap-2 pb-2">
          <h2 className="text-sm font-semibold">Accounts</h2>
          <span className="text-xs text-clay">{digs.length}</span>
          <span className="flex-1" />
          <Button size="sm" variant="primary" icon={<Plus />} onClick={() => setWizard(true)}>
            Add account
          </Button>
        </div>
        {digs.length >= SEARCH_FROM ? (
          <label className="relative mb-2 block">
            <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-clay" />
            <Input value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Find an account" className="pl-7" aria-label="Find an account" />
          </label>
        ) : null}
        <nav className="min-h-0 flex-1 overflow-auto">
          {digs.length === 0 ? (
            <div className="rounded-md border border-dashed border-line p-4 text-sm text-ink-2">
              <div className="font-medium text-ink">No accounts yet</div>
              <p className="mt-1 text-xs">Add a Gmail account to start, or a test mailbox to try Riddle without touching real mail.</p>
              <Button className="mt-3" size="sm" variant="primary" icon={<Plus />} onClick={() => setWizard(true)}>
                Add account
              </Button>
            </div>
          ) : null}
          {grouped.map(({ group, digs: members }) => (
            <div key={group ?? '__none'} className="mb-2">
              {groups.length > 0 ? <div className="px-2 pb-1 pt-2 text-2xs font-medium uppercase tracking-wide text-clay">{group ?? 'No group'}</div> : null}
              <ul>
                {members.map((d) => (
                  <li key={d.id}>
                    <AccountRow dig={d} settings={settings} dev={dev} armed={armed} selected={current.kind === 'dig' && current.id === d.id} onSelect={() => setSel({ kind: 'dig', id: d.id })} />
                  </li>
                ))}
              </ul>
            </div>
          ))}
          {query && grouped.length === 0 ? <p className="px-2 text-xs text-clay">No account matches “{query}”.</p> : null}

        </nav>
          <div className="shrink-0 border-t border-line pt-2 pb-1">
          <div className="px-2 pb-1 text-2xs font-medium uppercase tracking-wide text-clay">Setup</div>
          <NavItem icon={<KeyRound className="size-4" />} title="Google sign-in" detail={googleClientLine(settings.gmail).label} tone={googleClientLine(settings.gmail).tone} selected={current.kind === 'google'} onSelect={() => setSel({ kind: 'google' })} />
          <NavItem icon={<SlidersHorizontal className="size-4" />} title="Defaults and groups" detail={`Default: ${autonomyLabel(settings.autonomy.default)} · ${plural(groups.length, 'group')}`} selected={current.kind === 'defaults'} onSelect={() => setSel({ kind: 'defaults' })} />
        </div>
      </aside>

      <main ref={pane} className="min-h-0 overflow-auto pl-6 pr-1">
        <div className="max-w-3xl pb-8">
          {selectedDig ? <AccountDetail key={selectedDig.id} dig={selectedDig} settings={settings} groups={groups} /> : null}
          {current.kind === 'google' ? <GooglePanel settings={settings} /> : null}
          {current.kind === 'defaults' ? <DefaultsPanel settings={settings} digs={digs} /> : null}
        </div>
      </main>

      <AddAccountWizard open={wizard} onOpenChange={setWizard} onCreated={(id) => setSel({ kind: 'dig', id })} />
    </div>
  )
}

function AccountRow({ dig, settings, dev, armed, selected, onSelect }: { dig: Dig; settings: Settings; dev: boolean; armed: boolean; selected: boolean; onSelect: () => void }) {
  const chip = digStatusChip(dig.status)
  const { level } = autonomyOf(settings, dig.id)
  const effect = gmailEffect(level, dig.projection, { mailbox: dig.mailbox, dev, armed, provider: dig.provider })
  const attention = !dig.disabled && (chip.tone === 'bad' || chip.tone === 'warn')
  return (
    <button type="button" onClick={onSelect} aria-current={selected ? 'true' : undefined} title={dig.disabled ? 'Disabled: not synced, not sifted, left out of every list. Its mail is kept.' : `${chip.label}. ${effect.sentence}`} className={cx('flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left', selected ? 'bg-sand-2' : 'hover:bg-sand', dig.disabled && 'opacity-60')}>
      <Avatar id={dig.id} name={dig.name} />
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-1.5">
          <span className="truncate text-sm font-medium">{dig.name}</span>
          {dig.provider === 'fixture' ? <FlaskConical aria-label="Test mailbox" className="size-3 shrink-0 text-clay" /> : null}
          <StatusDot tone={chip.tone} title={chip.label} />
          <span className="ml-auto shrink-0 text-2xs text-ink-2">
            {autonomyLabel(level)}
            {effect.writes === 'labels' ? ' · Labels' : effect.writes === 'labels_and_archive' ? ' · Archive' : ''}
          </span>
        </span>
        <span className={cx('block truncate text-xs', attention ? (chip.tone === 'bad' ? 'text-rust' : 'text-ochre') : 'text-clay')}>
          {dig.disabled ? `Disabled · ${dig.email}` : dig.status.state === 'not_connected' ? `${chip.label} · Sign in again` : attention ? chip.label : dig.email}
        </span>
      </span>
    </button>
  )
}

function StatusDot({ tone, title }: { tone: string; title: string }) {
  const color = tone === 'ok' ? 'bg-moss' : tone === 'bad' ? 'bg-rust' : tone === 'neutral' ? 'bg-clay' : 'bg-ochre'
  return (
    <span className="inline-flex items-center gap-1 text-2xs text-clay" title={title}>
      <span aria-hidden className={cx('size-1.5 rounded-full', color)} />
      <span className="sr-only">{title}</span>
    </span>
  )
}

function NavItem({ icon, title, detail, selected, onSelect, tone }: { icon: React.ReactNode; title: string; detail: string; selected: boolean; onSelect: () => void; tone?: 'ok' | 'warn' }) {
  return (
    <button type="button" onClick={onSelect} aria-current={selected ? 'true' : undefined} className={cx('flex w-full items-center gap-2.5 rounded-md px-2 py-1.5 text-left', selected ? 'bg-sand-2' : 'hover:bg-sand')}>
      <span className="grid size-7 place-items-center rounded-full bg-sand-2 text-ink-2">{icon}</span>
      <span className="min-w-0 flex-1">
        <span className="block text-sm font-medium">{title}</span>
        <span className={cx('block truncate text-xs', tone === 'warn' ? 'text-ochre' : 'text-clay')}>{detail}</span>
      </span>
    </button>
  )
}

/**
 * The Google client, in one of three states: the build's own ("Using the
 * built-in Google client", with Use your own to bring one), a client the
 * person imported or typed, or none yet with the console steps open. A
 * build without a baked-in client shows today's steps as before.
 */
function GooglePanel({ settings }: { settings: Settings }) {
  const { run } = useAction()
  const g = settings.gmail
  const client = googleClientLine(g)
  const ready = client.source !== 'none'
  const setBuiltin = (useBuiltinClient: boolean) => run('save_settings', { settings: { ...settings, gmail: { ...g, useBuiltinClient } } }, { successToast: useBuiltinClient ? 'Using the built-in Google client' : 'Bring your own Google client' })
  if (client.source === 'builtin') {
    return (
      <div>
        <h2 className="text-base font-semibold">Google sign-in</h2>
        <div className="mt-3 flex items-start gap-2 rounded-md border border-moss/40 bg-moss/5 p-3">
          <KeyRound className="mt-0.5 size-4 text-moss" />
          <div>
            <div className="font-medium">Using the built-in Google client</div>
            <p className="mt-1 max-w-2xl text-xs text-ink-2">Every Gmail account signs in through the OAuth client this build of Riddle carries, so there is nothing to create in Google Cloud. Google asks you to approve access in your browser as it would for any app; Riddle asks only to read and label mail.</p>
            <div className="mt-2 flex items-center gap-2">
              <Button size="sm" onClick={() => setBuiltin(false)} title="Create your own Desktop app client in Google Cloud and import it; the built-in one is set aside">
                Use your own
              </Button>
              <span className="text-xs text-clay">Your own client keeps sign-in under a project you administer, which a Workspace domain may require.</span>
            </div>
          </div>
        </div>
      </div>
    )
  }
  return (
    <div>
      <h2 className="text-base font-semibold">Google sign-in</h2>
      <p className="mt-1 max-w-2xl text-sm text-ink-2">Every Gmail account signs in through one OAuth client you create in Google Cloud (a Desktop app client). Set it up once; Add account uses it for each account.</p>
      <div className="mt-3 flex flex-wrap gap-2">
        <Chip tone={ready ? 'ok' : 'warn'}>{ready ? 'Client id set' : 'No client id'}</Chip>
        <Chip tone={g.clientSecretPresent ? 'ok' : undefined}>{g.clientSecretPresent ? 'Secret in keychain' : 'No secret stored'}</Chip>
        {g.builtinClientAvailable && !g.useBuiltinClient && !g.clientId.trim() ? (
          <Button size="sm" variant="ghost" onClick={() => setBuiltin(true)}>
            Use the built-in client instead
          </Button>
        ) : null}
      </div>
      <GoogleConsoleSteps defaultOpen={!ready} />
      <Block title="Import from Google" hint="The JSON file Google Auth Platform → Clients offers as a download. It fills both fields below.">
        <ImportGmailClient />
      </Block>
      <Block title="Or enter it by hand">
        <GmailClientId value={settings.gmail.clientId} onSave={(clientId) => run('save_settings', { settings: { ...settings, gmail: { ...settings.gmail, clientId } } }, { successToast: 'Client id saved' })} />
        <SecretField secret="gmail_client_secret" label="Client secret (optional)" hint="Only if Google shows one for your Desktop client. Stored in the keychain." present={settings.gmail.clientSecretPresent} />
      </Block>
    </div>
  )
}

function DefaultsPanel({ settings, digs }: { settings: Settings; digs: Dig[] }) {
  const { run } = useAction()
  const groups = useDigGroups().data ?? []
  const save = (patch: Partial<Settings>, message = 'Saved') => run('save_settings', { settings: { ...settings, ...patch } }, { successToast: message })
  const overridden = digs.filter((d) => settings.autonomy.perDig[d.id]).length
  return (
    <div>
      <h2 className="text-base font-semibold">Defaults and groups</h2>
      <Block title="Default autonomy" hint={`Accounts without their own setting use this. ${overridden ? `${overridden} of ${digs.length} accounts have their own.` : 'No account overrides it.'} Trusted is only set per account.`}>
        <OptionCards label="Default autonomy" options={AUTONOMY_OPTIONS.filter((o) => o.value !== 'trusted')} value={settings.autonomy.default === 'trusted' ? 'act' : settings.autonomy.default} onChange={(v: Autonomy) => save({ autonomy: { ...settings.autonomy, default: v } }, `Default autonomy: ${autonomyLabel(v)}`)} />
      </Block>
      <Block title="Sending mail" hint="Sending is riskier than labelling, so it has its own level across all accounts.">
        <Select label="Autonomy for sending mail" value={settings.autonomy.perAction.send_message ?? 'suggest'} onChange={(v) => save({ autonomy: { ...settings.autonomy, perAction: { ...settings.autonomy.perAction, send_message: v as Autonomy } } })} options={AUTONOMY_OPTIONS.filter((a) => a.value !== 'trusted').map((o) => ({ value: o.value, label: o.label }))} />
      </Block>
      <Block title="New accounts" hint="Where Add account starts. Each account can change them afterwards.">
        <div className="flex flex-col gap-3">
          <div>
            <div className="mb-1 text-xs font-medium">Privacy</div>
            <OptionCards label="Default privacy" options={PRIVACY_OPTIONS} value={settings.privacyDefault} onChange={(v: PrivacyMode) => save({ privacyDefault: v })} />
          </div>
          <div>
            <div className="mb-1 text-xs font-medium">Changes in Gmail</div>
            <OptionCards label="Default Gmail changes" options={PROJECTION_OPTIONS} value={settings.projectionDefault} onChange={(v: ProjectionMode) => save({ projectionDefault: v })} />
          </div>
        </div>
      </Block>
      <Block title="Groups" hint="Meshes and Strata can be scoped to a group. Put an account in a group from the account itself; rename or delete groups here.">
        <Groups groups={groups} digs={digs} />
      </Block>
    </div>
  )
}

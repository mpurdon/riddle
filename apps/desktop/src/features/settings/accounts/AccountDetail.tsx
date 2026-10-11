/**
 * One account, in full: signing in again, what Riddle may do with it, what
 * it sends to the cloud, its group, and removing it. Replaces a card that
 * put four selects and a hold button side by side for every account at
 * once, which neither explained how the settings combine nor scaled past a
 * few accounts. Sign in again exists because the only way back from an
 * expired Google token used to be Remove, which wiped everything Riddle
 * had learned about the mailbox. Disable exists for the same reason: a test
 * account cluttering the real one could only be removed.
 */
import { useEffect, useRef, useState } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { AlertTriangle, CheckCircle2, CircleSlash, KeyRound, Power, PowerOff, RefreshCw, Trash2 } from 'lucide-react'
import type { Autonomy } from '@/bindings/Autonomy'
import type { Dig } from '@/bindings/Dig'
import type { DigGroup } from '@/bindings/DigGroup'
import type { PrivacyMode } from '@/bindings/PrivacyMode'
import type { ProjectionMode } from '@/bindings/ProjectionMode'
import { probeProvider } from '@/lib/api'
import { fmtRelative, plural } from '@/lib/format'
import { mailboxName } from '@/lib/gmailEffect'
import { toRiddleError } from '@/lib/ipc'
import { useDevArmed, useMeshes } from '@/lib/queries'
import { digStatusChip, lastSyncedPhrase, syncFailureLine } from '@/lib/selectors'
import type { PendingAction } from '@/bindings/PendingAction'
import { expectedCount } from '@/lib/confirm'
import type { ProbeResult, Settings } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Input } from '@/components/ui/Field'
import { Select } from '@/components/ui/Select'
import { HoldButton } from '@/components/confirm/HoldButton'
import { useDraft } from '../shared'
import { SignInProgress, useGmailSignIn } from './GoogleClient'
import { AUTONOMY_OPTIONS, PRIVACY_OPTIONS, PROJECTION_OPTIONS, autonomyLabel, autonomyOf, cappedRules, gmailEffect, labelOf } from './model'
import { Avatar, Block, OptionCards } from './pieces'

export function AccountDetail({ dig, settings, groups }: { dig: Dig; settings: Settings; groups: DigGroup[] }) {
  const { run } = useAction()
  const nav = useNavigate()
  const { dev, armed } = useDevArmed()
  const { level, own } = autonomyOf(settings, dig.id)
  const [pendingTrusted, setPendingTrusted] = useState(false)
  const [check, setCheck] = useState<ProbeResult | null>(null)
  useEffect(() => {
    setPendingTrusted(false)
    setCheck(null)
  }, [dig.id])
  const chip = digStatusChip(dig.status)
  const needsSignIn = dig.status.state === 'not_connected' || dig.status.state === 'degraded' || (dig.status.state === 'sync_failing' && dig.status.failure === 'sign_in')
  // The backend's answer for this account; the dev/armed flags are the fallback for a Dig read without it.
  const effect = gmailEffect(level, dig.projection, { mailbox: dig.mailbox, dev, armed, provider: dig.provider })
  const capped = cappedRules(useMeshes().data ?? [], dig)
  const update = (patch: Partial<Dig>, message: string) => run('update_dig', { dig: { ...dig, ...patch } }, { successToast: message })
  const setAutonomy = (a: Autonomy | null) => {
    const perDig = { ...settings.autonomy.perDig }
    if (a === null) delete perDig[dig.id]
    else perDig[dig.id] = a
    return run('save_settings', { settings: { ...settings, autonomy: { ...settings.autonomy, perDig } } }, { successToast: a ? `${dig.name}: ${autonomyLabel(a)}` : `${dig.name} uses the default` })
  }

  return (
    <div className="flex min-h-0 flex-col">
      <header className="flex items-start gap-3 pb-3">
        <Avatar id={dig.id} name={dig.name} size="lg" />
        <div className="min-w-0 flex-1">
          <NameField dig={dig} onSave={(name) => update({ name }, 'Renamed')} />
          <div className="mt-0.5 flex flex-wrap items-center gap-1.5 text-xs text-clay">
            <span className="selectable truncate">{dig.email}</span>
            <Chip>{dig.provider === 'fixture' ? 'Test mailbox' : 'Gmail'}</Chip>
            {dig.disabled ? <Chip>Disabled</Chip> : <Chip tone={chip.tone} title={chip.title}>{chip.label}</Chip>}
            <span>Added {fmtRelative(dig.createdAt)}</span>
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <ConnectionCheck dig={dig} onResult={setCheck} />
          <Button size="sm" icon={<RefreshCw />} disabledReason={dig.disabled ? 'Enable the account to sync it' : null} onClick={() => run('refresh_now', { digId: dig.id }, { successToast: `Syncing ${dig.name}` })}>
            Sync now
          </Button>
        </div>
      </header>
      {check ? <p className={`-mt-1 mb-3 text-xs ${check.ok ? 'text-moss' : 'text-rust'}`}>{check.message}</p> : null}

      {dig.disabled ? (
        <div role="status" className="mb-3 flex items-center gap-2 rounded-md border border-line bg-sand/60 px-3 py-2 text-xs">
          <PowerOff className="size-3.5 shrink-0 text-clay" />
          <span className="flex-1">
            <span className="font-medium">Disabled.</span> Riddle is not syncing or sifting this account, and leaves it out of every list, count and rule preview. Its mail and decisions are kept.
          </span>
          <Button size="sm" icon={<Power />} onClick={() => run('set_dig_enabled', { digId: dig.id, enabled: true }, { successToast: `${dig.name} enabled` })}>
            Enable
          </Button>
        </div>
      ) : null}

      {dig.status.state === 'not_connected' ? (
        <div role="alert" className="mb-3 flex items-start gap-2 rounded-md border border-ochre/40 bg-ochre/5 px-3 py-2 text-xs">
          <AlertTriangle className="mt-px size-3.5 shrink-0 text-ochre" />
          <span className="flex-1">
            <span className="font-medium">Not signed in.</span> Google no longer accepts this account's sign-in, so its mail is not being synced. Sign in again to resume; the account keeps its history, its Strata and its settings.
          </span>
        </div>
      ) : null}
      {dig.status.state === 'sync_failing' ? (
        <div role="alert" className="mb-3 flex items-start gap-2 rounded-md border border-rust/30 bg-rust/5 px-3 py-2 text-xs text-rust">
          <AlertTriangle className="mt-px size-3.5 shrink-0" />
          <span className="flex-1">
            <span className="font-medium">Not syncing:</span> {syncFailureLine(dig.status.failure)}; {lastSyncedPhrase(dig.status.last_synced_at)}. Mail since then is not in Riddle.
          </span>
        </div>
      ) : null}
      {dig.status.state === 'degraded' ? (
        <div role="alert" className="mb-3 flex items-start gap-2 rounded-md border border-rust/30 bg-rust/5 px-3 py-2 text-xs text-rust">
          <AlertTriangle className="mt-px size-3.5 shrink-0" />
          <span className="flex-1">{dig.status.reason}</span>
        </div>
      ) : null}
      <SignInAgain dig={dig} prominent={needsSignIn} />
      {dig.provider === 'gmail' && !needsSignIn ? <p className="-mt-2 mb-3 text-xs text-clay">Use this if Google has revoked access or asks for consent again. The account keeps its history.</p> : null}

      <Block title="What Riddle does with this account">
        <div className={`mb-3 flex items-start gap-2 rounded-md border px-3 py-2 text-sm ${effect.writes === 'nothing' ? 'border-line bg-sand/60' : 'border-ochre/40 bg-ochre/5'}`}>
          {effect.writes === 'nothing' ? <CircleSlash className="mt-0.5 size-4 shrink-0 text-clay" /> : <CheckCircle2 className="mt-0.5 size-4 shrink-0 text-ochre" />}
          <span>
            <span className="font-medium">Right now: </span>
            {effect.sentence}
          </span>
        </div>
        {capped.length ? (
          <p className="-mt-1 mb-3 text-xs text-ink-2">
            {plural(capped.length, 'rule')} {capped.length === 1 ? 'keeps its' : 'keep their'} decisions in Riddle whatever this account allows:{' '}
            {capped.map((m, i) => (
              <span key={m.id}>
                {i ? ', ' : ''}
                <Link to={`/meshes?name=${encodeURIComponent(m.name)}`} className="underline hover:text-ink">
                  {m.name}
                </Link>
              </span>
            ))}
            .
          </p>
        ) : null}
        <div className="mb-1 flex items-baseline gap-2 text-xs text-ink-2">
          <span className="font-medium text-ink">Autonomy</span>
          {own ? (
            <Button size="sm" variant="link" onClick={() => setAutonomy(null)}>
              Use the default ({autonomyLabel(settings.autonomy.default)})
            </Button>
          ) : (
            <span>Using the default for all accounts</span>
          )}
        </div>
        <OptionCards
          label="Autonomy"
          options={AUTONOMY_OPTIONS}
          value={pendingTrusted ? 'trusted' : level}
          onChange={(v) => {
            if (v === 'trusted') return setPendingTrusted(true)
            setPendingTrusted(false)
            void setAutonomy(v)
          }}
        />
        {pendingTrusted && level !== 'trusted' ? (
          <div className="mt-2 flex items-center gap-3 rounded-md border border-rust/30 bg-rust/5 p-2">
            <HoldButton
              label={`Hold to make ${dig.name} Trusted`}
              coaching="Trusted acts without asking, including sending mail. Hold for 1.5 seconds to confirm."
              onComplete={async () => {
                const perDig = { ...settings.autonomy.perDig, [dig.id]: 'trusted' as Autonomy }
                await run('save_settings', { settings: { ...settings, autonomy: { ...settings.autonomy, perDig } } }, { confirmed: true, successToast: `${dig.name} is Trusted` })
                setPendingTrusted(false)
              }}
            />
            <Button size="sm" variant="ghost" onClick={() => setPendingTrusted(false)}>
              Cancel
            </Button>
          </div>
        ) : null}

        <div className="mb-1 mt-4 text-xs font-medium text-ink">Changes in {mailboxName(dig.provider)}</div>
        <OptionCards label="Changes in Gmail" options={PROJECTION_OPTIONS} value={dig.projection} onChange={(v: ProjectionMode) => update({ projection: v }, `${dig.name}: ${labelOf(PROJECTION_OPTIONS, v)}`)} />
        {effect.held ? (
          <p className="mt-2 text-xs text-clay">
            Held because {effect.heldBecause}. Your own moves in Riddle still apply.
            {effect.held === 'dev_unarmed' ? (
              <Button size="sm" variant="link" className="ml-1" onClick={() => nav('/developer')}>
                Choose Ask or Allow on the Developer page
              </Button>
            ) : null}
          </p>
        ) : null}
      </Block>

      <Block title="Privacy" hint="What may leave this Mac when Riddle classifies this account's mail.">
        <OptionCards label="Privacy" options={PRIVACY_OPTIONS} value={dig.privacy} onChange={(v: PrivacyMode) => update({ privacy: v }, `${dig.name}: ${labelOf(PRIVACY_OPTIONS, v)}`)} />
      </Block>

      <Block title="Group" hint="Meshes and Strata can be scoped to a group. An account belongs to at most one.">
        <GroupPicker dig={dig} groups={groups} />
      </Block>

      {dig.disabled ? null : (
        <Block title="Disable this account">
          <div className="flex items-center gap-3">
            <p className="flex-1 text-xs text-ink-2">Stops syncing and sifting it and leaves it out of every list, count and rule preview, keeping its mail and decisions. Enable it again here at any time; it catches up from where it stopped.</p>
            <Button size="sm" icon={<PowerOff />} onClick={() => run('set_dig_enabled', { digId: dig.id, enabled: false }, { successToast: `${dig.name} disabled` })}>
              Disable
            </Button>
          </div>
        </Block>
      )}

      <Block title="Remove this account">
        <RemoveAccount dig={dig} />
      </Block>
    </div>
  )
}

function NameField({ dig, onSave }: { dig: Dig; onSave: (name: string) => void }) {
  const [name, setName] = useDraft(dig.name)
  const commit = () => {
    const n = name.trim()
    if (n && n !== dig.name) onSave(n)
    else setName(dig.name)
  }
  return (
    <input
      value={name}
      onChange={(e) => setName(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => {
        if (e.key === 'Enter') (e.target as HTMLInputElement).blur()
        if (e.key === 'Escape') setName(dig.name)
      }}
      aria-label="Account name"
      className="w-full rounded border border-transparent bg-transparent px-1 -mx-1 text-base font-semibold hover:border-line focus:border-ochre focus:outline-none"
    />
  )
}

/**
 * Signs an existing Gmail account in again: `connect_gmail { digId }` keeps
 * the history, replaces the token and runs a catch-up. The handshake is the
 * wizard's (`useGmailSignIn`); this also stands down when the account's
 * status moves on, which a catch-up starting does before any event lands.
 */
function SignInAgain({ dig, prominent }: { dig: Dig; prominent: boolean }) {
  const { state, start, reset } = useGmailSignIn({ digId: dig.id })
  const startedWith = useRef(dig.status.state)
  useEffect(() => {
    if (state.phase === 'connected' || (state.phase === 'waiting' && dig.status.state !== startedWith.current)) reset()
  }, [state, dig.status.state, reset])

  if (state.phase !== 'idle') return <SignInProgress state={state} className="mb-3" onRetry={reset} onCancel={state.phase === 'waiting' ? reset : undefined} />
  return (
    <div className="mb-3">
      <Button
        variant={prominent ? 'primary' : 'secondary'}
        size={prominent ? 'md' : 'sm'}
        icon={<KeyRound />}
        disabledReason={dig.provider === 'fixture' ? 'A test mailbox has no sign-in' : null}
        onClick={() => {
          startedWith.current = dig.status.state
          void start()
        }}
      >
        Sign in again
      </Button>
    </div>
  )
}

/** Proves the sign-in works and says how far sync has got. */
function ConnectionCheck({ dig, onResult }: { dig: Dig; onResult: (r: ProbeResult) => void }) {
  const [busy, setBusy] = useState(false)
  return (
    <Button
      size="sm"
      variant="ghost"
      busy={busy}
      onClick={async () => {
        setBusy(true)
        try {
          onResult(await probeProvider(dig.id))
        } catch (e) {
          onResult({ ok: false, latencyMs: 0, message: toRiddleError(e).message })
        } finally {
          setBusy(false)
        }
      }}
    >
      Test connection
    </Button>
  )
}

function GroupPicker({ dig, groups }: { dig: Dig; groups: DigGroup[] }) {
  const { run } = useAction()
  const [creating, setCreating] = useState(false)
  const [name, setName] = useState('')
  const current = groups.find((g) => g.name === dig.group)
  const moveTo = async (target: string | null) => {
    if (target === dig.group) return
    if (target === null) {
      if (current) await run('save_dig_group', { group: { ...current, digs: current.digs.filter((d) => d !== dig.id) } }, { successToast: `${dig.name} left ${current.name}` })
      return
    }
    const g = groups.find((x) => x.name === target)
    await run('save_dig_group', { group: { name: target, digs: [...(g?.digs ?? []).filter((d) => d !== dig.id), dig.id] } }, { successToast: `${dig.name} is in ${target}` })
  }
  if (creating) {
    const trimmed = name.trim()
    const clash = groups.some((g) => g.name.toLowerCase() === trimmed.toLowerCase())
    return (
      <div className="flex items-center gap-2">
        <Input autoFocus value={name} onChange={(e) => setName(e.target.value)} placeholder="Group name" className="w-56" aria-label="New group name" onKeyDown={(e) => e.key === 'Escape' && setCreating(false)} />
        <Button
          size="sm"
          variant="primary"
          disabledReason={!trimmed ? 'Name the group' : clash ? 'A group with that name exists; pick it from the list' : null}
          onClick={async () => {
            await moveTo(trimmed)
            setCreating(false)
            setName('')
          }}
        >
          Create and add
        </Button>
        <Button size="sm" variant="ghost" onClick={() => setCreating(false)}>
          Cancel
        </Button>
      </div>
    )
  }
  return (
    <div className="flex items-center gap-2">
      <Select
        label="Group"
        value={dig.group ?? '__none'}
        onChange={(v) => (v === '__new' ? setCreating(true) : void moveTo(v === '__none' ? null : v))}
        options={[{ value: '__none', label: 'No group' }, ...groups.map((g) => ({ value: g.name, label: `${g.name} (${g.digs.length})` })), { value: '__new', label: 'New group…' }]}
      />
    </div>
  )
}

type Ask = { pending: PendingAction; resolve: (ok: boolean) => void }

/**
 * Removing is confirmed here, in the panel, by typing the message count the
 * Action layer reports; the old flow opened a modal over the whole page.
 */
function RemoveAccount({ dig }: { dig: Dig }) {
  const { run } = useAction()
  const [ask, setAsk] = useState<Ask | null>(null)
  const [typed, setTyped] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const mailbox = mailboxName(dig.provider)
  // Leaving the panel mid-question cancels it rather than leaving the Action waiting.
  const pendingAsk = useRef<Ask | null>(null)
  pendingAsk.current = ask
  useEffect(() => () => pendingAsk.current?.resolve(false), [])
  const count = ask ? expectedCount(ask.pending) : null
  const matches = count !== null && typed.trim() === String(count)
  const finish = (ok: boolean) => {
    ask?.resolve(ok)
    setAsk(null)
    setTyped('')
  }
  const start = async () => {
    setError(null)
    setBusy(true)
    try {
      await run('remove_dig', { digId: dig.id }, { confirmWith: (pending) => new Promise<boolean>((resolve) => setAsk({ pending, resolve })) })
    } catch (e) {
      setError(toRiddleError(e).message)
    } finally {
      setBusy(false)
    }
  }
  if (!ask) {
    return (
      <div>
        <div className="flex items-center gap-3">
          <p className="flex-1 text-xs text-ink-2">Deletes Riddle's copy of this mailbox, its classifications and its sign-in. Nothing in {mailbox} is deleted.</p>
          <Button size="sm" variant="danger" icon={<Trash2 />} busy={busy} onClick={start}>
            Remove…
          </Button>
        </div>
        {error ? <p role="alert" className="mt-2 text-xs text-rust">{error}</p> : null}
      </div>
    )
  }
  return (
    <div role="alert" className="rounded-md border border-rust/40 bg-rust/5 p-3 text-sm">
      <div className="font-medium text-rust">{ask.pending.summary}</div>
      <p className="mt-1 text-xs text-ink-2">This cannot be undone: to use the account again you connect it and it syncs from scratch. Nothing in {mailbox} is deleted.</p>
      <label className="mt-3 flex items-center gap-2">
        <span className="text-xs text-ink-2">Type <span className="mono font-medium text-ink">{count ?? '?'}</span> to confirm</span>
        <Input
          autoFocus
          inputMode="numeric"
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter' && matches) finish(true)
            if (e.key === 'Escape') finish(false)
          }}
          className="w-32"
          aria-label={`Type ${count ?? ''} to confirm removing ${dig.name}`}
        />
        <span className="flex-1" />
        <Button size="sm" variant="ghost" onClick={() => finish(false)}>
          Cancel
        </Button>
        <Button size="sm" variant="danger" disabledReason={matches ? null : count === null ? 'The Action layer gave no count' : `Type ${count} exactly`} onClick={() => finish(true)}>
          Remove {dig.name}
        </Button>
      </label>
    </div>
  )
}

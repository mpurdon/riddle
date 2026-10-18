/**
 * Adding an account, one decision at a time: which kind, the Google client
 * (once per app), how Riddle should treat it, then the sign-in, which
 * happens in the browser and reports back as events.
 *
 * Before this, adding a Gmail account meant finding the right field among
 * a dozen, signing in, and then reading a card that said "Not connected".
 * The wizard starts every account at Observe, so a new account can be
 * looked at before Riddle is allowed to touch it; privacy and Gmail changes
 * start where Settings → Accounts → Defaults says, and at Observe a Gmail
 * change is held whatever that default is. The Behaviour step asks for a
 * name and a group and states the rest in one sentence; the nine option
 * cards it used to show before sign-in wait under "Advanced options".
 */
import { useEffect, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { CheckCircle2, FlaskConical, Mail, Sunrise } from 'lucide-react'
import type { Autonomy } from '@/bindings/Autonomy'
import type { Dig } from '@/bindings/Dig'
import type { PrivacyMode } from '@/bindings/PrivacyMode'
import type { ProjectionMode } from '@/bindings/ProjectionMode'
import { cx } from '@/lib/format'
import { mailboxName } from '@/lib/gmailEffect'
import { useDigGroups, useAllDigs, useSettings } from '@/lib/queries'
import { googleClientLine, judgeStateLine } from '@/lib/selectors'
import { useUi } from '@/lib/stores'
import type { Settings } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Dialog } from '@/components/ui/Dialog'
import { Field, Input } from '@/components/ui/Field'
import { Select } from '@/components/ui/Select'
import { SecretField } from '../shared'
import { GmailClientId, GoogleConsoleSteps, ImportGmailClient, SignInProgress, useGmailSignIn } from './GoogleClient'
import { AUTONOMY_OPTIONS, PRIVACY_OPTIONS, PROJECTION_OPTIONS, gmailEffect } from './model'
import { OptionCards } from './pieces'

type Kind = 'gmail' | 'fixture'
type Step = 'kind' | 'client' | 'setup' | 'signin' | 'done'

interface Setup {
  name: string
  email: string
  count: string
  group: string | null
  privacy: PrivacyMode
  autonomy: Autonomy
  projection: ProjectionMode
}

const STEPS: Record<Kind, { id: Step; label: string }[]> = {
  gmail: [
    { id: 'kind', label: 'Account type' },
    { id: 'client', label: 'Google client' },
    { id: 'setup', label: 'Behaviour' },
    { id: 'signin', label: 'Sign in' },
    { id: 'done', label: 'Done' },
  ],
  fixture: [
    { id: 'kind', label: 'Account type' },
    { id: 'setup', label: 'Behaviour' },
    { id: 'done', label: 'Done' },
  ],
}

export function AddAccountWizard({ open, onOpenChange, onCreated }: { open: boolean; onOpenChange: (o: boolean) => void; onCreated: (digId: string) => void }) {
  const settings = useSettings().data
  const groups = useDigGroups().data ?? []
  const { run } = useAction()
  const nav = useNavigate()
  const setDigId = useUi((s) => s.setDigId)
  const [kind, setKind] = useState<Kind>('gmail')
  const [step, setStep] = useState<Step>('kind')
  const [setup, setSetup] = useState<Setup | null>(null)
  const [created, setCreated] = useState<Dig | null>(null)

  // Start fresh each time the wizard opens; later settings refetches (an
  // import, a save) must not reset a wizard in progress.
  const ready = !!settings
  // The effective default: local-only until a judge key is configured, the stored default once one is.
  const privacyDefault = settings?.effectivePrivacyDefault ?? 'local_only'
  const projectionDefault = settings?.projectionDefault ?? 'none'
  const reset = () => {
    setStep('kind')
    setCreated(null)
    setSetup({ name: '', email: '', count: '120', group: null, privacy: privacyDefault, autonomy: 'observe', projection: projectionDefault })
  }
  useEffect(() => {
    if (open && ready) reset()
  }, [open, ready]) // the defaults are read once per opening
  /** "Watch it sort": the Summary, following the new account, where its first sync shows its progress. */
  const watch = (dig: Dig) => {
    setDigId(dig.id)
    onOpenChange(false)
    nav('/')
  }

  if (!settings || !setup) return null
  // A client baked into the build means there is nothing to set up: the
  // Google client step is left out, not shown as done.
  const client = googleClientLine(settings.gmail)
  const builtinClient = client.source === 'builtin'
  const steps = kind === 'gmail' && builtinClient ? STEPS.gmail.filter((s) => s.id !== 'client') : STEPS[kind]
  const clientReady = client.source !== 'none'
  const next = (s: Step) => setStep(s)

  /** Per-account autonomy lives in settings; Observe needs no entry when it is the default. */
  const applyAutonomy = async (digId: string) => {
    if (setup.autonomy === settings.autonomy.default) return
    await run('save_settings', { settings: { ...settings, autonomy: { ...settings.autonomy, perDig: { ...settings.autonomy.perDig, [digId]: setup.autonomy } } } }, { successToast: false })
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange} title="Add an account" wide>
      <Stepper steps={steps} current={step} />
      <div className="mt-4 min-h-[420px]">
        {step === 'kind' ? (
          <div className="flex flex-col gap-3">
            <p className="text-ink-2">Each account is its own Dig: its own copy of the mailbox, its own status, its own rules.</p>
            <div role="radiogroup" aria-label="Account type" className="grid grid-cols-2 gap-3">
              <KindCard on={kind === 'gmail'} icon={<Mail className="size-5" />} title="Gmail" detail="A Gmail or Google Workspace account. You sign in with Google in your browser; Riddle never sees your password." onClick={() => setKind('gmail')} />
              <KindCard on={kind === 'fixture'} icon={<FlaskConical className="size-5" />} title="Test mailbox" detail="Generated mail, no network. For trying Meshes and Signals without touching a real account." onClick={() => setKind('fixture')} />
            </div>
          </div>
        ) : null}

        {step === 'client' ? (
          <div className="flex flex-col gap-2">
            {clientReady ? (
              <div className="flex items-start gap-2 rounded-md border border-moss/40 bg-moss/5 p-3">
                <CheckCircle2 className="mt-0.5 size-4 text-moss" />
                <div>
                  <div className="font-medium">Google client is set up</div>
                  <div className="mono mt-0.5 text-xs text-ink-2 selectable">{settings.gmail.clientId}</div>
                  <div className="mt-1 text-xs text-ink-2">Every Gmail account signs in through this client. {settings.gmail.clientSecretPresent ? 'Its secret is in the keychain.' : 'No client secret is stored.'}</div>
                </div>
              </div>
            ) : (
              <>
                <p className="text-ink-2">
                  Riddle signs in to Gmail through an OAuth client you create once in Google Cloud: a <span className="font-medium">Desktop app</span> client with the Gmail API enabled. Follow the steps, then import the JSON Google offers as a download.
                </p>
                <GoogleConsoleSteps defaultOpen />
                <ImportGmailClient />
                <details className="mt-1 text-xs text-ink-2">
                  <summary className="cursor-pointer select-none">Enter the client id by hand instead</summary>
                  <GmailClientId value={settings.gmail.clientId} onSave={(clientId) => run('save_settings', { settings: { ...settings, gmail: { ...settings.gmail, clientId } } }, { successToast: 'Client id saved' })} />
                </details>
              </>
            )}
          </div>
        ) : null}

        {step === 'setup' ? <SetupStep kind={kind} setup={setup} onChange={setSetup} groups={groups.map((g) => g.name)} /> : null}

        {step === 'signin' ? (
          <SignIn
            setup={setup}
            onConnected={async (dig) => {
              await applyAutonomy(dig.id)
              if (setup.group) {
                const g = groups.find((x) => x.name === setup.group)
                await run('save_dig_group', { group: { name: setup.group, digs: [...(g?.digs ?? []), dig.id] } }, { successToast: false })
              }
              setCreated(dig)
              next('done')
            }}
          />
        ) : null}

        {step === 'done' && created ? <Done dig={created} setup={setup} settings={settings} /> : null}
      </div>

      <footer className="mt-5 flex items-center gap-2">
        {step !== 'kind' && step !== 'done' ? (
          <Button variant="ghost" onClick={() => next(steps[steps.findIndex((s) => s.id === step) - 1].id)}>
            Back
          </Button>
        ) : null}
        <span className="flex-1" />
        {step === 'kind' ? (
          <Button variant="primary" onClick={() => next(kind === 'gmail' && !builtinClient ? 'client' : 'setup')}>
            Continue
          </Button>
        ) : null}
        {step === 'client' ? (
          <Button variant="primary" disabledReason={clientReady ? null : 'Import the client file or enter its id first'} onClick={() => next('setup')}>
            Continue
          </Button>
        ) : null}
        {step === 'setup' && kind === 'gmail' ? (
          <Button variant="primary" onClick={() => next('signin')}>
            Continue to sign-in
          </Button>
        ) : null}
        {step === 'setup' && kind === 'fixture' ? (
          <Button
            variant="primary"
            disabledReason={!setup.name.trim() ? 'Name the account' : !/@/.test(setup.email) ? 'Give it an address' : null}
            onClick={async () => {
              const out = await run<Dig>('add_fixture_dig', { name: setup.name.trim(), email: setup.email.trim(), count: Number(setup.count) || 120, group: setup.group, privacy: setup.privacy, projection: setup.projection }, { successToast: false })
              if (!out?.result) return
              if (setup.group) {
                const g = groups.find((x) => x.name === setup.group)
                await run('save_dig_group', { group: { name: setup.group, digs: [...(g?.digs ?? []), out.result.id] } }, { successToast: false })
              }
              await applyAutonomy(out.result.id)
              setCreated(out.result)
              next('done')
            }}
          >
            Create test mailbox
          </Button>
        ) : null}
        {step === 'done' && created ? (
          <>
            <Button variant="ghost" onClick={reset}>
              Add another
            </Button>
            <Button onClick={() => { onCreated(created.id); onOpenChange(false) }}>Open the account settings</Button>
            <Button variant="primary" icon={<Sunrise />} onClick={() => watch(created)} title="Open the Summary for this account: the first sync's progress, and what Riddle has sorted so far">
              Watch it sort
            </Button>
          </>
        ) : null}
      </footer>
    </Dialog>
  )
}

function Stepper({ steps, current }: { steps: { id: Step; label: string }[]; current: Step }) {
  const at = steps.findIndex((s) => s.id === current)
  return (
    <ol className="flex items-center gap-2 text-xs" aria-label="Steps">
      {steps.map((s, i) => (
        <li key={s.id} aria-current={i === at ? 'step' : undefined} className="flex items-center gap-2">
          <span className={cx('grid size-5 place-items-center rounded-full border text-2xs font-semibold', i < at ? 'border-moss bg-moss text-paper' : i === at ? 'border-ochre text-ochre' : 'border-line text-clay')}>{i < at ? '✓' : i + 1}</span>
          <span className={i === at ? 'font-medium text-ink' : 'text-clay'}>{s.label}</span>
          {i < steps.length - 1 ? <span aria-hidden className="h-px w-6 bg-line" /> : null}
        </li>
      ))}
    </ol>
  )
}

function KindCard({ on, icon, title, detail, onClick }: { on: boolean; icon: React.ReactNode; title: string; detail: string; onClick: () => void }) {
  return (
    <button type="button" role="radio" aria-checked={on} onClick={onClick} className={cx('flex flex-col items-start gap-2 rounded-lg border p-4 text-left', on ? 'border-ochre bg-ochre/5' : 'border-line hover:bg-sand')}>
      <span className={on ? 'text-ochre' : 'text-ink-2'}>{icon}</span>
      <span className="font-medium">{title}</span>
      <span className="text-xs text-ink-2">{detail}</span>
    </button>
  )
}

const AUTONOMY_WORDS: Record<Autonomy, string> = {
  observe: 'observe only',
  suggest: 'suggest moves for you to apply',
  act: 'act on its decisions',
  trusted: 'act without asking',
}

const PRIVACY_WORDS: Record<PrivacyMode, string> = {
  local_only: 'keep everything on this Mac',
  cloud_judgment: 'use cloud judgment',
  cloud_reasoning: 'use cloud reasoning',
}

/**
 * The behaviour a new account will have, in one sentence, so what the user
 * accepts by continuing is said before sign-in: "Riddle will observe only,
 * change nothing in Gmail, and use cloud judgment." The Gmail clause says
 * what will happen, not what the setting is: at Observe a "Labels" default
 * writes nothing until the account is raised to Act.
 */
export function behaviourSentence(setup: Pick<Setup, 'autonomy' | 'projection' | 'privacy'>, kind: Kind): string {
  const provider = kind === 'fixture' ? 'fixture' : 'gmail'
  const mailbox = mailboxName(provider)
  const effect = gmailEffect(setup.autonomy, setup.projection, { provider })
  const changes =
    effect.writes === 'labels_and_archive'
      ? `add labels and archive mail in ${mailbox}`
      : effect.writes === 'labels'
        ? `add labels in ${mailbox}`
        : effect.held === 'autonomy'
          ? `change nothing in ${mailbox} until it is raised to Act`
          : `change nothing in ${mailbox}`
  return `Riddle will ${AUTONOMY_WORDS[setup.autonomy]}, ${changes}, and ${PRIVACY_WORDS[setup.privacy]}.`
}

function SetupStep({ kind, setup, onChange, groups }: { kind: Kind; setup: Setup; onChange: (s: Setup) => void; groups: string[] }) {
  const set = (patch: Partial<Setup>) => onChange({ ...setup, ...patch })
  const effect = gmailEffect(setup.autonomy, setup.projection, { provider: kind === 'fixture' ? 'fixture' : 'gmail' })
  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-3 gap-3">
        <Field label="Name" hint={kind === 'gmail' ? 'Shown in Riddle; the address comes from Google.' : undefined}>
          <Input autoFocus value={setup.name} onChange={(e) => set({ name: e.target.value })} placeholder={kind === 'gmail' ? 'Personal' : 'Side project'} />
        </Field>
        {kind === 'fixture' ? (
          <>
            <Field label="Address"><Input value={setup.email} onChange={(e) => set({ email: e.target.value })} placeholder="me@side.example" /></Field>
            <Field label="Messages to generate"><Input value={setup.count} onChange={(e) => set({ count: e.target.value })} inputMode="numeric" /></Field>
          </>
        ) : (
          <Field label="Group">
            <Select label="Group" value={setup.group ?? '__none'} onChange={(v) => set({ group: v === '__none' ? null : v })} options={[{ value: '__none', label: 'No group' }, ...groups.map((g) => ({ value: g, label: g }))]} />
          </Field>
        )}
      </div>
      <p className="text-sm text-ink-2">
        <span className="font-medium text-ink">{behaviourSentence(setup, kind)}</span> You can change any of this later, and raising to Trusted is done from the account itself.
      </p>
      <details className="rounded-md border border-line bg-sand/50 px-3 py-2">
        <summary className="cursor-pointer select-none text-xs font-medium text-ink-2 hover:text-ink">Advanced options</summary>
        <div className="mt-3 flex flex-col gap-4">
          <div>
            <div className="mb-1 text-xs font-medium">Autonomy</div>
            <OptionCards label="Autonomy" options={AUTONOMY_OPTIONS.filter((o) => o.value !== 'trusted')} value={setup.autonomy} onChange={(v) => set({ autonomy: v })} />
          </div>
          <div>
            <div className="mb-1 text-xs font-medium">Changes in {mailboxName(kind)}</div>
            <OptionCards label="Changes in Gmail" options={PROJECTION_OPTIONS} value={setup.projection} onChange={(v) => set({ projection: v })} />
          </div>
          <div>
            <div className="mb-1 text-xs font-medium">Privacy</div>
            <OptionCards label="Privacy" options={PRIVACY_OPTIONS} value={setup.privacy} onChange={(v) => set({ privacy: v })} />
          </div>
          <p className="rounded-md bg-sand/70 px-3 py-2 text-xs text-ink-2">
            <span className="font-medium text-ink">With these settings: </span>
            {effect.sentence}
          </p>
        </div>
      </details>
    </div>
  )
}

/**
 * The sign-in step: the shared handshake (`useGmailSignIn`), then the new
 * account is handed on once the connected event has refreshed the account
 * list (events.ts invalidates it) and the account is in it.
 */
function SignIn({ setup, onConnected }: { setup: Setup; onConnected: (dig: Dig) => void }) {
  const { state, start, reset } = useGmailSignIn()
  const digs = useAllDigs()
  const done = useRef(false)

  useEffect(() => {
    if (state.phase !== 'connected' || done.current) return
    const dig = digs.data?.find((d) => d.id === state.digId)
    if (dig) {
      done.current = true
      onConnected(dig)
    }
  }, [digs.data, state, onConnected])

  return (
    <div className="flex flex-col items-start gap-3">
      {state.phase === 'idle' ? (
        <>
          <p className="text-ink-2">Your browser opens Google's sign-in. Choose the account, approve access, and come back here. Riddle asks only to read and label mail (gmail.modify).</p>
          <Button
            variant="primary"
            icon={<Mail />}
            onClick={() => {
              done.current = false
              void start({ name: setup.name.trim() || 'Gmail', group: setup.group, privacy: setup.privacy, projection: setup.projection })
            }}
          >
            Sign in with Google
          </Button>
        </>
      ) : null}
      <SignInProgress state={state} onRetry={reset} />
    </div>
  )
}

/**
 * Done, with one optional thing left: a judge key. Without one Riddle
 * sorts by rules and hides nothing, which the step says in the same words
 * as the account's status; the key goes to the keychain through
 * `set_secret` and the judge picks it up at once.
 */
function Done({ dig, setup, settings }: { dig: Dig; setup: Setup; settings: Settings }) {
  const effect = gmailEffect(setup.autonomy, setup.projection, { provider: dig.provider })
  const noJudge = judgeStateLine(settings.judge)
  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-start gap-3 rounded-md border border-moss/40 bg-moss/5 p-4">
        <CheckCircle2 className="mt-0.5 size-5 text-moss" />
        <div>
          <div className="text-base font-semibold">{dig.name} is connected</div>
          <div className="text-sm text-ink-2 selectable">{dig.email}</div>
        </div>
      </div>
      <ul className="flex flex-col gap-1 text-sm text-ink-2">
        <li>The first sync has started; it continues in the background. Progress shows on the account and in the Task Centre.</li>
        <li>{effect.sentence}</li>
      </ul>
      {noJudge ? (
        <div className="rounded-md border border-line bg-sand/50 px-3 py-2 text-sm">
          <div className="text-ink-2">{noJudge}.</div>
          <SecretField secret="jev_api_key" label="Paste a judge key (optional)" hint="Stored in the OS keychain, never in a file or a log. The judge starts at once, and new accounts then start in your default privacy mode." present={settings.judge.apiKeyPresent} />
        </div>
      ) : null}
    </div>
  )
}

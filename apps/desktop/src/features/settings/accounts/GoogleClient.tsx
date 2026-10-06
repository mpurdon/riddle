/**
 * The Google OAuth client every Gmail account signs in through: one per
 * app, not per account. Import the JSON Google Auth Platform downloads, or
 * enter the id and secret by hand; the id goes to settings, the secret to
 * the keychain. The console steps are shown here, in the app, because the
 * old pointer to `docs/GMAIL_SETUP.md` sent a user to a file in a
 * repository they did not have.
 *
 * The sign-in handshake lives here too (`useGmailSignIn`, `SignInProgress`)
 * because Add account and Sign in again are the same exchange:
 * `connect_gmail` answers at once with the account's id and the consent
 * URL, the browser does the rest, and the backend reports back with a
 * `gmail_sign_in` event for that id. Before this each screen matched the
 * toast's title ("Gmail connected") and the wizard polled list_digs to
 * notice the account had appeared.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { CheckCircle2, ExternalLink, Loader2, Upload } from 'lucide-react'
import { onEvent } from '@/lib/events'
import { cx } from '@/lib/format'
import { openExternal, toRiddleError } from '@/lib/ipc'
import { useAction } from '@/lib/useAction'
import { toast } from '@/lib/stores'
import { Button } from '@/components/ui/Button'
import { Field, Input } from '@/components/ui/Field'
import { useDraft } from '../shared'

export const GOOGLE_CONSOLE_URL = 'https://console.cloud.google.com/apis/credentials'

/** docs/GMAIL_SETUP.md, "In the Google Cloud console", in six steps. */
const CONSOLE_STEPS: { title: string; detail: string }[] = [
  { title: 'Create or pick a project', detail: 'Project picker → New project. Any name; no billing account is needed.' },
  { title: 'Enable the Gmail API', detail: 'APIs & Services → Library → search "Gmail API" → Enable. While it is off, every sign-in fails with a bare 403.' },
  { title: 'Set the app name and audience', detail: 'Google Auth platform → Branding: an app name and support email. Audience: Internal for a Workspace domain you administer; otherwise External, and add yourself under Test users.' },
  { title: 'Add the one scope', detail: 'Data Access → Add or remove scopes → https://www.googleapis.com/auth/gmail.modify. Riddle asks for nothing else.' },
  { title: 'Create a Desktop app client', detail: 'Clients → Create client → Application type: Desktop app → name it → Create. No redirect URIs are needed.' },
  { title: 'Download the client JSON', detail: 'Download JSON from the dialog after Create, or from the client\'s row. Import it here, then delete the file.' },
]

/**
 * The console steps with a Done box per step, kept in local state: the
 * boxes are a place to keep your finger, not a record.
 */
export function GoogleConsoleSteps({ defaultOpen = false }: { defaultOpen?: boolean }) {
  const [done, setDone] = useState<boolean[]>(() => CONSOLE_STEPS.map(() => false))
  const n = done.filter(Boolean).length
  return (
    <details open={defaultOpen} className="mt-2 max-w-2xl rounded-md border border-line bg-sand/50 p-3 text-xs">
      <summary className="cursor-pointer select-none text-sm font-medium text-ink">
        Set up the client in Google Cloud <span className="font-normal text-clay">({n} of {CONSOLE_STEPS.length} done)</span>
      </summary>
      <p className="mt-2 text-ink-2">Once per app, not per account. Google reorganised this console in 2025; if a label has moved, the shape of each step is still the one below.</p>
      <ol className="mt-2 flex flex-col gap-1.5">
        {CONSOLE_STEPS.map((s, i) => (
          <li key={s.title} className="flex items-start gap-2">
            <label className="flex cursor-pointer items-start gap-2">
              <input type="checkbox" className="mt-0.5" checked={done[i]} onChange={(e) => setDone(done.map((d, j) => (j === i ? e.target.checked : d)))} aria-label={`Done: ${s.title}`} />
              <span className={done[i] ? 'text-clay line-through' : ''}>
                <span className="font-medium text-ink">
                  {i + 1}. {s.title}.
                </span>{' '}
                <span className="selectable">{s.detail}</span>
              </span>
            </label>
          </li>
        ))}
      </ol>
      <div className="mt-3">
        <Button size="sm" icon={<ExternalLink />} onClick={() => openExternal(GOOGLE_CONSOLE_URL)}>
          Open Google Cloud console
        </Button>
      </div>
    </details>
  )
}

/**
 * Google Auth Platform offers the client as a download
 * (`client_secret_<id>.json`). Importing it fills both fields below in one
 * step; the file is read here and parsed by the `import_gmail_client`
 * Action, which keeps the secret out of settings.toml.
 */
type ImportResult = { clientId: string; projectId: string | null; clientSecretStored: boolean; keychainService: string; fileName: string; at: Date }

export function ImportGmailClient() {
  const { run } = useAction()
  const input = useRef<HTMLInputElement>(null)
  const [done, setDone] = useState<ImportResult | null>(null)
  return (
    <div className="mt-3 max-w-2xl">
      <div className="flex items-center gap-2">
        <input
          ref={input}
          type="file"
          accept="application/json,.json"
          className="hidden"
          aria-label="Google OAuth client file"
          onChange={async (e) => {
            const file = e.target.files?.[0]
            e.target.value = ''
            if (!file) return
            setDone(null)
            const out = await run<Omit<ImportResult, 'fileName' | 'at'>>('import_gmail_client', { text: await file.text() }, { successToast: false })
            if (!out?.result) return
            const r = { ...out.result, fileName: file.name, at: new Date() }
            setDone(r)
            toast('Gmail client imported', {
              kind: 'success',
              body: r.clientSecretStored ? 'Client id saved to settings; client secret stored in the keychain.' : 'Client id saved to settings. The file had no client secret.',
            })
          }}
        />
        <Button size="sm" icon={<Upload />} onClick={() => input.current?.click()}>
          Import from Google's JSON…
        </Button>
      </div>
      {done ? (
        <div role="status" className="mt-2 rounded-md border border-moss/40 bg-moss/5 p-3 text-xs">
          <div className="flex items-center gap-2 text-sm font-medium text-ink">
            <CheckCircle2 className="size-4 text-moss" />
            Imported {done.fileName}
            <span className="text-xs font-normal text-clay">at {done.at.toLocaleTimeString()}</span>
            <span className="flex-1" />
            <button type="button" className="text-clay hover:text-ink" aria-label="Dismiss" onClick={() => setDone(null)}>
              ×
            </button>
          </div>
          <ul className="mt-2 flex flex-col gap-1 text-ink-2">
            {done.projectId ? <li>Google Cloud project: <span className="mono">{done.projectId}</span></li> : null}
            <li>
              <span className="text-moss">✓</span> Client id saved to settings: <span className="mono selectable">{done.clientId}</span>
            </li>
            <li>
              {done.clientSecretStored ? (
                <>
                  <span className="text-moss">✓</span> Client secret stored in the OS keychain (service <span className="mono">{done.keychainService}</span>, item <span className="mono">gmail_client_secret</span>). Riddle never shows it.
                </>
              ) : (
                <>The file had no client secret; any secret stored before was removed.</>
              )}
            </li>
            <li>Both apply now; no restart. Gmail accounts can sign in through it. You can delete the downloaded file.</li>
          </ul>
        </div>
      ) : null}
    </div>
  )
}

/**
 * The client id identifies Riddle to Google and is not a secret, so it lives
 * in settings.toml like any other setting. It had no field at first, which
 * left the setup guide pointing at a place that did not exist.
 */
export function GmailClientId({ value, onSave }: { value: string; onSave: (clientId: string) => void }) {
  const [draft, setDraft] = useDraft(value)
  const trimmed = draft.trim()
  const reason = trimmed === value ? 'No changes' : trimmed && !trimmed.endsWith('.apps.googleusercontent.com') ? 'A client id ends in .apps.googleusercontent.com' : null
  return (
    <div className="mt-3 max-w-2xl">
      <div className="flex items-end gap-2">
        <Field label="Gmail OAuth client id" className="flex-1">
          <Input value={draft} onChange={(e) => setDraft(e.target.value)} placeholder="1234567890-abc.apps.googleusercontent.com" className="mono" aria-label="Gmail OAuth client id" />
        </Field>
        <Button disabledReason={reason} onClick={() => onSave(trimmed)}>
          Save
        </Button>
      </div>
      <p className="mt-1 text-xs text-clay">From Google Auth platform → Clients (a Desktop app client), the steps above.</p>
    </div>
  )
}

export type SignInState = { phase: 'idle' } | { phase: 'waiting'; digId: string; authUrl: string | null } | { phase: 'connected'; digId: string } | { phase: 'failed'; message: string }

const SIGN_IN_TIMEOUT_MS = 5 * 60_000
const TIMED_OUT = 'Google did not answer within five minutes. Start the sign-in again.'

/**
 * Runs `connect_gmail` (for a new account, or again for `digId`) and waits
 * for the `gmail_sign_in` event that names the same account: connected, or
 * failed with Google's reason. A null consent URL (the browser mock) means
 * there is nothing to open. Five minutes without an answer is a failure
 * with a way to start again.
 */
export function useGmailSignIn({ digId }: { digId?: string } = {}) {
  const { run } = useAction()
  const [state, setState] = useState<SignInState>({ phase: 'idle' })
  useEffect(() => {
    if (state.phase !== 'waiting') return
    const id = state.digId
    const off = onEvent((e) => {
      if (e.type !== 'gmail_sign_in' || e.dig_id !== id) return
      setState(e.connected ? { phase: 'connected', digId: id } : { phase: 'failed', message: e.message ?? 'Google did not complete the sign-in.' })
    })
    const t = setTimeout(() => setState({ phase: 'failed', message: TIMED_OUT }), SIGN_IN_TIMEOUT_MS)
    return () => {
      off()
      clearTimeout(t)
    }
  }, [state])
  const start = useCallback(
    async (args: Record<string, unknown> = {}) => {
      try {
        const out = await run<{ digId: string; authUrl: string | null }>('connect_gmail', digId ? { ...args, digId } : args, { successToast: false })
        if (!out?.result) return
        setState({ phase: 'waiting', digId: out.result.digId, authUrl: out.result.authUrl })
        if (out.result.authUrl) openExternal(out.result.authUrl)
      } catch (e) {
        setState({ phase: 'failed', message: toRiddleError(e).message })
      }
    },
    [run, digId],
  )
  const reset = useCallback(() => setState({ phase: 'idle' }), [])
  return { state, start, reset }
}

/** The waiting and failed panels of a sign-in; nothing while idle. `onCancel` adds a Cancel to the waiting panel. */
export function SignInProgress({ state, onRetry, onCancel, className }: { state: SignInState; onRetry: () => void; onCancel?: () => void; className?: string }) {
  if (state.phase === 'waiting' || state.phase === 'connected') {
    const authUrl = state.phase === 'waiting' ? state.authUrl : null
    return (
      <div role="status" className={cx('flex w-full items-start gap-3 rounded-md border border-line bg-sand/60 p-3 text-sm', className)}>
        <Loader2 className="mt-0.5 size-4 animate-spin text-ochre" />
        <div className="flex-1">
          <div className="font-medium">Waiting for Google…</div>
          <div className="mt-0.5 text-xs text-ink-2">Finish the sign-in in your browser. This window updates by itself.</div>
          {authUrl || onCancel ? (
            <div className="mt-1 flex items-center gap-3">
              {authUrl ? (
                <Button size="sm" variant="link" onClick={() => openExternal(authUrl)}>
                  Open the sign-in page again
                </Button>
              ) : null}
              {onCancel ? (
                <Button size="sm" variant="ghost" onClick={onCancel}>
                  Cancel
                </Button>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>
    )
  }
  if (state.phase === 'failed') {
    return (
      <div role="alert" className={cx('flex w-full flex-col items-start gap-2 rounded-md border border-rust/30 bg-rust/5 p-3 text-sm', className)}>
        <div className="font-medium text-rust">Sign-in did not complete</div>
        <div className="text-xs text-ink-2">{state.message}</div>
        <Button size="sm" onClick={onRetry}>
          Try again
        </Button>
      </div>
    )
  }
  return null
}

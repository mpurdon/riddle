/**
 * Pieces every Settings tab uses: the section frame, a number field that
 * saves on commit, the keychain field, the probe button and a local draft.
 */
import { useEffect, useState } from 'react'
import { KeyRound } from 'lucide-react'
import { toRiddleError } from '@/lib/ipc'
import type { ProbeResult } from '@/lib/types'
import { useAction } from '@/lib/useAction'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { Field, Input } from '@/components/ui/Field'

export function Section({ id, title, children, description }: { id: string; title: string; description?: string; children: React.ReactNode }) {
  return (
    <section id={id} className="scroll-mt-4 border-b border-line py-5">
      <h2 className="text-sm font-semibold">{title}</h2>
      {description ? <p className="mt-0.5 max-w-2xl text-xs text-ink-2">{description}</p> : null}
      <div className="mt-3">{children}</div>
    </section>
  )
}

/**
 * A number that saves when you leave the field or press Enter. Saving on
 * every keystroke wrote an Activity row per digit and briefly saved the
 * partial value ("7" on the way to "70", 0 for an emptied budget).
 */
export function NumberSetting({ value, onCommit, min, max, step, className, label }: { value: number; onCommit: (v: number) => void; min?: number; max?: number; step?: number; className?: string; label: string }) {
  const [draft, setDraft] = useDraft(String(value))
  const commit = () => {
    const n = Number(draft)
    const valid = draft.trim() !== '' && Number.isFinite(n) && (min === undefined || n >= min) && (max === undefined || n <= max)
    if (valid && n !== value) onCommit(n)
    else if (!valid) setDraft(String(value))
  }
  return (
    <Input
      type="number"
      min={min}
      max={max}
      step={step}
      value={draft}
      onChange={(e) => setDraft(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === 'Enter' && (e.target as HTMLInputElement).blur()}
      className={className}
      aria-label={label}
    />
  )
}

/**
 * A key written straight to the OS keychain through `set_secret`, which only
 * a person can run. The field is cleared after saving; the app only ever
 * learns whether a key is present.
 */
export function SecretField({ secret, label, hint, present }: { secret: 'jev_api_key' | 'gmail_client_secret'; label: string; hint: string; present: boolean }) {
  const { run } = useAction()
  const [value, setValue] = useState('')
  return (
    <div className="mt-3 max-w-2xl">
      <div className="flex items-end gap-2">
        <Field label={label} className="flex-1">
          <Input type="password" autoComplete="off" value={value} onChange={(e) => setValue(e.target.value)} placeholder={present ? 'Stored in the keychain — paste to replace' : 'Not set'} aria-label={label} />
        </Field>
        <Button
          icon={<KeyRound />}
          disabledReason={value.trim() ? null : 'Paste the key first'}
          onClick={async () => {
            const out = await run('set_secret', { secret, value: value.trim() })
            if (out) setValue('')
          }}
        >
          Store
        </Button>
        <Chip className="mb-1.5" tone={present ? 'ok' : undefined} title={present ? 'The value stays in the keychain; Riddle never shows it.' : undefined}>
          {present ? 'Stored in keychain' : 'Not stored'}
        </Chip>
        {present ? (
          <Button variant="ghost" onClick={() => run('clear_secret', { secret })}>
            Remove
          </Button>
        ) : null}
      </div>
      <p className="mt-1 text-xs text-clay">{hint}</p>
    </div>
  )
}

export function Probe({ probe, disabledReason }: { probe: () => Promise<ProbeResult>; disabledReason: string | null }) {
  const [result, setResult] = useState<ProbeResult | null>(null)
  const [busy, setBusy] = useState(false)
  return (
    <div className="flex items-center gap-2">
      <Button size="sm" busy={busy} onClick={async () => { setBusy(true); try { setResult(await probe()) } catch (e) { setResult({ ok: false, latencyMs: 0, message: toRiddleError(e).message }) } finally { setBusy(false) } }} disabledReason={disabledReason}>
        Probe
      </Button>
      {result ? (
        <span className={`text-xs ${result.ok ? 'text-moss' : result.notSetUp ? 'text-ochre' : 'text-rust'}`}>
          {result.ok ? 'OK' : result.notSetUp ? 'Not set up' : 'Failed'} · {result.latencyMs}ms · {result.message}
        </span>
      ) : null}
    </div>
  )
}

/** Edits held locally and saved together, so a half-typed model id never runs. */
export function useDraft<T>(value: T) {
  const [draft, setDraft] = useState(value)
  useEffect(() => setDraft(value), [value])
  const dirty = JSON.stringify(draft) !== JSON.stringify(value)
  return [draft, setDraft, dirty] as const
}

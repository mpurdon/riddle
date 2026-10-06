/**
 * First-run tips, pulled by the page you open rather than pushed as a tour.
 * The three-step tour walked a new user through Finds, Spoil and Meshes
 * before any account existed, so every page it pointed at was empty, and it
 * promised a daily digest that does not exist. Now each of Finds, Unresolved
 * and the Director shows one tip the first time you arrive, once, dismissed
 * with Esc or ×, and Settings → General can bring them back.
 *
 * The export is still named `Tour`, with the same props, so the Shell that
 * mounts it needs no change; the key `riddle.tour.v1` is kept so an install
 * that had finished the old tour ("done") sees no tips either. What was
 * seen is a per-machine convenience in localStorage (`lib/local`): when it
 * cannot be read, the tips show.
 */
import { useEffect, useState, type ReactNode } from 'react'
import { Link, useLocation } from 'react-router-dom'
import { readLocal, removeLocal, writeLocal } from '@/lib/local'
import { Keys } from './ui/Kbd'

export const TOUR_KEY = 'riddle.tour.v1'

export type TipId = 'finds' | 'unresolved' | 'director'

interface Tip {
  id: TipId
  /** The route prefix the tip belongs to. */
  route: string
  title: string
  body: ReactNode
}

export const TIPS: Tip[] = [
  {
    id: 'finds',
    route: '/finds',
    title: 'Finds',
    body: (
      <>
        A Find is something that needs you: a reply, a decision, a payment, a date. Riddle shows only those, each with a reason. From the keyboard: <Keys keys="e" /> resolve, <Keys keys="s" /> snooze, <Keys keys="d" /> not important, <Keys keys="w" /> why.
      </>
    ),
  },
  {
    id: 'unresolved',
    route: '/unresolved',
    title: 'Unresolved',
    body: <>Riddle was not sure about these, so it hid nothing. Deciding them here teaches it: each choice becomes an example for mail like it.</>,
  },
  {
    id: 'director',
    route: '/director',
    title: 'Director',
    body: (
      <>
        Ask the Director about your mail or for a rule. What it proposes waits for your approval; nothing changes until you apply it. <Keys keys="enter" /> sends; <Keys keys="shift+enter" /> adds a line.
      </>
    ),
  },
]

function readSeen(): Set<TipId> {
  const raw = readLocal(TOUR_KEY)
  if (!raw) return new Set()
  if (raw === 'done') return new Set(TIPS.map((t) => t.id))
  return new Set(raw.split(',').filter((x): x is TipId => TIPS.some((t) => t.id === x)))
}

function markSeen(id: TipId): void {
  const seen = readSeen()
  seen.add(id)
  writeLocal(TOUR_KEY, seen.size === TIPS.length ? 'done' : [...seen].join(','))
}

/** True once every tip has been dismissed. */
export function tourSeen(): boolean {
  return readSeen().size === TIPS.length
}

export function resetTour(): void {
  removeLocal(TOUR_KEY)
}

export function tipFor(pathname: string): Tip | undefined {
  return TIPS.find((t) => pathname === t.route || pathname.startsWith(`${t.route}/`))
}

/** Mounted once in the Shell. `force` shows the current page's tip even if it was dismissed. */
export function Tour({ force = false, onDone }: { force?: boolean; onDone?: () => void }) {
  const { pathname } = useLocation()
  const tip = tipFor(pathname)
  const [dismissed, setDismissed] = useState<TipId | null>(null)
  const visible = !!tip && dismissed !== tip.id && (force || !readSeen().has(tip.id))
  useEffect(() => {
    if (!visible || !tip) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') dismiss()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [visible, tip?.id]) // eslint-disable-line react-hooks/exhaustive-deps
  if (!visible || !tip) return null
  const dismiss = () => {
    markSeen(tip.id)
    setDismissed(tip.id)
    onDone?.()
  }
  return (
    <div className="fixed bottom-6 left-1/2 z-50 w-[480px] max-w-[92vw] -translate-x-1/2 rounded-lg border border-ochre/40 bg-paper p-4 shadow-xl fade-in" role="dialog" aria-label={`Tip: ${tip.title}`}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0">
          <div className="text-xs uppercase tracking-wide text-clay">Tip</div>
          <h2 className="mt-0.5 text-base font-semibold">{tip.title}</h2>
        </div>
        <button type="button" className="rounded p-1 text-clay hover:bg-sand hover:text-ink" aria-label="Dismiss tip (Esc)" title="Dismiss (Esc)" onClick={dismiss}>
          ×
        </button>
      </div>
      <p className="mt-1 text-sm text-ink-2">{tip.body}</p>
      <p className="mt-2 text-xs text-clay">
        Shown once.{' '}
        <Link to="/settings#app" className="underline hover:text-ink" onClick={dismiss}>
          Settings → General
        </Link>{' '}
        brings the tips back.
      </p>
    </div>
  )
}

/**
 * The first-run tour: three steps (Finds, Spoil, Meshes), once, stored in
 * localStorage, never again unless asked from Settings.
 */
import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Button } from './ui/Button'

export const TOUR_KEY = 'riddle.tour.v1'

const STEPS = [
  {
    title: 'Finds',
    body: 'Riddle does not show you an inbox of 847 unread messages. It shows Finds: the few things that deserve attention, each with a Why? you can inspect. Resolve, snooze or dismiss them from the keyboard.',
    route: '/finds',
  },
  {
    title: 'Spoil',
    body: 'Low-value mail goes to Spoil. It is not deleted: it stays searchable and reviewable, and a daily digest catches anything misfiled. "Not Spoil" teaches Riddle; "Never hide" makes a sender exempt for good.',
    route: '/spoil',
  },
  {
    title: 'Meshes',
    body: 'Meshes are the policy. Each one is versioned text you can read and edit; every change can be previewed against your history before it takes effect, and a new Mesh only suggests until it has earned the right to act.',
    route: '/meshes',
  },
]

export function tourSeen(): boolean {
  try {
    return localStorage.getItem(TOUR_KEY) === 'done'
  } catch {
    return true
  }
}

export function resetTour(): void {
  try {
    localStorage.removeItem(TOUR_KEY)
  } catch {
    /* ignore */
  }
}

export function Tour({ force = false, onDone }: { force?: boolean; onDone?: () => void }) {
  const [step, setStep] = useState<number | null>(null)
  const nav = useNavigate()
  useEffect(() => {
    if (force || !tourSeen()) setStep(0)
  }, [force])
  useEffect(() => {
    if (step === null) return
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') finish()
    }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [step]) // eslint-disable-line react-hooks/exhaustive-deps
  if (step === null) return null
  const s = STEPS[step]
  const finish = () => {
    try {
      localStorage.setItem(TOUR_KEY, 'done')
    } catch {
      /* ignore */
    }
    setStep(null)
    onDone?.()
  }
  return (
    <div className="fixed bottom-6 left-1/2 z-50 w-[520px] max-w-[92vw] -translate-x-1/2 rounded-lg border border-ochre/40 bg-paper p-4 shadow-xl fade-in" role="dialog" aria-label={`Tour step ${step + 1} of ${STEPS.length}: ${s.title}`}>
      <div className="flex items-center justify-between">
        <div className="text-xs uppercase tracking-wide text-clay">
          Tour · {step + 1} of {STEPS.length}
        </div>
        <button type="button" className="text-xs text-clay hover:text-ink" onClick={finish}>
          Skip (Esc)
        </button>
      </div>
      <h2 className="mt-1 text-base font-semibold">{s.title}</h2>
      <p className="mt-1 text-sm text-ink-2">{s.body}</p>
      <div className="mt-3 flex justify-between">
        <Button variant="ghost" size="sm" onClick={() => nav(s.route)}>
          Show me
        </Button>
        <div className="flex gap-2">
          {step > 0 ? (
            <Button size="sm" onClick={() => setStep(step - 1)}>
              Back
            </Button>
          ) : null}
          {step < STEPS.length - 1 ? (
            <Button size="sm" variant="primary" onClick={() => setStep(step + 1)}>
              Next
            </Button>
          ) : (
            <Button size="sm" variant="primary" onClick={finish}>
              Done
            </Button>
          )}
        </div>
      </div>
    </div>
  )
}

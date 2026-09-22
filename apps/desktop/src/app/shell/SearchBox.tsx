/**
 * Ask Riddle: a plain query goes to search; a question goes to the Director.
 */
import { useEffect, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Search } from 'lucide-react'
import { useDirector } from '@/lib/stores'

export function looksLikeQuestion(q: string): boolean {
  const s = q.trim()
  if (!s) return false
  if (s.endsWith('?')) return true
  return /^(what|why|who|when|where|which|how|did|do|does|is|are|can|could|should|find (the|my|all|every)|show me|draft|summari[sz]e|tell me|go through)\b/i.test(s)
}

export function SearchBox() {
  const [q, setQ] = useState('')
  const ref = useRef<HTMLInputElement>(null)
  const nav = useNavigate()
  const setPrefill = useDirector((s) => s.setPrefill)
  useEffect(() => {
    const onFocus = () => ref.current?.focus()
    window.addEventListener('riddle:focus-search', onFocus)
    return () => window.removeEventListener('riddle:focus-search', onFocus)
  }, [])
  const question = looksLikeQuestion(q)
  return (
    <form
      className="relative w-[380px] max-w-[40vw]"
      onSubmit={(e) => {
        e.preventDefault()
        const s = q.trim()
        if (!s) return
        if (question) {
          setPrefill(s)
          nav('/director?send=1')
        } else nav(`/inbox?q=${encodeURIComponent(s)}`)
        setQ('')
        ref.current?.blur()
      }}
    >
      <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-clay" />
      <input
        ref={ref}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        onKeyDown={(e) => e.key === 'Escape' && (e.currentTarget.blur(), setQ(''))}
        placeholder="Ask Riddle: search, or a question for the Director"
        aria-label="Ask Riddle"
        className="h-7 w-full rounded-md border border-line bg-paper pl-7 pr-16 text-sm placeholder:text-clay focus:border-ochre focus:outline-none"
      />
      {q.trim() ? <span className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2 text-[11px] text-clay">{question ? 'Director ↵' : 'Search ↵'}</span> : <kbd className="pointer-events-none absolute right-2 top-1/2 -translate-y-1/2">/</kbd>}
    </form>
  )
}

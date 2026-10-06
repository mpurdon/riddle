/**
 * Ask Riddle. Anything with a search operator, or plain words, searches;
 * a question goes to the Director, but only as a prefilled draft the user
 * sends with Enter there. The earlier version sent questions at once and treated
 * `is:unread` as a question because it starts with "is", which cost a
 * model call and landed the user somewhere they had not asked to go. A
 * visible Search · Ask toggle lets the user say which they meant.
 */
import { useEffect, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Search } from 'lucide-react'
import { cx } from '@/lib/format'
import { useDirector } from '@/lib/stores'

const OPERATOR = /(^|\s)(from|to|cc|is|in|has|label|stratum|subject|before|after|since|until|newer_than|older_than):\S/i

/** True when the query carries a search operator such as `from:` or `is:unread`. */
export function hasOperator(q: string): boolean {
  return OPERATOR.test(q)
}

/** A question for the Director: a trailing ? or a leading "ask", never an operator query. */
export function looksLikeQuestion(q: string): boolean {
  const s = q.trim()
  if (!s || hasOperator(s)) return false
  if (s.endsWith('?')) return true
  return /^(ask|why|how|should|could|summari[sz]e|draft|tell me|go through)\b/i.test(s)
}

type Mode = 'auto' | 'search' | 'ask'

export function SearchBox() {
  const [q, setQ] = useState('')
  const [mode, setMode] = useState<Mode>('auto')
  const ref = useRef<HTMLInputElement>(null)
  const nav = useNavigate()
  const setPrefill = useDirector((s) => s.setPrefill)
  useEffect(() => {
    const onFocus = () => ref.current?.focus()
    window.addEventListener('riddle:focus-search', onFocus)
    return () => window.removeEventListener('riddle:focus-search', onFocus)
  }, [])
  const ask = mode === 'ask' || (mode === 'auto' && looksLikeQuestion(q))
  return (
    <form
      className="relative w-[420px] max-w-[42vw]"
      onSubmit={(e) => {
        e.preventDefault()
        const s = q.trim()
        if (!s) return
        if (ask) {
          setPrefill(s.replace(/^ask\s+/i, ''))
          nav('/director')
        } else nav(`/inbox?q=${encodeURIComponent(s)}`)
        setQ('')
        setMode('auto')
        ref.current?.blur()
      }}
    >
      <Search className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-clay" />
      <input
        ref={ref}
        value={q}
        onChange={(e) => setQ(e.target.value)}
        onKeyDown={(e) => e.key === 'Escape' && (e.currentTarget.blur(), setQ(''), setMode('auto'))}
        placeholder="Search your mail, or ask the Director"
        aria-label="Ask Riddle"
        title="Search: words, or from:, is:, stratum:, label:. Ask: end with ? or start with “ask”; the Director gets a draft you send yourself."
        className="h-7 w-full rounded-md border border-line bg-paper pl-7 pr-[7.5rem] text-sm placeholder:text-clay focus:border-ochre focus:outline-none"
      />
      <div className="absolute right-1 top-1/2 flex -translate-y-1/2 items-center gap-1">
        {q.trim() ? (
          <div className="inline-flex rounded border border-line bg-sand p-px text-2xs" role="radiogroup" aria-label="Search or ask">
            <button
              type="button"
              role="radio"
              aria-checked={!ask}
              className={cx('rounded px-1.5 py-px', !ask ? 'bg-paper font-medium text-ink' : 'text-clay hover:text-ink')}
              onClick={() => setMode('search')}
              title="Search your mail for these words (↵)"
            >
              Search
            </button>
            <button
              type="button"
              role="radio"
              aria-checked={ask}
              className={cx('rounded px-1.5 py-px', ask ? 'bg-paper font-medium text-ink' : 'text-clay hover:text-ink')}
              onClick={() => setMode('ask')}
              title="Draft this for the Director; nothing is sent until you press Enter there (↵)"
            >
              Ask
            </button>
          </div>
        ) : (
          <kbd className="pointer-events-none">/</kbd>
        )}
      </div>
    </form>
  )
}

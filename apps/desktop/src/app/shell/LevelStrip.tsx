/**
 * The strip above the page at Essentials (`lib/level`). Two jobs. Where one
 * rail entry folds sibling pages together (Needs you: Finds and Unresolved;
 * Mail: Inbox and Priority; Set aside: Spoil and Contamination), it is the
 * tab bar between them, so Unresolved and Contamination stay one click
 * away rather than hidden behind a shorter rail. Where a page the rail does
 * not list has been opened anyway, by a Director link, a shortcut or a
 * pasted route, it says so with an Advanced chip and offers the rest, so
 * the page is never a dead end and never a surprise. At Everything and
 * Developer it renders nothing: the rail lists every page.
 *
 * It lives in the Shell rather than in each page's header because the
 * pages are lazy and their headers are their own; one strip, one rule.
 */
import { NavLink, useLocation } from 'react-router-dom'
import { cx, num } from '@/lib/format'
import { isAdvancedFor, siblingsFor, useLevel, useSetLevel } from '@/lib/level'
import { useSummary } from '@/lib/queries'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'

const under = (pathname: string, prefix: string) => pathname === prefix || pathname.startsWith(`${prefix}/`)

export function LevelStrip() {
  const level = useLevel()
  const { pathname } = useLocation()
  const setLevel = useSetLevel()
  const summary = useSummary()
  if (level !== 'essentials') return null
  const group = siblingsFor(pathname)
  if (group) {
    const s = summary.data
    const counts: Record<string, number | null> = { '/finds': s ? num(s.findsOpen) : null, '/unresolved': s ? num(s.unresolved) : null }
    return (
      <div role="tablist" aria-label={group.label} className="flex shrink-0 items-center gap-1 border-b border-line bg-sand/40 px-3">
        {group.tabs.map((t) => {
          const on = under(pathname, t.to)
          const count = counts[t.to]
          return (
            <NavLink key={t.to} to={t.to} role="tab" aria-selected={on} className={cx('-mb-px flex items-center gap-1.5 border-b-2 px-2.5 py-1.5 text-sm', on ? 'border-ochre text-ink' : 'border-transparent text-ink-2 hover:text-ink')}>
              {t.label}
              {count ? <span className={cx('rounded px-1 text-2xs font-medium', t.to === '/unresolved' ? 'bg-ochre/15 text-ochre' : 'bg-moss/15 text-moss')}>{count}</span> : null}
            </NavLink>
          )
        })}
      </div>
    )
  }
  if (isAdvancedFor(pathname, level)) {
    return (
      <div role="note" aria-label="Advanced page" className="flex shrink-0 flex-wrap items-center gap-2 border-b border-line bg-sand/40 px-3 py-1.5 text-xs text-ink-2">
        <Chip tone="warn">Advanced</Chip>
        <span>This page is part of Everything. It works as usual; the rail just does not list it yet.</span>
        <Button size="sm" variant="ghost" onClick={() => void setLevel('everything')} title="Rules, packs, topics, labels, expeditions and Activity in the rail. Nothing changes in your mail.">
          Show everything
        </Button>
      </div>
    )
  }
  return null
}

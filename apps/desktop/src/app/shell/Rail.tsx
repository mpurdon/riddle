/**
 * The rail, by level (`lib/level`). Groups answer the questions a new user
 * asks: where do I look (Summary, Finds, Unresolved), where is the mail
 * (Inbox, Spoil, Contamination), where are the rules (Meshes, Strata;
 * Signals at Developer, since its page speaks of calibration and Platt
 * scaling), and what is the Director doing. The old "Excavation" heading
 * put review trays, taxonomies, policy editors and the audit log under one
 * word nothing else in the app used. Every item carries a one-line gloss
 * and its `g` chord as a tooltip, so the vocabulary and the keys are learnt
 * where they are used rather than from the help sheet alone.
 *
 * At Essentials the rail is five entries: Today, Needs you (Finds and
 * Unresolved as tabs, because Unresolved is a disposition and never
 * hidden), Mail (the Inbox, with Priority as a tab), Set aside (Spoil and
 * Contamination, because hidden mail must stay recoverable) and Settings,
 * plus the Director when an LLM is set (without one its page is empty).
 * Sixteen entries had met someone who wanted to read their mail (17 Oct
 * 2026). Nothing is removed: every route stays registered, and the footer
 * offers the rest with one click.
 */
import { NavLink, useLocation } from 'react-router-dom'
import { Activity, AlertTriangle, Archive, ClipboardCheck, Compass, FlaskConical, Funnel, Inbox, Layers, Lightbulb, ListChecks, MessageSquare, Package, Radio, Settings, Star, Sunrise, Tag, Wrench } from 'lucide-react'
import { cx, num } from '@/lib/format'
import { DEFAULT_LEVEL, useLlmConfigured, useSetLevel, type UiLevel } from '@/lib/level'
import { useSettings, useSummary, useDevInfo, usePending } from '@/lib/queries'
import { keyHint } from '@/lib/shortcuts'

interface Item {
  to: string
  label: string
  icon: React.ReactNode
  /** One line that says what the word means, in the user's terms. */
  gloss: string
  /** The shortcut id in the table, for the tooltip. */
  key?: string
  count?: number | null
  tone?: 'ok' | 'warn'
  /** Routes besides `to` that light this entry: the tabs an Essentials entry folds together. */
  also?: string[]
}

interface Group {
  title: string | null
  items: Item[]
}

const under = (pathname: string, prefix: string) => (prefix === '/' ? pathname === '/' : pathname === prefix || pathname.startsWith(`${prefix}/`))

export function Rail() {
  const settings = useSettings()
  const summary = useSummary()
  const dev = useDevInfo()
  const pending = usePending()
  const llm = useLlmConfigured()
  const setLevel = useSetLevel()
  const { pathname } = useLocation()
  const proposals = pending.data?.length ?? 0
  // The rail waits for the setting rather than drawing one set and then
  // the other; after a failed read it shows Everything, since nothing may
  // be unreachable because a query failed.
  const level: UiLevel | null = settings.data?.uiLevel ?? (settings.error ? DEFAULT_LEVEL : null)
  const s = summary.data
  const findsOpen = s ? num(s.findsOpen) : null

  const director: Item = { to: '/director', label: 'Director', icon: <MessageSquare />, gloss: 'Ask about your mail, or let it propose a rule change. Proposals wait for your approval.', key: 'go_director', count: proposals || null, tone: 'warn' }

  const essentials: Group[] = [
    {
      title: null,
      items: [
        { to: '/', label: 'Today', icon: <Sunrise />, gloss: 'Since yesterday: what needs you, what Riddle filed, what it could not decide.', key: 'go_summary' },
        { to: '/finds', label: 'Needs you', icon: <Lightbulb />, gloss: 'Mail that needs something from you, each with why; and what Riddle could not decide, which is never hidden.', key: 'go_finds', count: findsOpen, tone: 'ok', also: ['/unresolved'] },
        { to: '/inbox', label: 'Mail', icon: <Inbox />, gloss: 'Everything, newest first, with what Riddle decided about each message. Priority is the mail you chose to see first.', key: 'go_inbox', also: ['/priority'] },
        { to: '/spoil', label: 'Set aside', icon: <Archive />, gloss: 'Low-value and untrusted mail, kept and searchable, never deleted. Restore anything with one key.', key: 'go_spoil', also: ['/contamination'] },
        ...(llm ? [director] : []),
        { to: '/settings', label: 'Settings', icon: <Settings />, gloss: 'Accounts, privacy, Never Hidden, what changed, and how much Riddle shows.', key: 'settings' },
      ],
    },
  ]

  const full = (developer: boolean): Group[] => [
    {
      title: null,
      items: [
        { to: '/', label: 'Summary', icon: <Sunrise />, gloss: 'Since yesterday: what needs you, what Riddle filed, what it could not decide.', key: 'go_summary' },
        { to: '/finds', label: 'Finds', icon: <Lightbulb />, gloss: 'Mail that needs something from you: respond, pay, decide, review. Each says why.', key: 'go_finds', count: findsOpen, tone: 'ok' },
        { to: '/unresolved', label: 'Unresolved', icon: <ListChecks />, gloss: 'Mail Riddle could not decide. Deciding it here is how Riddle learns.', key: 'go_unresolved', count: s ? num(s.unresolved) : null, tone: 'warn' },
      ],
    },
    {
      title: 'Mail',
      items: [
        { to: '/priority', label: 'Priority', icon: <Star />, gloss: 'The mail you chose to see first: starred, important, chosen Strata and high-priority Finds.' },
        { to: '/inbox', label: 'Inbox', icon: <Inbox />, gloss: 'Everything, newest first, with what Riddle decided about each message.', key: 'go_inbox' },
        { to: '/spoil', label: 'Spoil', icon: <Archive />, gloss: 'Low-value mail, kept and searchable, never deleted. Archived in Gmail only at Act.', key: 'go_spoil' },
        { to: '/contamination', label: 'Contamination', icon: <AlertTriangle />, gloss: "Untrusted mail: spam, phishing, lookalike senders. Kept out of the Director's view." },
      ],
    },
    {
      title: 'Rules',
      items: [
        { to: '/meshes', label: 'Meshes', icon: <Funnel />, gloss: 'The sieves: versioned rules that sort mail. Preview a change against your history before it applies.', key: 'go_meshes' },
        { to: '/packs', label: 'Packs', icon: <Package />, gloss: 'Ready-made rules for kinds of mail: GitHub, money, shopping. Install one and answer its questions.' },
        { to: '/check', label: 'Rule check', icon: <ClipboardCheck />, gloss: 'Grade a few decisions each rule made, and correct the wrong ones on the spot.' },
        { to: '/strata', label: 'Strata', icon: <Layers />, gloss: 'Topics such as Finance or Travel. A message can carry several.' },
        { to: '/labels', label: 'Labels', icon: <Tag />, gloss: "Each account's labels in Gmail, with counts. Open one to read that mail." },
        ...(developer ? [{ to: '/signals', label: 'Signals', icon: <Radio />, gloss: 'What the judge can tell about a message, versioned and calibrated. Developer only.' }] : []),
      ],
    },
    {
      title: 'Director',
      items: [director, { to: '/expeditions', label: 'Expeditions', icon: <Compass />, gloss: 'Longer tasks with a budget. Results become Finds.' }],
    },
    {
      title: null,
      items: [
        { to: '/activity', label: 'Activity', icon: <Activity />, gloss: 'Everything that changed, by whom and why, with Undo.', key: 'go_activity' },
        { to: '/settings', label: 'Settings', icon: <Settings />, gloss: 'Accounts, what Riddle may change in Gmail, the judge, mail rules, and how much Riddle shows.', key: 'settings' },
        ...(developer ? [{ to: '/developer', label: 'Developer', icon: <Wrench />, gloss: "Inspectors, traces, logs and the dev build's arming switch." }] : []),
      ],
    },
  ]

  const groups: Group[] = level === null ? [] : level === 'essentials' ? essentials : full(level === 'developer')
  return (
    <nav className="flex w-48 shrink-0 flex-col border-r border-line bg-sand/60 pt-1" aria-label="Sections">
      <div className="flex-1 overflow-auto px-2 pb-2">
        {groups.map((g, i) => (
          <div key={i} className={cx(i > 0 && 'mt-3')}>
            {g.title ? <div className="px-2 pb-1 text-2xs font-semibold uppercase tracking-wider text-clay">{g.title}</div> : null}
            {g.items.map((it) => {
              const hint = it.key ? keyHint(it.key) : ''
              // An entry that folds tabs together lights for any of them.
              const folded = it.also ? [it.to, ...it.also].some((p) => under(pathname, p)) : null
              return (
                <NavLink
                  key={it.to}
                  to={it.to}
                  end={it.to === '/'}
                  className={({ isActive }) => cx('flex h-7 items-center gap-2 rounded-md px-2 text-sm text-ink-2 hover:bg-sand-2 hover:text-ink', (folded ?? isActive) && 'bg-sand-2 text-ink font-medium')}
                  aria-label={it.count ? `${it.label}, ${it.count}` : it.label}
                  title={hint ? `${it.gloss} (${hint})` : it.gloss}
                >
                  <span className="[&>svg]:size-4 text-clay">{it.icon}</span>
                  <span className="flex-1 truncate">{it.label}</span>
                  {it.count ? <span className={cx('rounded px-1 text-2xs font-medium', it.tone === 'warn' ? 'bg-ochre/15 text-ochre' : 'bg-moss/15 text-moss')}>{it.count}</span> : null}
                </NavLink>
              )
            })}
          </div>
        ))}
      </div>
      {level === 'essentials' || level === 'everything' ? (
        <div className="border-t border-line px-2 py-1.5">
          {level === 'essentials' ? (
            <button type="button" className="w-full rounded-md px-2 py-1 text-left text-xs text-ochre hover:bg-sand-2" onClick={() => void setLevel('everything')} title="Rules, packs, topics, labels, expeditions and Activity in the rail. Nothing changes in your mail, and you can come back.">
              Show everything
            </button>
          ) : (
            <button type="button" className="w-full rounded-md px-2 py-1 text-left text-xs text-clay hover:bg-sand-2 hover:text-ink" onClick={() => void setLevel('essentials')} title="Back to the essentials: Today, Needs you, Mail, Set aside and Settings. Every page stays reachable by link.">
              Show less
            </button>
          )}
        </div>
      ) : null}
      <div className="border-t border-line px-3 py-2 text-2xs text-clay">
        {dev.data ? (
          <span className="mono">
            {dev.data.build.version} {dev.data.build.gitSha}
            {dev.data.build.dirty ? '-dirty' : ''}
          </span>
        ) : (
          '…'
        )}
        {dev.data?.dev ? <FlaskConical className="ml-1 inline size-3 text-ochre" aria-label="Development build" /> : null}
      </div>
    </nav>
  )
}

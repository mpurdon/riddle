/**
 * The user level: how much of Riddle the rail, the go-to shortcuts, the help
 * sheet and the glossary show. Three rungs, `Settings.uiLevel`, owned by the
 * backend because the Director changes register with it and the CLI and MCP
 * read the same setting; `developerMode` is derived from it there (true
 * exactly at Developer), so the pages that still read the flag agree with
 * the rail. The rail showed sixteen entries to someone who wanted to read
 * their mail (17 Oct 2026); Essentials shows five, and nothing is removed:
 * every route stays registered, a link still opens an advanced page (with
 * an Advanced chip in its header), and what keeps mail recoverable is never
 * gated at any level: Never Hidden, Unresolved, Spoil and Contamination
 * review, Undo, the privacy statement, Why? on every Find.
 *
 * The one-time tip ("everything is there when you want it") is a per-machine
 * convenience in localStorage, like the first-run tips: it counts
 * corrections and Why? openings and shows once at the fifth of either.
 */
import { useCallback } from 'react'
import type { UiLevel } from '@/bindings/UiLevel'
import { readLocalJson, removeLocal, writeLocalJson } from './local'
import { useSettings } from './queries'
import { toast } from './stores'
import { useAction } from './useAction'

export type { UiLevel }

export const LEVELS: UiLevel[] = ['essentials', 'everything', 'developer']
const RANK: Record<UiLevel, number> = { essentials: 0, everything: 1, developer: 2 }

export const LEVEL_LABELS: Record<UiLevel, string> = { essentials: 'Essentials', everything: 'Everything', developer: 'Developer' }

/** True when `level` shows what `needed` gates: Developer implies Everything implies Essentials. */
export function levelShows(level: UiLevel, needed: UiLevel): boolean {
  return RANK[level] >= RANK[needed]
}

/** The lowest level whose rail lists a route. Anything not named here is Essentials. */
const ROUTE_LEVELS: { prefix: string; level: UiLevel }[] = [
  { prefix: '/meshes', level: 'everything' },
  { prefix: '/packs', level: 'everything' },
  { prefix: '/check', level: 'everything' },
  { prefix: '/strata', level: 'everything' },
  { prefix: '/labels', level: 'everything' },
  { prefix: '/expeditions', level: 'everything' },
  { prefix: '/activity', level: 'everything' },
  { prefix: '/signals', level: 'developer' },
  { prefix: '/developer', level: 'developer' },
]

const under = (pathname: string, prefix: string) => pathname === prefix || pathname.startsWith(`${prefix}/`)

export function routeLevel(pathname: string): UiLevel {
  return ROUTE_LEVELS.find((r) => under(pathname, r.prefix))?.level ?? 'essentials'
}

/** A page the current level's rail does not list, opened by a link or a shortcut. */
export function isAdvancedFor(pathname: string, level: UiLevel): boolean {
  // Activity is reachable from Settings at Essentials and is never "advanced": the audit is for everyone.
  if (under(pathname, '/activity')) return false
  return !levelShows(level, routeLevel(pathname))
}

/** In Essentials, sibling pages share one rail entry and a tab strip above the page. */
export interface SiblingTab {
  to: string
  label: string
}
export interface SiblingGroup {
  label: string
  tabs: SiblingTab[]
}
export const ESSENTIALS_GROUPS: SiblingGroup[] = [
  { label: 'Needs you', tabs: [{ to: '/finds', label: 'Finds' }, { to: '/unresolved', label: 'Unresolved' }] },
  { label: 'Mail', tabs: [{ to: '/inbox', label: 'Inbox' }, { to: '/priority', label: 'Priority' }] },
  { label: 'Set aside', tabs: [{ to: '/spoil', label: 'Spoil' }, { to: '/contamination', label: 'Contamination' }] },
]

export function siblingsFor(pathname: string): SiblingGroup | null {
  return ESSENTIALS_GROUPS.find((g) => g.tabs.some((t) => under(pathname, t.to))) ?? null
}

/**
 * The go-to shortcuts and the routes they open: the Shell binds from this
 * table and the help sheet filters by it, so a shortcut to a page the level
 * does not list is neither bound nor documented, and the palette (which
 * lists what is bound) follows without knowing about levels.
 */
export const GO_ROUTES: Record<string, string> = {
  go_summary: '/',
  go_finds: '/finds',
  go_unresolved: '/unresolved',
  go_inbox: '/inbox',
  go_spoil: '/spoil',
  go_activity: '/activity',
  go_meshes: '/meshes',
  go_director: '/director',
  go_settings: '/settings',
}

/** Whether a shortcut belongs at this level: go-to shortcuts follow their route, every other one stays. */
export function shortcutShown(id: string, level: UiLevel): boolean {
  const route = GO_ROUTES[id]
  return !route || levelShows(level, routeLevel(route))
}

/** While settings load, assume Everything: a page must never be unreachable because a query is in flight. */
export const DEFAULT_LEVEL: UiLevel = 'everything'

export function useLevel(): UiLevel {
  return useSettings().data?.uiLevel ?? DEFAULT_LEVEL
}

/** An LLM provider is set; without one the Director has nobody to ask. Assumed while settings load. */
export function useLlmConfigured(): boolean {
  const provider = useSettings().data?.llm.provider
  return provider === undefined || provider !== 'null'
}

/** "Show everything" from Essentials; "Show less" from the other two goes back to Essentials. */
export function nextLevel(level: UiLevel): UiLevel {
  return level === 'essentials' ? 'everything' : 'essentials'
}

/** Save a level through the same Action the settings page uses; the flag goes with it. */
export function useSetLevel(): (level: UiLevel) => Promise<unknown> {
  const settings = useSettings()
  const { run } = useAction()
  const current = settings.data
  return useCallback(
    (level: UiLevel) => {
      if (!current) return Promise.resolve(null)
      return run('save_settings', { settings: { ...current, uiLevel: level, developerMode: level === 'developer' } }, { successToast: level === 'essentials' ? 'Showing the essentials' : `Showing ${LEVEL_LABELS[level].toLowerCase()}` })
    },
    [current, run],
  )
}

// ---------------------------------------------------------------------------
// The one-time tip.

export const LEVEL_TIP_KEY = 'riddle.levelTip.v1'
export const TIP_AFTER = 5

interface TipState {
  corrections: number
  whys: number
  seen: boolean
}

/**
 * Count one correction or one Why?, and say whether the tip is due now: at
 * the fifth of either kind, once. Pure bookkeeping over localStorage, so the
 * rule can be tested without a page.
 */
export function levelTipDue(kind: 'correction' | 'why'): boolean {
  const st = readLocalJson<TipState>(LEVEL_TIP_KEY, { corrections: 0, whys: 0, seen: false })
  if (st.seen) return false
  if (kind === 'correction') st.corrections += 1
  else st.whys += 1
  const due = st.corrections >= TIP_AFTER || st.whys >= TIP_AFTER
  if (due) st.seen = true
  writeLocalJson(LEVEL_TIP_KEY, st)
  return due
}

export function resetLevelTip(): void {
  removeLocal(LEVEL_TIP_KEY)
}

/** A page calls this on each correction or Why?; at Essentials the fifth shows the tip, once. */
export function useLevelTip(): (kind: 'correction' | 'why') => void {
  const level = useLevel()
  return useCallback(
    (kind: 'correction' | 'why') => {
      if (level !== 'essentials') return
      if (levelTipDue(kind)) {
        toast('Everything is there when you want it', {
          body: 'Rules, topics, labels and the Director’s tools are one click away: Show everything, at the bottom of the rail, or in Settings.',
          kind: 'info',
        })
      }
    },
    [level],
  )
}

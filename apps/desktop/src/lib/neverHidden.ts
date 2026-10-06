/**
 * Whether a sender is on the Never Hidden list, and by which entry. Mirrors
 * `UserProfile::is_never_hidden` (crates/riddle-core/src/people.rs): an entry
 * is an address, or a domain written `@example.com`, compared without case.
 * The entry matters as much as the answer: removing a sender who is covered
 * by their domain means removing the domain.
 */
import { useProfile } from '@/lib/queries'

export function neverHiddenEntry(list: readonly string[], email: string): string | null {
  const e = email.toLowerCase()
  const domain = e.includes('@') ? e.slice(e.lastIndexOf('@') + 1) : ''
  return (
    list.find((entry) => {
      const x = entry.toLowerCase()
      return x.startsWith('@') ? x.slice(1) === domain : x === e
    }) ?? null
  )
}

/** The entry that keeps `email` from ever being hidden, or null; follows the profile as it changes. */
export function useNeverHiddenEntry(email: string | null | undefined): string | null {
  const profile = useProfile()
  if (!email) return null
  return neverHiddenEntry(profile.data?.neverHidden ?? [], email)
}

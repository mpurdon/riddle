/**
 * localStorage for per-machine conveniences (a remembered view, a dismissed
 * offer, tips already seen). Every access is guarded: a private window or
 * blocked site data throws on the accessor, and a convenience is never
 * worth failing a page over. Three features had grown their own copies of
 * this guard with three different fallbacks.
 */
export function readLocal(key: string): string | null {
  try {
    return localStorage.getItem(key)
  } catch {
    return null
  }
}

export function writeLocal(key: string, value: string): void {
  try {
    localStorage.setItem(key, value)
  } catch {
    /* a convenience, not state */
  }
}

export function removeLocal(key: string): void {
  try {
    localStorage.removeItem(key)
  } catch {
    /* ignore */
  }
}

/** A JSON value, or the fallback when absent or unreadable. */
export function readLocalJson<T>(key: string, fallback: T): T {
  const raw = readLocal(key)
  if (raw === null) return fallback
  try {
    return JSON.parse(raw) as T
  } catch {
    return fallback
  }
}

export function writeLocalJson(key: string, value: unknown): void {
  writeLocal(key, JSON.stringify(value))
}

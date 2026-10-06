/**
 * Formatting helpers. The bindings type u64 fields as `bigint`, but the
 * wire carries JSON numbers, so everything that displays or sums a count
 * goes through `num` and accepts either.
 */

import clsx from 'clsx'
import { twMerge } from 'tailwind-merge'

export type Num = bigint | number | null | undefined

export function num(v: Num): number {
  if (v === null || v === undefined) return 0
  return typeof v === 'bigint' ? Number(v) : v
}

/** Cast a number into a `bigint`-typed field; the wire value is a number. */
export function u64(n: number): bigint {
  return n as unknown as bigint
}

const nf = new Intl.NumberFormat('en-US')
export function fmtInt(v: Num): string {
  return nf.format(Math.round(num(v)))
}

export function fmtUsd(v: number | null | undefined, opts: { unpriced?: string } = {}): string {
  if (v === null || v === undefined) return opts.unpriced ?? 'unpriced'
  if (v === 0) return '$0.00'
  if (v < 0.01) return `$${v.toFixed(4)}`
  return `$${v.toFixed(2)}`
}

export function fmtPct(v: number | null | undefined, digits = 1): string {
  if (v === null || v === undefined) return '—'
  return `${(v * 100).toFixed(digits)}%`
}

export function fmtScore(v: number | null | undefined): string {
  if (v === null || v === undefined) return '—'
  return v.toFixed(2).replace(/^0/, '')
}

export function fmtMs(v: Num): string {
  const n = num(v)
  if (n < 1000) return `${Math.round(n)}ms`
  return `${(n / 1000).toFixed(1)}s`
}

export function fmtBytes(v: Num): string {
  const n = num(v)
  if (n < 1024) return `${n} B`
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(0)} KB`
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`
}

export function fmtDuration(seconds: Num): string {
  const s = Math.max(0, Math.round(num(seconds)))
  if (s < 60) return `${s}s`
  const m = Math.floor(s / 60)
  if (m < 60) return `${m} min`
  const h = Math.floor(m / 60)
  const rm = m % 60
  return rm ? `${h} h ${rm} min` : `${h} h`
}

export function fmtTime(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return '—'
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  const sameDay = d.toDateString() === now.toDateString()
  if (sameDay) return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
  const yesterday = new Date(now)
  yesterday.setDate(now.getDate() - 1)
  if (d.toDateString() === yesterday.toDateString()) return `Yesterday ${d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })}`
  if (d.getFullYear() === now.getFullYear()) return d.toLocaleDateString([], { month: 'short', day: 'numeric' })
  return d.toLocaleDateString([], { year: 'numeric', month: 'short', day: 'numeric' })
}

export function fmtClock(iso: string | null | undefined): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })
}

export function fmtClockMs(iso: string): string {
  const d = new Date(iso)
  const hh = String(d.getHours()).padStart(2, '0')
  const mm = String(d.getMinutes()).padStart(2, '0')
  const ss = String(d.getSeconds()).padStart(2, '0')
  const ms = String(d.getMilliseconds()).padStart(3, '0')
  return `${hh}:${mm}:${ss}.${ms}`
}

export function fmtDateTime(iso: string | null | undefined): string {
  if (!iso) return '—'
  const d = new Date(iso)
  return d.toLocaleString([], { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' })
}

export function fmtRelative(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return 'never'
  const d = new Date(iso)
  const diff = Math.round((now.getTime() - d.getTime()) / 1000)
  if (diff < 45) return 'just now'
  if (diff < 90) return '1 minute ago'
  if (diff < 3600) return `${Math.round(diff / 60)} minutes ago`
  if (diff < 7200) return '1 hour ago'
  if (diff < 86400) return `${Math.round(diff / 3600)} hours ago`
  if (diff < 172800) return 'yesterday'
  return `${Math.round(diff / 86400)} days ago`
}

export function fmtDue(iso: string | null | undefined, now: Date = new Date()): string | null {
  if (!iso) return null
  const d = new Date(iso)
  const days = Math.round((d.getTime() - now.getTime()) / 86_400_000)
  if (days < 0) return `overdue by ${plural(-days, 'day')}`
  if (days === 0) return 'due today'
  if (days === 1) return 'due tomorrow'
  return `due in ${days} days`
}

/** When a follow-up is due, in the reader's own days: "6:00 PM today", "tomorrow", "Tuesday", "Sep 30". */
export function fmtBy(iso: string, now: Date = new Date()): string {
  const d = new Date(iso)
  if (Number.isNaN(d.getTime())) return iso
  const day = (x: Date) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime()
  const days = Math.round((day(d) - day(now)) / 86_400_000)
  if (days <= 0) return `${d.toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' })} today`
  if (days === 1) return 'tomorrow'
  if (days < 7) return d.toLocaleDateString([], { weekday: 'long' })
  if (d.getFullYear() === now.getFullYear()) return d.toLocaleDateString([], { month: 'short', day: 'numeric' })
  return d.toLocaleDateString([], { year: 'numeric', month: 'short', day: 'numeric' })
}

export function plural(n: Num, one: string, many = `${one}s`): string {
  const v = num(n)
  return `${fmtInt(v)} ${v === 1 ? one : many}`
}

export function titleCase(s: string): string {
  return s.replace(/[_-]+/g, ' ').replace(/\b\w/g, (c) => c.toUpperCase())
}

export function initials(name: string | null | undefined, email: string): string {
  const src = (name && name.trim()) || email
  const parts = src.split(/[\s@._-]+/).filter(Boolean)
  return (parts[0]?.[0] ?? '?').toUpperCase() + (parts[1]?.[0] ?? '').toUpperCase()
}

/** JSON.stringify that survives the `bigint` fields a binding may carry at runtime. */
export function safeJson(v: unknown, indent = 2): string {
  return JSON.stringify(v, (_k, val) => (typeof val === 'bigint' ? Number(val) : val), indent)
}

export function clamp(n: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, n))
}

export function cx(...parts: Array<string | false | null | undefined>): string {
  return twMerge(clsx(parts))
}

/** `Name <email>`, or the address alone when there is no name. */
export function fmtAddress(a: { name?: string | null; email: string }): string {
  return a.name ? `${a.name} <${a.email}>` : a.email
}

export function fmtAddresses(list: { name?: string | null; email: string }[]): string {
  return list.map(fmtAddress).join(', ')
}

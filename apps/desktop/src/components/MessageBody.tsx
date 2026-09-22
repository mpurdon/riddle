/**
 * HTML mail renders in a sandboxed srcdoc iframe with scripts and same-origin
 * disabled (§58). The Rust sanitizer runs first; this side blocks remote
 * images by default (a "Load remote images (N)" control restores them),
 * neutralises links inside the frame, and lists the links below with an
 * allow-list (http, https, mailto) so nothing navigates from inside mail.
 */
import { useMemo, useState } from 'react'
import { ExternalLink, ImageOff } from 'lucide-react'
import { Button } from './ui/Button'
import { isTauri, log } from '@/lib/ipc'

export interface PreparedHtml {
  html: string
  remoteImages: number
  links: { href: string; text: string; allowed: boolean }[]
}

const ALLOWED_SCHEMES = ['http:', 'https:', 'mailto:']

export function prepareHtml(source: string, loadRemote: boolean): PreparedHtml {
  const doc = new DOMParser().parseFromString(source, 'text/html')
  let remoteImages = 0
  doc.querySelectorAll('script, iframe, object, embed, form, base, meta[http-equiv]').forEach((el) => el.remove())
  doc.querySelectorAll('*').forEach((el) => {
    for (const attr of Array.from(el.attributes)) {
      if (/^on/i.test(attr.name)) el.removeAttribute(attr.name)
    }
  })
  doc.querySelectorAll('img, [style*="url("]').forEach((el) => {
    const src = el.getAttribute('src') ?? ''
    if (/^(https?:)?\/\//i.test(src)) {
      remoteImages += 1
      if (!loadRemote) {
        el.setAttribute('data-riddle-src', src)
        el.setAttribute('alt', el.getAttribute('alt') || 'remote image blocked')
        el.removeAttribute('src')
        el.setAttribute('style', 'display:inline-block;min-width:16px;min-height:16px;background:#eee;border:1px dashed #bbb')
      }
    } else if (src && !/^(data:|cid:|asset:)/i.test(src)) {
      el.removeAttribute('src')
    }
    const style = el.getAttribute('style')
    if (style && /url\(/i.test(style) && !loadRemote) el.setAttribute('style', style.replace(/url\([^)]*\)/gi, 'none'))
  })
  const links: PreparedHtml['links'] = []
  doc.querySelectorAll('a[href]').forEach((a) => {
    const href = a.getAttribute('href') ?? ''
    let allowed = false
    try {
      allowed = ALLOWED_SCHEMES.includes(new URL(href, 'https://invalid.local/').protocol) && !/^javascript:/i.test(href)
    } catch {
      allowed = false
    }
    if (!links.some((l) => l.href === href)) links.push({ href, text: (a.textContent ?? '').trim().slice(0, 80), allowed })
    a.removeAttribute('href')
    a.setAttribute('title', href)
    a.setAttribute('style', `${a.getAttribute('style') ?? ''};text-decoration:underline;color:#7c2d12;cursor:default`)
  })
  const csp = `default-src 'none'; img-src data: cid: asset:${loadRemote ? ' https: http:' : ''}; style-src 'unsafe-inline'; font-src data:`
  const head = `<meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="${csp}"><style>body{margin:0;padding:12px;font:14px/1.5 -apple-system,system-ui,sans-serif;color:#1c1917;background:#fff;word-break:break-word}img{max-width:100%}pre{white-space:pre-wrap}</style>`
  const body = doc.body?.innerHTML ?? ''
  return { html: `<!doctype html><html><head>${head}</head><body>${body}</body></html>`, remoteImages, links }
}

export function openExternal(href: string): void {
  if (isTauri) {
    void import('@tauri-apps/plugin-opener')
      .then((m) => m.openUrl(href))
      .catch((e) => log('warn', 'ui', `openUrl failed: ${String((e as Error)?.message ?? e)}`))
  } else {
    window.open(href, '_blank', 'noopener,noreferrer')
  }
}

export function MessageBody({ html, text }: { html: string | null; text: string | null }) {
  const [loadRemote, setLoadRemote] = useState(false)
  const prepared = useMemo(() => (html ? prepareHtml(html, loadRemote) : null), [html, loadRemote])
  if (!prepared) {
    return <pre className="whitespace-pre-wrap rounded border border-line bg-paper p-4 font-sans text-sm leading-6 selectable">{text ?? <span className="text-clay">This message has no body yet. It will be fetched with the next sync.</span>}</pre>
  }
  const lines = (text ?? html ?? '').split('\n').length
  const height = Math.min(1200, Math.max(220, lines * 22 + 80))
  return (
    <div className="flex flex-col gap-2">
      {prepared.remoteImages > 0 ? (
        <div className="flex items-center gap-2 rounded border border-line bg-sand px-2 py-1 text-xs text-ink-2">
          <ImageOff className="size-3.5" />
          {loadRemote ? (
            <span>Remote images loaded for this message.</span>
          ) : (
            <>
              <span>Remote images blocked: the sender cannot see that you opened this.</span>
              <Button size="sm" variant="ghost" onClick={() => setLoadRemote(true)}>
                Load remote images ({prepared.remoteImages})
              </Button>
            </>
          )}
        </div>
      ) : null}
      <iframe
        title="Message body (sandboxed)"
        sandbox=""
        referrerPolicy="no-referrer"
        srcDoc={prepared.html}
        className="w-full rounded border border-line bg-white"
        style={{ height }}
      />
      {prepared.links.length ? (
        <details className="text-xs">
          <summary className="cursor-default text-ink-2">Links in this message ({prepared.links.length})</summary>
          <ul className="mt-1 flex flex-col gap-0.5">
            {prepared.links.map((l) => (
              <li key={l.href} className="flex items-center gap-2">
                {l.allowed ? (
                  <button type="button" className="inline-flex items-center gap-1 text-ochre hover:underline" onClick={() => openExternal(l.href)} aria-label={`Open ${l.href} in your browser`}>
                    <ExternalLink className="size-3" />
                    {l.text || l.href}
                  </button>
                ) : (
                  <span className="text-clay" title="Blocked: not http, https or mailto">
                    {l.text || l.href} (blocked)
                  </span>
                )}
                <span className="truncate text-clay mono">{l.href}</span>
              </li>
            ))}
          </ul>
        </details>
      ) : null}
    </div>
  )
}

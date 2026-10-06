/**
 * A message's attachments as chips: the name and the size, and the type
 * where there is room for it. The message page and the preview pane had
 * each written the list.
 */
import { Paperclip } from 'lucide-react'
import type { AttachmentMeta } from '@/bindings/AttachmentMeta'
import { cx, fmtBytes } from '@/lib/format'

export function AttachmentChips({ attachments, showType, className }: { attachments: AttachmentMeta[]; showType?: boolean; className?: string }) {
  if (!attachments.length) return null
  return (
    <ul className={cx('flex flex-wrap gap-2', className)} aria-label="Attachments">
      {attachments.map((a) => (
        <li key={a.filename} className="flex items-center gap-1.5 rounded border border-line bg-sand px-2 py-1 text-xs selectable">
          <Paperclip className="size-3 text-clay" aria-hidden />
          <span className="font-medium">{a.filename}</span>
          <span className="text-clay">{showType ? `${a.mimeType} · ${fmtBytes(a.sizeBytes)}` : fmtBytes(a.sizeBytes)}</span>
        </li>
      ))}
    </ul>
  )
}

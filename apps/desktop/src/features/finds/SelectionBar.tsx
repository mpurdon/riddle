/**
 * The bar above a list with a selection: the count with Clear, and the
 * line a bulk run leaves when some of it failed. Finds and Unresolved had
 * the same markup twice.
 */
import { Button } from '@/components/ui/Button'
import { Note } from '@/components/ui/Note'
import { keyHint } from '@/lib/shortcuts'

export function SelectionBar({ count, error, onClear }: { count: number; error: string | null; onClear: () => void }) {
  if (!count && !error) return null
  return (
    <div className="flex shrink-0 flex-col gap-1 border-b border-line bg-sand px-4 py-1.5 text-xs text-ink-2">
      {count ? (
        <div className="flex items-center justify-between">
          <span>{count} selected</span>
          <Button size="sm" variant="ghost" onClick={onClear} title={`Clear the selection (${keyHint('select_all')})`}>
            Clear
          </Button>
        </div>
      ) : null}
      {error ? (
        <Note tone="bad" className="py-1 text-xs">
          {error}
        </Note>
      ) : null}
    </div>
  )
}

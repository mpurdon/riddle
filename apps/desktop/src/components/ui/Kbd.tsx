import { keysLabel } from '@/lib/shortcuts'

export function Keys({ keys }: { keys: string }) {
  return (
    <span className="inline-flex items-center gap-0.5">
      {keysLabel(keys).map((k, i) => (
        <kbd key={i}>{k}</kbd>
      ))}
    </span>
  )
}

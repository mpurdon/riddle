import { keysLabel } from '@/lib/shortcuts'

export function Keys({ keys, also }: { keys: string; also?: string }) {
  return (
    <span className="inline-flex items-center gap-0.5">
      {keysLabel(keys).map((k, i) => (
        <kbd key={i}>{k}</kbd>
      ))}
      {also ? (
        <>
          <span className="px-0.5 text-2xs text-clay">or</span>
          {keysLabel(also).map((k, i) => (
            <kbd key={`also-${i}`}>{k}</kbd>
          ))}
        </>
      ) : null}
    </span>
  )
}

import type { ReactNode } from 'react'

/** Every empty state is an instruction or a proof (README). */
export function EmptyState({ title, body, actions, icon }: { title: string; body?: ReactNode; actions?: ReactNode; icon?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-8 py-16 text-center" role="status">
      {icon ? <div className="text-clay [&>svg]:size-8">{icon}</div> : null}
      <h2 className="text-base font-semibold text-ink">{title}</h2>
      {body ? <div className="max-w-md text-sm text-ink-2">{body}</div> : null}
      {actions ? <div className="mt-2 flex flex-wrap items-center justify-center gap-2">{actions}</div> : null}
    </div>
  )
}

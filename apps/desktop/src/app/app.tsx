import { Fragment } from 'react'
import { useQuery } from '@tanstack/react-query'
import { call } from '@/lib/ipc'
import type { BuildStamp } from '@/bindings/BuildStamp'

interface DevInfo {
  build: BuildStamp
  dev: boolean
  os: string
  dataDir: string
  logDir: string
  stores: { label: string; path: string; sizeBytes: number }[]
  storeFault: string | null
}

/** Placeholder shell: proves the round trip. The product surfaces replace it. */
export function App() {
  const info = useQuery({ queryKey: ['dev_info'], queryFn: () => call<DevInfo>('dev_info') })
  return (
    <div className="h-full flex flex-col">
      <div className="titlebar-drag h-10 flex items-center justify-center text-sm text-clay">
        Riddle {info.data?.dev ? <span className="ml-2 rounded bg-ochre/15 px-1.5 text-ochre font-medium">DEV</span> : null}
      </div>
      <main className="flex-1 overflow-auto p-8">
        <h1 className="text-2xl font-semibold">Good morning.</h1>
        {info.isLoading && <p className="mt-4 text-clay">Connecting…</p>}
        {info.error && <p className="mt-4 text-rust">{String((info.error as Error).message)}</p>}
        {info.data && (
          <dl className="mt-6 grid grid-cols-[10rem_1fr] gap-y-1 text-sm">
            <dt className="text-clay">Version</dt>
            <dd>{info.data.build.version} ({info.data.build.gitSha}{info.data.build.dirty ? '-dirty' : ''})</dd>
            <dt className="text-clay">Built</dt>
            <dd>{info.data.build.builtAt}</dd>
            <dt className="text-clay">Data</dt>
            <dd className="font-mono text-xs">{info.data.dataDir}</dd>
            <dt className="text-clay">Logs</dt>
            <dd className="font-mono text-xs">{info.data.logDir}</dd>
            {info.data.stores.map((s) => (
              <Fragment key={s.label}>
                <dt className="text-clay">{s.label}</dt>
                <dd className="font-mono text-xs">{s.path} · {(s.sizeBytes / 1024).toFixed(0)} KB</dd>
              </Fragment>
            ))}
          </dl>
        )}
      </main>
    </div>
  )
}

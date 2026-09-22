import React from 'react'
import ReactDOM from 'react-dom/client'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { App } from './app/app'
import { isTauri, log } from './lib/ipc'
import './index.css'

const queryClient = new QueryClient({
  defaultOptions: { queries: { retry: 1, staleTime: 5_000, refetchOnWindowFocus: true } },
})

window.addEventListener('error', (e) => log('error', 'ui', `uncaught: ${e.message}`))
window.addEventListener('unhandledrejection', (e) => log('error', 'ui', `unhandled rejection: ${String(e.reason?.message ?? e.reason)}`))

async function boot() {
  if (!isTauri) {
    // Browser mode: the mocks are only ever loaded here, never in Tauri.
    const mocks = await import('./lib/mocks')
    mocks.registerMocks()
    mocks.startMockTicker()
  }
  ReactDOM.createRoot(document.getElementById('root')!).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <App />
      </QueryClientProvider>
    </React.StrictMode>,
  )
}

void boot()

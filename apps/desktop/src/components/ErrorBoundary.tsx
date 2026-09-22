import { Component, type ErrorInfo, type ReactNode } from 'react'
import { log } from '@/lib/ipc'
import { Button } from './ui/Button'

interface State {
  error: Error | null
}

/** Per-route boundary: a broken page reports to the backend log and offers a retry, and the shell keeps working. */
export class ErrorBoundary extends Component<{ children: ReactNode; name: string }, State> {
  state: State = { error: null }
  static getDerivedStateFromError(error: Error): State {
    return { error }
  }
  componentDidCatch(error: Error, info: ErrorInfo) {
    log('error', 'ui', `route ${this.props.name} crashed: ${error.message} ${info.componentStack?.split('\n')[1]?.trim() ?? ''}`)
  }
  render() {
    if (this.state.error) {
      return (
        <div className="p-8" role="alert">
          <h2 className="text-base font-semibold text-rust">This page failed to render.</h2>
          <p className="mt-1 text-sm text-ink-2">{this.state.error.message}</p>
          <p className="mt-1 text-xs text-clay">The error was written to the app log. The rest of Riddle keeps working.</p>
          <Button className="mt-4" onClick={() => this.setState({ error: null })}>
            Try again
          </Button>
        </div>
      )
    }
    return this.props.children
  }
}

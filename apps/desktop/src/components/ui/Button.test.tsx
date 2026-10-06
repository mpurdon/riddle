import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { TooltipProvider } from './Tooltip'
import { Button } from './Button'

afterEach(() => cleanup())

describe('Button', () => {
  it('a button with a disabled reason does not run its click handler', () => {
    const onClick = vi.fn()
    render(
      <TooltipProvider>
        <Button onClick={onClick} disabledReason="Not yet">
          Continue
        </Button>
      </TooltipProvider>,
    )
    fireEvent.click(screen.getByText('Continue'))
    expect(onClick).not.toHaveBeenCalled()
  })

  it('an enabled button runs its click handler', () => {
    const onClick = vi.fn()
    render(<Button onClick={onClick}>Go</Button>)
    fireEvent.click(screen.getByText('Go'))
    expect(onClick).toHaveBeenCalledOnce()
  })

  it('a disabled primary button keeps its colour on hover', () => {
    render(
      <Button variant="primary" disabled>
        Save
      </Button>,
    )
    expect(screen.getByText('Save').className).not.toContain('hover:')
  })
})

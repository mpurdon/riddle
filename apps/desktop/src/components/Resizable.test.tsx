import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { Resizable } from './Resizable'

afterEach(cleanup)
beforeEach(() => localStorage.clear())

const width = () => Number(screen.getByRole('separator').getAttribute('aria-valuenow'))

describe('Resizable', () => {
  it('moves with the keyboard, remembers the width, and Home puts it back', () => {
    render(
      <Resizable id="t" side="left" initial={400} min={200} max={600} as="section">
        <p>pane</p>
      </Resizable>,
    )
    const sep = screen.getByRole('separator')
    expect(width()).toBe(400)
    fireEvent.keyDown(sep, { key: 'ArrowRight' })
    fireEvent.keyDown(sep, { key: 'ArrowRight' })
    expect(width()).toBe(432)
    expect(JSON.parse(localStorage.getItem('pane:t') ?? '0')).toBe(432)
    fireEvent.keyDown(sep, { key: 'Home' })
    expect(width()).toBe(400)
    cleanup()
    localStorage.setItem('pane:t', '900')
    render(
      <Resizable id="t" side="left" initial={400} min={200} max={600} as="section">
        <p>pane</p>
      </Resizable>,
    )
    expect(width()).toBe(600)
  })

  it('follows a drag, in the direction that widens the pane on its side', () => {
    render(
      <Resizable id="r" side="right" initial={500} min={300} max={900}>
        <p>preview</p>
      </Resizable>,
    )
    const sep = screen.getByRole('separator')
    fireEvent.pointerDown(sep, { button: 0, clientX: 1000, pointerId: 1 })
    fireEvent.pointerMove(sep, { clientX: 940, pointerId: 1 })
    fireEvent.pointerUp(sep, { clientX: 940, pointerId: 1 })
    expect(width()).toBe(560)
    fireEvent.doubleClick(sep)
    expect(width()).toBe(500)
  })
})

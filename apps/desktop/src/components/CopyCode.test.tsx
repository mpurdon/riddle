/**
 * A sign-in code is copied in one click, and the button says it worked;
 * the click does not open the Find row it sits in.
 */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { CopyCode } from './Bits'

afterEach(() => cleanup())

describe('CopyCode', () => {
  it('copies the code and keeps the click to itself', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', { value: { writeText }, configurable: true })
    const onRow = vi.fn()
    render(
      <div onClick={onRow}>
        <CopyCode code="002534" />
      </div>,
    )
    fireEvent.click(screen.getByRole('button', { name: 'Copy code 002534' }))
    await waitFor(() => expect(screen.getByText('Copied')).toBeTruthy())
    expect(writeText).toHaveBeenCalledWith('002534')
    expect(onRow).not.toHaveBeenCalled()
  })
})

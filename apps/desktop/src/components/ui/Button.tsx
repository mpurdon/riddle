import { forwardRef, type ButtonHTMLAttributes, type ReactNode } from 'react'
import { cx } from '@/lib/format'
import { Tip } from './Tooltip'

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger' | 'link'
export type ButtonSize = 'sm' | 'md'

export interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, 'disabled'> {
  variant?: ButtonVariant
  size?: ButtonSize
  /** When set the button is disabled and the reason is shown as a tooltip and read by screen readers. */
  disabledReason?: string | null
  disabled?: boolean
  icon?: ReactNode
  busy?: boolean
}

const variants: Record<ButtonVariant, string> = {
  primary: 'bg-ochre text-paper hover:bg-ochre/90 border-transparent',
  secondary: 'bg-sand text-ink hover:bg-sand-2 border-line',
  ghost: 'bg-transparent text-ink-2 hover:bg-sand hover:text-ink border-transparent',
  danger: 'bg-transparent text-rust hover:bg-rust/10 border-rust/30',
  link: 'bg-transparent text-ochre hover:underline border-transparent px-0',
}

const sizes: Record<ButtonSize, string> = {
  sm: 'h-6 px-2 text-xs gap-1',
  md: 'h-8 px-3 text-sm gap-1.5',
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button({ variant = 'secondary', size = 'md', disabledReason, disabled, icon, busy, className, children, title, ...rest }, ref) {
  const isDisabled = !!disabled || !!disabledReason || !!busy
  const btn = (
    <button
      ref={ref}
      type="button"
      aria-disabled={isDisabled || undefined}
      aria-label={typeof children === 'string' ? undefined : title}
      title={title}
      className={cx(
        'inline-flex items-center justify-center rounded-md border font-medium whitespace-nowrap select-none',
        variants[variant],
        sizes[size],
        isDisabled && 'opacity-50 cursor-not-allowed hover:bg-inherit',
        className,
      )}
      onClick={(e) => {
        if (isDisabled) {
          e.preventDefault()
          return
        }
        rest.onClick?.(e)
      }}
      {...rest}
    >
      {icon ? <span className="shrink-0 [&>svg]:size-3.5">{icon}</span> : null}
      {busy ? 'Working…' : children}
    </button>
  )
  if (disabledReason) {
    return (
      <Tip text={disabledReason}>
        <span className="inline-flex" aria-label={disabledReason}>
          {btn}
        </span>
      </Tip>
    )
  }
  return btn
})

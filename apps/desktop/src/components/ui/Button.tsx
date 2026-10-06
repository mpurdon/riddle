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
  primary: 'bg-ochre text-paper border-transparent',
  secondary: 'bg-sand text-ink border-line',
  ghost: 'bg-transparent text-ink-2 border-transparent',
  danger: 'bg-transparent text-rust border-rust/30',
  link: 'bg-transparent text-ochre border-transparent px-0',
}

// Hover styles apply only to enabled buttons. A disabled button used to get
// `hover:bg-inherit`, which on a primary button left white text on nothing:
// hovering a disabled Continue made it vanish.
const hovers: Record<ButtonVariant, string> = {
  primary: 'hover:bg-ochre/90',
  secondary: 'hover:bg-sand-2',
  ghost: 'hover:bg-sand hover:text-ink',
  danger: 'hover:bg-rust/10',
  link: 'hover:underline',
}

// Pressed states, one step down the same ramp as hover, and likewise only
// on enabled buttons. Focus-visible is the global ring in index.css; the
// disabled state is the opacity and cursor below.
const actives: Record<ButtonVariant, string> = {
  primary: 'active:bg-ochre/80',
  secondary: 'active:bg-line',
  ghost: 'active:bg-sand-2',
  danger: 'active:bg-rust/20',
  link: 'active:text-accent',
}

const sizes: Record<ButtonSize, string> = {
  sm: 'h-6 px-2 text-xs gap-1',
  md: 'h-8 px-3 text-sm gap-1.5',
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button({ variant = 'secondary', size = 'md', disabledReason, disabled, icon, busy, className, children, title, ...rest }, ref) {
  const isDisabled = !!disabled || !!disabledReason || !!busy
  const btn = (
    <button
      {...rest}
      ref={ref}
      type="button"
      aria-disabled={isDisabled || undefined}
      aria-label={rest['aria-label'] ?? (typeof children === 'string' ? undefined : title)}
      title={title}
      className={cx(
        'inline-flex items-center justify-center rounded-md border font-medium whitespace-nowrap select-none',
        variants[variant],
        sizes[size],
        isDisabled ? 'opacity-50 cursor-not-allowed' : cx(hovers[variant], actives[variant]),
        className,
      )}
      onClick={(e) => {
        if (isDisabled) {
          e.preventDefault()
          return
        }
        rest.onClick?.(e)
      }}
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

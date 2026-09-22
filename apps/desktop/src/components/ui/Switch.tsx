import * as RSw from '@radix-ui/react-switch'
import { cx } from '@/lib/format'
import { Tip } from './Tooltip'

export function Switch({ checked, onChange, label, disabledReason, id }: { checked: boolean; onChange: (v: boolean) => void; label: string; disabledReason?: string | null; id?: string }) {
  const sw = (
    <RSw.Root
      id={id}
      checked={checked}
      onCheckedChange={onChange}
      disabled={!!disabledReason}
      aria-label={label}
      className={cx('relative h-5 w-9 shrink-0 rounded-full border border-line transition-colors data-[state=checked]:bg-ochre data-[state=unchecked]:bg-sand-2', disabledReason && 'opacity-50 cursor-not-allowed')}
    >
      <RSw.Thumb className="block size-4 translate-x-0.5 rounded-full bg-paper shadow transition-transform data-[state=checked]:translate-x-[18px]" />
    </RSw.Root>
  )
  return disabledReason ? <Tip text={disabledReason}><span className="inline-flex">{sw}</span></Tip> : sw
}

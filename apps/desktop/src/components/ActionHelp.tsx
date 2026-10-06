/**
 * The "?" beside a row of action buttons: what each one does in Riddle and
 * in Gmail for this account, whether it teaches Riddle, and how to undo it.
 * Rendered from `lib/actionHelp`, the same table the help sheet uses.
 */
import { useState } from 'react'
import { HelpCircle } from 'lucide-react'
import { ACTION_HELP, CORRECTION_NOTE, type HelpContext, type HelpGroup } from '@/lib/actionHelp'
import { useDevArmed, useDigs } from '@/lib/queries'
import { PROJECTION_OPTIONS, labelOf } from '@/features/settings/accounts/model'
import { Button } from './ui/Button'
import { Dialog } from './ui/Dialog'
import { Keys } from './ui/Kbd'

export function useHelpContext(digId?: string | null): HelpContext {
  const digs = useDigs().data ?? []
  const { dev, armed } = useDevArmed()
  const dig = digId ? digs.find((d) => d.id === digId) : undefined
  // The backend's answer confirms an unarmed build; it says so only from Act
  // (below Act the held reason is the autonomy), while the build refuses your
  // own moves at every level, so the dev/armed flags still count.
  const devUnarmed = dig?.mailbox?.held === 'dev_unarmed' || (dev && !armed)
  return { projection: dig?.projection, devUnarmed }
}

export function ActionHelpTable({ group, ctx }: { group: HelpGroup; ctx: HelpContext }) {
  const g = ACTION_HELP[group]
  return (
    <table className="w-full border-collapse text-left text-xs">
      <thead>
        <tr className="border-b border-line text-2xs uppercase tracking-wide text-clay">
          <th className="py-1.5 pr-3 font-medium">Button</th>
          <th className="py-1.5 pr-3 font-medium">In Riddle</th>
          <th className="py-1.5 pr-3 font-medium">In Gmail</th>
          <th className="py-1.5 pr-3 font-medium">Teaches Riddle</th>
          <th className="py-1.5 font-medium">Undo</th>
        </tr>
      </thead>
      <tbody>
        {g.items.map((item) => (
          <tr key={item.id} className="border-b border-line/60 align-top">
            <td className="py-2 pr-3">
              <div className="font-medium text-ink">{item.label}</div>
              {item.keys ? <Keys keys={item.keys} /> : null}
              {item.mailbox && ctx.devUnarmed ? <div className="mt-1 text-2xs font-medium uppercase tracking-wide text-ochre">Gmail change waits</div> : null}
            </td>
            <td className="py-2 pr-3 text-ink-2">{item.riddle}</td>
            <td className="py-2 pr-3 text-ink-2">{item.gmail(ctx)}</td>
            <td className="py-2 pr-3 text-ink-2">{item.teaches}</td>
            <td className="py-2 text-ink-2">{item.undo}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}

export function ActionHelpButton({ group, digId }: { group: HelpGroup; digId?: string | null }) {
  const [open, setOpen] = useState(false)
  const ctx = useHelpContext(digId)
  const g = ACTION_HELP[group]
  return (
    <>
      <Button size="sm" variant="ghost" icon={<HelpCircle />} title="What do these buttons do?" aria-label="What do these buttons do?" onClick={() => setOpen(true)} />
      <Dialog open={open} onOpenChange={setOpen} title={g.title} description={g.intro} wide>
        {ctx.devUnarmed && g.items.some((i) => i.mailbox) ? (
          <p className="mb-3 rounded-md border border-ochre/40 bg-ochre/5 px-3 py-2 text-xs text-ink-2">
            This development build holds mailbox writes. The buttons marked <span className="font-medium text-ochre">Gmail change waits</span> still work in Riddle; their Gmail change waits until you send it or allow mailbox writes on the Developer page. The Gmail column says what they do once sent.
          </p>
        ) : null}
        <div className="max-h-[60vh] overflow-auto">
          <ActionHelpTable group={group} ctx={ctx} />
        </div>
        {g.items.some((i) => i.teaches.startsWith('Yes, as a correction')) ? <p className="mt-3 text-xs text-ink-2">{CORRECTION_NOTE}</p> : null}
        <p className="mt-2 text-xs text-clay">
          {ctx.projection
            ? `The Gmail column is for this account, whose "Changes in Gmail" is ${labelOf(PROJECTION_OPTIONS, ctx.projection)}. Change it in Settings → Accounts.`
            : 'What reaches Gmail depends on each account\'s "Changes in Gmail" setting in Settings → Accounts.'}
          {' '}Press ? for every shortcut and button.
        </p>
      </Dialog>
    </>
  )
}

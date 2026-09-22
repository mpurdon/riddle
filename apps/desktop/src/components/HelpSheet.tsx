/**
 * The `?` help sheet, rendered from the same table that binds the keys.
 */
import { Dialog } from './ui/Dialog'
import { Keys } from './ui/Kbd'
import { SCOPE_LABELS, SHORTCUTS, type ShortcutScope } from '@/lib/shortcuts'
import { useUi } from '@/lib/stores'

const ORDER: ShortcutScope[] = ['global', 'list', 'finds', 'messages', 'review', 'meshes', 'director']

export function HelpSheet() {
  const open = useUi((s) => s.helpOpen)
  const setOpen = useUi((s) => s.setHelpOpen)
  return (
    <Dialog open={open} onOpenChange={setOpen} title="Keyboard" description="One table drives these bindings and this sheet, so a shortcut cannot exist without being documented." wide>
      <div className="grid max-h-[70vh] grid-cols-2 gap-x-8 gap-y-4 overflow-auto pr-1">
        {ORDER.map((scope) => (
          <section key={scope}>
            <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-clay">{SCOPE_LABELS[scope]}</h3>
            <table className="w-full text-sm">
              <tbody>
                {SHORTCUTS.filter((s) => s.scope === scope).map((s) => (
                  <tr key={s.id}>
                    <td className="w-24 py-0.5 align-top">
                      <Keys keys={s.keys} />
                    </td>
                    <td className="py-0.5 text-ink-2">{s.label}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        ))}
      </div>
    </Dialog>
  )
}

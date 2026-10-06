/**
 * The `?` help sheet: the keyboard, rendered from the table that binds the
 * keys; then the glossary, so the nouns the keys act on (Find, Spoil,
 * Stratum, Mesh) can be looked up without leaving the page; then what every
 * action button does, rendered from the table the "?" panels beside the
 * buttons use.
 */
import { Dialog } from './ui/Dialog'
import { Keys } from './ui/Kbd'
import { SCOPE_LABELS, SHORTCUTS, type ShortcutScope } from '@/lib/shortcuts'
import { useUi } from '@/lib/stores'
import { useSettings } from '@/lib/queries'
import { GLOSSARY } from '@/lib/glossary'
import { ACTION_HELP, CORRECTION_NOTE, type HelpGroup } from '@/lib/actionHelp'
import { ActionHelpTable } from './ActionHelp'

const ORDER: ShortcutScope[] = ['global', 'list', 'finds', 'messages', 'review', 'meshes', 'director']
const PALETTE_KEYS = SHORTCUTS.find((s) => s.id === 'palette')?.keys ?? 'mod+k'

export function HelpSheet() {
  const open = useUi((s) => s.helpOpen)
  const setOpen = useUi((s) => s.setHelpOpen)
  const developerMode = useSettings().data?.developerMode ?? false
  const glossary = GLOSSARY.filter((g) => developerMode || !g.developer)
  return (
    <Dialog open={open} onOpenChange={setOpen} title="Help" description="Keyboard shortcuts, the words Riddle uses, then what every button does. The ? beside a row of buttons shows the same for the account you are looking at." wide>
      <div className="max-h-[70vh] overflow-auto pr-1 selectable">
      <h2 className="mb-1 text-sm font-semibold">Keyboard</h2>
      <p className="mb-2 text-xs text-ink-2">
        <Keys keys={PALETTE_KEYS} /> opens the command palette: the commands bound on the page you are on, each with its key.
      </p>
      <div className="grid grid-cols-2 gap-x-8 gap-y-4">
        {ORDER.map((scope) => (
          <section key={scope}>
            <h3 className="mb-1 text-xs font-semibold uppercase tracking-wide text-clay">{SCOPE_LABELS[scope]}</h3>
            <table className="w-full text-sm">
              <tbody>
                {SHORTCUTS.filter((s) => s.scope === scope).map((s) => (
                  <tr key={s.id}>
                    <td className="w-28 py-0.5 align-top">
                      <Keys keys={s.keys} also={s.also} />
                    </td>
                    <td className="py-0.5 text-ink-2">{s.label}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </section>
        ))}
      </div>
      <h2 className="mb-1 mt-6 text-sm font-semibold">Glossary</h2>
      <p className="text-xs text-ink-2">The nouns, in the order you meet them.</p>
      <dl className="mt-2 grid grid-cols-[max-content_1fr] gap-x-4 gap-y-1.5 text-sm">
        {glossary.map((g) => (
          <div key={g.term} className="contents">
            <dt className="font-medium text-ink">{g.term}</dt>
            <dd className="text-ink-2">{g.meaning}</dd>
          </div>
        ))}
      </dl>
      <h2 className="mb-1 mt-6 text-sm font-semibold">What the buttons do</h2>
      <p className="text-xs text-ink-2">{CORRECTION_NOTE} In a development build their Gmail changes wait until you send them or allow mailbox writes (Developer → Mailbox writes).</p>
      {(Object.keys(ACTION_HELP) as HelpGroup[]).map((g) => (
        <section key={g} className="mt-4">
          <h3 className="text-xs font-semibold uppercase tracking-wide text-clay">{ACTION_HELP[g].title}</h3>
          <p className="mb-1 mt-0.5 text-xs text-ink-2">{ACTION_HELP[g].intro}</p>
          <ActionHelpTable group={g} ctx={{}} />
        </section>
      ))}
      </div>
    </Dialog>
  )
}

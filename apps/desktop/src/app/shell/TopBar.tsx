import { HelpCircle, PenSquare } from 'lucide-react'
import { Button } from '@/components/ui/Button'
import { Chip } from '@/components/ui/Chip'
import { useDevInfo } from '@/lib/queries'
import { useUi } from '@/lib/stores'
import { DigSwitcher } from './DigSwitcher'
import { SearchBox } from './SearchBox'
import { TaskCentrePopover } from './TaskCentrePopover'
import { useComposerActions } from '@/features/messages/composer'

export function TopBar() {
  const dev = useDevInfo()
  const setHelpOpen = useUi((s) => s.setHelpOpen)
  const { compose } = useComposerActions()
  // `data-tauri-drag-region="deep"` is what actually moves the window: macOS
  // renders in WKWebView, which ignores `-webkit-app-region`. Tauri's script
  // excludes buttons, inputs and anything with an interactive role, so the
  // controls below still click. The left padding clears the traffic lights.
  return (
    <header
      data-tauri-drag-region="deep"
      className="flex h-11 shrink-0 items-center gap-3 border-b border-line bg-sand/60 pl-[84px] pr-3"
    >
      <DigSwitcher />
      <div className="flex-1" />
      <SearchBox />
      <div className="flex-1" />
      {dev.data?.dev ? (
        <Chip tone="warn" title="Development build: separate data directory and keychain; provider mutations need arming in Developer Mode">
          DEV
        </Chip>
      ) : null}
      <Button size="sm" variant="ghost" icon={<PenSquare />} onClick={() => compose()} title="Compose (c)">
        Compose
      </Button>
      <TaskCentrePopover />
      <Button size="sm" variant="ghost" icon={<HelpCircle />} onClick={() => setHelpOpen(true)} title="Keyboard help (?)" aria-label="Keyboard help">
        ?
      </Button>
    </header>
  )
}

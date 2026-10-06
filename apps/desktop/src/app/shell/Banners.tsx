/**
 * Persistent banners, until their condition clears: a store fault, an
 * update waiting for review, and sample mail. The sample-mail banner exists
 * because a test mailbox looks exactly like a real one once it has filled;
 * a reviewer resolved three Finds before noticing none of it was theirs.
 */
import { Link, useNavigate } from 'react-router-dom'
import { Button } from '@/components/ui/Button'
import { useDigs } from '@/lib/queries'
import { useBanners } from '@/lib/stores'
import { HeldChanges, useMailboxWrites } from '@/features/developer/MailboxWrites'

export function Banners() {
  const storeFault = useBanners((s) => s.storeFault)
  const update = useBanners((s) => s.updateAvailable)
  const setUpdate = useBanners((s) => s.setUpdateAvailable)
  const digs = useDigs().data ?? []
  const sampleOnly = digs.length > 0 && digs.every((d) => d.provider === 'fixture')
  const nav = useNavigate()
  // A dev build on Ask: the batch of mailbox changes waits for one "Send".
  const writes = useMailboxWrites().data
  const asking = writes?.writes === 'ask' && Number(writes.held) > 0
  if (!storeFault && !update && !sampleOnly && !asking) return null
  return (
    <div className="shrink-0">
      {storeFault ? (
        <div role="alert" className="flex items-center justify-between gap-3 border-b border-rust/40 bg-rust/10 px-4 py-1.5 text-sm">
          <span>
            <span className="font-semibold text-rust">Store fault.</span> {storeFault} Riddle has stopped writing until this clears; nothing is lost and nothing is hidden.
          </span>
          <Button size="sm" onClick={() => nav('/developer')}>
            Open Developer Mode
          </Button>
        </div>
      ) : null}
      {update ? (
        <div className="flex items-center justify-between gap-3 border-b border-ochre/40 bg-ochre/10 px-4 py-1.5 text-sm">
          <span>
            <span className="font-semibold">Riddle {update} is ready.</span> If it changes a default Mesh or Signal, you will preview the effect before anything is reclassified.
          </span>
          <div className="flex gap-2">
            <Button size="sm" variant="ghost" onClick={() => setUpdate(null)}>
              Later
            </Button>
            <Button size="sm" variant="primary" onClick={() => nav('/settings#update')}>
              Review the update
            </Button>
          </div>
        </div>
      ) : null}
      {asking && writes ? (
        <div role="status" className="flex items-center justify-between gap-3 border-b border-ochre/40 bg-ochre/10 px-4 py-1 text-sm">
          <span className="text-ink-2">This dev build asks before it writes to the mailbox.</span>
          <HeldChanges held={Number(writes.held)} compact />
        </div>
      ) : null}
      {sampleOnly ? (
        <div role="status" className="flex items-center justify-between gap-3 border-b border-line bg-sand px-4 py-1 text-xs text-ink-2">
          <span>
            <span className="font-medium text-ink">This is sample mail.</span> Connect Gmail when you are ready.
          </span>
          <Link to="/settings#add-account" className="shrink-0 text-ochre hover:underline">
            Connect Gmail in Settings
          </Link>
        </div>
      ) : null}
    </div>
  )
}

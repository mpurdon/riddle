import { useNavigate } from 'react-router-dom'
import { Button } from '@/components/ui/Button'
import { useBanners } from '@/lib/stores'

export function Banners() {
  const storeFault = useBanners((s) => s.storeFault)
  const update = useBanners((s) => s.updateAvailable)
  const setUpdate = useBanners((s) => s.setUpdateAvailable)
  const nav = useNavigate()
  if (!storeFault && !update) return null
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
              Review and relaunch
            </Button>
          </div>
        </div>
      ) : null}
    </div>
  )
}

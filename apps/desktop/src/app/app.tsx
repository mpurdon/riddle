import { RouterProvider } from 'react-router-dom'
import { TooltipProvider } from '@/components/ui/Tooltip'
import { router } from './routes'

export function App() {
  return (
    <TooltipProvider>
      <RouterProvider router={router} />
    </TooltipProvider>
  )
}

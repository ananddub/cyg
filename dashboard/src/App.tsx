import { Toaster } from "@/components/ui/sonner"
import { DashboardPage } from "@/pages/dashboard"

function App() {
  return (
    <>
      <DashboardPage />
      {/* Dark-only app: pin sonner to the dark palette rather than `system`,
          which would render light toasts on a light-mode OS. */}
      <Toaster theme="dark" position="bottom-right" closeButton gap={8} />
    </>
  )
}

export default App

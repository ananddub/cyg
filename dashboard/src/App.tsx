import { Toaster } from "@/components/ui/sonner"
import { ConsolePage } from "@/pages/console"
import { LoginPage } from "@/pages/login"
import { useHashRoute } from "@/hooks/use-hash-route"
import { consoleLogout } from "@/lib/console"
import { defaultArea, findArea } from "@/lib/areas"
import { clearSession, useSession } from "@/lib/session"

function App() {
  const session = useSession()
  const [route, navigate] = useHashRoute()

  // Dropping the local session first means the UI is on the login screen before
  // the network call settles; a failed revoke must not trap the operator inside.
  function logout() {
    const token = session?.token
    clearSession()
    if (token !== undefined && token !== "") {
      consoleLogout(token).catch(() => {
        // Already signed out locally; the server may have expired it anyway.
      })
    }
  }

  return (
    <>
      {session === null ? (
        <LoginPage />
      ) : (
        <ConsolePage
          session={session}
          area={findArea(route) ?? defaultArea()}
          onNavigate={navigate}
          onLogout={logout}
        />
      )}
      {/* Dark-only app: pin sonner to the dark palette rather than `system`,
          which would render light toasts on a light-mode OS. */}
      <Toaster theme="dark" position="bottom-right" closeButton gap={8} />
    </>
  )
}

export default App

import { useEffect, useState } from "react"

import { Toaster } from "@/components/ui/sonner"
import { ConsolePage } from "@/pages/console"
import { LoginPage } from "@/pages/login"
import { useHashRoute } from "@/hooks/use-hash-route"
import { consoleLogout } from "@/lib/console"
import { clearSession, saveSession, useSession } from "@/lib/session"
import type { ConsoleSession } from "@/lib/console"
import { consoleMode } from "@/lib/console"
import { useTheme } from "@/lib/theme"

/** The session `cyg run -d` implies, so the console opens with no login form. */
const DEV_SESSION: ConsoleSession = { token: "dev", username: "dev", role: "admin" }

function App() {
  const session = useSession()
  const mode = useTheme()
  const [route, navigate] = useHashRoute()

  // A local `-d` session skips the login form.
  //
  // The server answers every console request without a token in this mode, so
  // rendering a login would ask for a credential that is never checked. The
  // shell still needs *a* session object to render, so one is fabricated — and
  // only after the server confirms it is a dev process, never from the URL.
  const [devChecked, setDevChecked] = useState(false)
  useEffect(() => {
    if (session !== null) return
    let live = true
    consoleMode()
      .then((answer) => {
        if (live && answer.dev) saveSession(DEV_SESSION)
      })
      .catch(() => {
        // Older server or no console: fall through to the normal login.
      })
      .finally(() => {
        if (live) setDevChecked(true)
      })
    return () => {
      live = false
    }
  }, [session])

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
        devChecked ? <LoginPage /> : null
      ) : (
        <ConsolePage
          session={session}
          route={route}
          onNavigate={navigate}
          onLogout={logout}
        />
      )}
      {/* The toaster follows the chosen theme, so a light session does not get
          a dark toast dropped onto a white console. */}
      <Toaster theme={mode} position="bottom-right" closeButton gap={8} />
    </>
  )
}

export default App

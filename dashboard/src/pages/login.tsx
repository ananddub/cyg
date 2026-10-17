import { useState, type FormEvent } from "react"
import { Loader2, LockKeyhole } from "lucide-react"

import { Button } from "@/components/ui/button"
import { ConsoleError, MFA_CODE_REQUIRED, consoleLogin } from "@/lib/console"
import { saveSession } from "@/lib/session"
import logoUrl from "@/assets/logo.png"

/** The console gate. A successful login writes the session, and `App` re-renders
 * into the console shell — this page never navigates. */
export function LoginPage() {
  const [username, setUsername] = useState("")
  const [password, setPassword] = useState("")
  const [mfa, setMfa] = useState("")
  // Whether to show the code field. There is no endpoint that reports enrolment,
  // so the field appears the first time the server asks for a code rather than
  // up front, where it would be noise for every operator without one.
  const [mfaAsked, setMfaAsked] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  async function submit(event: FormEvent) {
    event.preventDefault()
    if (busy) return
    setBusy(true)
    setError(null)
    try {
      saveSession(await consoleLogin(username, password, mfa))
    } catch (cause) {
      // "MFA code required." is the one refusal the page acts on: the password was
      // right, and the console is asking for the code it has no field for yet.
      if (cause instanceof ConsoleError && cause.message === MFA_CODE_REQUIRED) {
        setMfaAsked(true)
      } else {
        setError(cause instanceof ConsoleError ? cause.message : String(cause))
      }
      setBusy(false)
    }
  }

  return (
    <div className="flex min-h-svh w-full items-center justify-center bg-background p-4 text-foreground">
      <div className="w-full max-w-sm">
        <div className="mb-6 flex flex-col items-center gap-3">
          <img
            src={logoUrl}
            alt="cookyourgame (cfg)"
            className="h-14 w-auto object-contain drop-shadow-sm"
          />
          <div className="text-center">
            <h1 className="font-heading text-lg font-semibold tracking-tight">Admin Console</h1>
            <p className="mt-0.5 text-xs text-muted-foreground">
              Sign in to manage accounts, groups and the runtime.
            </p>
          </div>
        </div>

        <form
          onSubmit={submit}
          className="space-y-3 rounded-lg border border-border/60 bg-card/60 p-5 shadow-xs backdrop-blur-md"
        >
          <label className="block space-y-1.5">
            <span className="text-xs font-medium text-muted-foreground">Username or email</span>
            <input
              autoFocus
              autoComplete="username"
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
            />
          </label>

          <label className="block space-y-1.5">
            <span className="text-xs font-medium text-muted-foreground">Password</span>
            <input
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              className="h-9 w-full rounded-md border border-border bg-background px-3 text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
            />
          </label>

          {mfaAsked && (
            <label className="block space-y-1.5">
              <span className="text-xs font-medium text-muted-foreground">Authenticator code</span>
              <input
                autoFocus
                inputMode="numeric"
                autoComplete="one-time-code"
                placeholder="123456, or a recovery code"
                value={mfa}
                onChange={(event) => setMfa(event.target.value)}
                className="h-9 w-full rounded-md border border-border bg-background px-3 font-mono text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
              />
              <span className="block text-[11px] text-muted-foreground">
                A code from your authenticator app, or one of the recovery codes you
                wrote down at enrolment.
              </span>
            </label>
          )}

          {error !== null && (
            <p className="rounded-md bg-destructive/10 px-3 py-2 text-xs text-destructive">{error}</p>
          )}

          <Button type="submit" className="h-9 w-full" disabled={busy}>
            {busy ? <Loader2 className="animate-spin" /> : <LockKeyhole />}
            {busy ? "Signing in…" : "Sign in"}
          </Button>
        </form>

        <p className="mt-4 text-center text-[11px] text-muted-foreground/70">
          Console credentials are separate from player sessions.
        </p>
      </div>
    </div>
  )
}

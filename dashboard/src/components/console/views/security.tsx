// The signed-in operator's own second factor. The console only offers this to
// `console_user` accounts — the config bootstrap admin has no row to enrol, and
// the server says so — so the page is written to show the server's wording rather
// than guess at who may use it.

import { useState } from "react"
import { Loader2, ShieldCheck, ShieldOff } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Mono, PageHeading, Panel } from "@/components/console/data-table"
import { CONSOLE_ENDPOINTS, consoleWrite, type MfaEnrolment } from "@/lib/console"
import { useSession } from "@/lib/session"

import { PasswordChangePanel } from "./security-password"

/** Which request is in flight, so one card's spinner does not light up another's. */
type Step = "enrol" | "confirm" | "disable"

export function SecurityView() {
  const token = useSession()?.token ?? ""
  const [enrolment, setEnrolment] = useState<MfaEnrolment | null>(null)
  const [confirmCode, setConfirmCode] = useState("")
  const [disableCode, setDisableCode] = useState("")
  const [busy, setBusy] = useState<Step | null>(null)

  /** Every request is "run one thing, report the failure verbatim": the server's
   * message is the one that names the cause, and a toast is where it goes. */
  async function act(step: Step, run: () => Promise<void>) {
    if (busy !== null) return
    setBusy(step)
    try {
      await run()
    } catch (cause) {
      toast.error(cause instanceof Error ? cause.message : String(cause))
    } finally {
      setBusy(null)
    }
  }

  async function enrol() {
    await act("enrol", async () => {
      const next = await consoleWrite<MfaEnrolment>("POST", CONSOLE_ENDPOINTS.mfa, token)
      setEnrolment(next)
      setConfirmCode("")
      toast.success("Secret generated — confirm it with a code.")
    })
  }

  async function confirm() {
    await act("confirm", async () => {
      await consoleWrite("POST", `${CONSOLE_ENDPOINTS.mfa}/verify`, token, { code: confirmCode })
      setEnrolment(null)
      setConfirmCode("")
      toast.success("Second factor confirmed.")
    })
  }

  async function disable() {
    await act("disable", async () => {
      await consoleWrite("DELETE", CONSOLE_ENDPOINTS.mfa, token, { code: disableCode })
      setDisableCode("")
      toast.success("Second factor removed.")
    })
  }

  const codeField = (
    label: string,
    value: string,
    onChange: (value: string) => void,
  ) => (
    <label className="block space-y-1.5">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <input
        inputMode="numeric"
        autoComplete="one-time-code"
        placeholder="123456, or a recovery code"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="h-9 w-full rounded-md border border-border bg-background px-3 font-mono text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
      />
    </label>
  )

  return (
    <div className="space-y-6">
      <PageHeading
        title="My Security"
        description="Your own console credential. The second factor is what a stolen password cannot replace."
      />

      <div className="grid gap-6 xl:grid-cols-2">
        <Panel
          title="Second factor"
          description="RFC 6238 TOTP: an authenticator app, checked with a one-step drift."
        >
          {enrolment === null ? (
            <div className="space-y-3 p-4">
              <p className="text-xs text-muted-foreground">
                Generate a secret, scan it into your authenticator app, then confirm the code it
                shows. Generating again replaces any secret already enrolled.
              </p>
              <Button onClick={enrol} disabled={busy !== null}>
                {busy === "enrol" ? <Loader2 className="animate-spin" /> : <ShieldCheck />}
                Generate a secret
              </Button>
            </div>
          ) : (
            <div className="space-y-4 p-4">
              <div className="space-y-1">
                <p className="text-xs text-muted-foreground">
                  Scan this into your app, or type the secret in by hand.
                </p>
                <Mono className="block break-all text-foreground">{enrolment.uri}</Mono>
                <Mono className="block tracking-widest text-foreground">{enrolment.secret}</Mono>
              </div>

              <div className="space-y-1">
                <p className="text-xs font-medium text-warning">
                  Recovery codes — write these down now. They are shown once.
                </p>
                <div className="grid grid-cols-2 gap-1 sm:grid-cols-4">
                  {enrolment.recovery_codes.map((code) => (
                    <Mono key={code} className="text-foreground">
                      {code}
                    </Mono>
                  ))}
                </div>
              </div>

              {codeField("Code from the app", confirmCode, setConfirmCode)}
              <div className="flex gap-2">
                <Button onClick={confirm} disabled={busy !== null || confirmCode === ""}>
                  {busy === "confirm" && <Loader2 className="animate-spin" />}
                  Confirm
                </Button>
                <Button variant="outline" onClick={() => setEnrolment(null)} disabled={busy !== null}>
                  Discard
                </Button>
              </div>
            </div>
          )}
        </Panel>

        <Panel
          title="Turn it off"
          description="A current code or a recovery code is required, so a stolen token cannot remove it."
        >
          <div className="space-y-3 p-4">
            {codeField("Current code", disableCode, setDisableCode)}
            <Button
              variant="outline"
              onClick={disable}
              disabled={busy !== null || disableCode === ""}
            >
              {busy === "disable" ? <Loader2 className="animate-spin" /> : <ShieldOff />}
              Remove the second factor
            </Button>
            <p className="text-[11px] text-muted-foreground">
              The config bootstrap account holds no second factor and cannot enrol one: it is the
              account of last resort, and stays password-only.
            </p>
          </div>
        </Panel>

        <PasswordChangePanel />
      </div>
    </div>
  )
}

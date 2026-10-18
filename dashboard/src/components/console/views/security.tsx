// The signed-in operator's own second factor. The console only offers this to
// `console_user` accounts — the config bootstrap admin has no row to enrol, and
// the server says so — so the page is written to show the server's wording rather
// than guess at who may use it.
//
// Settings-row layout: one row per act. The enrolment ceremony (secret, QR uri,
// recovery codes, confirm) is a real form, so it runs in the drawer; removing
// the factor is a single-field prompt, so it stays a centered confirm.

import { useState } from "react"
import { Loader2, ShieldCheck, ShieldOff } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { ConfirmDialog } from "@/components/console/modal"
import { FormSheet } from "@/components/console/form-sheet"
import { Mono, PageHeading, Panel } from "@/components/console/data-table"
import { CONSOLE_ENDPOINTS, consoleWrite, type MfaEnrolment } from "@/lib/console"
import { useSession } from "@/lib/session"

import { PasswordChangeDialog } from "./security-password"

/** Which request is in flight, so one card's spinner does not light up another's. */
type Step = "enrol" | "confirm" | "disable"

export function SecurityView() {
  const token = useSession()?.token ?? ""
  const [enrolment, setEnrolment] = useState<MfaEnrolment | null>(null)
  const [confirmCode, setConfirmCode] = useState("")
  const [disableOpen, setDisableOpen] = useState(false)
  const [disableCode, setDisableCode] = useState("")
  const [busy, setBusy] = useState<Step | null>(null)
  const [changingPassword, setChangingPassword] = useState(false)

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
      setDisableOpen(false)
      toast.success("Second factor removed.")
    })
  }

  return (
    <div className="max-w-3xl space-y-6">
      <PageHeading
        title="My Security"
        description="Your own console credential. The second factor is what a stolen password cannot replace."
      />

      <Panel
        title="Second factor"
        description="RFC 6238 TOTP: an authenticator app, checked with a one-step drift."
        actions={
          <Button size="sm" onClick={enrol} disabled={busy !== null}>
            {busy === "enrol" ? <Loader2 className="animate-spin" /> : <ShieldCheck />}
            Generate a secret
          </Button>
        }
      >
        <p className="px-4 py-4 text-xs text-muted-foreground">
          Generate a secret, scan it into your authenticator app, then confirm the code it shows.
          Generating again replaces any secret already enrolled.
        </p>
      </Panel>

      <Panel
        title="Turn it off"
        description="A current code or a recovery code is required, so a stolen token cannot remove it."
        actions={
          <Button size="sm" variant="outline" onClick={() => setDisableOpen(true)}>
            <ShieldOff />
            Remove
          </Button>
        }
      >
        <p className="px-4 py-4 text-[11px] text-muted-foreground">
          The config bootstrap account holds no second factor and cannot enrol one: it is the
          account of last resort, and stays password-only.
        </p>
      </Panel>

      <Panel
        title="Password"
        description="Prove the current password before setting the new one."
        actions={
          <Button size="sm" variant="outline" onClick={() => setChangingPassword(true)}>
            Change password
          </Button>
        }
      >
        <p className="px-4 py-4 text-[11px] text-muted-foreground">
          The change is verified against the stored credential first, so a console token that fell
          into the wrong hands cannot silently lock its owner out.
        </p>
      </Panel>

      <FormSheet
        open={enrolment !== null}
        title="Confirm the enrolment"
        description="Scan the secret into your app, then type the code it shows."
        onClose={() => setEnrolment(null)}
        footer={
          <>
            <Button variant="outline" onClick={() => setEnrolment(null)} disabled={busy !== null}>
              Discard
            </Button>
            <Button onClick={confirm} disabled={busy !== null || confirmCode === ""}>
              {busy === "confirm" && <Loader2 className="animate-spin" />}
              Confirm
            </Button>
          </>
        }
      >
        {enrolment !== null && (
          <>
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
              <div className="grid grid-cols-2 gap-1">
                {enrolment.recovery_codes.map((code) => (
                  <Mono key={code} className="text-foreground">
                    {code}
                  </Mono>
                ))}
              </div>
            </div>

            <label className="block space-y-1.5">
              <span className="text-xs font-medium text-muted-foreground">Code from the app</span>
              <input
                autoFocus
                inputMode="numeric"
                autoComplete="one-time-code"
                placeholder="123456, or a recovery code"
                value={confirmCode}
                onChange={(event) => setConfirmCode(event.target.value)}
                className="h-9 w-full rounded-md border border-border bg-background px-3 font-mono text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
              />
            </label>
          </>
        )}
      </FormSheet>

      <ConfirmDialog
        open={disableOpen}
        title="Remove the second factor"
        message="Sign-in falls back to the password alone until you enrol again."
        confirmLabel="Remove"
        destructive
        busy={busy !== null}
        onClose={() => {
          setDisableCode("")
          setDisableOpen(false)
        }}
        onConfirm={disable}
      >
        <label className="block space-y-1.5">
          <span className="text-xs font-medium text-muted-foreground">Current code</span>
          <input
            inputMode="numeric"
            autoComplete="one-time-code"
            placeholder="123456, or a recovery code"
            value={disableCode}
            onChange={(event) => setDisableCode(event.target.value)}
            className="h-9 w-full rounded-md border border-border bg-background px-3 font-mono text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
          />
        </label>
      </ConfirmDialog>

      <PasswordChangeDialog open={changingPassword} onClose={() => setChangingPassword(false)} />
    </div>
  )
}

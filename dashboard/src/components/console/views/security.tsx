// The signed-in operator's own second factor. The console only offers this to
// `console_user` accounts — the config bootstrap admin has no row to enrol, and
// the server says so — so the page is written to show the server's wording rather
// than guess at who may use it.
//
// One card, one row per credential: icon, name, one line of truth, and the act
// on the right. The enrolment ceremony runs in the drawer; removal is a
// single-field prompt, so it stays a centered confirm.

import { useState } from "react"
import { KeyRound, Loader2, ShieldCheck } from "lucide-react"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Mono, PageHeading, Panel } from "@/components/console/data-table"
import { FormSheet } from "@/components/console/form-sheet"
import { ConfirmDialog } from "@/components/console/modal"
import { CONSOLE_ENDPOINTS, consoleWrite, type MfaEnrolment } from "@/lib/console"
import { useSession } from "@/lib/session"

import { PasswordChangeDialog } from "./security-password"

/** Which request is in flight, so one row's spinner does not light up another's. */
type Step = "enrol" | "confirm" | "disable"

function CodeInput({
  label,
  value,
  onChange,
  autoFocus,
}: {
  label: string
  value: string
  onChange: (value: string) => void
  autoFocus?: boolean
}) {
  return (
    <label className="block space-y-1.5">
      <span className="text-xs font-medium text-muted-foreground">{label}</span>
      <input
        autoFocus={autoFocus}
        inputMode="numeric"
        autoComplete="one-time-code"
        placeholder="123456, or a recovery code"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="h-9 w-full rounded-md border border-border bg-background px-3 font-mono text-sm outline-none focus-visible:border-ring focus-visible:ring-3 focus-visible:ring-ring/50"
      />
    </label>
  )
}

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

  const enrolled = enrolment !== null

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-6">
      <PageHeading
        title="My Security"
        description="Your own console credential. The second factor is what a stolen password cannot replace."
      />

      <Panel
        title="Credentials"
        description="The config bootstrap account is password-only and cannot enrol."
        className="max-w-3xl"
      >
        <div className="divide-y divide-border/60">
          <div className="flex items-center gap-4 px-4 py-4">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/25">
              <ShieldCheck className="size-5" />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-foreground">Two-factor authentication</p>
              <p className="text-xs text-muted-foreground">
                RFC 6238 TOTP from an authenticator app, checked with a one-step drift.
              </p>
            </div>
            <div className="flex shrink-0 gap-2">
              <Button size="sm" onClick={enrol} disabled={busy !== null}>
                {busy === "enrol" ? <Loader2 className="animate-spin" /> : <ShieldCheck />}
                Set up
              </Button>
              <Button size="sm" variant="ghost" onClick={() => setDisableOpen(true)} disabled={busy !== null}>
                Remove
              </Button>
            </div>
          </div>

          <div className="flex items-center gap-4 px-4 py-4">
            <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-primary/10 text-primary ring-1 ring-primary/25">
              <KeyRound className="size-5" />
            </div>
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-foreground">Password</p>
              <p className="text-xs text-muted-foreground">
                Verified against the stored credential first, so a stolen token cannot lock you out.
              </p>
            </div>
            <Button size="sm" variant="outline" className="shrink-0" onClick={() => setChangingPassword(true)}>
              Change
            </Button>
          </div>
        </div>
      </Panel>

      <FormSheet
        open={enrolled}
        title="Confirm the enrolment"
        description="Scan the secret into your app, then type the code it shows."
        onClose={() => setEnrolment(null)}
        footer={
          <Button onClick={confirm} disabled={busy !== null || confirmCode === ""}>
            {busy === "confirm" && <Loader2 className="animate-spin" />}
            Confirm
          </Button>
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

            <CodeInput label="Code from the app" value={confirmCode} onChange={setConfirmCode} autoFocus />
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
        <CodeInput label="Current code" value={disableCode} onChange={setDisableCode} autoFocus />
      </ConfirmDialog>

      <PasswordChangeDialog open={changingPassword} onClose={() => setChangingPassword(false)} />
    </div>
  )
}

// `POST /v2/console/password_change` — the signed-in operator's own credential.
// The current password is verified first, so a console token that fell into the
// wrong hands cannot silently lock its owner out. The bootstrap account has no
// stored credential to check, so the server refuses it — that refusal is shown
// verbatim rather than pre-empted here.

import { useState } from "react"
import { Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { TextInput } from "@/components/console/form"
import { FormSheet } from "@/components/console/form-sheet"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS } from "@/lib/console"

export function PasswordChangeDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const [current, setCurrent] = useState("")
  const [next, setNext] = useState("")
  const write = useConsoleMutation()

  function close() {
    setCurrent("")
    setNext("")
    onClose()
  }

  const ready = current !== "" && next !== ""

  return (
    <FormSheet
      open={open}
      title="Change password"
      description="Prove the current one before setting the new one."
      onClose={close}
      footer={
        <>
          <Button variant="outline" onClick={close} disabled={write.isPending}>
            Cancel
          </Button>
          <Button
            onClick={() =>
              write.mutate(
                {
                  method: "POST",
                  path: CONSOLE_ENDPOINTS.passwordChange,
                  body: { current_password: current, password: next },
                  success: "Password changed.",
                },
                { onSuccess: close },
              )
            }
            disabled={!ready || write.isPending}
          >
            {write.isPending && <Loader2 className="animate-spin" />}
            Change password
          </Button>
        </>
      }
    >
      <TextInput
        label="Current password"
        type="password"
        value={current}
        onChange={setCurrent}
        autoFocus
      />
      <TextInput
        label="New password"
        type="password"
        value={next}
        onChange={setNext}
        hint="At least eight characters."
      />
    </FormSheet>
  )
}

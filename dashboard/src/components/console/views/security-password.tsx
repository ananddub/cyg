// `POST /v2/console/password_change` — the signed-in operator's own credential.
// The current password is verified first, so a console token that fell into the
// wrong hands cannot silently lock its owner out. The bootstrap account has no
// stored credential to check, so the server refuses it — that refusal is shown
// verbatim rather than pre-empted here.

import { useState } from "react"
import { Loader2 } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Panel } from "@/components/console/data-table"
import { TextInput } from "@/components/console/form"
import { useConsoleMutation } from "@/hooks/use-console"
import { CONSOLE_ENDPOINTS } from "@/lib/console"

export function PasswordChangePanel() {
  const [current, setCurrent] = useState("")
  const [next, setNext] = useState("")
  const write = useConsoleMutation()

  function close() {
    setCurrent("")
    setNext("")
  }

  const ready = current !== "" && next !== ""

  return (
    <Panel
      title="Change password"
      description="Prove the current one before setting the new one."
    >
      <form
        className="space-y-3 p-4"
        onSubmit={(event) => {
          event.preventDefault()
          if (!ready) return
          write.mutate(
            {
              method: "POST",
              path: CONSOLE_ENDPOINTS.passwordChange,
              body: { current_password: current, password: next },
              success: "Password changed.",
            },
            { onSuccess: close },
          )
        }}
      >
        <TextInput
          label="Current password"
          type="password"
          value={current}
          onChange={setCurrent}
        />
        <TextInput
          label="New password"
          type="password"
          value={next}
          onChange={setNext}
          hint="At least eight characters."
        />
        <Button type="submit" disabled={!ready || write.isPending}>
          {write.isPending && <Loader2 className="animate-spin" />}
          Change password
        </Button>
      </form>
    </Panel>
  )
}
